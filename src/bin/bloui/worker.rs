// SPDX-License-Identifier: GPL-2.0-only
//! Runs pipelines off the GUI thread.

use bloui::engine::{AnalysisTree, ColorLabels, Runner, StageReport};
use bloui::model::Metric;
use bloui::param::DimRef;
use bloui::pipeline::Pipeline;
use bloui::registry::Registry;
use gtk::gio;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct Output {
    pub tree: Arc<AnalysisTree>,
    pub reports: Vec<StageReport>,
    /// Labels of the dimension to color by, if one was asked for.
    pub colors: Option<Result<Arc<ColorLabels>, String>>,
    /// The files the pipeline read.
    pub files: Vec<PathBuf>,
}

/// Owns the `Runner`, so its per-stage cache survives between runs.
#[derive(Clone)]
pub struct Worker {
    runner: Arc<Mutex<Runner>>,
}

impl Worker {
    pub fn new(registry: Arc<Registry>) -> Self {
        Worker {
            runner: Arc::new(Mutex::new(Runner::new(registry))),
        }
    }

    /// Run `pipeline` on a worker thread, and label the result by `color`
    /// if given. With `reload`, forget cached stage outputs and files
    /// first.
    pub async fn run(
        &self,
        pipeline: Pipeline,
        base_dir: PathBuf,
        metric: Metric,
        reload: bool,
        color: Option<DimRef>,
    ) -> Output {
        let runner = self.runner.clone();
        gio::spawn_blocking(move || {
            let mut runner = runner.lock().unwrap_or_else(|e| e.into_inner());
            if reload || runner.ctx.base_dir != base_dir {
                runner.invalidate();
            }
            runner.ctx.base_dir = base_dir;
            let result = runner.run(&pipeline);
            let tree = AnalysisTree::build(&result.state, &runner.ctx.interner, metric);
            let colors = color.map(|dim| {
                ColorLabels::compute(&mut runner.ctx, &result.state, &tree, &dim)
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            });
            Output {
                tree: Arc::new(tree),
                files: result.files(),
                reports: result.reports,
                colors,
            }
        })
        .await
        .expect("pipeline worker panicked")
    }
}
