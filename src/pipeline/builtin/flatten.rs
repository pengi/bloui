// SPDX-License-Identifier: GPL-2.0-only
use crate::model::{Availability, Label};
use crate::param::{DimRef, LevelRef, ParamSpec, ParamValue, ParamValues};
use crate::pipeline::{PipelineCtx, PipelineState, Stage, StageFactory, StageKind};
use crate::registry::Registry;
use crate::{Error, Result};

pub struct Flatten;

impl StageFactory for Flatten {
    fn id(&self) -> &'static str {
        "flatten"
    }
    fn kind(&self) -> StageKind {
        StageKind::Transform
    }
    fn name(&self) -> &'static str {
        "Flatten"
    }
    fn description(&self) -> &'static str {
        "Drop a tree level, or cut its path labels to a depth"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::level("level", "Level").help("Name or index; the last level if not set"),
            ParamSpec::choice(
                "mode",
                "Mode",
                &[
                    ("drop", "Drop the level"),
                    ("depth", "Cut its labels to a depth"),
                ],
            ),
            ParamSpec::int("depth", "Depth", 1, 64, 1)
                .help("Path parts to keep: 1 keeps libhal.a of libhal.a / gpio.o")
                .visible_when("mode", "depth"),
        ]
    }
    fn summary(&self, p: &ParamValues, _registry: &Registry) -> String {
        let level = p.level("level").summary();
        match p.text("mode") {
            "depth" => format!("cut {level} to depth {}", p.int("depth")),
            _ => format!("drop {level}"),
        }
    }
    fn create(&self, params: &ParamValues, _registry: &Registry) -> Result<Box<dyn Stage>> {
        Ok(Box::new(FlattenStage {
            level: params.level("level"),
            depth: (params.text("mode") == "depth").then(|| params.int("depth") as usize),
        }))
    }
}

struct FlattenStage {
    level: LevelRef,
    /// `None`: drop the level.
    depth: Option<usize>,
}

impl Stage for FlattenStage {
    fn check(&self, _ctx: &PipelineCtx, input: &PipelineState) -> Availability {
        match self.level.find(&input.set.levels) {
            Some(_) => Availability::Available,
            None => Availability::Unavailable("no such tree level".into()),
        }
    }

    fn apply(&self, _ctx: &mut PipelineCtx, mut state: PipelineState) -> Result<PipelineState> {
        let level = self
            .level
            .find(&state.set.levels)
            .ok_or_else(|| Error::Other("no such tree level".into()))?;
        let Some(depth) = self.depth else {
            for f in &mut state.set.fragments {
                f.labels.remove(level);
            }
            state.set.levels.remove(level);
            return Ok(state);
        };
        for f in &mut state.set.fragments {
            let parts = f.labels[level].parts();
            if parts.len() > depth {
                f.labels[level] = Label::from_path(parts[..depth].iter().copied());
            }
        }
        // Later inputs and free space get their labels from the level's
        // dimension: cut those the same way, with a rule on the path text.
        let dim = &mut state.set.levels[level].dim;
        let rule = format!("^((?:[^/]*/){{0,{}}}[^/]*)=>$1", depth - 1);
        let params = ParamValues::new()
            .with("dimension", dim.to_value())
            .with("rules", ParamValue::List(vec![rule]));
        *dim = DimRef {
            id: "rules".into(),
            params,
        };
        Ok(state)
    }
}
