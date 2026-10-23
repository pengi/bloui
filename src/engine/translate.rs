// SPDX-License-Identifier: GPL-2.0-only
//! Translating a dimension's labels into unified space.

use crate::dimension::Labeling;
use crate::model::{Binary, Label, RangeMap, RangeMapBuilder};

/// A dimension's labels for one binary, in unified space.
#[derive(Clone, Debug, Default)]
pub struct ResolvedLabels {
    pub map: RangeMap<Label>,
    pub fallback: Label,
}

impl ResolvedLabels {
    /// The label of a unified address.
    pub fn label_at(&self, unified: u64) -> &Label {
        self.map.get(unified).map_or(&self.fallback, |(_, l)| l)
    }
}

/// Translate each entry into the unified ranges it covers, then resolve
/// overlaps with the labeling's policy. The overlap weight is the length of
/// the original entry, so "smallest wins" compares symbols, not pieces.
pub fn resolve(bin: &Binary, labeling: Labeling) -> ResolvedLabels {
    let mut b = RangeMapBuilder::new();
    for e in labeling.entries {
        let weight = e.span.len();
        bin.chunks.for_each_unified(e.space, e.span, |_, u| {
            b.push_weighted(e.tier, weight, u, e.label.clone());
        });
    }
    ResolvedLabels {
        map: b.build(labeling.policy),
        fallback: labeling.fallback,
    }
}
