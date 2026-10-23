// SPDX-License-Identifier: GPL-2.0-only
//! Cushion shading (van Wijk and van de Wetering), as in WinDirStat.

use super::{ColorScheme, Image, Treemap};
use crate::engine::AnalysisTree;

#[derive(Clone, Copy, Debug)]
pub struct CushionOptions {
    pub ambient: f64,
    /// Light direction; normalized when rendering.
    pub light: [f64; 3],
    /// Multiplies the shaded colors. Above 1, so cushion tops are lighter
    /// than the palette color, like WinDirStat.
    pub brightness: f64,
}

impl Default for CushionOptions {
    fn default() -> Self {
        CushionOptions {
            ambient: 0.13,
            light: [-1.0, -1.0, 10.0],
            brightness: 1.45,
        }
    }
}

/// Render a treemap. Pixels outside the layout's tiles are transparent.
pub fn render(
    map: &Treemap,
    tree: &AnalysisTree,
    colors: &dyn ColorScheme,
    width: u32,
    height: u32,
    opts: &CushionOptions,
) -> Image {
    let mut img = Image::new(width, height);
    let [lx, ly, lz] = opts.light;
    let len = (lx * lx + ly * ly + lz * lz).sqrt();
    let (lx, ly, lz) = (lx / len, ly / len, lz / len);
    let diffuse = 1.0 - opts.ambient;
    // Parents first, children paint over them; a parent only shows where
    // its small children were folded into it.
    for tile in &map.tiles {
        let paint = colors.paint(tree, tile.node);
        let r = tile.rect;
        let x0 = r.x.round().max(0.0) as u32;
        let y0 = r.y.round().max(0.0) as u32;
        let x1 = (r.right().round().max(0.0) as u32).min(width);
        let y1 = (r.bottom().round().max(0.0) as u32).min(height);
        let [b, a, d, c] = tile.surface;
        let base = [
            paint.color.0 as f64,
            paint.color.1 as f64,
            paint.color.2 as f64,
        ];
        for y in y0..y1 {
            let fy = y as f64 + 0.5;
            let ny = -(2.0 * c * fy + d);
            for x in x0..x1 {
                let f = if paint.shaded {
                    let fx = x as f64 + 0.5;
                    let nx = -(2.0 * a * fx + b);
                    let cos = (nx * lx + ny * ly + lz) / (nx * nx + ny * ny + 1.0).sqrt();
                    (opts.ambient + diffuse * cos.max(0.0)) * opts.brightness
                } else if x == x0 || y == y0 || x + 1 == x1 || y + 1 == y1 {
                    // Flat tiles get a faint edge, so neighbours stay apart.
                    0.85
                } else {
                    1.0
                };
                let px = |v: f64| (v * f).round().clamp(0.0, 255.0) as u8;
                img.set(x, y, [px(base[0]), px(base[1]), px(base[2]), 255]);
            }
        }
    }
    img
}
