// SPDX-License-Identifier: GPL-2.0-only
use crate::model::{Availability, Fragment, Interner, Label};
use crate::param::{LevelRef, ParamSpec, ParamValues};
use crate::pipeline::{PipelineCtx, PipelineState, Stage, StageFactory, StageKind};
use crate::registry::{DimHandle, Registry};
use crate::{Error, Result};
use regex::Regex;

pub struct FilterLabel;

impl StageFactory for FilterLabel {
    fn id(&self) -> &'static str {
        "filter-label"
    }
    fn kind(&self) -> StageKind {
        StageKind::Filter
    }
    fn name(&self) -> &'static str {
        "Filter by label"
    }
    fn description(&self) -> &'static str {
        "Keep or remove bytes by their label in a dimension or a tree level. \
         Path labels match as text joined by '/'"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::choice(
                "source",
                "Match on",
                &[
                    ("dimension", "A dimension"),
                    ("level", "An existing tree level"),
                ],
            ),
            ParamSpec::dimension("dimension", "Dimension", "section")
                .visible_when("source", "dimension"),
            ParamSpec::level("level", "Level")
                .help("Name or index; the last level if not set")
                .visible_when("source", "level"),
            ParamSpec::choice(
                "mode",
                "Mode",
                &[
                    ("exclude", "Remove matches"),
                    ("include", "Keep only matches"),
                ],
            ),
            ParamSpec::choice(
                "match",
                "Patterns are",
                &[("regex", "Regular expressions"), ("exact", "Exact names")],
            ),
            ParamSpec::list("patterns", "Patterns"),
        ]
    }
    fn summary(&self, p: &ParamValues, registry: &Registry) -> String {
        let source = if p.text("source") == "level" {
            p.level("level").summary()
        } else {
            registry.dimension_summary(&p.dimension("dimension"))
        };
        let op = if p.text("match") == "regex" { "~" } else { "=" };
        match p.list("patterns") {
            [] => format!("{} {source} (no patterns)", p.text("mode")),
            pats => format!("{} {source} {op} {}", p.text("mode"), pats.join(", ")),
        }
    }
    fn create(&self, params: &ParamValues, registry: &Registry) -> Result<Box<dyn Stage>> {
        let patterns = params.list("patterns");
        let matcher = if params.text("match") == "regex" {
            let res = patterns
                .iter()
                .map(|p| Regex::new(p).map_err(|e| Error::param("patterns", e.to_string())))
                .collect::<Result<Vec<_>>>()?;
            Matcher::Regex(res)
        } else {
            Matcher::Exact(patterns.to_vec())
        };
        let source = if params.text("source") == "level" {
            Source::Level(params.level("level"))
        } else {
            Source::Dimension(registry.create_dimension(&params.dimension("dimension"))?)
        };
        Ok(Box::new(Filter {
            source,
            matcher,
            include: params.text("mode") == "include",
        }))
    }
}

enum Matcher {
    Regex(Vec<Regex>),
    Exact(Vec<String>),
}

impl Matcher {
    fn matches(&self, label: &Label, interner: &Interner) -> bool {
        let text = label.text(interner);
        match self {
            Matcher::Regex(res) => res.iter().any(|r| r.is_match(&text)),
            Matcher::Exact(names) => names.contains(&text),
        }
    }
}

enum Source {
    Dimension(DimHandle),
    Level(LevelRef),
}

struct Filter {
    source: Source,
    matcher: Matcher,
    include: bool,
}

impl Stage for Filter {
    fn check(&self, _ctx: &PipelineCtx, input: &PipelineState) -> Availability {
        match &self.source {
            Source::Dimension(d) => d.availability(input),
            Source::Level(r) => match r.find(&input.set.levels) {
                Some(_) => Availability::Available,
                None => Availability::Unavailable("no such tree level".into()),
            },
        }
    }

    fn apply(&self, ctx: &mut PipelineCtx, mut state: PipelineState) -> Result<PipelineState> {
        let fragments = std::mem::take(&mut state.set.fragments);
        let mut out: Vec<Fragment> = Vec::with_capacity(fragments.len());
        let keep = |label: &Label, interner: &Interner| {
            self.matcher.matches(label, interner) == self.include
        };
        match &self.source {
            Source::Dimension(d) => {
                ctx.cut_all(&state, &fragments, &d.key, &*d.dim, |f, label, interner| {
                    if keep(label, interner) {
                        out.push(f);
                    }
                })?;
            }
            Source::Level(r) => {
                let level = r
                    .find(&state.set.levels)
                    .ok_or_else(|| Error::Other("no such tree level".into()))?;
                out.extend(
                    fragments
                        .into_iter()
                        .filter(|f| keep(&f.labels[level], &ctx.interner)),
                );
            }
        }
        state.set.fragments = out;
        Ok(state)
    }
}
