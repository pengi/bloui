// SPDX-License-Identifier: GPL-2.0-only
use crate::Result;
use crate::dimension::{Dimension, DimensionFactory, Labeling};
use crate::model::{Availability, Binary, Interner, Label};
use crate::param::{ParamSpec, ParamValues};
use crate::registry::{DimHandle, Registry};
use crate::rules::{Relabeler, Rules};

pub struct RulesDim;

impl DimensionFactory for RulesDim {
    fn id(&self) -> &'static str {
        "rules"
    }
    fn name(&self) -> &'static str {
        "rules"
    }
    fn description(&self) -> &'static str {
        "Another dimension's labels, renamed or grouped by regex rules"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::dimension("dimension", "Labels from", "symbol"),
            Rules::param("rules", "Rules"),
            ParamSpec::choice(
                "unmatched",
                "Labels no rule matches",
                &[("keep", "Keep them"), ("other", "Group them as [other]")],
            ),
        ]
    }
    fn create(&self, params: &ParamValues, registry: &Registry) -> Result<Box<dyn Dimension>> {
        Ok(Box::new(RulesLabels {
            source: registry.create_dimension(&params.dimension("dimension"))?,
            rules: Rules::from_table("rules", params.table("rules"))?,
            group_unmatched: params.text("unmatched") == "other",
        }))
    }
}

struct RulesLabels {
    source: DimHandle,
    rules: Rules,
    group_unmatched: bool,
}

impl Dimension for RulesLabels {
    fn label(&self, bin: &Binary, interner: &mut Interner) -> Result<Labeling> {
        let mut l = self.source.dim.label(bin, interner)?;
        let unmatched = self
            .group_unmatched
            .then(|| Label::single(interner.intern("[other]")));
        let mut relabeler = Relabeler::new(&self.rules, unmatched);
        for e in &mut l.entries {
            e.label = relabeler.relabel(&e.label, interner);
        }
        l.fallback = relabeler.relabel(&l.fallback, interner);
        Ok(l)
    }

    fn availability(&self, bin: &Binary) -> Availability {
        self.source.dim.availability(bin)
    }

    fn labels_free_space(&self) -> bool {
        self.source.dim.labels_free_space()
    }
}
