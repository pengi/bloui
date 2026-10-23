// SPDX-License-Identifier: GPL-2.0-only
use crate::Result;
use crate::dimension::{Dimension, DimensionFactory, Labeling};
use crate::model::{AddrSpace, Availability, Binary, Interner, Label, Overlap};
use crate::param::{ParamSpec, ParamValues};

pub struct RegionDim;

impl DimensionFactory for RegionDim {
    fn id(&self) -> &'static str {
        "memory-region"
    }
    fn name(&self) -> &'static str {
        "memory region"
    }
    fn description(&self) -> &'static str {
        "Memory region (FLASH, RAM, …) from the linker map or a memory-regions stage"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![ParamSpec::choice(
            "by",
            "Place bytes by",
            &[
                (
                    "auto",
                    "Where they are: runtime address, or load address for load images",
                ),
                ("vma", "Runtime address"),
                ("lma", "Load address"),
            ],
        )]
    }
    fn availability(&self, bin: &Binary) -> Availability {
        if bin.memory_regions().is_empty() {
            Availability::Unavailable(
                "no memory regions: attach a linker map or add a memory-regions stage".into(),
            )
        } else {
            Availability::Available
        }
    }
    fn create(&self, params: &ParamValues) -> Result<Box<dyn Dimension>> {
        let spaces: &[AddrSpace] = match params.text("by") {
            "vma" => &[AddrSpace::Vma],
            "lma" => &[AddrSpace::Lma],
            _ => &[AddrSpace::Vma, AddrSpace::Lma],
        };
        Ok(Box::new(Regions { spaces }))
    }
}

struct Regions {
    spaces: &'static [AddrSpace],
}

impl Dimension for Regions {
    fn label(&self, bin: &Binary, interner: &mut Interner) -> Result<Labeling> {
        let mut l = Labeling::new(
            Overlap::FirstWins,
            Label::single(interner.intern("[no region]")),
        );
        // Runtime addresses first, so they win where a chunk has both.
        for &space in self.spaces {
            for r in bin.memory_regions() {
                l.push(space, r.span(), Label::single(interner.intern(&r.name)));
            }
        }
        Ok(l)
    }

    fn labels_free_space(&self) -> bool {
        true
    }
}
