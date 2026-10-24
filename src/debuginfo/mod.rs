// SPDX-License-Identifier: GPL-2.0-only
//! Reading DWARF debug information into the format-neutral
//! [`DebugInfo`], with gimli.

use crate::model::debuginfo::{
    CompUnit, DebugInfo, DebugSource, Function, LineRange, Scope, SourceFile, Variable,
};
use crate::model::{Binary, Span};
use crate::{Error, Result};
use gimli::{AttributeValue, EndianSlice, Operation, RunTimeEndian, UnitOffset};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

type R<'a> = EndianSlice<'a, RunTimeEndian>;
type UnitRef<'u, 'd> = gimli::UnitRef<'u, R<'d>>;
type Entry<'u, 'd> = gimli::DebuggingInformationEntry<'u, 'u, R<'d>>;

/// The binary's debug information, parsed on first use.
pub fn get(bin: &Binary) -> Result<Arc<DebugInfo>> {
    let src = bin
        .attachments
        .debug
        .as_ref()
        .ok_or_else(|| Error::Other("no DWARF debug info".into()))?;
    src.parsed
        .get_or_init(|| load(src).map(Arc::new).map_err(|e| e.to_string()))
        .clone()
        .map_err(Error::Other)
}

/// The binary's own debug sections, if it has DWARF.
pub fn own_source(bin: &Binary) -> Option<DebugSource> {
    let sections: Vec<(String, Span)> = bin
        .sections
        .iter()
        .filter(|s| s.name.starts_with(".debug_"))
        .filter_map(|s| Some((s.name.clone(), s.file?)))
        .collect();
    if !sections.iter().any(|(n, _)| n == ".debug_info") {
        return None;
    }
    Some(source(bin, bin, sections))
}

/// A debug source over `file`'s data, for the code of `bin`.
pub(crate) fn source(bin: &Binary, file: &Binary, sections: Vec<(String, Span)>) -> DebugSource {
    let little_endian = match file.format {
        crate::model::FormatInfo::Elf { little_endian, .. } => little_endian,
        crate::model::FormatInfo::Unknown => true,
    };
    DebugSource {
        path: file.path.clone(),
        data: file.data.clone(),
        little_endian,
        sections,
        valid: bin
            .sections
            .iter()
            .filter(|s| s.alloc && s.size > 0)
            .map(|s| Span::at(s.vma, s.size))
            .collect(),
        parsed: OnceLock::new(),
    }
}

pub fn load(src: &DebugSource) -> Result<DebugInfo> {
    if src.sections.iter().any(|(n, _)| n.starts_with(".zdebug")) {
        return Err(Error::Other(
            "compressed debug sections are not supported".into(),
        ));
    }
    let endian = if src.little_endian {
        RunTimeEndian::Little
    } else {
        RunTimeEndian::Big
    };
    let dwarf = gimli::Dwarf::load(|id| -> std::result::Result<R<'_>, gimli::Error> {
        Ok(EndianSlice::new(src.section(id.name()), endian))
    })
    .map_err(dwarf_err)?;
    let mut b = Builder {
        src,
        info: DebugInfo::default(),
        files: HashMap::new(),
        functions: HashMap::new(),
    };
    let mut units = dwarf.units();
    while let Some(header) = units.next().map_err(dwarf_err)? {
        let unit = dwarf.unit(header).map_err(dwarf_err)?;
        b.unit(unit.unit_ref(&dwarf)).map_err(dwarf_err)?;
    }
    b.info.lines.sort_by_key(|l| l.span.start);
    Ok(b.info)
}

fn dwarf_err(e: gimli::Error) -> Error {
    Error::Other(format!("DWARF: {e}"))
}

struct Builder<'s> {
    src: &'s DebugSource,
    info: DebugInfo,
    files: HashMap<SourceFile, u32>,
    functions: HashMap<Function, u32>,
}

impl Builder<'_> {
    /// A range that belongs to the binary: not a dropped function's
    /// (address 0 or a tombstone) and inside an allocated section.
    fn valid(&self, span: Span) -> bool {
        span.start != 0
            && span.start < u64::MAX - 1
            && !span.is_empty()
            && self.src.valid.iter().any(|v| v.contains(span.start))
    }

    fn file(&mut self, file: SourceFile) -> u32 {
        let next = self.info.files.len() as u32;
        *self.files.entry(file.clone()).or_insert_with(|| {
            self.info.files.push(file);
            next
        })
    }

    fn function(&mut self, f: Function) -> u32 {
        let next = self.info.functions.len() as u32;
        *self.functions.entry(f.clone()).or_insert_with(|| {
            self.info.functions.push(f);
            next
        })
    }

    fn unit<'d>(&mut self, unit: UnitRef<'_, 'd>) -> gimli::Result<()> {
        let string = |v: Option<R<'_>>| v.map(|s| s.to_string_lossy().into_owned());
        let comp_dir = string(unit.comp_dir);
        let mut cu = CompUnit {
            name: string(unit.name).unwrap_or_default(),
            comp_dir: comp_dir.clone(),
            producer: None,
            language: None,
            ranges: Vec::new(),
        };
        let mut ranges = unit.unit_ranges()?;
        while let Some(r) = ranges.next()? {
            let span = Span::new(r.begin, r.end);
            if self.valid(span) {
                cu.ranges.push(span);
            }
        }
        let unit_index = self.info.units.len() as u32;

        // Files of the line program, by index; and the line rows.
        let mut file_ids: HashMap<u64, u32> = HashMap::new();
        if let Some(program) = unit.line_program.clone() {
            let header = program.header().clone();
            let mut rows = program.rows();
            let mut prev: Option<(u64, u64, u32)> = None;
            while let Some((_, row)) = rows.next_row()? {
                let addr = row.address();
                if let Some((start, file_index, line)) = prev
                    && addr > start
                {
                    let span = Span::new(start, addr);
                    if self.valid(span) {
                        let file = match file_ids.get(&file_index) {
                            Some(&f) => f,
                            None => {
                                let f = self.line_file(unit, &header, file_index, &comp_dir)?;
                                file_ids.insert(file_index, f);
                                f
                            }
                        };
                        self.push_line(LineRange {
                            span,
                            file,
                            line,
                            unit: unit_index,
                        });
                    }
                }
                prev = (!row.end_sequence()).then(|| {
                    let line = row.line().map_or(0, |l| l.get() as u32);
                    (addr, row.file_index(), line)
                });
            }
            // Files only variables refer to are looked up as they come.
            self.walk_entries(
                unit,
                unit_index,
                Some(&header),
                &mut file_ids,
                &comp_dir,
                &mut cu,
            )?;
        } else {
            self.walk_entries(unit, unit_index, None, &mut file_ids, &comp_dir, &mut cu)?;
        }
        self.info.units.push(cu);
        Ok(())
    }

    fn push_line(&mut self, l: LineRange) {
        if let Some(last) = self.info.lines.last_mut()
            && last.span.end == l.span.start
            && (last.file, last.line, last.unit) == (l.file, l.line, l.unit)
        {
            last.span.end = l.span.end;
            return;
        }
        self.info.lines.push(l);
    }

    fn line_file<'d>(
        &mut self,
        unit: UnitRef<'_, 'd>,
        header: &gimli::LineProgramHeader<R<'d>>,
        index: u64,
        comp_dir: &Option<String>,
    ) -> gimli::Result<u32> {
        let Some(entry) = header.file(index) else {
            return Ok(self.file(SourceFile {
                path: "[unknown file]".into(),
                comp_dir: None,
            }));
        };
        let name = unit
            .attr_string(entry.path_name())?
            .to_string_lossy()
            .into_owned();
        let path = if name.starts_with('/') {
            name
        } else {
            match entry.directory(header) {
                Some(dir) => {
                    let dir = unit.attr_string(dir)?.to_string_lossy().into_owned();
                    join(&dir, &name)
                }
                None => name,
            }
        };
        // A path under the compilation directory is stored relative to it.
        let path = match comp_dir {
            Some(c) => path
                .strip_prefix(c.as_str())
                .and_then(|p| p.strip_prefix('/'))
                .map_or(path.clone(), str::to_owned),
            None => path,
        };
        Ok(self.file(SourceFile {
            path,
            comp_dir: comp_dir.clone(),
        }))
    }

    /// Functions, inlined code and variables.
    fn walk_entries<'d>(
        &mut self,
        unit: UnitRef<'_, 'd>,
        unit_index: u32,
        header: Option<&gimli::LineProgramHeader<R<'d>>>,
        file_ids: &mut HashMap<u64, u32>,
        comp_dir: &Option<String>,
        cu: &mut CompUnit,
    ) -> gimli::Result<()> {
        let mut entries = unit.entries();
        let mut depth = 0isize;
        // Depths of the enclosing entries that are scopes with code.
        let mut scopes: Vec<isize> = Vec::new();
        while let Some((delta, entry)) = entries.next_dfs()? {
            depth += delta;
            while scopes.last().is_some_and(|&d| d >= depth) {
                scopes.pop();
            }
            match entry.tag() {
                gimli::DW_TAG_compile_unit => {
                    cu.producer = attr_text(unit, entry, gimli::DW_AT_producer)?;
                    if let Some(AttributeValue::Language(l)) =
                        entry.attr_value(gimli::DW_AT_language)?
                    {
                        cu.language = l
                            .static_string()
                            .map(|s| s.trim_start_matches("DW_LANG_").to_owned());
                    }
                }
                gimli::DW_TAG_subprogram | gimli::DW_TAG_inlined_subroutine => {
                    let mut spans = Vec::new();
                    let mut ranges = unit.die_ranges(entry)?;
                    while let Some(r) = ranges.next()? {
                        let span = Span::new(r.begin, r.end);
                        if self.valid(span) {
                            spans.push(span);
                        }
                    }
                    if spans.is_empty() {
                        continue;
                    }
                    let f = names(unit, entry.offset(), 0)?;
                    let function = self.function(Function {
                        name: f.0.unwrap_or_else(|| "[unnamed]".into()),
                        linkage_name: f.1,
                    });
                    let scope_depth = scopes.len() as u32;
                    for span in spans {
                        self.info.scopes.push(Scope {
                            span,
                            depth: scope_depth,
                            function,
                        });
                    }
                    scopes.push(depth);
                }
                gimli::DW_TAG_variable => {
                    let Some(addr) = fixed_address(unit, entry)? else {
                        continue;
                    };
                    if !self.valid(Span::at(addr, 1)) {
                        continue;
                    }
                    let (name, linkage_name) = names(unit, entry.offset(), 0)?;
                    let size = match follow(unit, entry.offset(), gimli::DW_AT_type, 0)? {
                        Some(t) => type_size(unit, t, 0)?,
                        None => None,
                    };
                    let file = match (decl_file(unit, entry.offset(), 0)?, header) {
                        (Some(i), Some(h)) => Some(match file_ids.get(&i) {
                            Some(&f) => f,
                            None => {
                                let f = self.line_file(unit, h, i, comp_dir)?;
                                file_ids.insert(i, f);
                                f
                            }
                        }),
                        _ => None,
                    };
                    self.info.variables.push(Variable {
                        name: name.unwrap_or_else(|| "[unnamed]".into()),
                        linkage_name,
                        addr,
                        size,
                        unit: unit_index,
                        file,
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }
}

fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() || dir == "." {
        name.to_owned()
    } else {
        format!("{}/{name}", dir.trim_end_matches('/'))
    }
}

fn attr_text<'d>(
    unit: UnitRef<'_, 'd>,
    entry: &Entry<'_, 'd>,
    at: gimli::DwAt,
) -> gimli::Result<Option<String>> {
    match entry.attr_value(at)? {
        Some(v) => Ok(unit
            .attr_string(v)
            .ok()
            .map(|s| s.to_string_lossy().into_owned())),
        None => Ok(None),
    }
}

/// The entry an attribute refers to, in the same unit.
fn reference(entry: &Entry<'_, '_>, at: gimli::DwAt) -> gimli::Result<Option<UnitOffset>> {
    Ok(match entry.attr_value(at)? {
        Some(AttributeValue::UnitRef(o)) => Some(o),
        _ => None,
    })
}

/// Name and linkage name, following `DW_AT_abstract_origin` and
/// `DW_AT_specification` (an out-of-line copy or a definition refers to
/// the declaration that has them).
fn names(
    unit: UnitRef<'_, '_>,
    offset: UnitOffset,
    hops: u32,
) -> gimli::Result<(Option<String>, Option<String>)> {
    let entry = unit.entry(offset)?;
    let mut name = attr_text(unit, &entry, gimli::DW_AT_name)?;
    let mut linkage = attr_text(unit, &entry, gimli::DW_AT_linkage_name)?;
    if linkage.is_none() {
        linkage = attr_text(unit, &entry, gimli::DW_AT_MIPS_linkage_name)?;
    }
    if (name.is_none() || linkage.is_none()) && hops < 4 {
        for at in [gimli::DW_AT_abstract_origin, gimli::DW_AT_specification] {
            if let Some(o) = reference(&entry, at)? {
                let (n, l) = names(unit, o, hops + 1)?;
                name = name.or(n);
                linkage = linkage.or(l);
            }
        }
    }
    Ok((name, linkage))
}

/// A reference attribute of an entry, or of the declaration it refers to.
fn follow(
    unit: UnitRef<'_, '_>,
    offset: UnitOffset,
    at: gimli::DwAt,
    hops: u32,
) -> gimli::Result<Option<UnitOffset>> {
    let entry = unit.entry(offset)?;
    if let Some(o) = reference(&entry, at)? {
        return Ok(Some(o));
    }
    if hops < 4 {
        for origin in [gimli::DW_AT_abstract_origin, gimli::DW_AT_specification] {
            if let Some(o) = reference(&entry, origin)? {
                return follow(unit, o, at, hops + 1);
            }
        }
    }
    Ok(None)
}

fn decl_file(unit: UnitRef<'_, '_>, offset: UnitOffset, hops: u32) -> gimli::Result<Option<u64>> {
    let entry = unit.entry(offset)?;
    if let Some(v) = entry.attr_value(gimli::DW_AT_decl_file)? {
        return Ok(match v {
            AttributeValue::FileIndex(i) => Some(i),
            v => v.udata_value(),
        });
    }
    if hops < 4 {
        for at in [gimli::DW_AT_abstract_origin, gimli::DW_AT_specification] {
            if let Some(o) = reference(&entry, at)? {
                return decl_file(unit, o, hops + 1);
            }
        }
    }
    Ok(None)
}

/// The address of a variable whose location is just `DW_OP_addr`.
fn fixed_address<'d>(unit: UnitRef<'_, 'd>, entry: &Entry<'_, 'd>) -> gimli::Result<Option<u64>> {
    let Some(expr) = entry
        .attr_value(gimli::DW_AT_location)?
        .and_then(|v| v.exprloc_value())
    else {
        return Ok(None);
    };
    let mut ops = expr.operations(unit.encoding());
    let addr = match ops.next()? {
        Some(Operation::Address { address }) => address,
        Some(Operation::AddressIndex { index }) => unit.address(index)?,
        _ => return Ok(None),
    };
    Ok(ops.next()?.is_none().then_some(addr))
}

/// The size of a type in bytes, through typedefs, qualifiers and arrays.
fn type_size(unit: UnitRef<'_, '_>, offset: UnitOffset, hops: u32) -> gimli::Result<Option<u64>> {
    if hops > 16 {
        return Ok(None);
    }
    let entry = unit.entry(offset)?;
    if let Some(size) = entry
        .attr_value(gimli::DW_AT_byte_size)?
        .and_then(|v| v.udata_value())
    {
        return Ok(Some(size));
    }
    let inner = reference(&entry, gimli::DW_AT_type)?;
    match entry.tag() {
        gimli::DW_TAG_typedef
        | gimli::DW_TAG_const_type
        | gimli::DW_TAG_volatile_type
        | gimli::DW_TAG_restrict_type
        | gimli::DW_TAG_atomic_type => match inner {
            Some(t) => type_size(unit, t, hops + 1),
            None => Ok(None),
        },
        gimli::DW_TAG_array_type => {
            let Some(elem) = inner.map(|t| type_size(unit, t, hops + 1)).transpose()? else {
                return Ok(None);
            };
            let Some(elem) = elem else {
                return Ok(None);
            };
            let mut count = 1u64;
            let mut tree = unit.entries_tree(Some(offset))?;
            let root = tree.root()?;
            let mut children = root.children();
            while let Some(child) = children.next()? {
                let e = child.entry();
                if e.tag() != gimli::DW_TAG_subrange_type {
                    continue;
                }
                let n = match e
                    .attr_value(gimli::DW_AT_count)?
                    .and_then(|v| v.udata_value())
                {
                    Some(n) => n,
                    None => match e
                        .attr_value(gimli::DW_AT_upper_bound)?
                        .and_then(|v| v.udata_value())
                    {
                        Some(u) => u + 1,
                        None => return Ok(None),
                    },
                };
                count = count.saturating_mul(n);
            }
            Ok(Some(elem.saturating_mul(count)))
        }
        _ => Ok(None),
    }
}
