// SPDX-License-Identifier: GPL-2.0-only
//! Lists of all readers, dimensions and stages.

use crate::dimension::{Dimension, DimensionFactory};
use crate::model::Availability;
use crate::param::DimRef;
use crate::pipeline::{PipelineState, Stage, StageConfig, StageFactory};
use crate::reader::Reader;
use crate::{Error, Result};

pub struct Registry {
    pub readers: Vec<Box<dyn Reader>>,
    pub dimensions: Vec<Box<dyn DimensionFactory>>,
    pub stages: Vec<Box<dyn StageFactory>>,
}

/// A created dimension, with its cache key.
pub struct DimHandle {
    pub id: String,
    /// The dimension with its resolved parameters.
    pub dim_ref: DimRef,
    pub name: &'static str,
    /// Dimension id plus resolved parameters.
    pub key: String,
    pub dim: Box<dyn Dimension>,
}

impl DimHandle {
    /// Available when it is for at least one input binary.
    pub fn availability(&self, input: &PipelineState) -> Availability {
        let mut why = None;
        for bin in &input.binaries {
            match self.dim.availability(bin) {
                Availability::Available => return Availability::Available,
                Availability::Unavailable(w) => why = Some(w),
            }
        }
        match why {
            Some(w) => Availability::Unavailable(w),
            None => Availability::Available,
        }
    }
}

impl Registry {
    pub fn builtin() -> Self {
        Registry {
            readers: vec![Box::new(crate::reader::elf::ElfReader)],
            dimensions: crate::dimension::builtin::all(),
            stages: crate::pipeline::builtin::all(),
        }
    }

    pub fn dimension(&self, id: &str) -> Option<&dyn DimensionFactory> {
        self.dimensions.iter().find(|d| d.id() == id).map(|d| &**d)
    }

    pub fn stage(&self, id: &str) -> Option<&dyn StageFactory> {
        self.stages.iter().find(|s| s.id() == id).map(|s| &**s)
    }

    /// A dimension with the parameters that differ from their defaults:
    /// `symbol (fallback=none)`.
    pub fn dimension_summary(&self, r: &DimRef) -> String {
        let Some(factory) = self.dimension(&r.id) else {
            return r.id.clone();
        };
        let specs = factory.params();
        let Ok(params) = r.params.resolve(&specs) else {
            return format!("{} (invalid)", r.id);
        };
        let changed: Vec<String> = specs
            .iter()
            .filter(|s| params.get(s.key) != Some(&s.default))
            .map(|s| {
                let v = params.get(s.key).unwrap_or(&s.default);
                format!("{}={}", s.key, crate::param::value_text(s, v))
            })
            .collect();
        if changed.is_empty() {
            r.id.clone()
        } else {
            format!("{} ({})", r.id, changed.join(", "))
        }
    }

    pub fn create_dimension(&self, r: &DimRef) -> Result<DimHandle> {
        let factory = self.dimension(&r.id).ok_or_else(|| Error::Unknown {
            kind: "dimension",
            id: r.id.clone(),
        })?;
        let params = r.params.resolve(&factory.params())?;
        Ok(DimHandle {
            id: r.id.clone(),
            dim_ref: DimRef {
                id: r.id.clone(),
                params: params.clone(),
            },
            name: factory.name(),
            key: format!("{}{}", r.id, params.canonical()),
            dim: factory.create(&params, self)?,
        })
    }

    pub fn create_stage(&self, config: &StageConfig) -> Result<Box<dyn Stage>> {
        let factory = self.stage(&config.id).ok_or_else(|| Error::Unknown {
            kind: "stage",
            id: config.id.clone(),
        })?;
        let params = config.params.resolve(&factory.params())?;
        factory.create(&params, self)
    }
}
