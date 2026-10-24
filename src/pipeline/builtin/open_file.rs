// SPDX-License-Identifier: GPL-2.0-only
use super::attach_linkmap;
use crate::model::{BinaryId, Fragment};
use crate::param::{ParamSpec, ParamValues};
use crate::pipeline::{PipelineCtx, PipelineState, Stage, StageFactory, StageKind};
use crate::registry::Registry;
use crate::{Error, Result};
use std::path::PathBuf;
use std::sync::Arc;

pub struct OpenFile;

impl StageFactory for OpenFile {
    fn id(&self) -> &'static str {
        "open-file"
    }
    fn kind(&self) -> StageKind {
        StageKind::Input
    }
    fn name(&self) -> &'static str {
        "Open file"
    }
    fn description(&self) -> &'static str {
        "Load a binary and add all its bytes"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::path("path", "File"),
            ParamSpec::text("name", "Name", "").help("Shown in the tree; the file name if empty"),
            ParamSpec::choice("reader", "Format", &[("auto", "Detect"), ("elf", "ELF")]),
            ParamSpec::choice(
                "map",
                "Linker map",
                &[
                    (
                        "auto",
                        "Attach fw.map or fw.elf.map if it is next to fw.elf",
                    ),
                    ("off", "Don't look for one"),
                ],
            ),
            ParamSpec::choice(
                "role",
                "Role",
                &[
                    ("current", "The build to look at"),
                    ("base", "A build to compare with: only gives the deltas"),
                ],
            ),
            ParamSpec::choice(
                "debug",
                "Debug info",
                &[
                    (
                        "auto",
                        "The file's DWARF, or a debug file found by build id, debug link or name",
                    ),
                    ("off", "Don't use it"),
                ],
            ),
        ]
    }
    fn summary(&self, p: &ParamValues, _registry: &Registry) -> String {
        let path = std::path::Path::new(p.text("path"));
        let verb = if p.text("role") == "base" {
            "compare with"
        } else {
            "open"
        };
        match path.file_name() {
            Some(n) => format!("{verb} {}", n.to_string_lossy()),
            None => "open (no file)".into(),
        }
    }
    fn create(&self, params: &ParamValues, _registry: &Registry) -> Result<Box<dyn Stage>> {
        let path = PathBuf::from(params.text("path"));
        if path.as_os_str().is_empty() {
            return Err(Error::param("path", "no file given"));
        }
        let name = match params.text("name") {
            "" => path.file_name().map_or_else(
                || path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
            n => n.to_owned(),
        };
        let reader = match params.text("reader") {
            "auto" => None,
            r => Some(r.to_owned()),
        };
        Ok(Box::new(Open {
            path,
            name,
            reader,
            auto_map: params.text("map") == "auto",
            debug: params.text("debug") == "auto",
            base: params.text("role") == "base",
        }))
    }
}

struct Open {
    path: PathBuf,
    name: String,
    reader: Option<String>,
    auto_map: bool,
    debug: bool,
    base: bool,
}

impl Stage for Open {
    fn apply(&self, ctx: &mut PipelineCtx, mut state: PipelineState) -> Result<PipelineState> {
        let path = ctx.resolve_path(&self.path);
        let mut bin = ctx.open_binary(&path, self.reader.as_deref(), &self.name)?;
        if self.auto_map
            && let Some(map) = crate::linkmap::find_next_to(&path)
        {
            match attach_linkmap(ctx, &bin, &map) {
                Ok(b) => {
                    ctx.note(format!("attached {}", map.display()));
                    bin = b;
                }
                Err(e) => ctx.warn(format!("linker map not attached: {e}")),
            }
        }
        if self.debug {
            if crate::debuginfo::own_source(&bin).is_some() {
                bin = ctx.derive_binary(&bin, "debug:own", |b| {
                    let src = crate::debuginfo::own_source(b).map(Arc::new);
                    Ok(b.derive(|d| d.attachments.debug = src))
                })?;
            } else if let Some(b) = super::find_debug_file(ctx, &bin) {
                bin = b;
            }
        }
        if self.base {
            bin = ctx.derive_binary(&bin, "role:base", |b| {
                Ok(b.derive(|d| d.role = crate::model::Role::Base))
            })?;
        }
        let id = BinaryId(state.binaries.len() as u32);
        let fragments = bin
            .chunks
            .chunks()
            .iter()
            .enumerate()
            .map(|(i, chunk)| Fragment::new(id, i, chunk, chunk.unified))
            .collect();
        state.binaries.push(bin);
        // Levels split before this file was added are applied to it too.
        let fragments = ctx.apply_levels(&state, fragments)?;
        state.set.fragments.extend(fragments);
        Ok(state)
    }
}
