// SPDX-License-Identifier: GPL-2.0-only
#![allow(dead_code)]

use bloui::engine::{AnalysisTree, RunResult, Runner};
use bloui::model::{Binary, Metric};
use bloui::param::{ParamValue, ParamValues};
use bloui::pipeline::{Pipeline, StageConfig};
use bloui::registry::Registry;
use std::path::PathBuf;
use std::sync::Arc;

pub fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel)
}

pub const FW: &str = "cortexm/fw.elf";
pub const HOST: &str = "host/hello.elf";

pub fn read(rel: &str) -> Binary {
    let reg = Registry::builtin();
    bloui::reader::read_file(&reg.readers, &fixture(rel), None, rel.into()).unwrap()
}

pub fn open(rel: &str) -> StageConfig {
    let path = fixture(rel).to_string_lossy().into_owned();
    StageConfig::new(
        "open-file",
        ParamValues::new().with("path", ParamValue::Text(path)),
    )
}

pub fn split(dim: &str) -> StageConfig {
    StageConfig::new(
        "split",
        ParamValues::new().with("dimension", ParamValue::Text(dim.into())),
    )
}

pub fn stage(id: &str, params: &[(&str, ParamValue)]) -> StageConfig {
    let mut v = ParamValues::new();
    for (k, val) in params {
        v.set(k, val.clone());
    }
    StageConfig::new(id, v)
}

pub fn text(s: &str) -> ParamValue {
    ParamValue::Text(s.into())
}

pub fn run(stages: Vec<StageConfig>) -> (RunResult, Runner) {
    let mut runner = Runner::new(Arc::new(Registry::builtin()));
    let result = runner.run(&Pipeline { stages });
    if let Some((i, e)) = result.errors().next() {
        panic!("stage {i}: {e}");
    }
    (result, runner)
}

pub fn tree(stages: Vec<StageConfig>, metric: Metric) -> AnalysisTree {
    let (result, runner) = run(stages);
    AnalysisTree::build(&result.state, &runner.ctx.interner, metric)
}

/// Child of the root (or deeper, by path) with this name.
pub fn find<'a>(tree: &'a AnalysisTree, path: &[&str]) -> Option<&'a bloui::engine::Node> {
    let mut cur = AnalysisTree::ROOT;
    for name in path {
        cur = *tree
            .children(cur)
            .iter()
            .find(|&&c| &*tree.node(c).name == *name)?;
    }
    Some(tree.node(cur))
}
