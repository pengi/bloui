// SPDX-License-Identifier: GPL-2.0-only
use crate::Result;
use crate::dimension::Labeling;
use crate::engine::{refine, translate};
use crate::model::{AddrSpace, Fragment, Label, Overlap, Span};
use crate::param::{ParamSpec, ParamValues};
use crate::pipeline::{PipelineCtx, PipelineState, Stage, StageFactory, StageKind};
use crate::registry::Registry;

pub struct FilterSpace;

impl StageFactory for FilterSpace {
    fn id(&self) -> &'static str {
        "filter-space"
    }
    fn kind(&self) -> StageKind {
        StageKind::Filter
    }
    fn name(&self) -> &'static str {
        "Filter by space"
    }
    fn description(&self) -> &'static str {
        "Keep bytes that are in the file, in memory or loaded, and/or in an address range"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::choice(
                "keep",
                "Keep",
                &[
                    ("any", "All bytes"),
                    ("file", "Bytes in the file"),
                    ("memory", "Bytes in memory at runtime"),
                    ("load", "Loaded bytes (flash image)"),
                ],
            ),
            ParamSpec::choice(
                "space",
                "Range is in",
                &[
                    ("vma", "Runtime address"),
                    ("lma", "Load address"),
                    ("file", "File offset"),
                ],
            ),
            ParamSpec::address("start", "Range start").help("Empty: from the beginning"),
            ParamSpec::address("end", "Range end").help("Empty: to the end"),
        ]
    }
    fn summary(&self, p: &ParamValues, _registry: &Registry) -> String {
        let mut s = format!("keep {}", p.text("keep"));
        let (start, end) = (p.opt_u64("start"), p.opt_u64("end"));
        if start.is_some() || end.is_some() {
            let a = start.map_or(String::new(), |a| format!("{a:#x}"));
            let b = end.map_or(String::new(), |b| format!("{b:#x}"));
            s.push_str(&format!(" in {} {a}..{b}", p.text("space")));
        }
        s
    }
    fn create(&self, params: &ParamValues, _registry: &Registry) -> Result<Box<dyn Stage>> {
        let space = match params.text("space") {
            "lma" => AddrSpace::Lma,
            "file" => AddrSpace::File,
            _ => AddrSpace::Vma,
        };
        let (start, end) = (params.opt_u64("start"), params.opt_u64("end"));
        let range = (start.is_some() || end.is_some()).then(|| {
            (
                space,
                Span::new(
                    start.unwrap_or(0),
                    end.unwrap_or(u64::MAX).max(start.unwrap_or(0)),
                ),
            )
        });
        Ok(Box::new(Filter {
            keep: params.text("keep").to_owned(),
            range,
        }))
    }
}

struct Filter {
    keep: String,
    range: Option<(AddrSpace, Span)>,
}

impl Stage for Filter {
    fn apply(&self, ctx: &mut PipelineCtx, mut state: PipelineState) -> Result<PipelineState> {
        let keep = |f: &Fragment| match self.keep.as_str() {
            "file" => f.sizes.file > 0,
            "memory" => f.sizes.memory > 0,
            "load" => f.sizes.load > 0,
            _ => true,
        };
        let mut fragments: Vec<Fragment> = std::mem::take(&mut state.set.fragments)
            .into_iter()
            .filter(keep)
            .collect();
        if let Some((space, span)) = self.range {
            let inside = Label::single(ctx.interner.intern("in"));
            let labels: Vec<_> = state
                .binaries
                .iter()
                .map(|bin| {
                    let mut l = Labeling::new(Overlap::FirstWins, Label::default());
                    l.push(space, span, inside.clone());
                    translate::resolve(bin, l)
                })
                .collect();
            let mut out = Vec::with_capacity(fragments.len());
            for f in &fragments {
                refine::cut(f, &labels[f.bin.0 as usize], |piece, label| {
                    if *label == inside {
                        out.push(piece);
                    }
                });
            }
            fragments = out;
        }
        state.set.fragments = fragments;
        Ok(state)
    }
}
