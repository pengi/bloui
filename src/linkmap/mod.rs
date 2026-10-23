// SPDX-License-Identifier: GPL-2.0-only
//! Linker map files: parsing, and checking that a map belongs to a binary.

mod gnu;

use crate::model::{Binary, LinkMap};
use crate::{Error, Result};
use std::path::{Path, PathBuf};

/// Parse a map file. Only the GNU ld format is supported so far.
pub fn parse(text: &str, path: &Path) -> Result<LinkMap> {
    let mut map = gnu::parse(text).map_err(|msg| Error::Format {
        path: path.to_owned(),
        msg,
    })?;
    map.path = path.to_owned();
    Ok(map)
}

pub fn load(path: &Path) -> Result<LinkMap> {
    let text = std::fs::read(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    parse(&String::from_utf8_lossy(&text), path)
}

/// Where a map file for `elf` would be: `fw.map` or `fw.elf.map` next to
/// `fw.elf`. The first that exists.
pub fn find_next_to(elf: &Path) -> Option<PathBuf> {
    let candidates = [elf.with_extension("map"), {
        let mut p = elf.as_os_str().to_owned();
        p.push(".map");
        PathBuf::from(p)
    }];
    candidates.into_iter().find(|p| p != elf && p.is_file())
}

/// Check that a map describes this binary: every allocated section that
/// the map also has must have the same address and size. Catches a map
/// left over from an older build.
pub fn check(map: &LinkMap, bin: &Binary) -> Result<()> {
    let mut matched = 0;
    for sec in bin.sections.iter().filter(|s| s.alloc && s.size > 0) {
        if let Some(out) = map.output(&sec.name) {
            if out.addr != sec.vma || out.size != sec.size {
                return Err(Error::Other(format!(
                    "{} doesn't match {}: {} is {:#x}+{:#x} in the map, {:#x}+{:#x} in the file",
                    map.path.display(),
                    bin.name,
                    sec.name,
                    out.addr,
                    out.size,
                    sec.vma,
                    sec.size
                )));
            }
            matched += 1;
        }
    }
    if matched == 0 {
        return Err(Error::Other(format!(
            "{} has none of the sections of {}",
            map.path.display(),
            bin.name
        )));
    }
    Ok(())
}
