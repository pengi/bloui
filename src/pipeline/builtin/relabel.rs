// SPDX-License-Identifier: GPL-2.0-only
use crate::model::Availability;
use crate::param::{DimRef, LevelRef, ParamSpec, ParamValue, ParamValues};
use crate::pipeline::{PipelineCtx, PipelineState, Stage, StageFactory, StageKind};
use crate::registry::Registry;
use crate::rules::{Relabeler, Rules};
use crate::{Error, Result};

pub struct Relabel;

impl StageFactory for Relabel {
    fn id(&self) -> &'static str {
        "relabel"
    }
    fn kind(&self) -> StageKind {
        StageKind::Transform
    }
    fn name(&self) -> &'static str {
        "Relabel"
    }
    fn description(&self) -> &'static str {
        "Rename a tree level's labels by regex rules; labels no rule matches stay"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::level("level", "Level").help("Name or index; the last level if not set"),
            Rules::param("rules", "Rules"),
        ]
    }
    fn summary(&self, p: &ParamValues, _registry: &Registry) -> String {
        let rules = p.table("rules");
        let first = rules
            .first()
            .map(|r| format!("{} → {}", r.text("pattern"), r.text("label")));
        match (first, rules.len()) {
            (None, _) => format!("relabel {} (no rules)", p.level("level").summary()),
            (Some(r), 1) => format!("relabel {}: {r}", p.level("level").summary()),
            (Some(r), n) => format!(
                "relabel {}: {r}, +{} more",
                p.level("level").summary(),
                n - 1
            ),
        }
    }
    fn create(&self, params: &ParamValues, _registry: &Registry) -> Result<Box<dyn Stage>> {
        Ok(Box::new(RelabelStage {
            level: params.level("level"),
            rules: Rules::from_table("rules", params.table("rules"))?,
            rows: params
                .get("rules")
                .cloned()
                .unwrap_or(ParamValue::Table(Vec::new())),
        }))
    }
}

struct RelabelStage {
    level: LevelRef,
    rules: Rules,
    /// The rules as a parameter value, for the level's new dimension.
    rows: ParamValue,
}

impl Stage for RelabelStage {
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
        let mut relabeler = Relabeler::new(&self.rules, None);
        for f in &mut state.set.fragments {
            f.labels[level] = relabeler.relabel(&f.labels[level], &mut ctx.interner);
        }
        // Later inputs and free space get their labels from the level's
        // dimension: make it the same rules over the old one.
        let dim = &mut state.set.levels[level].dim;
        let params = ParamValues::new()
            .with("dimension", dim.to_value())
            .with("rules", self.rows.clone());
        *dim = DimRef {
            id: "rules".into(),
            params,
        };
        Ok(state)
    }
}
