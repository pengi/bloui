// SPDX-License-Identifier: GPL-2.0-only
//! The built-in stages.

mod attach;
mod filter_label;
mod filter_space;
mod free_space;
mod open_file;
mod relabel;
mod split;

use super::StageFactory;
pub use attach::attach_linkmap;

pub fn all() -> Vec<Box<dyn StageFactory>> {
    vec![
        Box::new(open_file::OpenFile),
        Box::new(split::Split),
        Box::new(filter_label::FilterLabel),
        Box::new(filter_space::FilterSpace),
        Box::new(attach::LinkerMap),
        Box::new(attach::MemoryRegions),
        Box::new(free_space::FreeSpace),
        Box::new(relabel::Relabel),
    ]
}
