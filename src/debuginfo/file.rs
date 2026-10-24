// SPDX-License-Identifier: GPL-2.0-only
//! Separate debug files: finding them (build id, `.gnu_debuglink`, name)
//! and checking that they belong to a binary.

use crate::model::{Binary, DebugSource, Span};
use crate::{Error, Result};
use std::path::{Path, PathBuf};

/// Where the system keeps debug files.
pub const DEBUG_ROOT: &str = "/usr/lib/debug";

/// The bytes of a section with content.
fn section_data<'a>(bin: &'a Binary, name: &str) -> Option<&'a [u8]> {
    let span = bin.sections.iter().find(|s| s.name == name)?.file?;
    bin.data.get(span.start as usize..span.end as usize)
}

fn little_endian(bin: &Binary) -> bool {
    !matches!(
        bin.format,
        crate::model::FormatInfo::Elf {
            little_endian: false,
            ..
        }
    )
}

/// The GNU build id, from the `.note.gnu.build-id` note.
pub fn build_id(bin: &Binary) -> Option<Vec<u8>> {
    let note = section_data(bin, ".note.gnu.build-id")?;
    let word = |i: usize| -> Option<usize> {
        let b: [u8; 4] = note.get(i..i + 4)?.try_into().ok()?;
        Some(if little_endian(bin) {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        } as usize)
    };
    let (namesz, descsz) = (word(0)?, word(4)?);
    let desc = 12 + namesz.div_ceil(4) * 4;
    note.get(desc..desc + descsz).map(<[u8]>::to_vec)
}

/// The debug file name in `.gnu_debuglink`.
pub fn debuglink(bin: &Binary) -> Option<String> {
    let data = section_data(bin, ".gnu_debuglink")?;
    let end = data.iter().position(|&b| b == 0)?;
    Some(String::from_utf8_lossy(&data[..end]).into_owned())
}

/// Where a separate debug file for `bin` may be, in the order to try:
/// by build id under each root, by debug link next to the binary, in
/// `.debug/` and under each root, then `fw.debug` and `fw.elf.debug`.
pub fn candidates(bin: &Binary, roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(id) = build_id(bin).filter(|id| id.len() > 1) {
        let hex: String = id.iter().map(|b| format!("{b:02x}")).collect();
        for root in roots {
            out.push(
                root.join(".build-id")
                    .join(&hex[..2])
                    .join(format!("{}.debug", &hex[2..])),
            );
        }
    }
    let dir = bin.path.parent().unwrap_or(Path::new("."));
    if let Some(link) = debuglink(bin) {
        out.push(dir.join(&link));
        out.push(dir.join(".debug").join(&link));
        if let Ok(abs) = dir.canonicalize() {
            for root in roots {
                out.push(root.join(abs.strip_prefix("/").unwrap_or(&abs)).join(&link));
            }
        }
    }
    if let Some(stem) = bin.path.file_stem() {
        out.push(dir.join(format!("{}.debug", stem.to_string_lossy())));
    }
    if let Some(name) = bin.path.file_name() {
        out.push(dir.join(format!("{}.debug", name.to_string_lossy())));
    }
    out.dedup();
    out
}

/// Whether `debug` holds the debug info of `bin`: the same build id if
/// both have one, else the same address and size for each allocated
/// section.
pub fn check(bin: &Binary, debug: &Binary) -> Result<()> {
    if !debug.sections.iter().any(|s| s.name == ".debug_info") {
        return Err(Error::Other(format!(
            "{} has no DWARF debug info",
            debug.path.display()
        )));
    }
    if let (Some(a), Some(b)) = (build_id(bin), build_id(debug)) {
        return if a == b {
            Ok(())
        } else {
            Err(Error::Other(format!(
                "{} is for another build (build id differs)",
                debug.path.display()
            )))
        };
    }
    for sec in bin.sections.iter().filter(|s| s.alloc && s.size > 0) {
        let same = debug
            .sections
            .iter()
            .any(|d| d.name == sec.name && d.vma == sec.vma && d.size == sec.size);
        if !same {
            return Err(Error::Other(format!(
                "{} doesn't match: section {} differs",
                debug.path.display(),
                sec.name
            )));
        }
    }
    Ok(())
}

/// The debug sections of a separate debug file, for `bin`'s code.
pub fn separate_source(bin: &Binary, debug: &Binary) -> DebugSource {
    let sections: Vec<(String, Span)> = debug
        .sections
        .iter()
        .filter(|s| s.name.starts_with(".debug_") || s.name.starts_with(".zdebug_"))
        .filter_map(|s| Some((s.name.clone(), s.file?)))
        .collect();
    super::source(bin, debug, sections)
}
