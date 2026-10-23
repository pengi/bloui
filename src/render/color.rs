// SPDX-License-Identifier: GPL-2.0-only
//! Tile colors.

use crate::engine::{AnalysisTree, NodeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }
}

/// How to paint a tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Paint {
    pub color: Rgb,
    /// Cushion-shaded; flat if not (used for free space).
    pub shaded: bool,
}

pub trait ColorScheme {
    fn paint(&self, tree: &AnalysisTree, node: NodeId) -> Paint;
}

/// A palette in the spirit of WinDirStat: saturated, distinct hues that
/// stay readable after shading.
pub const PALETTE: [Rgb; 12] = [
    Rgb(0x3b, 0x6f, 0xe0),
    Rgb(0xd9, 0x48, 0x3b),
    Rgb(0x3f, 0xa8, 0x4c),
    Rgb(0xe0, 0xb0, 0x2a),
    Rgb(0x9b, 0x52, 0xd1),
    Rgb(0x2b, 0xae, 0xb8),
    Rgb(0xe0, 0x7a, 0x2c),
    Rgb(0xd1, 0x4f, 0x9b),
    Rgb(0x7d, 0xa8, 0x2f),
    Rgb(0x5a, 0x7d, 0xa8),
    Rgb(0xb8, 0x87, 0x5a),
    Rgb(0x8c, 0x8c, 0xd8),
];

/// Color of free space.
pub const FREE: Rgb = Rgb(0xd8, 0xd8, 0xd8);
/// Color of bracketed placeholder labels like `[no symbol]`.
pub const PLACEHOLDER: Rgb = Rgb(0x9a, 0x9a, 0x9a);

/// FNV-1a: stable across runs and platforms, so a label keeps its color.
pub fn stable_hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

pub fn label_color(name: &str) -> Rgb {
    PALETTE[(stable_hash(name) % PALETTE.len() as u64) as usize]
}

/// Color each tile by the label of its ancestor at one tree level (or its
/// own label above that level). `[free]` is flat light grey; other
/// bracketed labels are grey.
#[derive(Clone, Copy, Debug, Default)]
pub struct LevelColors {
    /// Tree level (0 = the first split).
    pub level: u32,
}

impl ColorScheme for LevelColors {
    fn paint(&self, tree: &AnalysisTree, node: NodeId) -> Paint {
        let path = tree.path(node);
        if path.iter().any(|&n| &*tree.node(n).name == "[free]") {
            return Paint {
                color: FREE,
                shaded: false,
            };
        }
        // path[0] is the root; level k is at depth k + 1 (for single-part
        // labels; path labels make several depths share a level).
        let at_level = path
            .iter()
            .skip(1)
            .find(|&&n| tree.node(n).level.is_some_and(|l| l >= self.level))
            .copied();
        let Some(n) = at_level else {
            return Paint {
                color: PLACEHOLDER,
                shaded: true,
            };
        };
        let name = &tree.node(n).name;
        let color = if name.starts_with('[') {
            PLACEHOLDER
        } else {
            label_color(name)
        };
        Paint {
            color,
            shaded: true,
        }
    }
}
