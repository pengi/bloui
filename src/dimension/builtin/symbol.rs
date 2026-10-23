// SPDX-License-Identifier: GPL-2.0-only
use crate::Result;
use crate::dimension::{Dimension, DimensionFactory, Labeling};
use crate::model::{Binary, Interner, Label, Overlap, SymbolKind};
use crate::param::{ParamSpec, ParamValues};

pub struct SymbolDim;

impl DimensionFactory for SymbolDim {
    fn id(&self) -> &'static str {
        "symbol"
    }
    fn name(&self) -> &'static str {
        "symbol"
    }
    fn description(&self) -> &'static str {
        "Symbol from the symbol table. The smallest symbol wins where symbols overlap"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![ParamSpec::choice(
            "fallback",
            "Bytes without a symbol",
            &[
                ("section", "Label by section, like [.text]"),
                ("none", "Label as [no symbol]"),
            ],
        )]
    }
    fn create(&self, params: &ParamValues) -> Result<Box<dyn Dimension>> {
        Ok(Box::new(Symbols {
            section_fallback: params.text("fallback") == "section",
        }))
    }
}

struct Symbols {
    section_fallback: bool,
}

impl Dimension for Symbols {
    fn label(&self, bin: &Binary, interner: &mut Interner) -> Result<Labeling> {
        let mut l = Labeling::new(
            Overlap::SmallestWins,
            Label::single(interner.intern("[no symbol]")),
        );
        // For aliases (same range), prefer global over weak over local, then
        // the name, so the result is stable.
        let mut syms: Vec<_> = bin
            .symbols
            .iter()
            .filter(|s| !matches!(s.kind, SymbolKind::Section | SymbolKind::File))
            .collect();
        syms.sort_by(|a, b| a.binding.cmp(&b.binding).then_with(|| a.name.cmp(&b.name)));
        for sym in syms {
            let label = Label::single(interner.intern(&sym.name));
            for (space, span) in bin.symbol_places(sym) {
                l.push(space, span, label.clone());
            }
        }
        if self.section_fallback {
            super::push_section_fallback(&mut l, bin, interner);
        }
        Ok(l)
    }
}
