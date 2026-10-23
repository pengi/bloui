// SPDX-License-Identifier: GPL-2.0-only
//! Runs pipelines off the GUI thread.

use bloui::engine::{AnalysisTree, Runner, StageReport};
use bloui::model::Metric;
use bloui::pipeline::Pipeline;
use bloui::registry::Registry;
use gtk::gio;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct Output {
    pub tree: Arc<AnalysisTree>,
    pub reports: Vec<StageReport>,
}

/// Owns the `Runner`, so its per-stage cache survives between runs.
#[derive(Clone)]
pub struct Worker {
    runner: Arc<Mutex<Runner>>,
}

impl Worker {
    pub fn new() -> Self {
        let registry = Arc::new(Registry::builtin());
        Worker {
            runner: Arc::new(Mutex::new(Runner::new(registry))),
        }
    }

    /// Run `pipeline` on a worker thread. With `reload`, forget cached
    /// stage outputs and files first.
    pub async fn run(
        &self,
        pipeline: Pipeline,
        base_dir: PathBuf,
        metric: Metric,
        reload: bool,
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
            Output {
                tree: Arc::new(tree),
                reports: result.reports,
            }
        })
        .await
        .expect("pipeline worker panicked")
    }
}
