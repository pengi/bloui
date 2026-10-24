// SPDX-License-Identifier: GPL-2.0-only
//! The rules dimension and the relabel stage.

mod common;
use bloui::engine::AnalysisTree;
use bloui::model::Metric;
use bloui::param::{ParamValue, ParamValues};
use common::*;

fn rule_rows(rules: &[&str]) -> ParamValue {
    ParamValue::List(rules.iter().map(|r| (*r).to_owned()).collect())
}

fn rules_dim(source: ParamValue, rules: &[&str], unmatched: &str) -> ParamValue {
    ParamValue::Map(
        ParamValues::new()
            .with("id", text("rules"))
            .with("dimension", source)
            .with("rules", rule_rows(rules))
            .with("unmatched", text(unmatched)),
    )
}

#[test]
fn rules_over_symbol_paths() {
    let source = ParamValue::Map(
        ParamValues::new()
            .with("id", text("symbol-path"))
            .with("depth", text("2")),
    );
    let dim = rules_dim(
        source,
        &[
            "^hw/uart=>drivers/uart",
            "^hw/RingBuffer=>containers",
            "^(app)=>$1",
        ],
        "other",
    );
    let t = tree(
        vec![open("cxx/fw.elf"), stage("split", &[("dimension", dim)])],
        Metric::Load,
    );
    let load = |p: &[&str]| find(&t, p).map(|n| n.sizes.load);
    assert_eq!(load(&["drivers", "uart"]), Some(6 + 0x10 + 8));
    assert_eq!(load(&["containers"]), Some(0x16 + 0x18 + 2 * 0x1e));
    assert_eq!(load(&["app"]), Some(0x18 + 0x1a));
    // The rest, fallbacks included.
    let all = tree(vec![open("cxx/fw.elf"), split("symbol")], Metric::Load);
    let total = all.node(AnalysisTree::ROOT).sizes.load;
    assert_eq!(t.node(AnalysisTree::ROOT).sizes.load, total);
    assert_eq!(
        load(&["[other]"]),
        Some(
            total
                - load(&["drivers"]).unwrap()
                - load(&["containers"]).unwrap()
                - load(&["app"]).unwrap()
        )
    );
}

#[test]
fn rules_keep_unmatched_and_need_a_map() {
    let dim = rules_dim(text("object-file"), &[r"^libhal\.a/(.*)=>hal/$1"], "keep");
    let t = tree(
        vec![open(FW), stage("split", &[("dimension", dim.clone())])],
        Metric::Load,
    );
    assert!(find(&t, &["hal", "gpio.o"]).is_some());
    assert!(find(&t, &["main.o"]).is_some());
    // Like object-file, unavailable without a linker map: the split is
    // skipped.
    let mut runner =
        bloui::engine::Runner::new(std::sync::Arc::new(bloui::registry::Registry::builtin()));
    let result = runner.run(&bloui::pipeline::Pipeline {
        stages: vec![open(HOST), stage("split", &[("dimension", dim)])],
    });
    let errors: Vec<_> = result.errors().map(|(i, e)| (i, e.to_string())).collect();
    assert_eq!(errors, [(1, "needs a linker map".to_owned())]);
    assert!(result.state.set.levels.is_empty());
}

#[test]
fn relabel_applies_to_later_free_space() {
    let t = tree(
        vec![
            open(FW),
            split("memory-region"),
            stage(
                "relabel",
                &[("rules", rule_rows(&["^FLASH$=>flash", "^RAM$=>ram"]))],
            ),
            stage("free-space", &[]),
        ],
        Metric::Memory,
    );
    let plain = tree(
        vec![open(FW), split("memory-region"), stage("free-space", &[])],
        Metric::Memory,
    );
    let memory = |t: &AnalysisTree, name| find(t, &[name]).map(|n| n.sizes.memory);
    assert_eq!(memory(&t, "flash"), Some(64 * 1024));
    assert_eq!(memory(&t, "ram"), memory(&plain, "RAM"));
    assert!(find(&t, &["FLASH"]).is_none());
    assert!(find(&t, &["[no region]"]).is_some());
}

#[test]
fn relabel_by_level_name() {
    let t = tree(
        vec![
            open(FW),
            split("section"),
            split("symbol"),
            stage(
                "relabel",
                &[
                    ("level", text("section")),
                    ("rules", rule_rows(&[r"^\.(text|rodata)$=>code/$1"])),
                ],
            ),
        ],
        Metric::Load,
    );
    assert!(find(&t, &["code", "text", "main"]).is_some());
    assert!(find(&t, &[".data"]).is_some());
}

#[test]
fn bad_rules_are_errors() {
    let mut runner =
        bloui::engine::Runner::new(std::sync::Arc::new(bloui::registry::Registry::builtin()));
    let result = runner.run(&bloui::pipeline::Pipeline {
        stages: vec![
            open(FW),
            split("section"),
            stage("relabel", &[("rules", rule_rows(&["(=>x"]))]),
        ],
    });
    assert!(result.errors().any(|(i, _)| i == 2));
    let result = runner.run(&bloui::pipeline::Pipeline {
        stages: vec![
            open(FW),
            split("section"),
            stage("relabel", &[("rules", rule_rows(&["a=>b=>c"]))]),
        ],
    });
    assert!(result.errors().any(|(i, _)| i == 2));
}
