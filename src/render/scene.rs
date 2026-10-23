// SPDX-License-Identifier: GPL-2.0-only
//! Scenes: what a view looks like, as a display list. A frontend paints
//! the primitives in order and does nothing else; layout and hit testing
//! stay in the library.

use super::{Image, Rect, Rgb};
use crate::engine::NodeId;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub enum Prim {
    Image {
        rect: Rect,
        image: Arc<Image>,
    },
    Fill {
        rect: Rect,
        color: Rgb,
        alpha: f64,
    },
    Outline {
        rect: Rect,
        color: Rgb,
        width: f64,
    },
    Line {
        from: (f64, f64),
        to: (f64, f64),
        color: Rgb,
        width: f64,
    },
    Text {
        pos: (f64, f64),
        text: String,
        size: f64,
        color: Rgb,
    },
}

#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub width: f64,
    pub height: f64,
    /// Primitives in paint order, each with the node it shows, if any.
    pub prims: Vec<(Option<NodeId>, Prim)>,
}

impl Scene {
    pub fn push(&mut self, node: Option<NodeId>, prim: Prim) {
        self.prims.push((node, prim));
    }
}

/// Outline colors for the treemap scene.
pub const SELECTION: Rgb = Rgb(0xff, 0xff, 0xff);
pub const HOVER: Rgb = Rgb(0xff, 0xe0, 0x40);
/// Tiles of the label hovered in the legend.
pub const HIGHLIGHT: Rgb = Rgb(0x40, 0xe0, 0xff);

/// The treemap view: the rendered cushions, then outlines for the
/// selected and hovered nodes (or their nearest visible ancestors).
pub fn treemap_scene(
    map: &super::Treemap,
    tree: &crate::engine::AnalysisTree,
    image: Arc<Image>,
    selected: Option<NodeId>,
    hover: Option<NodeId>,
) -> Scene {
    let mut scene = Scene {
        width: map.bounds.w,
        height: map.bounds.h,
        prims: Vec::new(),
    };
    scene.push(
        None,
        Prim::Image {
            rect: map.bounds,
            image,
        },
    );
    for (node, color, width) in [(hover, HOVER, 1.0), (selected, SELECTION, 2.0)] {
        if let Some(t) = node.and_then(|n| map.visible_tile_of(tree, n)) {
            scene.push(
                Some(t.node),
                Prim::Outline {
                    rect: t.rect,
                    color,
                    width,
                },
            );
        }
    }
    scene
}
