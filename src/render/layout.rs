// SPDX-License-Identifier: GPL-2.0-only
//! Treemap layouts.

use crate::engine::{AnalysisTree, NodeId};
use crate::model::Metric;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Rect { x, y, w, h }
    }

    pub fn area(&self) -> f64 {
        self.w * self.h
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
}

/// One node's rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tile {
    pub node: NodeId,
    pub rect: Rect,
    /// Depth below the layout's root (0 for the root).
    pub depth: u32,
    /// Index just past this tile's subtree; tiles are in pre-order.
    pub next: u32,
    /// Cushion surface coefficients: `z = a·x² + b·x + c·y² + d·y`, stored
    /// as `[b, a, d, c]` (linear and quadratic terms, per axis).
    pub surface: [f64; 4],
}

impl Tile {
    pub fn has_children(&self, index: usize) -> bool {
        self.next as usize > index + 1
    }
}

/// A laid-out treemap: tiles in pre-order, parents before children.
#[derive(Clone, Debug, Default)]
pub struct Treemap {
    pub bounds: Rect,
    pub tiles: Vec<Tile>,
    pub metric: Metric,
}

impl Treemap {
    /// The deepest tile at a point.
    pub fn tile_at(&self, x: f64, y: f64) -> Option<&Tile> {
        let first = self.tiles.first()?;
        if !first.rect.contains(x, y) {
            return None;
        }
        let mut cur = 0;
        'down: loop {
            let end = self.tiles[cur].next as usize;
            let mut child = cur + 1;
            while child < end {
                if self.tiles[child].rect.contains(x, y) {
                    cur = child;
                    continue 'down;
                }
                child = self.tiles[child].next as usize;
            }
            return Some(&self.tiles[cur]);
        }
    }

    /// The tile of a node, if it was big enough to get one.
    pub fn tile_of(&self, node: NodeId) -> Option<&Tile> {
        self.tiles.iter().find(|t| t.node == node)
    }

    /// The tile of a node, or of its nearest ancestor that has one.
    pub fn visible_tile_of(&self, tree: &AnalysisTree, node: NodeId) -> Option<&Tile> {
        tree.path(node).iter().rev().find_map(|&n| self.tile_of(n))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LayoutOptions {
    /// Tiles smaller than this (in square units, normally pixels) are not
    /// made; their parent's tile covers them.
    pub min_area: f64,
    /// Lay out at most this many levels below the root.
    pub max_depth: Option<u32>,
    /// Cushion height of the top level, and the factor per level below.
    pub cushion_height: f64,
    pub cushion_scale: f64,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        LayoutOptions {
            min_area: 1.0,
            max_depth: None,
            cushion_height: 0.38,
            cushion_scale: 0.91,
        }
    }
}

pub trait TreemapLayout: Send + Sync {
    /// Split `rect` among `sizes` (largest first, all > 0), in order.
    fn split(&self, rect: Rect, sizes: &[f64], depth: u32) -> Vec<Rect>;

    /// Lay out the subtree of `root` in `bounds`.
    fn layout(
        &self,
        tree: &AnalysisTree,
        root: NodeId,
        metric: Metric,
        bounds: Rect,
        opts: &LayoutOptions,
    ) -> Treemap {
        let mut map = Treemap {
            bounds,
            tiles: Vec::new(),
            metric,
        };
        if tree.node(root).sizes.get(metric) > 0 && bounds.area() > 0.0 {
            add(
                self,
                tree,
                root,
                metric,
                bounds,
                0,
                [0.0; 4],
                opts,
                &mut map.tiles,
            );
        }
        map
    }
}

/// Add a ridge for `[x1, x2)` of height `h` to surface coefficients.
/// `coef` is `[linear, quadratic]`.
fn ridge(coef: &mut [f64], x1: f64, x2: f64, h: f64) {
    let w = x2 - x1;
    if w > 0.0 {
        coef[0] += 4.0 * h * (x2 + x1) / w;
        coef[1] -= 4.0 * h / w;
    }
}

#[allow(clippy::too_many_arguments)]
fn add<L: TreemapLayout + ?Sized>(
    layout: &L,
    tree: &AnalysisTree,
    node: NodeId,
    metric: Metric,
    rect: Rect,
    depth: u32,
    parent_surface: [f64; 4],
    opts: &LayoutOptions,
    tiles: &mut Vec<Tile>,
) {
    let mut surface = parent_surface;
    let h = opts.cushion_height * opts.cushion_scale.powi(depth as i32);
    ridge(&mut surface[0..2], rect.x, rect.right(), h);
    ridge(&mut surface[2..4], rect.y, rect.bottom(), h);
    let index = tiles.len();
    tiles.push(Tile {
        node,
        rect,
        depth,
        next: 0,
        surface,
    });
    if opts.max_depth.is_none_or(|d| depth < d) {
        let mut kids: Vec<(NodeId, u64)> = tree
            .children(node)
            .iter()
            .map(|&c| (c, tree.node(c).sizes.get(metric)))
            .filter(|&(_, s)| s > 0)
            .collect();
        kids.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        // Children may not add up to the parent (the root of a filtered
        // view does); scale to the parent's size so the parent shows the rest.
        let parent = tree
            .node(node)
            .sizes
            .get(metric)
            .max(kids.iter().map(|k| k.1).sum());
        if !kids.is_empty() && parent > 0 {
            let scale = rect.area() / parent as f64;
            let areas: Vec<f64> = kids.iter().map(|k| k.1 as f64 * scale).collect();
            let used: f64 = areas.iter().sum();
            // Lay the children out in the part of the rect they fill.
            let sub = if used < rect.area() * 0.999_999 {
                let frac = used / rect.area();
                if rect.w >= rect.h {
                    Rect::new(rect.x, rect.y, rect.w * frac, rect.h)
                } else {
                    Rect::new(rect.x, rect.y, rect.w, rect.h * frac)
                }
            } else {
                rect
            };
            let rects = layout.split(sub, &areas, depth);
            for ((kid, _), r) in kids.iter().zip(rects) {
                if r.area() >= opts.min_area && r.w > 0.0 && r.h > 0.0 {
                    add(
                        layout,
                        tree,
                        *kid,
                        metric,
                        r,
                        depth + 1,
                        surface,
                        opts,
                        tiles,
                    );
                }
            }
        }
    }
    tiles[index].next = tiles.len() as u32;
}

/// Squarified treemap (Bruls, Huizing, van Wijk): rows of children with
/// aspect ratios as close to 1 as possible.
#[derive(Clone, Copy, Debug, Default)]
pub struct Squarified;

fn worst(row: &[f64], side: f64) -> f64 {
    let sum: f64 = row.iter().sum();
    let (min, max) = row
        .iter()
        .fold((f64::MAX, 0.0f64), |(lo, hi), &a| (lo.min(a), hi.max(a)));
    let s2 = sum * sum;
    let w2 = side * side;
    (w2 * max / s2).max(s2 / (w2 * min))
}

impl TreemapLayout for Squarified {
    fn split(&self, rect: Rect, areas: &[f64], _depth: u32) -> Vec<Rect> {
        let mut out = Vec::with_capacity(areas.len());
        let mut free = rect;
        let mut start = 0;
        while start < areas.len() {
            let side = free.w.min(free.h);
            let mut end = start + 1;
            while end < areas.len()
                && worst(&areas[start..=end], side) <= worst(&areas[start..end], side)
            {
                end += 1;
            }
            let row = &areas[start..end];
            let sum: f64 = row.iter().sum();
            let last = end == areas.len();
            if free.w >= free.h {
                // A column on the left.
                let width = if last {
                    free.w
                } else {
                    (sum / free.h).min(free.w)
                };
                let mut y = free.y;
                for (i, &a) in row.iter().enumerate() {
                    let h = if i + 1 == row.len() {
                        free.bottom() - y
                    } else {
                        a / width
                    };
                    out.push(Rect::new(free.x, y, width, h));
                    y += h;
                }
                free = Rect::new(free.x + width, free.y, free.w - width, free.h);
            } else {
                // A row at the top.
                let height = if last {
                    free.h
                } else {
                    (sum / free.w).min(free.h)
                };
                let mut x = free.x;
                for (i, &a) in row.iter().enumerate() {
                    let w = if i + 1 == row.len() {
                        free.right() - x
                    } else {
                        a / height
                    };
                    out.push(Rect::new(x, free.y, w, height));
                    x += w;
                }
                free = Rect::new(free.x, free.y + height, free.w, free.h - height);
            }
            start = end;
        }
        out
    }
}

/// Slice and dice: children side by side, alternating direction per level.
#[derive(Clone, Copy, Debug, Default)]
pub struct SliceAndDice;

impl TreemapLayout for SliceAndDice {
    fn split(&self, rect: Rect, areas: &[f64], depth: u32) -> Vec<Rect> {
        let total: f64 = areas.iter().sum();
        let mut out = Vec::with_capacity(areas.len());
        let mut pos = 0.0;
        for (i, &a) in areas.iter().enumerate() {
            let frac = a / total;
            let last = i + 1 == areas.len();
            if depth % 2 == 0 {
                let w = if last { rect.w - pos } else { rect.w * frac };
                out.push(Rect::new(rect.x + pos, rect.y, w, rect.h));
                pos += w;
            } else {
                let h = if last { rect.h - pos } else { rect.h * frac };
                out.push(Rect::new(rect.x, rect.y + pos, rect.w, h));
                pos += h;
            }
        }
        out
    }
}
