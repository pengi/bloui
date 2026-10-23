// SPDX-License-Identifier: GPL-2.0-only
use super::ViewState;
use crate::engine::{AnalysisTree, NodeId};
use crate::model::Metric;
use crate::render::{
    self, CushionOptions, Image, LayoutOptions, LevelColors, Rect, Scene, Treemap, TreemapLayout,
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
    color_level: u32,
}

impl PartialEq for Key {
    fn eq(&self, o: &Key) -> bool {
        Arc::ptr_eq(&self.tree, &o.tree)
            && (
                self.root,
                self.metric,
                self.width,
                self.height,
                self.layout,
                self.color_level,
            ) == (o.root, o.metric, o.width, o.height, o.layout, o.color_level)
    }
}

/// The treemap view. Keeps the layout and the rendered cushions, and only
/// redoes them when the tree, zoom, metric, size or colors change; hover
/// and selection only change the scene's outlines.
#[derive(Default)]
pub struct TreemapView {
    pub layout: LayoutKind,
    pub color_level: u32,
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
            color_level: self.color_level,
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
        let colors = LevelColors {
            level: self.color_level,
        };
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
        render::scene::treemap_scene(
            map,
            state.tree(),
            image.clone(),
            state.selected(),
            state.hover(),
        )
    }

    /// The node under a point, from the last scene's layout.
    pub fn node_at(&self, x: f64, y: f64) -> Option<NodeId> {
        self.cache.as_ref()?.1.tile_at(x, y).map(|t| t.node)
    }

    /// The last layout.
    pub fn treemap(&self) -> Option<&Treemap> {
        self.cache.as_ref().map(|c| &c.1)
    }
}
