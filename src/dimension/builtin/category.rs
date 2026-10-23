// SPDX-License-Identifier: GPL-2.0-only
use super::simple_create;
use crate::Result;
use crate::dimension::{Dimension, DimensionFactory, Labeling};
use crate::model::{AddrSpace, Binary, Interner, Label, Overlap, SectionKind};

pub struct CategoryDim;

impl DimensionFactory for CategoryDim {
    fn id(&self) -> &'static str {
        "category"
    }
    fn name(&self) -> &'static str {
        "category"
    }
    fn description(&self) -> &'static str {
        "Kind of content: code, rodata, data, bss, unwind, debug, symtab, reloc, meta, headers"
    }
    simple_create!();
}

pub fn category_name(kind: SectionKind) -> &'static str {
    match kind {
        SectionKind::Code => "code",
        SectionKind::ReadOnlyData => "rodata",
        SectionKind::Data => "data",
        SectionKind::Bss => "bss",
        SectionKind::Unwind => "unwind",
        SectionKind::Debug => "debug",
        SectionKind::SymbolTable | SectionKind::StringTable => "symtab",
        SectionKind::Relocation => "reloc",
        SectionKind::Note | SectionKind::Other => "meta",
    }
}

impl Dimension for CategoryDim {
    fn label(&self, bin: &Binary, interner: &mut Interner) -> Result<Labeling> {
        let mut l = Labeling::new(
            Overlap::FirstWins,
            Label::single(interner.intern("[unmapped]")),
        );
        let headers = Label::single(interner.intern("headers"));
        for r in bin.headers.iter() {
            l.push(AddrSpace::File, r.span, headers.clone());
        }
        for sec in bin.sections.iter() {
            let label = Label::single(interner.intern(category_name(sec.kind)));
            for (space, span) in bin.section_places(sec) {
                l.push(space, span, label.clone());
            }
        }
        Ok(l)
    }
}
