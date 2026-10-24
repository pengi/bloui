// SPDX-License-Identifier: GPL-2.0-only
use crate::model::{Availability, Label, Metric};
use crate::param::{LevelRef, ParamSpec, ParamValues};
use crate::pipeline::{PipelineCtx, PipelineState, Stage, StageFactory, StageKind};
use crate::registry::Registry;
use crate::{Error, Result};
use std::collections::{HashMap, HashSet};

pub struct GroupSmall;

impl StageFactory for GroupSmall {
    fn id(&self) -> &'static str {
        "group-small"
    }
    fn kind(&self) -> StageKind {
        StageKind::Transform
    }
    fn name(&self) -> &'static str {
        "Group small"
    }
    fn description(&self) -> &'static str {
        "Within each parent, merge a level's small labels into one, like [other]"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::level("level", "Level").help("Name or index; the last level if not set"),
            ParamSpec::size("min-size", "Minimum size", 0)
                .help("Group labels smaller than this; 0 for no limit"),
            ParamSpec::int("min-percent", "Minimum percent", 0, 100, 0)
                .help("Group labels smaller than this share of their parent; 0 for no limit"),
            ParamSpec::int("top", "Keep the biggest", 0, 1_000_000, 0)
                .help("Keep at most this many labels per parent; 0 for no limit"),
            ParamSpec::choice(
                "metric",
                "Sizes by",
                &[
                    ("view", "The metric shown"),
                    ("file", "File"),
                    ("memory", "Memory"),
                    ("load", "Load"),
                ],
            ),
            ParamSpec::text("label", "Group label", "[other]"),
        ]
    }
    fn summary(&self, p: &ParamValues, _registry: &Registry) -> String {
        let mut limits = Vec::new();
        if let Some(s) = p.opt_u64("min-size").filter(|&s| s > 0) {
            limits.push(format!("< {}", crate::model::human(s)));
        }
        if p.int("min-percent") > 0 {
            limits.push(format!("< {}%", p.int("min-percent")));
        }
        if p.int("top") > 0 {
            limits.push(format!("beyond top {}", p.int("top")));
        }
        let what = if limits.is_empty() {
            "nothing".to_owned()
        } else {
            limits.join(" or ")
        };
        format!(
            "group {} {what} as {}",
            p.level("level").summary(),
            p.text("label")
        )
    }
    fn create(&self, params: &ParamValues, _registry: &Registry) -> Result<Box<dyn Stage>> {
        let metric = match params.text("metric") {
            "view" => None,
            m => Some(Metric::from_name(m).ok_or_else(|| Error::param("metric", m))?),
        };
        Ok(Box::new(GroupStage {
            level: params.level("level"),
            min_size: params.opt_u64("min-size").unwrap_or(0),
            min_percent: params.int("min-percent") as u64,
            top: params.int("top") as usize,
            metric,
            label: params.text("label").to_owned(),
        }))
    }
}

struct GroupStage {
    level: LevelRef,
    min_size: u64,
    min_percent: u64,
    top: usize,
    /// `None`: the metric the result is shown in.
    metric: Option<Metric>,
    label: String,
}

impl Stage for GroupStage {
    fn check(&self, _ctx: &PipelineCtx, input: &PipelineState) -> Availability {
        match self.level.find(&input.set.levels) {
            Some(_) => Availability::Available,
            None => Availability::Unavailable("no such tree level".into()),
        }
    }

    fn apply(&self, ctx: &mut PipelineCtx, mut state: PipelineState) -> Result<PipelineState> {
        let level = self
            .level
            .find(&state.set.levels)
            .ok_or_else(|| Error::Other("no such tree level".into()))?;
        let metric = self.metric.unwrap_or_else(|| ctx.metric());
        // Sizes per parent (the labels above the level) and label.
        let mut sizes: HashMap<&[Label], HashMap<&Label, u64>> = HashMap::new();
        for f in &state.set.fragments {
            *sizes
                .entry(&f.labels[..level])
                .or_default()
                .entry(&f.labels[level])
                .or_default() += f.sizes.get(metric);
        }
        let free = ctx.interner.intern("[free]");
        let mut grouped: HashSet<(Vec<Label>, Label)> = HashSet::new();
        for (parent, children) in &sizes {
            let total: u64 = children.values().sum();
            let mut by_size: Vec<(&Label, u64)> = children.iter().map(|(l, s)| (*l, *s)).collect();
            by_size.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then_with(|| a.0.text(&ctx.interner).cmp(&b.0.text(&ctx.interner)))
            });
            let small: Vec<&Label> = by_size
                .iter()
                .enumerate()
                .filter(|&(rank, &(label, size))| {
                    label.parts() != [free]
                        && (size < self.min_size
                            || size * 100 < total * self.min_percent
                            || (self.top > 0 && rank >= self.top))
                })
                .map(|(_, &(label, _))| label)
                .collect();
            // Grouping a single label only hides its name.
            if small.len() > 1 {
                grouped.extend(small.into_iter().map(|l| (parent.to_vec(), l.clone())));
            }
        }
        if grouped.is_empty() {
            return Ok(state);
        }
        let other = Label::single(ctx.interner.intern(&self.label));
        let mut key = (Vec::new(), Label::default());
        for f in &mut state.set.fragments {
            key.0.clear();
            key.0.extend_from_slice(&f.labels[..level]);
            key.1 = f.labels[level].clone();
            if grouped.contains(&key) {
                f.labels[level] = other.clone();
            }
        }
        ctx.note(format!(
            "{} labels grouped as {}",
            grouped.len(),
            self.label
        ));
        Ok(state)
    }
}
