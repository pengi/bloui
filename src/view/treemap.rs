// SPDX-License-Identifier: GPL-2.0-only
use super::ViewState;
use crate::engine::{AnalysisTree, NodeId};
use crate::model::Metric;
use crate::render::{
    self, ColorBy, CushionOptions, Image, LayoutOptions, Prim, Rect, Scene, Treemap, TreemapLayout,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LayoutKind {
    #[default]
    Squarified,
    SliceAndDice,
}

#[derive(Clone)]
struct Key {
    tree: Arc<AnalysisTree>,
    root: NodeId,
    metric: Metric,
    width: u32,
    height: u32,
    layout: LayoutKind,
    color: ColorBy,
}

impl PartialEq for Key {
    fn eq(&self, o: &Key) -> bool {
        Arc::ptr_eq(&self.tree, &o.tree)
            && (self.root, self.metric, self.width, self.height, self.layout)
                == (o.root, o.metric, o.width, o.height, o.layout)
            && self.color == o.color
    }
}

/// The treemap view. Keeps the layout and the rendered cushions, and only
/// redoes them when the tree, zoom, metric, size or colors change; hover
/// and selection only change the scene's outlines.
#[derive(Default)]
pub struct TreemapView {
    pub layout: LayoutKind,
    pub color: ColorBy,
    /// Outline the tiles with this color label (hovering the legend).
    pub highlight: Option<Arc<str>>,
    pub cushion: CushionOptions,
    cache: Option<(Key, Treemap, Arc<Image>)>,
}

impl TreemapView {
    pub fn new() -> Self {
        Self::default()
    }

    fn update(&mut self, state: &ViewState, width: u32, height: u32) {
        let key = Key {
            tree: state.tree().clone(),
            root: state.zoom_root(),
            metric: state.metric,
            width,
            height,
            layout: self.layout,
            color: self.color.clone(),
        };
        if self.cache.as_ref().is_some_and(|(k, _, _)| *k == key) {
            return;
        }
        let tree = state.tree();
        let layout: &dyn TreemapLayout = match self.layout {
            LayoutKind::Squarified => &render::Squarified,
            LayoutKind::SliceAndDice => &render::SliceAndDice,
        };
        let bounds = Rect::new(0.0, 0.0, width as f64, height as f64);
        let map = layout.layout(
            tree,
            key.root,
            key.metric,
            bounds,
            &LayoutOptions::default(),
        );
        let colors = self.color.scheme(tree, key.metric);
        let image = render::cushion::render(&map, tree, &colors, width, height, &self.cushion);
        self.cache = Some((key, map, Arc::new(image)));
    }

    /// The scene for a view of `width` × `height` pixels.
    pub fn scene(&mut self, state: &ViewState, width: u32, height: u32) -> Scene {
        if width == 0 || height == 0 {
            return Scene::default();
        }
        self.update(state, width, height);
        let (_, map, image) = self.cache.as_ref().unwrap();
        let mut scene = render::scene::treemap_scene(
            map,
            state.tree(),
            image.clone(),
            state.selected(),
            state.hover(),
        );
        if let Some(label) = &self.highlight {
            for tile in self.highlighted(state, label) {
                scene.push(
                    Some(tile.0),
                    Prim::Outline {
                        rect: tile.1,
                        color: render::scene::HIGHLIGHT,
                        width: 2.0,
                    },
                );
            }
        }
        scene
    }

    /// The topmost tiles whose color label is `label`.
    fn highlighted(&self, state: &ViewState, label: &str) -> Vec<(NodeId, Rect)> {
        let Some((_, map, _)) = self.cache.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut i = 0;
        while i < map.tiles.len() {
            let t = &map.tiles[i];
            let l = self.color.label_of(state.tree(), t.node, state.metric);
            if l.as_deref() == Some(label) {
                out.push((t.node, t.rect));
                i = t.next as usize;
            } else {
                i += 1;
            }
        }
        out
    }

    /// The node under a point, from the last scene's layout. `None` if that
    /// layout is of another tree or zoom than `state`'s (input can arrive
    /// after a new tree is set but before it is painted), so a node id is
    /// never used with the wrong tree.
    pub fn node_at(&self, state: &ViewState, x: f64, y: f64) -> Option<NodeId> {
        let (key, map, _) = self.cache.as_ref()?;
        if !Arc::ptr_eq(&key.tree, state.tree()) || key.root != state.zoom_root() {
            return None;
        }
        map.tile_at(x, y).map(|t| t.node)
    }

    /// The last layout.
    pub fn treemap(&self) -> Option<&Treemap> {
        self.cache.as_ref().map(|c| &c.1)
    }
}
