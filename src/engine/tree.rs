// SPDX-License-Identifier: GPL-2.0-only
//! The result tree: fragments aggregated level by level.

use crate::model::{FragmentSet, Interner, LevelInfo, Metric, Role, Sizes, Sym};
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
    /// Sizes in the current build (all inputs if there is no base).
    pub sizes: Sizes,
    /// Sizes in the base build, when comparing two (see [`Role`]).
    pub base: Sizes,
    /// Number of fragments under the node.
    pub fragments: u64,
    /// Lowest runtime address under the node, in the current build.
    pub vma: Option<u64>,
}

/// How a node changed from the base build to the current one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Same,
    Grown,
    Shrunk,
    New,
    Removed,
}

impl Node {
    fn new(name: Arc<str>, parent: Option<NodeId>, depth: u32, level: Option<u32>) -> Self {
        Node {
            name,
            parent,
            children: Vec::new(),
            depth,
            level,
            sizes: Sizes::default(),
            base: Sizes::default(),
            fragments: 0,
            vma: None,
        }
    }

    /// Counts a fragment under this node.
    fn add(&mut self, sizes: Sizes, role: Role, vma: Option<u64>) {
        self.fragments += 1;
        if role == Role::Base {
            self.base += sizes;
            return;
        }
        self.sizes += sizes;
        if sizes.memory > 0 {
            self.vma = match (self.vma, vma) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            };
        }
    }

    /// Current minus base size.
    pub fn delta(&self, metric: Metric) -> i64 {
        self.sizes.get(metric) as i64 - self.base.get(metric) as i64
    }

    pub fn change(&self, metric: Metric) -> Change {
        let (cur, base) = (self.sizes.get(metric), self.base.get(metric));
        match (base, cur) {
            (b, c) if b == c => Change::Same,
            (0, _) => Change::New,
            (_, 0) => Change::Removed,
            (b, c) if c > b => Change::Grown,
            _ => Change::Shrunk,
        }
    }
}

/// The order of children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    /// Biggest first.
    Size(Metric),
    /// Biggest change (either way) first.
    Delta(Metric),
    Name,
}

impl SortKey {
    /// The metric sorted by, if any.
    pub fn metric(self) -> Option<Metric> {
        match self {
            SortKey::Size(m) | SortKey::Delta(m) => Some(m),
            SortKey::Name => None,
        }
    }
}

/// An immutable result, ready for a frontend.
#[derive(Clone, Debug)]
pub struct AnalysisTree {
    pub nodes: Vec<Node>,
    pub levels: Vec<LevelInfo>,
    pub sorted_by: SortKey,
    /// Whether some input is a base build: nodes have base sizes.
    pub has_base: bool,
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
        let mut nodes = vec![Node::new("TOTAL".into(), None, 0, None)];
        let role = |f: &crate::model::Fragment| {
            state
                .binaries
                .get(f.bin.0 as usize)
                .map_or(Role::Current, |b| b.role)
        };
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
                        nodes.push(Node::new(
                            interner.resolve_arc(sym),
                            Some(cur),
                            depth,
                            Some(level as u32),
                        ));
                        nodes[cur.0 as usize].children.push(id);
                        id
                    });
                    path.push(cur);
                }
            }
            leaf_of.push(cur);
            vmas.push(vma);
            for id in &path {
                nodes[id.0 as usize].add(f.sizes, role(f), vma);
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
                nodes.push(Node::new(
                    Self::SELF.into(),
                    Some(leaf),
                    n.depth + 1,
                    n.level,
                ));
                nodes[leaf.0 as usize].children.push(id);
                id
            });
            nodes[child.0 as usize].add(f.sizes, role(f), vmas[i]);
            leaf_of[i] = child;
        }
        let mut tree = AnalysisTree {
            nodes,
            levels: levels.clone(),
            sorted_by: SortKey::Size(sort),
            has_base: state.binaries.iter().any(|b| b.role == Role::Base),
            leaf_of,
        };
        tree.sort(sort);
        tree
    }

    /// Sort children by size, largest first, then by name.
    pub fn sort(&mut self, metric: Metric) {
        self.sort_by(SortKey::Size(metric));
    }

    pub fn sort_by(&mut self, key: SortKey) {
        let mut nodes = std::mem::take(&mut self.nodes);
        for i in 0..nodes.len() {
            let mut children = std::mem::take(&mut nodes[i].children);
            children.sort_by(|a, b| {
                let (a, b) = (&nodes[a.0 as usize], &nodes[b.0 as usize]);
                let by_name = || a.name.cmp(&b.name);
                match key {
                    SortKey::Size(m) => b.sizes.get(m).cmp(&a.sizes.get(m)).then_with(by_name),
                    SortKey::Delta(m) => b
                        .delta(m)
                        .unsigned_abs()
                        .cmp(&a.delta(m).unsigned_abs())
                        .then_with(|| b.sizes.get(m).cmp(&a.sizes.get(m)))
                        .then_with(by_name),
                    SortKey::Name => by_name(),
                }
            });
            nodes[i].children = children;
        }
        self.nodes = nodes;
        self.sorted_by = key;
    }

    /// A copy with only the nodes whose sizes changed from the base (and
    /// the root). Fragments under a dropped node map to its nearest kept
    /// ancestor.
    pub fn changed_only(&self) -> AnalysisTree {
        let keep = |n: &Node| n.sizes != n.base;
        let mut map: Vec<Option<NodeId>> = vec![None; self.nodes.len()];
        let mut nodes: Vec<Node> = Vec::new();
        // Parents come before children in the arena.
        for (i, n) in self.nodes.iter().enumerate() {
            let parent = n.parent.and_then(|p| map[p.0 as usize]);
            if i != 0 && (parent.is_none() || !keep(n)) {
                continue;
            }
            let id = NodeId(nodes.len() as u32);
            map[i] = Some(id);
            let mut copy = n.clone();
            copy.parent = parent;
            copy.children.clear();
            nodes.push(copy);
        }
        for (i, n) in self.nodes.iter().enumerate() {
            let Some(id) = map[i] else { continue };
            nodes[id.0 as usize].children = n
                .children
                .iter()
                .filter_map(|c| map[c.0 as usize])
                .collect();
        }
        let leaf_of = self
            .leaf_of
            .iter()
            .map(|&l| {
                let mut cur = l;
                loop {
                    if let Some(id) = map[cur.0 as usize] {
                        return id;
                    }
                    cur = self.node(cur).parent.unwrap_or(Self::ROOT);
                }
            })
            .collect();
        AnalysisTree {
            nodes,
            levels: self.levels.clone(),
            sorted_by: self.sorted_by,
            has_base: self.has_base,
            leaf_of,
        }
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
