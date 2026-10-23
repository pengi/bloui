// SPDX-License-Identifier: GPL-2.0-only
//! ELF reader, using the `object` crate's raw ELF API.

use super::{Confidence, ReadOptions, Reader};
use crate::model::{
    Binary, BinaryParts, Bytes, FileRegion, FormatInfo, Section, SectionKind, Segment, SegmentKind,
    Span, Symbol, SymbolBinding, SymbolKind,
};
use crate::{Error, Result};
use object::Endianness;
use object::read::elf::{ElfFile, FileHeader, ProgramHeader, SectionHeader, Sym};
use std::path::Path;

const PT_LOAD: u32 = 1;
const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

const SHT_SYMTAB: u32 = 2;
const SHT_STRTAB: u32 = 3;
const SHT_RELA: u32 = 4;
const SHT_NOTE: u32 = 7;
const SHT_NOBITS: u32 = 8;
const SHT_REL: u32 = 9;
const SHT_DYNSYM: u32 = 11;
const SHT_RELR: u32 = 19;
const SHT_ARM_EXIDX: u32 = 0x7000_0001;
const SHT_X86_64_UNWIND: u32 = 0x7000_0001;

const SHF_WRITE: u64 = 1;
const SHF_ALLOC: u64 = 2;
const SHF_EXECINSTR: u64 = 4;
const SHF_TLS: u64 = 0x400;

const ET_REL: u16 = 1;
const EM_X86_64: u16 = 62;

const STT_OBJECT: u8 = 1;
const STT_FUNC: u8 = 2;
const STT_SECTION: u8 = 3;
const STT_FILE: u8 = 4;
const STT_TLS: u8 = 6;
const STB_LOCAL: u8 = 0;
const STB_GLOBAL: u8 = 1;
const STB_WEAK: u8 = 2;

pub struct ElfReader;

impl Reader for ElfReader {
    fn id(&self) -> &'static str {
        "elf"
    }

    fn name(&self) -> &'static str {
        "ELF"
    }

    fn probe(&self, header: &[u8]) -> Confidence {
        if header.starts_with(b"\x7fELF") {
            Confidence::Yes
        } else {
            Confidence::No
        }
    }

    fn read(&self, data: Bytes, path: &Path, name: String, opts: ReadOptions) -> Result<Binary> {
        let fail = |msg: String| Error::Format {
            path: path.to_owned(),
            msg,
        };
        let parts = match data.get(4) {
            Some(1) => parse::<object::elf::FileHeader32<Endianness>>(&data),
            Some(2) => parse::<object::elf::FileHeader64<Endianness>>(&data),
            _ => return Err(fail("not a 32- or 64-bit ELF file".into())),
        }
        .map_err(|e| fail(e.to_string()))?;
        Ok(Binary::from_parts(BinaryParts {
            name,
            path: path.to_owned(),
            data,
            format: parts.format,
            headers: parts.headers,
            sections: parts.sections,
            segments: parts.segments,
            symbols: parts.symbols,
            split_copies: opts.split_copies,
        }))
    }
}

struct Parsed {
    format: FormatInfo,
    headers: Vec<FileRegion>,
    sections: Vec<Section>,
    segments: Vec<Segment>,
    symbols: Vec<Symbol>,
}

fn parse<Elf: FileHeader<Endian = Endianness>>(data: &[u8]) -> object::Result<Parsed> {
    let file = ElfFile::<Elf>::parse(data)?;
    let e = file.endian();
    let hdr = file.elf_header();
    let machine = hdr.e_machine(e).0;
    let format = FormatInfo::Elf {
        machine,
        class64: hdr.is_class_64(),
        little_endian: hdr.is_little_endian(),
        relocatable: hdr.e_type(e).0 == ET_REL,
    };

    let mut headers = vec![FileRegion {
        name: "[ELF header]".into(),
        span: Span::at(0, hdr.e_ehsize(e).into()),
    }];
    let phoff: u64 = hdr.e_phoff(e).into();
    let phlen = u64::from(hdr.e_phnum(e)) * u64::from(hdr.e_phentsize(e));
    if phoff != 0 && phlen != 0 {
        headers.push(FileRegion {
            name: "[program headers]".into(),
            span: Span::at(phoff, phlen),
        });
    }
    let shoff: u64 = hdr.e_shoff(e).into();
    let table = file.elf_section_table();
    let shlen = table.len() as u64 * u64::from(hdr.e_shentsize(e));
    if shoff != 0 && shlen != 0 {
        headers.push(FileRegion {
            name: "[section headers]".into(),
            span: Span::at(shoff, shlen),
        });
    }

    let segments = file
        .elf_program_headers()
        .iter()
        .enumerate()
        .map(|(index, ph)| {
            let ty = ph.p_type(e).0;
            let flags = ph.p_flags(e).0;
            Segment {
                index,
                kind: if ty == PT_LOAD {
                    SegmentKind::Load
                } else {
                    SegmentKind::Other(ty)
                },
                read: flags & PF_R != 0,
                write: flags & PF_W != 0,
                exec: flags & PF_X != 0,
                file: Span::at(ph.p_offset(e).into(), ph.p_filesz(e).into()),
                vaddr: ph.p_vaddr(e).into(),
                paddr: ph.p_paddr(e).into(),
                memsz: ph.p_memsz(e).into(),
            }
        })
        .collect();

    let file_len = data.len() as u64;
    let mut sections = Vec::new();
    for (index, sh) in table.iter().enumerate().skip(1) {
        let name = table.section_name(e, sh).unwrap_or(b"");
        let name = String::from_utf8_lossy(name).into_owned();
        let ty = sh.sh_type(e).0;
        let flags: u64 = sh.sh_flags(e).0;
        let size: u64 = sh.sh_size(e).into();
        let offset: u64 = sh.sh_offset(e).into();
        let nobits = ty == SHT_NOBITS;
        let file = (!nobits && size > 0 && offset.saturating_add(size) <= file_len)
            .then(|| Span::at(offset, size));
        sections.push(Section {
            index,
            kind: section_kind(&name, ty, flags, machine),
            name,
            alloc: flags & SHF_ALLOC != 0,
            write: flags & SHF_WRITE != 0,
            exec: flags & SHF_EXECINSTR != 0,
            nobits,
            tls: flags & SHF_TLS != 0,
            file,
            vma: sh.sh_addr(e).into(),
            lma: None,
            size,
            raw_type: ty,
            raw_flags: flags,
        });
    }

    let is_arm = machine == FormatInfo::EM_ARM;
    let symtab = file.elf_symbol_table();
    let strings = symtab.strings();
    let mut symbols = Vec::new();
    for sym in symtab.iter().skip(1) {
        let name = String::from_utf8_lossy(sym.name(e, strings).unwrap_or(b"")).into_owned();
        if is_arm && is_arm_mapping_symbol(&name) {
            continue;
        }
        let st_type = sym.st_type().0;
        let mut addr: u64 = sym.st_value(e).into();
        if is_arm && st_type == STT_FUNC {
            addr &= !1;
        }
        // Section indices in the symbol table count the null section;
        // ours don't.
        let section = sym
            .st_shndx(e)
            .index()
            .and_then(|i| usize::from(i).checked_sub(1))
            .filter(|&i| i < sections.len());
        symbols.push(Symbol {
            name,
            addr,
            size: sym.st_size(e).into(),
            kind: match st_type {
                STT_FUNC => SymbolKind::Function,
                STT_OBJECT => SymbolKind::Object,
                STT_SECTION => SymbolKind::Section,
                STT_FILE => SymbolKind::File,
                STT_TLS => SymbolKind::Tls,
                _ => SymbolKind::Other,
            },
            binding: match sym.st_bind().0 {
                STB_GLOBAL => SymbolBinding::Global,
                STB_WEAK => SymbolBinding::Weak,
                STB_LOCAL => SymbolBinding::Local,
                _ => SymbolBinding::Other,
            },
            section,
        });
    }

    Ok(Parsed {
        format,
        headers,
        sections,
        segments,
        symbols,
    })
}

/// ARM ELF mapping symbols: `$a`, `$t`, `$d`, optionally `.suffix`.
fn is_arm_mapping_symbol(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() >= 2
        && b[0] == b'$'
        && matches!(b[1], b'a' | b't' | b'd')
        && (b.len() == 2 || b[2] == b'.')
}

fn section_kind(name: &str, ty: u32, flags: u64, machine: u16) -> SectionKind {
    let alloc = flags & SHF_ALLOC != 0;
    let unwind_type = (machine == FormatInfo::EM_ARM && ty == SHT_ARM_EXIDX)
        || (machine == EM_X86_64 && ty == SHT_X86_64_UNWIND);
    if unwind_type
        || name.starts_with(".ARM.extab")
        || name.starts_with(".ARM.exidx")
        || name.starts_with(".eh_frame")
        || name == ".gcc_except_table"
    {
        return SectionKind::Unwind;
    }
    if [".debug", ".zdebug", ".stab", ".line"]
        .iter()
        .any(|p| name.starts_with(p))
    {
        return SectionKind::Debug;
    }
    match ty {
        SHT_NOBITS if alloc => SectionKind::Bss,
        SHT_SYMTAB | SHT_DYNSYM => SectionKind::SymbolTable,
        SHT_STRTAB => SectionKind::StringTable,
        SHT_REL | SHT_RELA | SHT_RELR => SectionKind::Relocation,
        SHT_NOTE => SectionKind::Note,
        _ if alloc && flags & SHF_EXECINSTR != 0 => SectionKind::Code,
        _ if alloc && flags & SHF_WRITE != 0 => SectionKind::Data,
        _ if alloc => SectionKind::ReadOnlyData,
        _ => SectionKind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapping_symbols() {
        for s in ["$t", "$d", "$a", "$t.12", "$d.foo"] {
            assert!(is_arm_mapping_symbol(s), "{s}");
        }
        for s in ["$x", "$", "$tt", "main", "$d1"] {
            assert!(!is_arm_mapping_symbol(s), "{s}");
        }
    }
}
