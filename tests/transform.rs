// SPDX-License-Identifier: GPL-2.0-only
//! The group-small and flatten stages, and stages that follow the metric.

mod common;
use bloui::engine::{AnalysisTree, Runner};
use bloui::model::{Metric, Sizes};
use bloui::param::ParamValue;
use bloui::pipeline::Pipeline;
use bloui::registry::Registry;
use common::*;
use std::sync::Arc;

const RUST: &str = "rust/fw.elf";

fn group(params: &[(&str, &str)]) -> bloui::pipeline::StageConfig {
    let params: Vec<_> = params.iter().map(|(k, v)| (*k, text(v))).collect();
    stage("group-small", &params)
}

fn children(t: &AnalysisTree, path: &[&str]) -> Vec<(String, Sizes)> {
    t.children(find_id(t, path))
        .iter()
        .map(|&c| (t.node(c).name.to_string(), t.node(c).sizes))
        .collect()
}

#[test]
fn top_n_keeps_totals() {
    let plain = tree(vec![open(RUST), split("symbol")], Metric::Load);
    let t = tree(
        vec![
            open(RUST),
            split("symbol"),
            group(&[("top", "5"), ("metric", "load")]),
        ],
        Metric::Load,
    );
    assert_eq!(t.root().sizes, plain.root().sizes);
    let kids = children(&t, &[]);
    assert_eq!(kids.len(), 6, "{kids:?}");
    // The five biggest stay; the rest add up to [other].
    let mut sizes: Vec<u64> = plain
        .children(AnalysisTree::ROOT)
        .iter()
        .map(|&c| plain.node(c).sizes.load)
        .collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    let rest: u64 = sizes[5..].iter().sum();
    assert_eq!(find(&t, &["[other]"]).map(|n| n.sizes.load), Some(rest));
}

#[test]
fn groups_per_parent_and_spares_free_space() {
    let t = tree(
        vec![
            open(FW),
            split("memory-region"),
            stage("free-space", &[]),
            split("symbol"),
            group(&[("min-size", "100"), ("metric", "memory")]),
        ],
        Metric::Memory,
    );
    for region in ["FLASH", "RAM"] {
        let kids = children(&t, &[region]);
        assert!(kids.iter().any(|(n, _)| n == "[free]"), "{region}");
        for (name, sizes) in &kids {
            if name != "[other]" && name != "[free]" {
                assert!(sizes.memory >= 100, "{region}/{name}");
            }
        }
    }
    // In RAM, scratch (256) and the stack stay.
    assert!(find(&t, &["RAM", "scratch"]).is_some());
    assert!(find(&t, &["RAM", "[other]"]).is_some());
}

#[test]
fn a_single_small_label_is_not_grouped() {
    // In flash, only 2 bytes have no object (fill): nothing to merge it with.
    let t = tree(
        vec![
            open(FW),
            stage("filter-space", &[("keep", text("load"))]),
            split("memory-region"),
            group(&[("min-size", "1"), ("metric", "load")]),
        ],
        Metric::Load,
    );
    assert!(find(&t, &["[other]"]).is_none());
    assert!(find(&t, &["FLASH"]).is_some());
}

#[test]
fn view_metric_reruns_on_change() {
    let mut runner = Runner::new(Arc::new(Registry::builtin()));
    let p = Pipeline {
        stages: vec![open(FW), split("section"), group(&[("min-size", "1")])],
    };
    let head = Pipeline {
        stages: p.stages[..2].to_vec(),
    };
    // By file size, .bss (no file bytes) is small; by memory it isn't.
    runner.ctx.set_metric(Metric::File);
    let split_state = runner.run(&head).state;
    let by_file = runner.run(&p);
    assert!(by_file.reports[2].uses_metric);
    assert!(!by_file.reports[1].uses_metric);
    let t = AnalysisTree::build(&by_file.state, &runner.ctx.interner, Metric::File);
    assert!(find(&t, &[".bss"]).is_none());
    runner.ctx.set_metric(Metric::Memory);
    let by_memory = runner.run(&p);
    let t = AnalysisTree::build(&by_memory.state, &runner.ctx.interner, Metric::Memory);
    assert!(find(&t, &[".bss"]).is_some());
    // The stages before it don't depend on the metric: still cached.
    assert!(Arc::ptr_eq(&runner.run(&head).state, &split_state));
}

#[test]
fn flatten_drops_a_level() {
    let t = tree(
        vec![
            open(FW),
            split("section"),
            split("object-file"),
            stage("flatten", &[("level", text("section"))]),
        ],
        Metric::Load,
    );
    assert_eq!(t.levels.len(), 1);
    assert_eq!(t.levels[0].name, "object file");
    assert!(find(&t, &["libhal.a", "gpio.o"]).is_some());
    let plain = tree(vec![open(FW), split("object-file")], Metric::Load);
    assert_eq!(t.root().sizes, plain.root().sizes);
}

#[test]
fn flatten_cuts_depth_also_for_later_inputs() {
    let flat = |rest: Vec<bloui::pipeline::StageConfig>| {
        let mut stages = vec![
            open(FW),
            split("object-file"),
            stage(
                "flatten",
                &[("mode", text("depth")), ("depth", ParamValue::Int(1))],
            ),
        ];
        stages.extend(rest);
        tree(stages, Metric::Load)
    };
    let t = flat(vec![]);
    assert_eq!(find(&t, &["libhal.a"]).map(|n| n.children.len()), Some(0));
    // A second input is labeled by the level's (now cut) dimension.
    let second = stage(
        "open-file",
        &[
            ("path", text(&fixture(FW).to_string_lossy())),
            ("name", text("again")),
        ],
    );
    let t = flat(vec![second]);
    let hal = find(&t, &["libhal.a"]).unwrap();
    assert_eq!(hal.children.len(), 0);
    assert_eq!(hal.fragments % 2, 0);
}
