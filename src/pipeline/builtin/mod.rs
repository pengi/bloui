// SPDX-License-Identifier: GPL-2.0-only
//! The built-in stages.

mod filter_label;
mod filter_space;
mod open_file;
mod split;

use super::StageFactory;

pub fn all() -> Vec<Box<dyn StageFactory>> {
    vec![
        Box::new(open_file::OpenFile),
        Box::new(split::Split),
        Box::new(filter_label::FilterLabel),
        Box::new(filter_space::FilterSpace),
    ]
}
