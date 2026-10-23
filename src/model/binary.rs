// SPDX-License-Identifier: GPL-2.0-only
//! The format-neutral binary, and its chunk table.

use super::linkmap::{LinkMap, MemoryRegion};
use super::{AddrSpace, Sizes, Span};
use smallvec::SmallVec;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Index of a binary in the pipeline state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BinaryId(pub u32);

/// The raw bytes of a file.
pub enum Bytes {
    Mmap(memmap2::Mmap),
    Owned(Vec<u8>),
}

impl Deref for Bytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Bytes::Mmap(m) => m,
            Bytes::Owned(v) => v,
        }
    }
}

impl std::fmt::Debug for Bytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Bytes({} bytes)", self.len())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatInfo {
    Elf {
        machine: u16,
        class64: bool,
        little_endian: bool,
        relocatable: bool,
    },
    Unknown,
}

impl FormatInfo {
    pub const EM_ARM: u16 = 40;

    pub fn is_arm(self) -> bool {
        matches!(self, FormatInfo::Elf { machine, .. } if machine == Self::EM_ARM)
    }
}

/// Generic section kind. The raw type and flags are kept on [`Section`]
/// for format-specific needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SectionKind {
    Code,
    ReadOnlyData,
    Data,
    Bss,
    /// Unwind tables (`.ARM.exidx`, `.eh_frame`, …), code-adjacent.
    Unwind,
    Debug,
    SymbolTable,
    StringTable,
    Relocation,
    Note,
    Other,
}

#[derive(Clone, Debug)]
pub struct Section {
    pub index: usize,
    pub name: String,
    pub kind: SectionKind,
    pub alloc: bool,
    pub write: bool,
    pub exec: bool,
    /// No content in the file (ELF `SHT_NOBITS`).
    pub nobits: bool,
    /// Thread-local; its NOBITS part takes no address space of its own.
    pub tls: bool,
    /// Range in the file, if it has content there.
    pub file: Option<Span>,
    pub vma: u64,
    /// Load address, for allocated sections with content. Filled in by
    /// [`Binary::from_parts`] from the segments.
    pub lma: Option<u64>,
    pub size: u64,
    pub raw_type: u32,
    pub raw_flags: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentKind {
    Load,
    Other(u32),
}

#[derive(Clone, Debug)]
pub struct Segment {
    pub index: usize,
    pub kind: SegmentKind,
    pub read: bool,
    pub write: bool,
    pub exec: bool,
    /// The file part, `p_offset .. p_offset + p_filesz`.
    pub file: Span,
    pub vaddr: u64,
    pub paddr: u64,
    pub memsz: u64,
}

impl Segment {
    pub fn vma_span(&self) -> Span {
        Span::at(self.vaddr, self.memsz)
    }

    pub fn flags_text(&self) -> String {
        format!(
            "{}{}{}",
            if self.read { 'R' } else { '-' },
            if self.write { 'W' } else { '-' },
            if self.exec { 'X' } else { '-' }
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Function,
    Object,
    Section,
    File,
    Tls,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SymbolBinding {
    Global,
    Weak,
    Local,
    Other,
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    /// Runtime address. For ARM, the Thumb bit is already cleared.
    pub addr: u64,
    pub size: u64,
    pub kind: SymbolKind,
    pub binding: SymbolBinding,
    /// Index into [`Binary::sections`].
    pub section: Option<usize>,
}

/// A named file range outside the sections, such as the ELF header.
#[derive(Clone, Debug)]
pub struct FileRegion {
    pub name: String,
    pub span: Span,
}

/// A contiguous run of bytes with the same length in every space it has.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Chunk {
    pub unified: Span,
    pub file: Option<u64>,
    pub vma: Option<u64>,
    pub lma: Option<u64>,
    /// Free space in a memory region, added by the `free-space` stage.
    pub free: bool,
}

impl Chunk {
    pub fn len(&self) -> u64 {
        self.unified.len()
    }

    pub fn is_empty(&self) -> bool {
        self.unified.is_empty()
    }

    pub fn start_in(&self, space: AddrSpace) -> Option<u64> {
        match space {
            AddrSpace::File => self.file,
            AddrSpace::Vma => self.vma,
            AddrSpace::Lma => self.lma,
            AddrSpace::Unified => Some(self.unified.start),
        }
    }

    pub fn span_in(&self, space: AddrSpace) -> Option<Span> {
        self.start_in(space).map(|s| Span::at(s, self.len()))
    }

    pub fn has_file(&self) -> bool {
        self.file.is_some()
    }

    /// Allocated at runtime.
    pub fn is_memory(&self) -> bool {
        self.vma.is_some()
    }

    /// Has content that is loaded: part of the flash image.
    pub fn is_load(&self) -> bool {
        self.file.is_some() && self.is_memory()
    }

    /// Where the chunk is stored: the load address for bytes with content
    /// (`.data` is stored in flash), the runtime address otherwise.
    pub fn addr(&self) -> Option<u64> {
        if self.file.is_some() {
            self.lma.or(self.vma)
        } else {
            self.vma
        }
    }

    /// Sizes of `len` bytes of this chunk.
    pub fn sizes(&self, len: u64) -> Sizes {
        Sizes {
            file: if self.has_file() { len } else { 0 },
            memory: if self.is_memory() { len } else { 0 },
            load: if self.is_load() { len } else { 0 },
        }
    }
}

#[derive(Clone, Debug, Default)]
struct SpaceIndex {
    /// Chunk indices sorted by start in the space.
    order: Vec<u32>,
    /// Running maximum of the end address over `order`.
    max_end: Vec<u64>,
}

/// The chunks of a binary, laid out after each other in unified space.
#[derive(Clone, Debug, Default)]
pub struct ChunkTable {
    chunks: Vec<Chunk>,
    index: [SpaceIndex; 3],
}

/// A runtime mapping: file bytes (maybe none) placed at a VMA and LMA.
struct Mapping {
    file: Option<Span>,
    vma: u64,
    lma: u64,
    memsz: u64,
}

impl ChunkTable {
    /// From chunks laid out back to back in unified space.
    pub fn new(chunks: Vec<Chunk>) -> Self {
        debug_assert!(
            chunks
                .windows(2)
                .all(|w| w[0].unified.end == w[1].unified.start)
        );
        let mut index: [SpaceIndex; 3] = Default::default();
        for (slot, space) in [AddrSpace::File, AddrSpace::Vma, AddrSpace::Lma]
            .into_iter()
            .enumerate()
        {
            let mut order: Vec<u32> = (0..chunks.len() as u32)
                .filter(|&i| chunks[i as usize].start_in(space).is_some())
                .collect();
            order.sort_by_key(|&i| chunks[i as usize].start_in(space));
            let mut max = 0;
            let max_end = order
                .iter()
                .map(|&i| {
                    max = max.max(chunks[i as usize].span_in(space).unwrap().end);
                    max
                })
                .collect();
            index[slot] = SpaceIndex { order, max_end };
        }
        ChunkTable { chunks, index }
    }

    /// Cut a file into chunks from its segments, or from its sections when
    /// there are no loadable segments (relocatable objects).
    pub fn build(file_len: u64, segments: &[Segment], sections: &[Section]) -> Self {
        let mut maps: Vec<Mapping> = segments
            .iter()
            .filter(|s| s.kind == SegmentKind::Load)
            .map(|s| Mapping {
                file: (!s.file.is_empty()).then_some(s.file),
                vma: s.vaddr,
                lma: s.paddr,
                memsz: s.memsz,
            })
            .collect();
        if maps.is_empty() {
            maps = sections
                .iter()
                .filter(|s| s.alloc && s.size > 0 && !(s.nobits && s.tls))
                .map(|s| Mapping {
                    file: s.file,
                    vma: s.vma,
                    lma: s.vma,
                    memsz: s.size,
                })
                .collect();
        }

        let mut cuts = vec![0, file_len];
        for m in &maps {
            if let Some(f) = m.file {
                cuts.push(f.start.min(file_len));
                cuts.push(f.end.min(file_len));
            }
        }
        cuts.sort_unstable();
        cuts.dedup();

        let mut chunks = Vec::new();
        let mut unified = 0;
        let mut push = |len: u64, file, vma, lma| {
            chunks.push(Chunk {
                unified: Span::at(unified, len),
                file,
                vma,
                lma,
                free: false,
            });
            unified += len;
        };
        for w in cuts.windows(2) {
            let piece = Span::new(w[0], w[1]);
            if piece.is_empty() {
                continue;
            }
            let map = maps
                .iter()
                .find(|m| m.file.is_some_and(|f| f.contains(piece.start)));
            match map {
                Some(m) => {
                    let off = piece.start - m.file.unwrap().start;
                    push(
                        piece.len(),
                        Some(piece.start),
                        Some(m.vma + off),
                        Some(m.lma + off),
                    );
                }
                None => push(piece.len(), Some(piece.start), None, None),
            }
        }
        // Memory-only tails (NOBITS) get no load address: nothing is
        // loaded for them, so they don't take room in flash.
        for m in &maps {
            let filesz = m.file.map_or(0, |f| f.len());
            if m.memsz > filesz {
                push(m.memsz - filesz, None, Some(m.vma + filesz), None);
            }
        }
        ChunkTable::new(chunks)
    }

    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    /// A copy with free-space chunks added at the end, one per VMA range.
    pub fn with_free(&self, free: &[Span]) -> Self {
        let mut chunks = self.chunks.clone();
        let mut unified = self.total();
        for span in free.iter().filter(|s| !s.is_empty()) {
            chunks.push(Chunk {
                unified: Span::at(unified, span.len()),
                file: None,
                vma: Some(span.start),
                lma: None,
                free: true,
            });
            unified += span.len();
        }
        ChunkTable::new(chunks)
    }

    pub fn chunk(&self, index: usize) -> &Chunk {
        &self.chunks[index]
    }

    /// Length of the unified space.
    pub fn total(&self) -> u64 {
        self.chunks.last().map_or(0, |c| c.unified.end)
    }

    /// The chunk containing a unified address.
    pub fn chunk_at(&self, unified: u64) -> Option<usize> {
        let i = self.chunks.partition_point(|c| c.unified.end <= unified);
        (i < self.chunks.len() && self.chunks[i].unified.contains(unified)).then_some(i)
    }

    /// The unified ranges that a range in another space covers.
    pub fn to_unified(&self, space: AddrSpace, span: Span) -> Vec<Span> {
        let mut out = Vec::new();
        self.for_each_unified(space, span, |_, s| out.push(s));
        out
    }

    /// Like [`Self::to_unified`], but calls `f(chunk index, unified span)`.
    pub fn for_each_unified(&self, space: AddrSpace, span: Span, mut f: impl FnMut(usize, Span)) {
        if span.is_empty() {
            return;
        }
        let slot = match space {
            AddrSpace::Unified => {
                let mut i = self.chunks.partition_point(|c| c.unified.end <= span.start);
                while let Some(c) = self.chunks.get(i) {
                    if c.unified.start >= span.end {
                        break;
                    }
                    f(i, c.unified.intersect(span).unwrap());
                    i += 1;
                }
                return;
            }
            AddrSpace::File => 0,
            AddrSpace::Vma => 1,
            AddrSpace::Lma => 2,
        };
        let idx = &self.index[slot];
        let first = idx.max_end.partition_point(|&e| e <= span.start);
        for &ci in &idx.order[first..] {
            let c = &self.chunks[ci as usize];
            let cs = c.span_in(space).unwrap();
            if cs.start >= span.end {
                break;
            }
            if let Some(hit) = cs.intersect(span) {
                let u = c.unified.start + (hit.start - cs.start);
                f(ci as usize, Span::at(u, hit.len()));
            }
        }
    }
}

static NEXT_UID: AtomicU64 = AtomicU64::new(1);

/// Data added to a binary by pipeline stages, from outside the binary.
#[derive(Debug, Clone, Default)]
pub struct Attachments {
    pub linkmap: Option<Arc<LinkMap>>,
    /// Memory regions set by a stage. Override the linker map's.
    pub memory_regions: Option<Arc<Vec<MemoryRegion>>>,
}

/// A loaded binary, independent of its file format. Cheap to clone; a
/// stage that changes a binary clones it and gives it a new `uid`.
#[derive(Debug, Clone)]
pub struct Binary {
    /// Unique per loaded or modified binary. Caches key on it.
    pub uid: u64,
    pub name: String,
    pub path: PathBuf,
    pub data: Arc<Bytes>,
    pub format: FormatInfo,
    /// Named file ranges outside the sections, like the ELF header.
    pub headers: Arc<Vec<FileRegion>>,
    pub sections: Arc<Vec<Section>>,
    pub segments: Arc<Vec<Segment>>,
    pub symbols: Arc<Vec<Symbol>>,
    pub chunks: Arc<ChunkTable>,
    pub attachments: Attachments,
}

pub struct BinaryParts {
    pub name: String,
    pub path: PathBuf,
    pub data: Bytes,
    pub format: FormatInfo,
    pub headers: Vec<FileRegion>,
    pub sections: Vec<Section>,
    pub segments: Vec<Segment>,
    pub symbols: Vec<Symbol>,
}

/// Up to two places for one range: in the file, and at runtime.
pub type Places = SmallVec<[(AddrSpace, Span); 2]>;

impl Binary {
    /// Build a binary: fills in section load addresses and the chunk table.
    pub fn from_parts(parts: BinaryParts) -> Self {
        let BinaryParts {
            name,
            path,
            data,
            format,
            headers,
            mut sections,
            segments,
            symbols,
        } = parts;
        for sec in sections.iter_mut().filter(|s| s.alloc && !s.nobits) {
            sec.lma = segments
                .iter()
                .filter(|seg| seg.kind == SegmentKind::Load)
                .find(|seg| seg.vma_span().contains(sec.vma))
                .map(|seg| seg.paddr + (sec.vma - seg.vaddr))
                .or_else(|| segments.is_empty().then_some(sec.vma));
        }
        let chunks = ChunkTable::build(data.len() as u64, &segments, &sections);
        Binary {
            uid: Self::new_uid(),
            name,
            path,
            data: Arc::new(data),
            format,
            headers: Arc::new(headers),
            sections: Arc::new(sections),
            segments: Arc::new(segments),
            symbols: Arc::new(symbols),
            chunks: Arc::new(chunks),
            attachments: Attachments::default(),
        }
    }

    /// A modified copy, with a new uid.
    pub fn derive(&self, f: impl FnOnce(&mut Binary)) -> Binary {
        let mut b = self.clone();
        b.uid = Self::new_uid();
        f(&mut b);
        b
    }

    /// The memory regions: set by a stage, or from the linker map.
    pub fn memory_regions(&self) -> &[MemoryRegion] {
        if let Some(r) = &self.attachments.memory_regions {
            r
        } else if let Some(m) = &self.attachments.linkmap {
            &m.regions
        } else {
            &[]
        }
    }

    pub fn new_uid() -> u64 {
        NEXT_UID.fetch_add(1, Ordering::Relaxed)
    }

    /// Where `len` bytes at `offset` into a section are: in the file if
    /// the section has content there, and at runtime if it is allocated
    /// (which covers NOBITS sections like `.bss`).
    pub fn places_in_section(&self, sec: &Section, offset: u64, len: u64) -> Places {
        let mut out = Places::new();
        if len == 0 || offset >= sec.size.max(1) {
            return out;
        }
        if let Some(f) = sec.file {
            out.push((AddrSpace::File, Span::at(f.start + offset, len)));
        }
        if sec.alloc && !sec.tls {
            out.push((AddrSpace::Vma, Span::at(sec.vma + offset, len)));
        }
        out
    }

    pub fn section_places(&self, sec: &Section) -> Places {
        self.places_in_section(sec, 0, sec.size)
    }

    /// Where a symbol's bytes are. Placed relative to its section, which
    /// also works for relocatable objects where every section is at 0.
    pub fn symbol_places(&self, sym: &Symbol) -> Places {
        let Some(sec) = sym.section.map(|i| &self.sections[i]) else {
            return Places::new();
        };
        if !sec.alloc || sym.size == 0 {
            return Places::new();
        }
        match sym.addr.checked_sub(sec.vma) {
            Some(off) => self.places_in_section(sec, off, sym.size),
            None => Places::new(),
        }
    }

    /// A section by name.
    pub fn section(&self, name: &str) -> Option<&Section> {
        self.sections.iter().find(|s| s.name == name)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn section(
        index: usize,
        name: &str,
        kind: SectionKind,
        file: Option<Span>,
        vma: u64,
        size: u64,
    ) -> Section {
        Section {
            index,
            name: name.into(),
            kind,
            alloc: !matches!(
                kind,
                SectionKind::Debug | SectionKind::SymbolTable | SectionKind::Other
            ),
            write: matches!(kind, SectionKind::Data | SectionKind::Bss),
            exec: kind == SectionKind::Code,
            nobits: kind == SectionKind::Bss,
            tls: false,
            file,
            vma,
            lma: None,
            size,
            raw_type: 0,
            raw_flags: 0,
        }
    }

    pub fn load(index: usize, file: Span, vaddr: u64, paddr: u64, memsz: u64) -> Segment {
        Segment {
            index,
            kind: SegmentKind::Load,
            read: true,
            write: false,
            exec: false,
            file,
            vaddr,
            paddr,
            memsz,
        }
    }

    /// A Cortex-M-like binary: 0x100 bytes of file.
    ///
    /// - `0x00..0x10` header, not loaded
    /// - `.text` at file 0x10..0x50, VMA 0x0800_0000
    /// - `.data` at file 0x50..0x60, VMA 0x2000_0000, LMA 0x0800_0040
    /// - `.bss` 0x20 bytes at VMA 0x2000_0010
    /// - `.debug` at file 0x60..0x100
    pub fn cortex_m() -> Binary {
        let sections = vec![
            section(
                1,
                ".text",
                SectionKind::Code,
                Some(Span::new(0x10, 0x50)),
                0x0800_0000,
                0x40,
            ),
            section(
                2,
                ".data",
                SectionKind::Data,
                Some(Span::new(0x50, 0x60)),
                0x2000_0000,
                0x10,
            ),
            section(3, ".bss", SectionKind::Bss, None, 0x2000_0010, 0x20),
            section(
                4,
                ".debug",
                SectionKind::Debug,
                Some(Span::new(0x60, 0x100)),
                0,
                0xa0,
            ),
        ];
        let segments = vec![
            load(0, Span::new(0x10, 0x50), 0x0800_0000, 0x0800_0000, 0x40),
            load(1, Span::new(0x50, 0x60), 0x2000_0000, 0x0800_0040, 0x30),
        ];
        let symbols = vec![
            Symbol {
                name: "main".into(),
                addr: 0x0800_0000,
                size: 0x20,
                kind: SymbolKind::Function,
                binding: SymbolBinding::Global,
                section: Some(0),
            },
            Symbol {
                name: "buf".into(),
                addr: 0x2000_0010,
                size: 0x10,
                kind: SymbolKind::Object,
                binding: SymbolBinding::Local,
                section: Some(2),
            },
        ];
        Binary::from_parts(BinaryParts {
            name: "fw.elf".into(),
            path: "fw.elf".into(),
            data: Bytes::Owned(vec![0; 0x100]),
            format: FormatInfo::Unknown,
            headers: vec![FileRegion {
                name: "[header]".into(),
                span: Span::new(0, 0x10),
            }],
            sections,
            segments,
            symbols,
        })
    }

    #[test]
    fn chunk_table_cortex_m() {
        let b = cortex_m();
        let c = b.chunks.chunks();
        // header, text, data, debug, bss tail
        assert_eq!(c.len(), 5);
        let chunk = |u: (u64, u64), file, vma, lma| Chunk {
            unified: Span::new(u.0, u.1),
            file,
            vma,
            lma,
            free: false,
        };
        assert_eq!(c[0], chunk((0, 0x10), Some(0), None, None));
        let text = chunk(
            (0x10, 0x50),
            Some(0x10),
            Some(0x0800_0000),
            Some(0x0800_0000),
        );
        assert_eq!(c[1], text);
        let data = chunk(
            (0x50, 0x60),
            Some(0x50),
            Some(0x2000_0000),
            Some(0x0800_0040),
        );
        assert_eq!(c[2], data);
        assert_eq!(c[3], chunk((0x60, 0x100), Some(0x60), None, None));
        assert_eq!(c[4], chunk((0x100, 0x120), None, Some(0x2000_0010), None));
        assert_eq!(b.sections[1].lma, Some(0x0800_0040));
        // .data is stored in flash; .bss only exists at runtime.
        assert_eq!(c[2].addr(), Some(0x0800_0040));
        assert_eq!(c[4].addr(), Some(0x2000_0010));
        assert_eq!(
            c[2].sizes(4),
            Sizes {
                file: 4,
                memory: 4,
                load: 4
            }
        );
        assert_eq!(
            c[4].sizes(4),
            Sizes {
                file: 0,
                memory: 4,
                load: 0
            }
        );
    }

    #[test]
    fn translate_spaces() {
        let b = cortex_m();
        // .data + .bss by VMA.
        let u = b
            .chunks
            .to_unified(AddrSpace::Vma, Span::new(0x2000_0000, 0x2000_0030));
        assert_eq!(u, vec![Span::new(0x50, 0x60), Span::new(0x100, 0x120)]);
        // Flash by LMA: .text and .data's initial values.
        let u = b
            .chunks
            .to_unified(AddrSpace::Lma, Span::new(0x0800_0000, 0x0800_0050));
        assert_eq!(u, vec![Span::new(0x10, 0x50), Span::new(0x50, 0x60)]);
        let u = b.chunks.to_unified(AddrSpace::File, Span::new(0x58, 0x70));
        assert_eq!(u, vec![Span::new(0x58, 0x60), Span::new(0x60, 0x70)]);
    }
}
