// SPDX-License-Identifier: GPL-2.0-only
//! Readers turn a file into a format-neutral [`Binary`].
//!
//! Format-specific code lives only in the submodules here.

pub mod elf;

use crate::model::{Binary, Bytes};
use crate::{Error, Result};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Confidence {
    No,
    Maybe,
    Yes,
}

pub trait Reader: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    /// Look at the first bytes of a file.
    fn probe(&self, header: &[u8]) -> Confidence;
    fn read(&self, data: Bytes, path: &Path, name: String) -> Result<Binary>;
}

/// Map a file into memory.
pub fn load_bytes(path: &Path) -> Result<Bytes> {
    let io = |source| Error::Io {
        path: path.to_owned(),
        source,
    };
    let file = std::fs::File::open(path).map_err(io)?;
    if file.metadata().map_err(io)?.len() == 0 {
        return Ok(Bytes::Owned(Vec::new()));
    }
    // SAFETY: the map is read-only. If another process truncates the file
    // while it is mapped, reads may fault; that is the usual mmap caveat.
    let map = unsafe { memmap2::Mmap::map(&file) }.map_err(io)?;
    Ok(Bytes::Mmap(map))
}

/// Read a file with the given reader, or the one that probes best.
pub fn read_file(
    readers: &[Box<dyn Reader>],
    path: &Path,
    reader: Option<&str>,
    name: String,
) -> Result<Binary> {
    let data = load_bytes(path)?;
    let reader = match reader {
        Some(id) => readers
            .iter()
            .find(|r| r.id() == id)
            .ok_or_else(|| Error::Unknown {
                kind: "reader",
                id: id.into(),
            })?,
        None => readers
            .iter()
            .map(|r| (r.probe(&data), r))
            .filter(|(c, _)| *c > Confidence::No)
            .max_by_key(|(c, _)| *c)
            .map(|(_, r)| r)
            .ok_or_else(|| Error::Format {
                path: path.to_owned(),
                msg: "unknown file format".into(),
            })?,
    };
    reader.read(data, path, name)
}
