// SPDX-License-Identifier: GPL-2.0-only
//! The result tree: fragments aggregated level by level.

use crate::model::{FragmentSet, Interner, LevelInfo, Metric, Sizes, Sym};
use crate::pipeline::PipelineState;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

#[derive(Clone, Debug)]
pub struct Node {
    pub name: Arc<str>,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    /// 0 for the root.
    pub depth: u32,
    /// The pipeline level this node belongs to; `None` for the root.
    pub level: Option<u32>,
    pub sizes: Sizes,
    /// Number of fragments under the node.
    pub fragments: u64,
    /// Lowest runtime address under the node.
    pub vma: Option<u64>,
}

impl Node {
    /// Counts a fragment under this node.
    fn add(&mut self, sizes: Sizes, vma: Option<u64>) {
        self.sizes += sizes;
        self.fragments += 1;
        if sizes.memory > 0 {
            self.vma = match (self.vma, vma) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
    }
}

/// An immutable result, ready for a frontend.
#[derive(Clone, Debug)]
pub struct AnalysisTree {
    pub nodes: Vec<Node>,
    pub levels: Vec<LevelInfo>,
    pub sorted_by: Metric,
    /// The leaf node of each fragment of the state the tree was built
    /// from, by fragment index.
    pub leaf_of: Vec<NodeId>,
}

impl AnalysisTree {
    pub const ROOT: NodeId = NodeId(0);
    /// The name of the child holding a node's own bytes, when the node has
    /// children too.
    pub const SELF: &'static str = "[self]";

    pub fn build(state: &PipelineState, interner: &Interner, sort: Metric) -> Self {
        let FragmentSet { fragments, levels } = &state.set;
        let mut nodes = vec![Node {
            name: "TOTAL".into(),
            parent: None,
            children: Vec::new(),
            depth: 0,
            level: None,
            sizes: Sizes::default(),
            fragments: 0,
            vma: None,
        }];
        let mut index: HashMap<(u32, u32, Sym), NodeId> = HashMap::new();
        let mut leaf_of = Vec::with_capacity(fragments.len());
        let mut vmas = Vec::with_capacity(fragments.len());
        let mut path = Vec::new();
        for f in fragments {
            let vma = state
                .binaries
                .get(f.bin.0 as usize)
                .and_then(|b| b.chunks.chunk(f.chunk as usize).vma)
                .map(|v| {
                    let c = state.binaries[f.bin.0 as usize]
                        .chunks
                        .chunk(f.chunk as usize);
                    v + (f.span.start - c.unified.start)
                });
            path.clear();
            path.push(Self::ROOT);
            let mut cur = Self::ROOT;
            for (level, label) in f.labels.iter().enumerate() {
                for &sym in label.parts() {
                    let key = (cur.0, level as u32, sym);
                    cur = *index.entry(key).or_insert_with(|| {
                        let id = NodeId(nodes.len() as u32);
                        let depth = nodes[cur.0 as usize].depth + 1;
                        nodes.push(Node {
                            name: interner.resolve_arc(sym),
                            parent: Some(cur),
                            children: Vec::new(),
                            depth,
                            level: Some(level as u32),
                            sizes: Sizes::default(),
                            fragments: 0,
                            vma: None,
                        });
                        nodes[cur.0 as usize].children.push(id);
                        id
                    });
                    path.push(cur);
                }
            }
            leaf_of.push(cur);
            vmas.push(vma);
            for id in &path {
                nodes[id.0 as usize].add(f.sizes, vma);
            }
        }
        // A path label can end where another one goes on (`fw › main` and
        // `fw › main › {{closure}}`). Such a node's own bytes go to a
        // `[self]` child, so children always add up to their parent.
        let mut self_child: HashMap<NodeId, NodeId> = HashMap::new();
        for (i, f) in fragments.iter().enumerate() {
            let leaf = leaf_of[i];
            if nodes[leaf.0 as usize].children.is_empty() {
                continue;
            }
            let child = *self_child.entry(leaf).or_insert_with(|| {
                let id = NodeId(nodes.len() as u32);
                let n = &nodes[leaf.0 as usize];
                nodes.push(Node {
                    name: Self::SELF.into(),
                    parent: Some(leaf),
                    children: Vec::new(),
                    depth: n.depth + 1,
                    level: n.level,
                    sizes: Sizes::default(),
                    fragments: 0,
                    vma: None,
                });
                nodes[leaf.0 as usize].children.push(id);
                id
            });
            nodes[child.0 as usize].add(f.sizes, vmas[i]);
            leaf_of[i] = child;
        }
        let mut tree = AnalysisTree {
            nodes,
            levels: levels.clone(),
            sorted_by: sort,
            leaf_of,
        };
        tree.sort(sort);
        tree
    }

    /// Sort children by size, largest first, then by name.
    pub fn sort(&mut self, metric: Metric) {
        let mut nodes = std::mem::take(&mut self.nodes);
        for i in 0..nodes.len() {
            let mut children = std::mem::take(&mut nodes[i].children);
            children.sort_by(|a, b| {
                let (a, b) = (&nodes[a.0 as usize], &nodes[b.0 as usize]);
                b.sizes
                    .get(metric)
                    .cmp(&a.sizes.get(metric))
                    .then_with(|| a.name.cmp(&b.name))
            });
            nodes[i].children = children;
        }
        self.nodes = nodes;
        self.sorted_by = metric;
    }

    pub fn root(&self) -> &Node {
        &self.nodes[0]
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id.0 as usize]
    }

    pub fn children(&self, id: NodeId) -> &[NodeId] {
        &self.node(id).children
    }

    /// From the root down to `id`.
    pub fn path(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = vec![id];
        let mut cur = id;
        while let Some(p) = self.node(cur).parent {
            out.push(p);
            cur = p;
        }
        out.reverse();
        out
    }
}
