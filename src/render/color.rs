// SPDX-License-Identifier: GPL-2.0-only
//! Tile colors.

use crate::engine::{AnalysisTree, ColorLabels, NodeId};
use crate::model::Metric;
use std::collections::HashMap;
use std::sync::Arc;

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

fn palette_slot(name: &str) -> usize {
    (stable_hash(name) % PALETTE.len() as u64) as usize
}

/// A label's own color, from its hash.
pub fn label_color(name: &str) -> Rgb {
    PALETTE[palette_slot(name)]
}

/// The colors of the labels in a tree. Biggest first, each label takes
/// its own (hashed) color if no bigger label has it, else the next free
/// one; so up to `PALETTE.len()` labels get distinct colors, and a label
/// keeps its color across pipeline changes unless a bigger one takes it.
#[derive(Clone, Debug, Default)]
pub struct ColorMap {
    colors: HashMap<Arc<str>, Rgb>,
}

impl ColorMap {
    /// The colors for `color_by` over the whole tree, so zooming doesn't
    /// change them.
    pub fn new(tree: &AnalysisTree, color_by: &ColorBy, metric: Metric) -> Self {
        let labels = collect(tree, color_by, AnalysisTree::ROOT, metric);
        let mut taken = [false; PALETTE.len()];
        let mut colors = HashMap::new();
        for (label, _, _) in labels.into_iter().filter(|(l, _, _)| !l.starts_with('[')) {
            let own = palette_slot(&label);
            let slot = (0..PALETTE.len())
                .map(|i| (own + i) % PALETTE.len())
                .find(|&s| !taken[s])
                .unwrap_or(own);
            taken[slot] = true;
            colors.insert(label, PALETTE[slot]);
        }
        ColorMap { colors }
    }

    /// How a label is painted: `[free]` flat light grey, other bracketed
    /// labels (and no label) grey, the rest in their color.
    pub fn paint(&self, label: Option<&str>) -> Paint {
        match label {
            Some("[free]") => Paint {
                color: FREE,
                shaded: false,
            },
            Some(name) if !name.starts_with('[') => Paint {
                color: self
                    .colors
                    .get(name)
                    .copied()
                    .unwrap_or_else(|| label_color(name)),
                shaded: true,
            },
            _ => Paint {
                color: PLACEHOLDER,
                shaded: true,
            },
        }
    }
}

/// The label of `node`'s ancestor at a tree level (or its own label at or
/// below it); `[free]` for anything in free space; `None` above the level.
pub fn level_label(tree: &AnalysisTree, node: NodeId, level: u32) -> Option<Arc<str>> {
    let path = tree.path(node);
    if let Some(&n) = path.iter().find(|&&n| &*tree.node(n).name == "[free]") {
        return Some(tree.node(n).name.clone());
    }
    // path[0] is the root; level k is at depth k + 1 (for single-part
    // labels; path labels make several depths share a level).
    path.iter()
        .skip(1)
        .find(|&&n| tree.node(n).level.is_some_and(|l| l >= level))
        .map(|&n| tree.node(n).name.clone())
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
        ColorMap::default().paint(level_label(tree, node, self.level).as_deref())
    }
}

/// What the tile colors stand for: the labels of a tree level, or those
/// of another dimension.
#[derive(Clone, Debug)]
pub enum ColorBy {
    Level(u32),
    Labels(Arc<ColorLabels>),
}

impl Default for ColorBy {
    fn default() -> Self {
        ColorBy::Level(0)
    }
}

impl PartialEq for ColorBy {
    fn eq(&self, o: &ColorBy) -> bool {
        match (self, o) {
            (ColorBy::Level(a), ColorBy::Level(b)) => a == b,
            (ColorBy::Labels(a), ColorBy::Labels(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl ColorBy {
    /// The label that gives `node` its color.
    pub fn label_of(&self, tree: &AnalysisTree, node: NodeId, metric: Metric) -> Option<Arc<str>> {
        match self {
            ColorBy::Level(l) => level_label(tree, node, *l),
            ColorBy::Labels(labels) => labels.dominant(node, metric).cloned(),
        }
    }

    pub fn scheme(&self, tree: &AnalysisTree, metric: Metric) -> ColorByScheme<'_> {
        ColorByScheme {
            color_by: self,
            metric,
            map: ColorMap::new(tree, self, metric),
        }
    }
}

pub struct ColorByScheme<'a> {
    color_by: &'a ColorBy,
    metric: Metric,
    map: ColorMap,
}

impl ColorScheme for ColorByScheme<'_> {
    fn paint(&self, tree: &AnalysisTree, node: NodeId) -> Paint {
        self.map
            .paint(self.color_by.label_of(tree, node, self.metric).as_deref())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegendEntry {
    pub label: Arc<str>,
    pub color: Rgb,
    pub size: u64,
    /// The biggest node with this label (the zoom root for "other").
    pub node: NodeId,
}

/// Label used for the entries past the legend's limit.
pub const OTHER: &str = "[other]";

/// The labels under `root`, with their colors and sizes, biggest first;
/// past `max` entries, the rest are summed as `[other]`.
pub fn legend(
    tree: &AnalysisTree,
    color_by: &ColorBy,
    root: NodeId,
    metric: Metric,
    max: usize,
) -> Vec<LegendEntry> {
    let map = ColorMap::new(tree, color_by, metric);
    let mut entries: Vec<LegendEntry> = collect(tree, color_by, root, metric)
        .into_iter()
        .map(|(label, size, node)| LegendEntry {
            color: map.paint(Some(&label)).color,
            label,
            size,
            node,
        })
        .collect();
    if entries.len() > max {
        let rest: u64 = entries[max..].iter().map(|e| e.size).sum();
        entries.truncate(max);
        entries.push(LegendEntry {
            label: OTHER.into(),
            color: PLACEHOLDER,
            size: rest,
            node: root,
        });
    }
    entries
}

/// The labels under `root` with their sizes and biggest nodes, biggest
/// first.
fn collect(
    tree: &AnalysisTree,
    color_by: &ColorBy,
    root: NodeId,
    metric: Metric,
) -> Vec<(Arc<str>, u64, NodeId)> {
    // The topmost nodes with each label: their sizes (for levels) and
    // the biggest of them. With another dimension, a node's label only
    // says which label wins there, so the zoom root itself isn't one.
    let by_level = matches!(color_by, ColorBy::Level(_));
    let mut found: HashMap<Arc<str>, (u64, NodeId, u64)> = HashMap::new();
    let mut stack = vec![(root, None::<Arc<str>>)];
    while let Some((n, parent_label)) = stack.pop() {
        let label = color_by.label_of(tree, n, metric);
        let candidate = (by_level || n != root) && label.is_some() && label != parent_label;
        if let (true, Some(l)) = (candidate, &label) {
            let size = tree.node(n).sizes.get(metric);
            let e = found.entry(l.clone()).or_insert((0, n, 0));
            e.0 += size;
            if size > e.2 {
                (e.1, e.2) = (n, size);
            }
            if by_level {
                // Everything below has the same label.
                continue;
            }
        }
        let own = if n == root && !by_level { None } else { label };
        stack.extend(tree.children(n).iter().map(|&c| (c, own.clone())));
    }
    let mut entries: Vec<(Arc<str>, u64, NodeId)> = found
        .into_iter()
        .map(|(label, (size, node, _))| (label, size, node))
        .collect();
    // Another dimension: exact sizes per label, not just where it wins.
    if let ColorBy::Labels(labels) = color_by {
        for e in &mut entries {
            e.1 = 0;
        }
        for (label, sizes) in labels.labels(root) {
            match entries.iter_mut().find(|e| e.0 == *label) {
                Some(e) => e.1 = sizes.get(metric),
                None => entries.push((label.clone(), sizes.get(metric), root)),
            }
        }
    }
    entries.retain(|e| e.1 > 0);
    entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    entries
}
