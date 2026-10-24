// SPDX-License-Identifier: GPL-2.0-only
use crate::engine::{AnalysisTree, NodeId};
use crate::model::Metric;
use std::sync::Arc;

/// What the user is looking at. Survives a new result tree (after a rerun)
/// by remembering nodes by their path of names.
#[derive(Clone, Debug)]
pub struct ViewState {
    tree: Arc<AnalysisTree>,
    pub metric: Metric,
    selected: Option<NodeId>,
    hover: Option<NodeId>,
    /// Zoomed-in nodes, outermost first. Empty: the root.
    zoom: Vec<NodeId>,
}

impl ViewState {
    pub fn new(tree: Arc<AnalysisTree>) -> Self {
        let metric = tree.sorted_by.metric().unwrap_or_default();
        ViewState {
            tree,
            metric,
            selected: None,
            hover: None,
            zoom: Vec::new(),
        }
    }

    pub fn tree(&self) -> &Arc<AnalysisTree> {
        &self.tree
    }

    /// Switch to a new result. Selection and zoom move to the nodes with
    /// the same path, or the nearest ancestor that still exists.
    pub fn set_tree(&mut self, tree: Arc<AnalysisTree>) {
        let selected = self.selected.map(|n| self.path_names(n));
        let zoom: Vec<_> = self.zoom.iter().map(|&n| self.path_names(n)).collect();
        self.tree = tree;
        self.hover = None;
        self.selected = selected.map(|p| self.find_nearest(&p));
        self.zoom = zoom
            .iter()
            .map(|p| self.find_nearest(p))
            .filter(|&n| n != AnalysisTree::ROOT)
            .collect();
        self.zoom.dedup();
    }

    pub fn selected(&self) -> Option<NodeId> {
        self.selected
    }

    pub fn hover(&self) -> Option<NodeId> {
        self.hover
    }

    /// Returns whether anything changed.
    pub fn select(&mut self, node: Option<NodeId>) -> bool {
        let changed = self.selected != node;
        self.selected = node;
        changed
    }

    /// Returns whether anything changed.
    pub fn set_hover(&mut self, node: Option<NodeId>) -> bool {
        let changed = self.hover != node;
        self.hover = node;
        changed
    }

    /// The node the treemap shows.
    pub fn zoom_root(&self) -> NodeId {
        self.zoom.last().copied().unwrap_or(AnalysisTree::ROOT)
    }

    /// Zoom in, towards `node`: to the child of the current zoom root on
    /// the way to it, or to `node` itself. Returns whether it zoomed.
    pub fn zoom_in(&mut self, node: NodeId) -> bool {
        let root = self.zoom_root();
        let path = self.tree.path(node);
        let Some(pos) = path.iter().position(|&n| n == root) else {
            return false;
        };
        let target = match path.get(pos + 1) {
            Some(&n) => n,
            None => return false,
        };
        if self.tree.children(target).is_empty() {
            return false;
        }
        self.zoom.push(target);
        true
    }

    /// Zoom straight to `node`, if it has children.
    pub fn zoom_to(&mut self, node: NodeId) -> bool {
        if self.tree.children(node).is_empty() {
            return false;
        }
        self.zoom = self.tree.path(node)[1..].to_vec();
        true
    }

    pub fn zoom_out(&mut self) -> bool {
        self.zoom.pop().is_some()
    }

    pub fn zoom_reset(&mut self) {
        self.zoom.clear();
    }

    /// Select the parent of the selection (not above the zoom root).
    pub fn select_parent(&mut self) -> bool {
        let Some(sel) = self.selected else {
            return false;
        };
        if sel == self.zoom_root() {
            return false;
        }
        self.select(self.tree.node(sel).parent)
    }

    /// Names from below the root to `node`.
    pub fn path_names(&self, node: NodeId) -> Vec<String> {
        self.tree.path(node)[1..]
            .iter()
            .map(|&n| self.tree.node(n).name.to_string())
            .collect()
    }

    /// The node with this path, or its nearest existing ancestor.
    pub fn find_nearest(&self, names: &[String]) -> NodeId {
        let mut cur = AnalysisTree::ROOT;
        for name in names {
            match self
                .tree
                .children(cur)
                .iter()
                .find(|&&c| *self.tree.node(c).name == **name)
            {
                Some(&c) => cur = c,
                None => break,
            }
        }
        cur
    }

    /// One line about a node: path, size, share of the total, address.
    pub fn describe(&self, node: NodeId) -> String {
        let n = self.tree.node(node);
        let mut path = self.path_names(node);
        if path.is_empty() {
            path.push(n.name.to_string());
        }
        let size = n.sizes.get(self.metric);
        let total = self.tree.root().sizes.get(self.metric);
        let mut s = format!(
            "{}   {} bytes {}",
            path.join(" › "),
            size,
            self.metric.name()
        );
        if total > 0 {
            s.push_str(&format!(" ({:.1}%)", size as f64 * 100.0 / total as f64));
        }
        if let Some(a) = n.vma {
            s.push_str(&format!("   {a:#x}"));
        }
        s
    }
}
