// SPDX-License-Identifier: GPL-2.0-only
//! Dimensions label the bytes of a binary: by section, symbol, memory
//! region, and so on. Split and filter stages use them, and so does
//! "color by" in the GUI.

pub mod builtin;

use crate::Result;
use crate::model::{AddrSpace, Availability, Binary, Interner, Label, Overlap, Span};
use crate::param::{ParamSpec, ParamValues};

/// One labeled range, in whatever space suits the dimension.
#[derive(Clone, Debug)]
pub struct LabelEntry {
    pub space: AddrSpace,
    pub span: Span,
    /// Lower tiers win over higher ones, before the overlap policy applies.
    /// Use a higher tier for fallbacks such as `[.text]`.
    pub tier: u32,
    pub label: Label,
}

/// The output of a dimension for one binary.
#[derive(Clone, Debug)]
pub struct Labeling {
    pub entries: Vec<LabelEntry>,
    pub policy: Overlap,
    /// For bytes no entry covers.
    pub fallback: Label,
}

impl Labeling {
    pub fn new(policy: Overlap, fallback: Label) -> Self {
        Labeling {
            entries: Vec::new(),
            policy,
            fallback,
        }
    }

    pub fn push(&mut self, space: AddrSpace, span: Span, label: Label) {
        self.push_tier(0, space, span, label);
    }

    pub fn push_tier(&mut self, tier: u32, space: AddrSpace, span: Span, label: Label) {
        if !span.is_empty() {
            self.entries.push(LabelEntry {
                space,
                span,
                tier,
                label,
            });
        }
    }
}

pub trait Dimension: Send + Sync {
    fn label(&self, bin: &Binary, interner: &mut Interner) -> Result<Labeling>;
}

pub trait DimensionFactory: Send + Sync {
    /// Stable id, used in project files: `"section"`.
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn params(&self) -> Vec<ParamSpec> {
        Vec::new()
    }
    fn availability(&self, _bin: &Binary) -> Availability {
        Availability::Available
    }
    /// `params` are already resolved against [`Self::params`].
    fn create(&self, params: &ParamValues) -> Result<Box<dyn Dimension>>;
}
