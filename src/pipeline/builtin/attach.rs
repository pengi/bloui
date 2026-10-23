// SPDX-License-Identifier: GPL-2.0-only
//! Attach stages: linker map and memory regions.

use crate::model::{Binary, BinaryId, MemoryRegion};
use crate::param::{ParamSpec, ParamValues};
use crate::pipeline::{PipelineCtx, PipelineState, Stage, StageFactory, StageKind};
use crate::registry::Registry;
use crate::{Error, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Parse a map file, check it against `bin`, and attach it. Cached, so a
/// rerun gives the same binary while the map file is unchanged.
pub fn attach_linkmap(
    ctx: &mut PipelineCtx,
    bin: &Arc<Binary>,
    path: &Path,
) -> Result<Arc<Binary>> {
    ctx.reads_file(path);
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let key = format!("linkmap:{}:{mtime:?}", path.display());
    ctx.derive_binary(bin, &key, |b| {
        let map = crate::linkmap::load(path)?;
        crate::linkmap::check(&map, b)?;
        Ok(b.derive(|d| d.attachments.linkmap = Some(Arc::new(map))))
    })
}

/// The binaries a stage applies to: one by name, or all.
fn targets(state: &PipelineState, name: &str) -> Result<Vec<BinaryId>> {
    if name.is_empty() {
        Ok((0..state.binaries.len() as u32).map(BinaryId).collect())
    } else {
        state
            .find_binary(name)
            .map(|b| vec![b])
            .ok_or_else(|| Error::param("binary", format!("no input named '{name}'")))
    }
}

pub struct LinkerMap;

impl StageFactory for LinkerMap {
    fn id(&self) -> &'static str {
        "linker-map"
    }
    fn kind(&self) -> StageKind {
        StageKind::Attach
    }
    fn name(&self) -> &'static str {
        "Linker map"
    }
    fn description(&self) -> &'static str {
        "Attach a GNU ld map file: object files, input sections, memory regions"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::path("path", "Map file")
                .help("Empty: fw.map or fw.elf.map next to the input"),
            ParamSpec::text("binary", "Input", "").help("Name of the input; the last one if empty"),
        ]
    }
    fn summary(&self, p: &ParamValues, _registry: &Registry) -> String {
        match Path::new(p.text("path")).file_name() {
            Some(n) => format!("map {}", n.to_string_lossy()),
            None => "map (next to the input)".into(),
        }
    }
    fn create(&self, params: &ParamValues, _registry: &Registry) -> Result<Box<dyn Stage>> {
        Ok(Box::new(AttachMap {
            path: PathBuf::from(params.text("path")),
            binary: params.text("binary").to_owned(),
        }))
    }
}

struct AttachMap {
    path: PathBuf,
    binary: String,
}

impl Stage for AttachMap {
    fn check(&self, _ctx: &PipelineCtx, input: &PipelineState) -> crate::model::Availability {
        match input.find_binary(&self.binary) {
            Some(_) => crate::model::Availability::Available,
            None => crate::model::Availability::Unavailable("no input file".into()),
        }
    }

    fn apply(&self, ctx: &mut PipelineCtx, mut state: PipelineState) -> Result<PipelineState> {
        let id = state
            .find_binary(&self.binary)
            .ok_or_else(|| Error::param("binary", format!("no input named '{}'", self.binary)))?;
        let bin = state.binary(id).clone();
        let path = if self.path.as_os_str().is_empty() {
            crate::linkmap::find_next_to(&bin.path).ok_or_else(|| {
                Error::Other(format!("no map file next to {}", bin.path.display()))
            })?
        } else {
            ctx.resolve_path(&self.path)
        };
        let b = attach_linkmap(ctx, &bin, &path)?;
        let map = b.attachments.linkmap.as_ref().unwrap();
        ctx.note(format!(
            "{}: {} input sections, {} objects, {} memory regions",
            path.display(),
            map.inputs.len(),
            map.objects.len(),
            map.regions.len()
        ));
        state.binaries[id.0 as usize] = b;
        Ok(state)
    }
}

pub struct MemoryRegions;

impl StageFactory for MemoryRegions {
    fn id(&self) -> &'static str {
        "memory-regions"
    }
    fn kind(&self) -> StageKind {
        StageKind::Attach
    }
    fn name(&self) -> &'static str {
        "Memory regions"
    }
    fn description(&self) -> &'static str {
        "Set the memory regions by hand. They replace the linker map's"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::table(
                "regions",
                "Regions",
                vec![
                    ParamSpec::text("name", "Name", ""),
                    ParamSpec::address("origin", "Origin"),
                    ParamSpec::size("length", "Length", 0),
                    ParamSpec::text("attrs", "Attributes", ""),
                ],
            )
            .help("On the command line: NAME:ORIGIN:LENGTH, e.g. FLASH:0x08000000:64K"),
            ParamSpec::text("binary", "Input", "").help("Name of the input; all if empty"),
        ]
    }
    fn summary(&self, p: &ParamValues, _registry: &Registry) -> String {
        let names: Vec<&str> = p.table("regions").iter().map(|r| r.text("name")).collect();
        if names.is_empty() {
            "regions (none)".into()
        } else {
            format!("regions {}", names.join(", "))
        }
    }
    fn create(&self, params: &ParamValues, _registry: &Registry) -> Result<Box<dyn Stage>> {
        let mut regions = Vec::new();
        for row in params.table("regions") {
            let name = row.text("name");
            let origin = row.opt_u64("origin");
            let (true, Some(origin)) = (!name.is_empty(), origin) else {
                return Err(Error::param(
                    "regions",
                    "every region needs a name and an origin",
                ));
            };
            regions.push(MemoryRegion {
                name: name.to_owned(),
                origin,
                length: row.opt_u64("length").unwrap_or(0),
                attrs: row.text("attrs").to_owned(),
            });
        }
        let key = format!("regions:{}", params.canonical());
        Ok(Box::new(SetRegions {
            regions: Arc::new(regions),
            binary: params.text("binary").to_owned(),
            key,
        }))
    }
}

struct SetRegions {
    regions: Arc<Vec<MemoryRegion>>,
    binary: String,
    key: String,
}

impl Stage for SetRegions {
    fn apply(&self, ctx: &mut PipelineCtx, mut state: PipelineState) -> Result<PipelineState> {
        for id in targets(&state, &self.binary)? {
            let bin = state.binary(id).clone();
            let b = ctx.derive_binary(&bin, &self.key, |b| {
                Ok(b.derive(|d| d.attachments.memory_regions = Some(self.regions.clone())))
            })?;
            state.binaries[id.0 as usize] = b;
        }
        Ok(state)
    }
}
