// SPDX-License-Identifier: GPL-2.0-only
//! The built-in dimensions.

mod category;
mod file;
mod section;
mod segment;
mod symbol;

use super::DimensionFactory;

pub fn all() -> Vec<Box<dyn DimensionFactory>> {
    vec![
        Box::new(file::FileDim),
        Box::new(segment::SegmentDim),
        Box::new(section::SectionDim),
        Box::new(category::CategoryDim),
        Box::new(symbol::SymbolDim),
    ]
}

/// A dimension without parameters is its own factory product.
macro_rules! simple_create {
    () => {
        fn create(
            &self,
            _params: &crate::param::ParamValues,
        ) -> crate::Result<Box<dyn crate::dimension::Dimension>> {
            Ok(Box::new(Self))
        }
    };
}
pub(crate) use simple_create;
