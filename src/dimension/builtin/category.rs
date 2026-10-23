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
        for r in &bin.regions {
            l.push(AddrSpace::File, r.span, headers.clone());
        }
        for sec in &bin.sections {
            if let Some((space, span)) = bin.section_place(sec) {
                let name = category_name(sec.kind);
                l.push(space, span, Label::single(interner.intern(name)));
            }
        }
        Ok(l)
    }
}
