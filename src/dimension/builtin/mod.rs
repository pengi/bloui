// SPDX-License-Identifier: GPL-2.0-only
//! The built-in dimensions.

mod category;
mod dwarf;
mod file;
mod object_file;
mod region;
mod rules;
mod section;
mod segment;
mod symbol;
mod symbol_path;
mod symbol_props;

use super::DimensionFactory;

pub fn all() -> Vec<Box<dyn DimensionFactory>> {
    vec![
        Box::new(file::FileDim),
        Box::new(segment::SegmentDim),
        Box::new(section::SectionDim),
        Box::new(category::CategoryDim),
        Box::new(symbol::SymbolDim),
        Box::new(symbol_path::SymbolPathDim),
        Box::new(symbol_props::SymbolKindDim),
        Box::new(symbol_props::SymbolBindingDim),
        Box::new(region::RegionDim),
        Box::new(object_file::ObjectFileDim),
        Box::new(object_file::InputSectionDim),
        Box::new(dwarf::CompileUnitDim),
        Box::new(dwarf::SourceFileDim),
        Box::new(rules::RulesDim),
    ]
}

/// A dimension without parameters is its own factory product.
macro_rules! simple_create {
    () => {
        fn create(
            &self,
            _params: &crate::param::ParamValues,
            _registry: &crate::registry::Registry,
        ) -> crate::Result<Box<dyn crate::dimension::Dimension>> {
            Ok(Box::new(Self))
        }
    };
}
pub(crate) use simple_create;

/// Label every section as `[<name>]` (and headers by their name) in tier 1,
/// as the fallback of a finer dimension.
pub(crate) fn push_section_fallback(
    l: &mut super::Labeling,
    bin: &crate::model::Binary,
    interner: &mut crate::model::Interner,
) {
    use crate::model::{AddrSpace, Label};
    for r in bin.headers.iter() {
        l.push_tier(
            1,
            AddrSpace::File,
            r.span,
            Label::single(interner.intern(&r.name)),
        );
    }
    for sec in bin.sections.iter() {
        let label = Label::single(interner.intern(&format!("[{}]", sec.name)));
        for (space, span) in bin.section_places(sec) {
            l.push_tier(1, space, span, label.clone());
        }
    }
}
