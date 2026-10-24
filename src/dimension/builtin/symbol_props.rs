// SPDX-License-Identifier: GPL-2.0-only
//! Dimensions by symbol properties: kind and binding.

use super::symbol::{fallback_param, label_symbols, section_fallback};
use crate::Result;
use crate::dimension::{Dimension, DimensionFactory, Labeling};
use crate::model::{Binary, Interner, Label, Symbol, SymbolBinding, SymbolKind};
use crate::param::{ParamSpec, ParamValues};

pub struct SymbolKindDim;

impl DimensionFactory for SymbolKindDim {
    fn id(&self) -> &'static str {
        "symbol-kind"
    }
    fn name(&self) -> &'static str {
        "symbol kind"
    }
    fn description(&self) -> &'static str {
        "Kind of the symbol: function, object, tls or other"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![fallback_param()]
    }
    fn create(&self, params: &ParamValues) -> Result<Box<dyn Dimension>> {
        Ok(Box::new(ByProperty {
            section_fallback: section_fallback(params),
            property: |s| match s.kind {
                SymbolKind::Function => "function",
                SymbolKind::Object => "object",
                SymbolKind::Tls => "tls",
                _ => "other",
            },
        }))
    }
}

pub struct SymbolBindingDim;

impl DimensionFactory for SymbolBindingDim {
    fn id(&self) -> &'static str {
        "symbol-binding"
    }
    fn name(&self) -> &'static str {
        "symbol binding"
    }
    fn description(&self) -> &'static str {
        "Binding of the symbol: global, weak or local"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![fallback_param()]
    }
    fn create(&self, params: &ParamValues) -> Result<Box<dyn Dimension>> {
        Ok(Box::new(ByProperty {
            section_fallback: section_fallback(params),
            property: |s| match s.binding {
                SymbolBinding::Global => "global",
                SymbolBinding::Weak => "weak",
                SymbolBinding::Local => "local",
                SymbolBinding::Other => "other",
            },
        }))
    }
}

struct ByProperty {
    section_fallback: bool,
    property: fn(&Symbol) -> &'static str,
}

impl Dimension for ByProperty {
    fn label(&self, bin: &Binary, interner: &mut Interner) -> Result<Labeling> {
        Ok(label_symbols(
            bin,
            interner,
            self.section_fallback,
            |sym, interner| Some(Label::single(interner.intern((self.property)(sym)))),
        ))
    }
}
