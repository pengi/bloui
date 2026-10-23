// SPDX-License-Identifier: GPL-2.0-only
//! GUI-neutral rendering: treemap layout, hit testing, cushion shading,
//! colors, and scenes (display lists) that a frontend only has to paint.

pub mod color;
pub mod cushion;
pub mod image;
pub mod layout;
pub mod scene;

pub use color::{ColorBy, ColorScheme, LegendEntry, LevelColors, Paint, Rgb, legend};
pub use cushion::CushionOptions;
pub use image::Image;
pub use layout::{LayoutOptions, Rect, SliceAndDice, Squarified, Tile, Treemap, TreemapLayout};
pub use scene::{Prim, Scene};
