// SPDX-License-Identifier: GPL-2.0-only
//! GUI-neutral view state: selection, hover, zoom, navigation, and the
//! treemap view with its caches. Frontends forward input here and paint
//! what comes back.

mod state;
mod treemap;

pub use state::ViewState;
pub use treemap::{LayoutKind, TreemapView};
