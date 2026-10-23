// SPDX-License-Identifier: GPL-2.0-only
use super::simple_create;
use crate::Result;
use crate::dimension::{Dimension, DimensionFactory, Labeling};
use crate::model::{AddrSpace, Binary, Interner, Label, Overlap, Span};

pub struct FileDim;

impl DimensionFactory for FileDim {
    fn id(&self) -> &'static str {
        "file"
    }
    fn name(&self) -> &'static str {
        "file"
    }
    fn description(&self) -> &'static str {
        "The input file"
    }
    simple_create!();
}

impl Dimension for FileDim {
    fn label(&self, bin: &Binary, interner: &mut Interner) -> Result<Labeling> {
        let label = Label::single(interner.intern(&bin.name));
        let mut l = Labeling::new(Overlap::FirstWins, label.clone());
        l.push(AddrSpace::Unified, Span::new(0, bin.chunks.total()), label);
        Ok(l)
    }
}
