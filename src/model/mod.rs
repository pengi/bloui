// SPDX-License-Identifier: GPL-2.0-only
//! Core types. Nothing here knows about a specific file format.

pub mod addr;
pub mod binary;
pub mod fragment;
pub mod intern;
pub mod linkmap;
pub mod rangemap;
pub mod size;

pub use addr::{AddrSpace, Span};
pub use binary::{
    Attachments, Binary, BinaryId, BinaryParts, Bytes, Chunk, ChunkTable, FileRegion, FormatInfo,
    Section, SectionKind, Segment, SegmentKind, Symbol, SymbolBinding, SymbolKind,
};
pub use fragment::{Fragment, FragmentSet, LevelInfo};
pub use intern::{Interner, Label, Sym};
pub use linkmap::{InputSection, LinkMap, MemoryRegion, ObjectFile, OutputSection};
pub use rangemap::{Overlap, RangeMap, RangeMapBuilder};
pub use size::{Metric, Sizes};

/// Whether a stage or dimension can run on the current input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Availability {
    Available,
    /// Can't run, with the reason ("needs a linker map", …).
    Unavailable(String),
}

impl Availability {
    pub fn is_available(&self) -> bool {
        matches!(self, Availability::Available)
    }
}
