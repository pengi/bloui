// SPDX-License-Identifier: GPL-2.0-only
use crate::Result;
use crate::dimension::{Dimension, DimensionFactory, Labeling};
use crate::model::{Binary, Interner, Label, Overlap, Symbol, SymbolKind};
use crate::param::{ParamSpec, ParamValues};
use crate::registry::Registry;
use crate::symname;

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
        let mut params = demangle_params();
        params.push(fallback_param());
        params
    }
    fn create(&self, params: &ParamValues, _registry: &Registry) -> Result<Box<dyn Dimension>> {
        Ok(Box::new(Symbols {
            names: NameStyle::from_params(params),
            section_fallback: section_fallback(params),
        }))
    }
}

struct Symbols {
    names: NameStyle,
    section_fallback: bool,
}

impl Dimension for Symbols {
    fn label(&self, bin: &Binary, interner: &mut Interner) -> Result<Labeling> {
        Ok(label_symbols(
            bin,
            interner,
            self.section_fallback,
            |sym, interner| Some(Label::single(interner.intern(&self.names.name(sym)))),
        ))
    }
}

/// How symbol dimensions show names: the `demangle` and `hash` parameters.
pub(crate) struct NameStyle {
    demangle: bool,
    keep_hash: bool,
}

impl NameStyle {
    pub(crate) fn from_params(params: &ParamValues) -> Self {
        NameStyle {
            demangle: params.text("demangle") == "auto",
            keep_hash: params.text("hash") == "keep",
        }
    }

    pub(crate) fn name<'a>(&self, sym: &'a Symbol) -> std::borrow::Cow<'a, str> {
        self.show(&sym.name)
    }

    /// A raw (maybe mangled) name as shown.
    pub(crate) fn show<'a>(&self, raw: &'a str) -> std::borrow::Cow<'a, str> {
        if self.demangle {
            symname::demangle(raw, self.keep_hash)
        } else {
            std::borrow::Cow::Borrowed(raw)
        }
    }
}

pub(crate) fn demangle_params() -> Vec<ParamSpec> {
    vec![
        ParamSpec::choice(
            "demangle",
            "Demangle",
            &[("auto", "Rust and C++ names"), ("off", "Show raw names")],
        ),
        ParamSpec::choice(
            "hash",
            "Rust hashes",
            &[("strip", "Drop them"), ("keep", "Keep them")],
        )
        .visible_when("demangle", "auto"),
    ]
}

pub(crate) fn fallback_param() -> ParamSpec {
    ParamSpec::choice(
        "fallback",
        "Bytes without a symbol",
        &[
            ("section", "Label by section, like [.text]"),
            ("none", "Label as [no symbol]"),
        ],
    )
}

pub(crate) fn section_fallback(params: &ParamValues) -> bool {
    params.text("fallback") == "section"
}

/// Labels every symbol's bytes by `label_of` (symbols it returns `None` for
/// are skipped), the smallest symbol winning. Bytes without a symbol are
/// `[<section>]` with `section_fallback`, otherwise `[no symbol]`.
pub(crate) fn label_symbols(
    bin: &Binary,
    interner: &mut Interner,
    section_fallback: bool,
    mut label_of: impl FnMut(&Symbol, &mut Interner) -> Option<Label>,
) -> Labeling {
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
        let Some(label) = label_of(sym, interner) else {
            continue;
        };
        for (space, span) in bin.symbol_places(sym) {
            l.push(space, span, label.clone());
        }
    }
    if section_fallback {
        super::push_section_fallback(&mut l, bin, interner);
    }
    l
}
