// SPDX-License-Identifier: GPL-2.0-only
use super::simple_create;
use crate::Result;
use crate::dimension::{Dimension, DimensionFactory, Labeling};
use crate::model::{AddrSpace, Binary, Interner, Label, Overlap, SegmentKind};

pub struct SegmentDim;

impl DimensionFactory for SegmentDim {
    fn id(&self) -> &'static str {
        "segment"
    }
    fn name(&self) -> &'static str {
        "segment"
    }
    fn description(&self) -> &'static str {
        "Loadable segment (program header), like LOAD #1 [R-X]"
    }
    simple_create!();
}

impl Dimension for SegmentDim {
    fn label(&self, bin: &Binary, interner: &mut Interner) -> Result<Labeling> {
        let mut l = Labeling::new(
            Overlap::FirstWins,
            Label::single(interner.intern("[no segment]")),
        );
        for seg in bin.segments.iter().filter(|s| s.kind == SegmentKind::Load) {
            let name = format!("LOAD #{} [{}]", seg.index, seg.flags_text());
            let label = Label::single(interner.intern(&name));
            // The file part covers a load image; the runtime range covers
            // everything at runtime.
            l.push(AddrSpace::File, seg.file, label.clone());
            l.push(AddrSpace::Vma, seg.vma_span(), label);
        }
        Ok(l)
    }
}
