// SPDX-License-Identifier: GPL-2.0-only
use crate::Result;
use crate::model::{Availability, BinaryId, Fragment, Span};
use crate::param::{ParamSpec, ParamValues};
use crate::pipeline::{PipelineCtx, PipelineState, Stage, StageFactory, StageKind};
use crate::registry::Registry;

pub struct FreeSpace;

impl StageFactory for FreeSpace {
    fn id(&self) -> &'static str {
        "free-space"
    }
    fn kind(&self) -> StageKind {
        StageKind::Add
    }
    fn name(&self) -> &'static str {
        "Free space"
    }
    fn description(&self) -> &'static str {
        "Add the unused part of each memory region as [free], so regions add up to their size"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::list("regions", "Regions").help("Names; all if empty"),
            ParamSpec::text("binary", "Input", "").help("Name of the input; all if empty"),
        ]
    }
    fn create(&self, params: &ParamValues, _registry: &Registry) -> Result<Box<dyn Stage>> {
        Ok(Box::new(Free {
            regions: params.list("regions").to_vec(),
            binary: params.text("binary").to_owned(),
        }))
    }
}

struct Free {
    regions: Vec<String>,
    binary: String,
}

/// `span` minus the sorted, merged `taken` spans.
fn subtract(span: Span, taken: &[Span]) -> Vec<Span> {
    let mut out = Vec::new();
    let mut cursor = span.start;
    for t in taken {
        if t.end <= cursor {
            continue;
        }
        if t.start >= span.end {
            break;
        }
        if t.start > cursor {
            out.push(Span::new(cursor, t.start));
        }
        cursor = cursor.max(t.end);
    }
    if cursor < span.end {
        out.push(Span::new(cursor, span.end));
    }
    out
}

impl Stage for Free {
    fn check(&self, _ctx: &PipelineCtx, input: &PipelineState) -> Availability {
        if input
            .binaries
            .iter()
            .any(|b| !b.memory_regions().is_empty())
        {
            Availability::Available
        } else {
            Availability::Unavailable(
                "no memory regions: attach a linker map or add a memory-regions stage".into(),
            )
        }
    }

    fn apply(&self, ctx: &mut PipelineCtx, mut state: PipelineState) -> Result<PipelineState> {
        let ids: Vec<BinaryId> = match state.find_binary(&self.binary) {
            Some(id) if !self.binary.is_empty() => vec![id],
            _ => (0..state.binaries.len() as u32).map(BinaryId).collect(),
        };
        for id in ids {
            let bin = state.binary(id).clone();
            let regions: Vec<_> = bin
                .memory_regions()
                .iter()
                .filter(|r| self.regions.is_empty() || self.regions.contains(&r.name))
                .cloned()
                .collect();
            if regions.is_empty() {
                continue;
            }
            // Taken: everything with an address, at runtime or load time.
            let mut taken: Vec<Span> = Vec::new();
            for c in bin.chunks.chunks().iter().filter(|c| !c.free) {
                taken.extend(c.span_in(crate::model::AddrSpace::Vma));
                taken.extend(c.span_in(crate::model::AddrSpace::Lma));
            }
            taken.sort();
            let mut free = Vec::new();
            for r in &regions {
                free.extend(subtract(r.span(), &taken));
            }
            let old = bin.chunks.chunks().len();
            let key = format!("free:{:?}", self.regions);
            let derived = ctx.derive_binary(&bin, &key, |b| {
                Ok(b.derive(|d| d.chunks = std::sync::Arc::new(d.chunks.with_free(&free))))
            })?;
            let fragments: Vec<Fragment> = derived.chunks.chunks()[old..]
                .iter()
                .enumerate()
                .map(|(i, c)| Fragment::new(id, old + i, c, c.unified))
                .collect();
            let total: u64 = fragments.iter().map(|f| f.span.len()).sum();
            ctx.note(format!(
                "{}: {total} bytes free in {} regions",
                bin.name,
                regions.len()
            ));
            state.binaries[id.0 as usize] = derived;
            let fragments = ctx.apply_levels(&state, fragments)?;
            state.set.fragments.extend(fragments);
        }
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtract_spans() {
        let taken = [Span::new(0, 10), Span::new(5, 20), Span::new(30, 40)];
        assert_eq!(
            subtract(Span::new(0, 50), &taken),
            [Span::new(20, 30), Span::new(40, 50)]
        );
        assert_eq!(subtract(Span::new(12, 18), &taken), []);
        assert_eq!(subtract(Span::new(100, 110), &taken), [Span::new(100, 110)]);
    }
}
