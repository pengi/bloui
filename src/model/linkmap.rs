// SPDX-License-Identifier: GPL-2.0-only
//! What a linker map file says about a binary. The parsers are in the
//! `linkmap` module.

use std::path::PathBuf;

/// A memory region (from a linker script's `MEMORY`, or entered by hand).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryRegion {
    pub name: String,
    pub origin: u64,
    pub length: u64,
    /// Like `rx`; may be empty.
    pub attrs: String,
}

impl MemoryRegion {
    pub fn span(&self) -> super::Span {
        super::Span::at(self.origin, self.length)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputSection {
    pub name: String,
    pub addr: u64,
    pub size: u64,
    pub load_addr: Option<u64>,
}

/// An input section placed in an output section, or `*fill*` padding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputSection {
    /// Index into [`LinkMap::outputs`].
    pub output: usize,
    /// Like `.text.uart_init`; `*fill*` for padding.
    pub name: String,
    pub addr: u64,
    pub size: u64,
    /// Index into [`LinkMap::objects`]. `None` for fill.
    pub object: Option<usize>,
}

impl InputSection {
    pub fn is_fill(&self) -> bool {
        self.object.is_none()
    }
}

/// An object file, as the map names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectFile {
    /// As written: `build/libhal.a(gpio.o)`.
    pub full: String,
    /// `build/libhal.a`, for archive members.
    pub archive: Option<String>,
    /// `gpio.o`, or the object's path.
    pub member: String,
}

#[derive(Clone, Debug, Default)]
pub struct LinkMap {
    pub path: PathBuf,
    pub regions: Vec<MemoryRegion>,
    pub outputs: Vec<OutputSection>,
    pub inputs: Vec<InputSection>,
    /// Input sections removed by `--gc-sections` (`output` is unused).
    pub discarded: Vec<InputSection>,
    pub objects: Vec<ObjectFile>,
}

impl LinkMap {
    pub fn output(&self, name: &str) -> Option<&OutputSection> {
        self.outputs.iter().find(|o| o.name == name)
    }
}
