// SPDX-License-Identifier: GPL-2.0-only
//! Fragments: the pieces of bytes that flow through the pipeline.

use super::{BinaryId, Chunk, Label, Sizes, Span};
use smallvec::SmallVec;

/// A run of bytes of one binary, within one chunk, with one label per tree
/// level so far.
#[derive(Clone, Debug)]
pub struct Fragment {
    pub bin: BinaryId,
    /// Index into the binary's chunk table.
    pub chunk: u32,
    /// Range in unified space.
    pub span: Span,
    /// Sizes, from the chunk's properties.
    pub sizes: Sizes,
    pub labels: SmallVec<[Label; 4]>,
}

impl Fragment {
    pub fn new(bin: BinaryId, chunk_index: usize, chunk: &Chunk, span: Span) -> Self {
        Fragment {
            bin,
            chunk: chunk_index as u32,
            span,
            sizes: chunk.sizes(span.len()),
            labels: SmallVec::new(),
        }
    }

    /// A piece of this fragment, with the same labels. Sizes are scaled
    /// to the piece (every byte of a fragment has the same properties).
    pub fn piece(&self, span: Span) -> Fragment {
        debug_assert!(span.start >= self.span.start && span.end <= self.span.end);
        let len = span.len();
        let scale = |v: u64| if v == 0 { 0 } else { len };
        Fragment {
            bin: self.bin,
            chunk: self.chunk,
            span,
            sizes: Sizes {
                file: scale(self.sizes.file),
                memory: scale(self.sizes.memory),
                load: scale(self.sizes.load),
            },
            labels: self.labels.clone(),
        }
    }
}

/// Describes one tree level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelInfo {
    /// Display name, e.g. "section".
    pub name: String,
    /// The dimension that made it.
    pub dimension: String,
}

/// The set of fragments passed from stage to stage.
#[derive(Clone, Debug, Default)]
pub struct FragmentSet {
    pub fragments: Vec<Fragment>,
    pub levels: Vec<LevelInfo>,
}

impl FragmentSet {
    pub fn total(&self) -> Sizes {
        self.fragments
            .iter()
            .fold(Sizes::default(), |acc, f| acc + f.sizes)
    }
}
