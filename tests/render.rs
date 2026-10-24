// SPDX-License-Identifier: GPL-2.0-only
//! Treemap layout invariants, hit testing and cushion rendering.

mod common;
use bloui::engine::AnalysisTree;
use bloui::model::Metric;
use bloui::render::{self, LayoutOptions, Rect, Treemap, TreemapLayout};
use common::*;

fn fw_tree() -> AnalysisTree {
    tree(
        vec![
            open(FW),
            split("memory-region"),
            stage("free-space", &[]),
            split("object-file"),
            split("input-section"),
        ],
        Metric::Memory,
    )
}

const EPS: f64 = 1e-6;

fn inside(inner: &Rect, outer: &Rect) -> bool {
    inner.x >= outer.x - EPS
        && inner.y >= outer.y - EPS
        && inner.right() <= outer.right() + EPS
        && inner.bottom() <= outer.bottom() + EPS
}

fn overlap(a: &Rect, b: &Rect) -> f64 {
    let w = a.right().min(b.right()) - a.x.max(b.x);
    let h = a.bottom().min(b.bottom()) - a.y.max(b.y);
    if w > 0.0 && h > 0.0 { w * h } else { 0.0 }
}

fn check(map: &Treemap, tree: &AnalysisTree, metric: Metric) {
    let total = tree.root().sizes.get(metric) as f64;
    let area = map.bounds.area();
    for (i, t) in map.tiles.iter().enumerate() {
        assert!(inside(&t.rect, &map.bounds), "tile {i} outside bounds");
        // Area proportional to size.
        let want = tree.node(t.node).sizes.get(metric) as f64 / total * area;
        assert!(
            (t.rect.area() - want).abs() < 1e-3 * area.max(1.0),
            "tile {i}: {} vs {want}",
            t.rect.area()
        );
        // Children inside, not overlapping each other.
        let mut kids = Vec::new();
        let mut c = i + 1;
        while c < t.next as usize {
            assert!(
                inside(&map.tiles[c].rect, &t.rect),
                "child {c} of {i} outside"
            );
            kids.push(c);
            c = map.tiles[c].next as usize;
        }
        for (k, &a) in kids.iter().enumerate() {
            for &b in &kids[k + 1..] {
                assert!(
                    overlap(&map.tiles[a].rect, &map.tiles[b].rect) < 1e-6,
                    "{a} overlaps {b}"
                );
            }
        }
    }
}

#[test]
fn squarified_and_slice_invariants() {
    let tree = fw_tree();
    let bounds = Rect::new(10.0, 20.0, 800.0, 500.0);
    let opts = LayoutOptions {
        min_area: 0.0,
        ..Default::default()
    };
    for layout in [
        &render::Squarified as &dyn TreemapLayout,
        &render::SliceAndDice,
    ] {
        let map = layout.layout(&tree, AnalysisTree::ROOT, Metric::Memory, bounds, &opts);
        assert_eq!(map.tiles[0].rect, bounds);
        check(&map, &tree, Metric::Memory);
        // Every node with memory got a tile.
        let with_size = tree.nodes.iter().filter(|n| n.sizes.memory > 0).count();
        assert_eq!(map.tiles.len(), with_size);
    }
}

#[test]
fn squarified_aspect_ratios() {
    // Equal sizes in a square give squares.
    let rects = render::Squarified.split(Rect::new(0.0, 0.0, 100.0, 100.0), &[2500.0; 4], 0);
    for r in &rects {
        assert!(
            (r.w - 50.0).abs() < EPS && (r.h - 50.0).abs() < EPS,
            "{r:?}"
        );
    }
    // The classic example from the paper: 6, 6, 4, 3, 2, 2, 1 in 6x4.
    let rects = render::Squarified.split(
        Rect::new(0.0, 0.0, 6.0, 4.0),
        &[6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0],
        0,
    );
    let worst = rects
        .iter()
        .map(|r| (r.w / r.h).max(r.h / r.w))
        .fold(0.0, f64::max);
    assert!(worst < 3.0, "worst aspect {worst}");
    let area: f64 = rects.iter().map(Rect::area).sum();
    assert!((area - 24.0).abs() < EPS);
}

#[test]
fn small_tiles_fold_into_parents() {
    let tree = fw_tree();
    let bounds = Rect::new(0.0, 0.0, 200.0, 100.0);
    let map = render::Squarified.layout(
        &tree,
        AnalysisTree::ROOT,
        Metric::Memory,
        bounds,
        &LayoutOptions::default(),
    );
    assert!(map.tiles.iter().all(|t| t.rect.area() >= 1.0));
    let all = tree.nodes.iter().filter(|n| n.sizes.memory > 0).count();
    assert!(map.tiles.len() < all);
    let depth = render::Squarified.layout(
        &tree,
        AnalysisTree::ROOT,
        Metric::Memory,
        bounds,
        &LayoutOptions {
            max_depth: Some(1),
            ..Default::default()
        },
    );
    assert!(depth.tiles.iter().all(|t| t.depth <= 1));
}

#[test]
fn hit_testing() {
    let tree = fw_tree();
    let bounds = Rect::new(0.0, 0.0, 640.0, 400.0);
    let map = render::Squarified.layout(
        &tree,
        AnalysisTree::ROOT,
        Metric::Memory,
        bounds,
        &LayoutOptions::default(),
    );
    for y in (0..400).step_by(7) {
        for x in (0..640).step_by(7) {
            let (fx, fy) = (x as f64 + 0.25, y as f64 + 0.25);
            let hit = map.tile_at(fx, fy).unwrap();
            // Brute force: the deepest tile containing the point.
            let want = map
                .tiles
                .iter()
                .filter(|t| t.rect.contains(fx, fy))
                .max_by_key(|t| t.depth)
                .unwrap();
            assert_eq!(hit.node, want.node, "at {x},{y}");
        }
    }
    assert!(map.tile_at(-1.0, 5.0).is_none());
    assert!(map.tile_at(640.0, 5.0).is_none());
    let main = tree
        .nodes
        .iter()
        .position(|n| &*n.name == ".text.startup.main")
        .map(|i| bloui::engine::NodeId(i as u32))
        .unwrap();
    let t = map.visible_tile_of(&tree, main).unwrap();
    assert!(tree.path(main).contains(&t.node));
}

#[test]
fn cushion_rendering() {
    let tree = fw_tree();
    let (w, h) = (320, 200);
    let bounds = Rect::new(0.0, 0.0, w as f64, h as f64);
    let map = render::Squarified.layout(
        &tree,
        AnalysisTree::ROOT,
        Metric::Memory,
        bounds,
        &LayoutOptions::default(),
    );
    let colors = render::LevelColors { level: 1 };
    let a = render::cushion::render(&map, &tree, &colors, w, h, &Default::default());
    let b = render::cushion::render(&map, &tree, &colors, w, h, &Default::default());
    assert_eq!(a, b, "deterministic");
    assert!(a.rgba.chunks(4).all(|p| p[3] == 255), "fully covered");
    // Free space is flat: the same color across its tile, inside the edge.
    let free = map
        .tiles
        .iter()
        .find(|t| &*tree.node(t.node).name == "[free]" && t.rect.w > 20.0 && t.rect.h > 20.0)
        .unwrap();
    let (cx, cy) = (
        (free.rect.x + free.rect.w / 2.0) as u32,
        (free.rect.y + free.rect.h / 2.0) as u32,
    );
    assert_eq!(a.pixel(cx, cy), a.pixel(cx + 5, cy + 5));
    assert_eq!(a.pixel(cx, cy), [0xd8, 0xd8, 0xd8, 255]);
    // A shaded tile is lighter in the middle than at its edge.
    let big = map
        .tiles
        .iter()
        .enumerate()
        .filter(|(i, t)| !t.has_children(*i) && &*tree.node(t.node).name != "[free]")
        .map(|(_, t)| t)
        .max_by(|x, y| x.rect.area().total_cmp(&y.rect.area()))
        .unwrap();
    let lum = |p: [u8; 4]| p[0] as u32 + p[1] as u32 + p[2] as u32;
    let mid = a.pixel(
        (big.rect.x + big.rect.w / 2.0) as u32,
        (big.rect.y + big.rect.h / 2.0) as u32,
    );
    let edge = a.pixel(
        (big.rect.right() - 1.0) as u32,
        (big.rect.bottom() - 1.0) as u32,
    );
    assert!(lum(mid) > lum(edge));
}

#[test]
fn colors_are_stable() {
    assert_eq!(
        render::color::label_color("main.o"),
        render::color::label_color("main.o")
    );
    assert_eq!(render::color::stable_hash("abc"), 0xe71fa2190541574b);
}

#[test]
fn treemap_scene() {
    let tree = fw_tree();
    let bounds = Rect::new(0.0, 0.0, 100.0, 50.0);
    let map = render::Squarified.layout(
        &tree,
        AnalysisTree::ROOT,
        Metric::Memory,
        bounds,
        &LayoutOptions::default(),
    );
    let img = std::sync::Arc::new(render::Image::new(100, 50));
    let first_child = tree.children(AnalysisTree::ROOT)[0];
    let s = render::scene::treemap_scene(&map, &tree, img, Some(first_child), None);
    assert_eq!(s.prims.len(), 2);
    assert!(matches!(s.prims[0].1, render::Prim::Image { .. }));
    assert_eq!(s.prims[1].0, Some(first_child));
}

#[test]
fn color_by_another_dimension() {
    use bloui::engine::ColorLabels;
    use bloui::param::DimRef;
    use bloui::render::{ColorBy, legend};
    use std::sync::Arc;
    let (result, mut runner) = run(vec![
        open(FW),
        stage("filter-space", &[("keep", text("load"))]),
        split("section"),
    ]);
    let t = AnalysisTree::build(&result.state, &runner.ctx.interner, Metric::Load);
    let labels = ColorLabels::compute(
        &mut runner.ctx,
        &result.state,
        &t,
        &DimRef::new("object-file"),
    )
    .unwrap();
    let text = find(&t, &[".text"]).unwrap();
    let text_id = *t
        .children(AnalysisTree::ROOT)
        .iter()
        .find(|&&c| &*t.node(c).name == ".text")
        .unwrap();
    // Labels under a node add up to its size.
    let sum: u64 = labels.labels(text_id).iter().map(|l| l.1.load).sum();
    assert_eq!(sum, text.sizes.load);
    assert_eq!(
        labels.dominant(text_id, Metric::Load).map(|l| &**l),
        Some("libgcc.a/_udivmoddi4.o")
    );

    let by_obj = ColorBy::Labels(Arc::new(labels));
    let l = legend(&t, &by_obj, AnalysisTree::ROOT, Metric::Load, 3);
    let names: Vec<&str> = l.iter().map(|e| &*e.label).collect();
    assert_eq!(
        names,
        ["libgcc.a/_udivmoddi4.o", "main.o", "uart.o", "[other]"]
    );
    assert_eq!(l[0].size, 792);
    assert_eq!(l[0].node, text_id);
    assert_eq!(l.iter().map(|e| e.size).sum::<u64>(), 1668);

    let l = legend(&t, &ColorBy::Level(0), AnalysisTree::ROOT, Metric::Load, 10);
    assert_eq!(
        (&*l[0].label, l[0].size, l[0].node),
        (".text", 1320, text_id)
    );
    assert_eq!(l.len(), 5);
}

#[test]
fn legend_highlight_outlines_tiles() {
    use bloui::render::{ColorBy, Prim};
    use bloui::view::{TreemapView, ViewState};
    use std::sync::Arc;
    let t = Arc::new(fw_tree());
    let state = ViewState::new(t.clone());
    let mut view = TreemapView::new();
    view.color = ColorBy::Level(0);
    view.highlight = Some("FLASH".into());
    let scene = view.scene(&state, 400, 300);
    let outlines: Vec<_> = scene
        .prims
        .iter()
        .filter(
            |(_, p)| matches!(p, Prim::Outline { color, .. } if *color == render::scene::HIGHLIGHT),
        )
        .collect();
    assert_eq!(outlines.len(), 1);
    assert_eq!(&*t.node(outlines[0].0.unwrap()).name, "FLASH");
}

#[test]
fn colors_are_spread() {
    use render::ColorScheme;
    use render::color::{ColorBy, ColorMap, PALETTE};
    use render::legend;
    // Symbols of the Rust firmware: far more than 12 labels.
    let t = tree(vec![open("rust/fw.elf"), split("symbol")], Metric::Load);
    let by = ColorBy::Level(0);
    let l = legend(&t, &by, AnalysisTree::ROOT, Metric::Load, 100);
    let named: Vec<_> = l.iter().filter(|e| !e.label.starts_with('[')).collect();
    assert!(named.len() > PALETTE.len());
    // The biggest 12 named labels all differ.
    let mut first: Vec<_> = named[..PALETTE.len()].iter().map(|e| e.color).collect();
    first.sort_by_key(|c| (c.0, c.1, c.2));
    first.dedup();
    assert_eq!(first.len(), PALETTE.len());
    // Tiles are painted like the legend, also when zoomed (the map is of
    // the whole tree).
    let map = ColorMap::new(&t, &by, Metric::Load);
    let scheme = by.scheme(&t, Metric::Load);
    for e in &named {
        assert_eq!(scheme.paint(&t, e.node).color, e.color, "{}", e.label);
        assert_eq!(map.paint(Some(&e.label)).color, e.color);
    }
    let sub = legend(&t, &by, named[3].node, Metric::Load, 100);
    assert_eq!(sub[0].color, named[3].color);
}
