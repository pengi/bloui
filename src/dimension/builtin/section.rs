// SPDX-License-Identifier: GPL-2.0-only
use super::simple_create;
use crate::Result;
use crate::dimension::{Dimension, DimensionFactory, Labeling};
use crate::model::{AddrSpace, Binary, Interner, Label, Overlap};

pub struct SectionDim;

impl DimensionFactory for SectionDim {
    fn id(&self) -> &'static str {
        "section"
    }
    fn name(&self) -> &'static str {
        "section"
    }
    fn description(&self) -> &'static str {
        "Output section, like .text; headers are shown as [ELF header] etc."
    }
    simple_create!();
}

impl Dimension for SectionDim {
    fn label(&self, bin: &Binary, interner: &mut Interner) -> Result<Labeling> {
        let mut l = Labeling::new(
            Overlap::FirstWins,
            Label::single(interner.intern("[no section]")),
        );
        for r in bin.headers.iter() {
            l.push(
                AddrSpace::File,
                r.span,
                Label::single(interner.intern(&r.name)),
            );
        }
        for sec in bin.sections.iter() {
            let label = Label::single(interner.intern(&sec.name));
            for (space, span) in bin.section_places(sec) {
                l.push(space, span, label.clone());
            }
        }
        Ok(l)
    }
}
