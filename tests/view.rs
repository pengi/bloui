// SPDX-License-Identifier: GPL-2.0-only
//! GUI-neutral view state and the treemap view.

mod common;
use bloui::engine::AnalysisTree;
use bloui::model::Metric;
use bloui::view::{TreemapView, ViewState};
use common::*;
use std::sync::Arc;

fn fw(extra_split: bool) -> Arc<AnalysisTree> {
    let mut stages = vec![
        open(FW),
        split("memory-region"),
        stage("free-space", &[]),
        split("section"),
    ];
    if extra_split {
        stages.push(split("symbol"));
    }
    Arc::new(tree(stages, Metric::Memory))
}

fn node(tree: &AnalysisTree, path: &[&str]) -> bloui::engine::NodeId {
    let n = find(tree, path).unwrap() as *const _;
    let i = tree.nodes.iter().position(|x| std::ptr::eq(x, n)).unwrap();
    bloui::engine::NodeId(i as u32)
}

#[test]
fn zoom_and_select() {
    let t = fw(true);
    let mut v = ViewState::new(t.clone());
    let main = node(&t, &["FLASH", ".text", "main"]);
    assert_eq!(v.zoom_root(), AnalysisTree::ROOT);
    // Zooming towards main goes one level at a time.
    assert!(v.zoom_in(main));
    assert_eq!(v.zoom_root(), node(&t, &["FLASH"]));
    assert!(v.zoom_in(main));
    assert_eq!(v.zoom_root(), node(&t, &["FLASH", ".text"]));
    // main has no children: no further.
    assert!(!v.zoom_in(main));
    assert!(v.zoom_out());
    assert_eq!(v.zoom_root(), node(&t, &["FLASH"]));
    v.zoom_reset();
    assert!(!v.zoom_out());

    v.select(Some(main));
    assert!(v.select_parent());
    assert_eq!(v.selected(), Some(node(&t, &["FLASH", ".text"])));
    assert!(
        v.describe(main)
            .starts_with("FLASH › .text › main   136 bytes memory")
    );
}

#[test]
fn survives_a_new_tree() {
    let t = fw(true);
    let mut v = ViewState::new(t.clone());
    let main = node(&t, &["FLASH", ".text", "main"]);
    v.zoom_to(node(&t, &["FLASH", ".text"]));
    v.select(Some(main));
    // Same pipeline: same places.
    let t2 = fw(true);
    v.set_tree(t2.clone());
    assert_eq!(v.selected(), Some(node(&t2, &["FLASH", ".text", "main"])));
    assert_eq!(v.zoom_root(), node(&t2, &["FLASH", ".text"]));
    // Without the symbol level: the nearest ancestor.
    let t3 = fw(false);
    v.set_tree(t3.clone());
    assert_eq!(v.selected(), Some(node(&t3, &["FLASH", ".text"])));
}

#[test]
fn treemap_view_caches_and_hits() {
    let t = fw(true);
    let mut v = ViewState::new(t.clone());
    let mut tm = TreemapView::new();
    let s1 = tm.scene(&v, 400, 300);
    let img1 = match &s1.prims[0].1 {
        bloui::render::Prim::Image { image, .. } => image.clone(),
        _ => panic!(),
    };
    // Hover only changes outlines; the image is reused.
    let hit = tm.node_at(10.0, 10.0).unwrap();
    v.set_hover(Some(hit));
    let s2 = tm.scene(&v, 400, 300);
    match &s2.prims[0].1 {
        bloui::render::Prim::Image { image, .. } => assert!(Arc::ptr_eq(image, &img1)),
        _ => panic!(),
    }
    assert_eq!(s2.prims.len(), 2);
    // Zooming re-renders, with the zoom root filling the view.
    v.zoom_to(node(&t, &["RAM"]));
    let s3 = tm.scene(&v, 400, 300);
    match &s3.prims[0].1 {
        bloui::render::Prim::Image { image, .. } => assert!(!Arc::ptr_eq(image, &img1)),
        _ => panic!(),
    }
    let root_tile = tm.treemap().unwrap().tiles[0];
    assert_eq!(root_tile.node, node(&t, &["RAM"]));
    assert!(tm.scene(&v, 0, 10).prims.is_empty());
}
