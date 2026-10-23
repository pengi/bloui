// SPDX-License-Identifier: GPL-2.0-only
//! Labels of an extra dimension per tree node, without adding a level:
//! what colors the treemap when coloring "by object file" while the tree
//! is split by section and symbol.

use super::{AnalysisTree, NodeId};
use crate::Result;
use crate::model::{Metric, Sizes};
use crate::param::DimRef;
use crate::pipeline::{PipelineCtx, PipelineState};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct ColorLabels {
    /// The dimension's display name.
    pub name: String,
    /// Per node: the labels of the bytes under it, with their sizes,
    /// sorted by label.
    per_node: Vec<Vec<(Arc<str>, Sizes)>>,
}

impl ColorLabels {
    /// Label the fragments `tree` was built from by `dim`, and sum the
    /// sizes per node and label. Free space is `[free]`, unless the
    /// dimension labels free space itself.
    pub fn compute(
        ctx: &mut PipelineCtx,
        state: &PipelineState,
        tree: &AnalysisTree,
        dim: &DimRef,
    ) -> Result<Self> {
        let registry = ctx.registry.clone();
        let handle = registry.create_dimension(dim)?;
        let mut names: HashMap<String, Arc<str>> = HashMap::new();
        let mut maps: Vec<HashMap<Arc<str>, Sizes>> = vec![HashMap::new(); tree.nodes.len()];
        for (i, frag) in state.set.fragments.iter().enumerate() {
            let Some(&leaf) = tree.leaf_of.get(i) else {
                break;
            };
            let frags = std::slice::from_ref(frag);
            ctx.cut_all(
                state,
                frags,
                &handle.key,
                &*handle.dim,
                |piece, label, interner| {
                    let text = label.text(interner);
                    let name = names
                        .entry(text)
                        .or_insert_with_key(|t| Arc::from(t.as_str()))
                        .clone();
                    *maps[leaf.0 as usize].entry(name).or_default() += piece.sizes;
                },
            )?;
        }
        // Children come after their parents in the node list.
        for id in (1..tree.nodes.len()).rev() {
            let parent = tree.nodes[id].parent.expect("only the root has no parent");
            let child = std::mem::take(&mut maps[id]);
            for (name, sizes) in &child {
                *maps[parent.0 as usize].entry(name.clone()).or_default() += *sizes;
            }
            maps[id] = child;
        }
        let per_node = maps
            .into_iter()
            .map(|m| {
                let mut v: Vec<_> = m.into_iter().collect();
                v.sort_by(|a, b| a.0.cmp(&b.0));
                v
            })
            .collect();
        Ok(ColorLabels {
            name: handle.name.to_owned(),
            per_node,
        })
    }

    /// The labels of the bytes under `node`, sorted by label.
    pub fn labels(&self, node: NodeId) -> &[(Arc<str>, Sizes)] {
        self.per_node
            .get(node.0 as usize)
            .map_or(&[], |v| v.as_slice())
    }

    /// The label with the most bytes under `node`.
    pub fn dominant(&self, node: NodeId, metric: Metric) -> Option<&Arc<str>> {
        self.labels(node)
            .iter()
            .max_by(|a, b| {
                a.1.get(metric)
                    .cmp(&b.1.get(metric))
                    .then_with(|| b.0.cmp(&a.0))
            })
            .map(|(name, _)| name)
    }
}
