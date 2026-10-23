// SPDX-License-Identifier: GPL-2.0-only
use crate::Result;
use crate::model::{Availability, LevelInfo};
use crate::param::{ParamSpec, ParamValues};
use crate::pipeline::{PipelineCtx, PipelineState, Stage, StageFactory, StageKind};
use crate::registry::{DimHandle, Registry};

pub struct Split;

impl StageFactory for Split {
    fn id(&self) -> &'static str {
        "split"
    }
    fn kind(&self) -> StageKind {
        StageKind::Split
    }
    fn name(&self) -> &'static str {
        "Split"
    }
    fn description(&self) -> &'static str {
        "Cut the bytes by a dimension, and add a tree level"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::dimension("dimension", "Dimension", "section"),
            ParamSpec::text("name", "Level name", "").help("The dimension's name if empty"),
        ]
    }
    fn create(&self, params: &ParamValues, registry: &Registry) -> Result<Box<dyn Stage>> {
        let dim = registry.create_dimension(&params.dimension("dimension"))?;
        let name = match params.text("name") {
            "" => dim.name.to_owned(),
            n => n.to_owned(),
        };
        Ok(Box::new(SplitStage { dim, name }))
    }
}

struct SplitStage {
    dim: DimHandle,
    name: String,
}

impl Stage for SplitStage {
    fn check(&self, ctx: &PipelineCtx, input: &PipelineState) -> Availability {
        self.dim.availability(ctx, input)
    }

    fn apply(&self, ctx: &mut PipelineCtx, mut state: PipelineState) -> Result<PipelineState> {
        let fragments = std::mem::take(&mut state.set.fragments);
        let mut out = Vec::with_capacity(fragments.len());
        ctx.cut_all(
            &state,
            &fragments,
            &self.dim.key,
            &*self.dim.dim,
            |mut f, label, _| {
                f.labels.push(label.clone());
                out.push(f);
            },
        )?;
        state.set.fragments = out;
        state.set.levels.push(LevelInfo {
            name: self.name.clone(),
            dimension: self.dim.id.clone(),
        });
        Ok(state)
    }
}
