// SPDX-License-Identifier: GPL-2.0-only
//! Pipeline and engine invariants on the fixtures.

mod common;
use bloui::model::{Metric, Sizes};
use bloui::param::ParamValue;
use common::*;

fn total(stages: Vec<bloui::pipeline::StageConfig>) -> Sizes {
    tree(stages, Metric::Memory).root().sizes
}

#[test]
fn split_order_keeps_totals() {
    for fixture in [FW, HOST] {
        let base = total(vec![open(fixture)]);
        assert_eq!(base.file, read(fixture).data.len() as u64);
        let mut dims = vec!["file", "segment", "section", "category", "symbol"];
        if fixture == FW {
            // Need the linker map.
            dims.extend(["memory-region", "object-file", "input-section"]);
        }
        for &a in &dims {
            for &b in &dims {
                let t = total(vec![open(fixture), split(a), split(b)]);
                assert_eq!(t, base, "{fixture}: {a} then {b}");
            }
        }
    }
}

#[test]
fn children_sum_to_parent() {
    let t = tree(
        vec![open(FW), split("section"), split("symbol")],
        Metric::Load,
    );
    for (i, n) in t.nodes.iter().enumerate() {
        if n.children.is_empty() {
            continue;
        }
        let sum = n
            .children
            .iter()
            .fold(Sizes::default(), |a, &c| a + t.node(c).sizes);
        assert_eq!(sum, n.sizes, "node {i} {}", n.name);
    }
}

#[test]
fn filter_stats_match() {
    let stages = vec![
        open(FW),
        stage(
            "filter-label",
            &[
                ("dimension", text("section")),
                ("patterns", ParamValue::List(vec![r"^\.debug".into()])),
            ],
        ),
        split("section"),
    ];
    let (result, _) = run(stages);
    let r = &result.reports[1];
    let debug = read(FW)
        .sections
        .iter()
        .filter(|s| s.name.starts_with(".debug"))
        .map(|s| s.size)
        .sum::<u64>();
    assert_eq!(r.stats.removed().file, debug);
    assert_eq!(r.stats.input - r.stats.output, r.stats.removed());
    assert_eq!(result.state.set.total(), r.stats.output);
}

#[test]
fn include_by_level() {
    let t = tree(
        vec![
            open(FW),
            split("section"),
            stage(
                "filter-label",
                &[
                    ("source", text("level")),
                    ("mode", text("include")),
                    ("match", text("exact")),
                    (
                        "patterns",
                        ParamValue::List(vec![".text".into(), ".data".into()]),
                    ),
                ],
            ),
        ],
        Metric::Memory,
    );
    let names: Vec<_> = t
        .children(bloui::engine::AnalysisTree::ROOT)
        .iter()
        .map(|&c| &*t.node(c).name)
        .collect();
    assert_eq!(names, [".text", ".data"]);
}

#[test]
fn flash_and_ram_by_space() {
    // Everything loaded (flash image), in sections.
    let t = tree(
        vec![
            open(FW),
            stage("filter-space", &[("keep", text("load"))]),
            split("section"),
        ],
        Metric::Load,
    );
    assert_eq!(t.root().sizes.load, 0x67c + 8);
    assert!(find(&t, &[".bss"]).is_none());
    assert_eq!(find(&t, &[".data"]).unwrap().sizes.load, 8);

    // RAM by address range.
    let t = tree(
        vec![
            open(FW),
            stage(
                "filter-space",
                &[
                    ("space", text("vma")),
                    ("start", text("0x20000000")),
                    ("end", text("0x20004000")),
                ],
            ),
            split("section"),
        ],
        Metric::Memory,
    );
    assert_eq!(t.root().sizes.memory, 0x1a4 + 0x404);
    // .data by its runtime address, with its content.
    assert_eq!(t.root().sizes.load, 8);

    // Flash by load address includes .data's initial values.
    let t = tree(
        vec![
            open(FW),
            stage(
                "filter-space",
                &[
                    ("space", text("lma")),
                    ("start", text("0x08000000")),
                    ("end", text("0x08010000")),
                ],
            ),
            split("section"),
        ],
        Metric::Load,
    );
    assert_eq!(t.root().sizes.load, 0x67c + 8);
}

#[test]
fn symbols_with_fallback() {
    let t = tree(
        vec![open(FW), split("section"), split("symbol")],
        Metric::Memory,
    );
    assert_eq!(find(&t, &[".text", "main"]).unwrap().sizes.memory, 0x88);
    // Weak aliases of Default_Handler: one name wins, stably.
    assert!(find(&t, &[".text", "HardFault_Handler"]).is_some());
    assert!(find(&t, &[".text", "[.text]"]).is_some());
    assert_eq!(
        find(&t, &[".stack", "[.stack]"]).unwrap().sizes.memory,
        0x404
    );
    assert_eq!(find(&t, &[".bss", "scratch"]).unwrap().sizes.memory, 256);
    assert!(find(&t, &[".text", "main"]).unwrap().vma == Some(0x0800_00f4));
}

#[test]
fn two_inputs() {
    let t = tree(vec![open(FW), open(HOST), split("file")], Metric::File);
    assert_eq!(t.root().children.len(), 2);
    assert_eq!(
        t.root().sizes.file,
        read(FW).data.len() as u64 + read(HOST).data.len() as u64
    );
}

#[test]
fn reruns_use_the_cache() {
    use bloui::pipeline::Pipeline;
    let mut runner =
        bloui::engine::Runner::new(std::sync::Arc::new(bloui::registry::Registry::builtin()));
    let mut p = Pipeline {
        stages: vec![open(FW), split("section")],
    };
    let a = runner.run(&p);
    p.stages.push(split("symbol"));
    let b = runner.run(&p);
    // The first two stages' outputs are reused.
    assert!(std::sync::Arc::ptr_eq(
        &a.state,
        &runner
            .run(&Pipeline {
                stages: p.stages[..2].to_vec()
            })
            .state
    ));
    assert_eq!(b.state.set.levels.len(), 2);
}

#[test]
fn bad_stage_is_reported() {
    let mut runner =
        bloui::engine::Runner::new(std::sync::Arc::new(bloui::registry::Registry::builtin()));
    let p = bloui::pipeline::Pipeline {
        stages: vec![
            open(FW),
            split("no-such-dimension"),
            stage("split", &[("bogus", text("1"))]),
        ],
    };
    let r = runner.run(&p);
    assert_eq!(r.errors().count(), 2);
    assert_eq!(r.state.set.levels.len(), 0);
}

#[test]
fn stage_summaries() {
    let reg = bloui::registry::Registry::builtin();
    let sum = |s: &bloui::pipeline::StageConfig| bloui::pipeline::summary(s, &reg);
    assert_eq!(sum(&open(FW)), "open fw.elf");
    assert_eq!(sum(&split("section")), "split by section");
    let sym = ParamValue::Map(
        bloui::param::ParamValues::new()
            .with("id", text("symbol"))
            .with("fallback", text("none")),
    );
    assert_eq!(
        sum(&stage("split", &[("dimension", sym)])),
        "split by symbol (fallback=none)"
    );
    let excl = stage(
        "filter-label",
        &[("patterns", ParamValue::List(vec!["^\\.debug".into()]))],
    );
    assert_eq!(sum(&excl), "exclude section ~ ^\\.debug");
    assert_eq!(
        sum(&stage("filter-space", &[("keep", text("load"))])),
        "keep load"
    );
    assert_eq!(
        sum(&stage(
            "memory-regions",
            &[("regions", text("FLASH:0:64K"))]
        )),
        "regions FLASH"
    );
    assert_eq!(sum(&stage("free-space", &[])), "free space");
    assert_eq!(
        sum(&stage(
            "relabel",
            &[(
                "rules",
                ParamValue::List(vec!["^a=>b".into(), "c=>d".into()])
            )]
        )),
        "relabel last level: ^a → b, +1 more"
    );
    assert_eq!(sum(&stage("nope", &[])), "nope");
    assert_eq!(sum(&stage("split", &[("x", text("y"))])), "split (invalid)");
}
