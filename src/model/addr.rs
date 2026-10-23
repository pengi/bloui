// SPDX-License-Identifier: GPL-2.0-only
//! Address spaces and address ranges.

use serde::{Deserialize, Serialize};
use std::fmt;

/// The address spaces a byte can live in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AddrSpace {
    /// Offset in the file.
    File,
    /// Runtime address.
    Vma,
    /// Load address (ELF `p_paddr`).
    Lma,
    /// The engine's own linear space over all chunks of a binary.
    Unified,
}

impl AddrSpace {
    pub fn name(self) -> &'static str {
        match self {
            AddrSpace::File => "file",
            AddrSpace::Vma => "vma",
            AddrSpace::Lma => "lma",
            AddrSpace::Unified => "unified",
        }
    }
}

/// A half-open range `[start, end)`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Span {
    pub start: u64,
    pub end: u64,
}

impl Span {
    pub fn new(start: u64, end: u64) -> Self {
        debug_assert!(start <= end, "span {start:#x}..{end:#x}");
        Span { start, end }
    }

    /// A span from a start and a length. Saturates at the end of the
    /// address space.
    pub fn at(start: u64, len: u64) -> Self {
        Span {
            start,
            end: start.saturating_add(len),
        }
    }

    pub fn len(self) -> u64 {
        self.end - self.start
    }

    pub fn is_empty(self) -> bool {
        self.start >= self.end
    }

    pub fn contains(self, addr: u64) -> bool {
        self.start <= addr && addr < self.end
    }

    pub fn overlaps(self, other: Span) -> bool {
        self.start < other.end && other.start < self.end
    }

    pub fn intersect(self, other: Span) -> Option<Span> {
        let start = self.start.max(other.start);
        let end = self.end.min(other.end);
        (start < end).then_some(Span { start, end })
    }
}

impl fmt::Debug for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:#x}..{:#x}", self.start, self.end)
    }
}
