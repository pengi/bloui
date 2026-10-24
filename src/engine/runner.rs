// SPDX-License-Identifier: GPL-2.0-only
//! Running the stages, with a per-stage cache.

use crate::model::{Metric, Sizes};
use crate::pipeline::{Pipeline, PipelineCtx, PipelineState, StageConfig};
use crate::registry::Registry;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StageStats {
    pub input: Sizes,
    pub output: Sizes,
    pub fragments_in: usize,
    pub fragments_out: usize,
}

impl StageStats {
    pub fn removed(&self) -> Sizes {
        self.input - self.output
    }

    pub fn added(&self) -> Sizes {
        self.output - self.input
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StageStatus {
    Ok,
    Disabled,
    /// Skipped: can't run on this input.
    Unavailable(String),
    /// Skipped: the stage failed.
    Failed(String),
}

#[derive(Clone, Debug)]
pub struct StageReport {
    pub status: StageStatus,
    pub stats: StageStats,
    /// Notes from the stage; warnings start with `warning: `.
    pub notes: Vec<String>,
    /// Files the stage read (or tried to): inputs, linker maps.
    pub files: Vec<PathBuf>,
    /// Whether the stage depends on the metric the result is shown in.
    pub uses_metric: bool,
}

pub struct RunResult {
    pub state: Arc<PipelineState>,
    pub reports: Vec<StageReport>,
}

impl RunResult {
    /// Every file the pipeline read, for watching.
    pub fn files(&self) -> Vec<PathBuf> {
        let mut files: Vec<PathBuf> = self
            .reports
            .iter()
            .flat_map(|r| r.files.iter().cloned())
            .collect();
        files.sort();
        files.dedup();
        files
    }

    pub fn errors(&self) -> impl Iterator<Item = (usize, &str)> {
        self.reports
            .iter()
            .enumerate()
            .filter_map(|(i, r)| match &r.status {
                StageStatus::Failed(e) | StageStatus::Unavailable(e) => Some((i, e.as_str())),
                _ => None,
            })
    }
}

struct Cached {
    config: StageConfig,
    /// The metric, if the stage used it.
    metric: Option<Metric>,
    state: Arc<PipelineState>,
    report: StageReport,
}

/// Runs a pipeline. Keeps each stage's output, so after an edit to stage
/// *k* only stages *k* and later run again.
pub struct Runner {
    pub ctx: PipelineCtx,
    cache: Vec<Cached>,
}

impl Runner {
    pub fn new(registry: Arc<Registry>) -> Self {
        Runner {
            ctx: PipelineCtx::new(registry),
            cache: Vec::new(),
        }
    }

    /// Forget cached stage outputs from stage `index` on.
    pub fn invalidate_from(&mut self, index: usize) {
        self.cache.truncate(index);
    }

    /// Forget all cached stage outputs and loaded files, e.g. after a
    /// rebuild changed an input.
    pub fn invalidate(&mut self) {
        self.cache.clear();
        self.ctx.forget_files();
    }

    pub fn run(&mut self, pipeline: &Pipeline) -> RunResult {
        let keep = self
            .cache
            .iter()
            .zip(&pipeline.stages)
            .take_while(|(c, s)| {
                c.config == **s && c.metric.is_none_or(|m| m == self.ctx.current_metric())
            })
            .count();
        self.cache.truncate(keep);
        let mut state = self
            .cache
            .last()
            .map_or_else(|| Arc::new(PipelineState::default()), |c| c.state.clone());
        for config in &pipeline.stages[keep..] {
            let (next, report) = self.run_stage(config, state);
            state = next;
            self.cache.push(Cached {
                config: config.clone(),
                metric: report.uses_metric.then(|| self.ctx.current_metric()),
                state: state.clone(),
                report,
            });
        }
        let live: std::collections::HashSet<u64> = self
            .cache
            .iter()
            .flat_map(|c| c.state.binaries.iter().map(|b| b.uid))
            .collect();
        self.ctx.retain_binaries(&live);
        RunResult {
            state,
            reports: self.cache.iter().map(|c| c.report.clone()).collect(),
        }
    }

    fn run_stage(
        &mut self,
        config: &StageConfig,
        input: Arc<PipelineState>,
    ) -> (Arc<PipelineState>, StageReport) {
        let in_total = input.current_total();
        let in_count = input.set.fragments.len();
        let pass = |status| StageReport {
            status,
            stats: StageStats {
                input: in_total,
                output: in_total,
                fragments_in: in_count,
                fragments_out: in_count,
            },
            notes: Vec::new(),
            files: Vec::new(),
            uses_metric: false,
        };
        if !config.enabled {
            return (input, pass(StageStatus::Disabled));
        }
        let stage = match self.ctx.registry.clone().create_stage(config) {
            Ok(s) => s,
            Err(e) => return (input, pass(StageStatus::Failed(e.to_string()))),
        };
        if let crate::model::Availability::Unavailable(why) = stage.check(&self.ctx, &input) {
            return (input, pass(StageStatus::Unavailable(why)));
        }
        let result = stage.apply(&mut self.ctx, (*input).clone());
        let notes = self.ctx.take_notes();
        let files = self.ctx.take_read_files();
        let uses_metric = self.ctx.take_metric_read();
        match result {
            Ok(out) => {
                let report = StageReport {
                    notes,
                    files,
                    uses_metric,
                    status: StageStatus::Ok,
                    stats: StageStats {
                        input: in_total,
                        output: out.current_total(),
                        fragments_in: in_count,
                        fragments_out: out.set.fragments.len(),
                    },
                };
                (Arc::new(out), report)
            }
            Err(e) => {
                let mut report = pass(StageStatus::Failed(e.to_string()));
                report.notes = notes;
                report.files = files;
                report.uses_metric = uses_metric;
                (input, report)
            }
        }
    }
}
