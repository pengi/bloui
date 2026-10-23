// SPDX-License-Identifier: GPL-2.0-only
use crate::model::{BinaryId, Fragment, Label};
use crate::param::{ParamSpec, ParamValues};
use crate::pipeline::{PipelineCtx, PipelineState, Stage, StageFactory, StageKind};
use crate::registry::Registry;
use crate::{Error, Result};
use std::path::PathBuf;

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
        ]
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
        Ok(Box::new(Open { path, name, reader }))
    }
}

struct Open {
    path: PathBuf,
    name: String,
    reader: Option<String>,
}

impl Stage for Open {
    fn apply(&self, ctx: &mut PipelineCtx, mut state: PipelineState) -> Result<PipelineState> {
        let path = ctx.resolve_path(&self.path);
        let bin = ctx.open_binary(&path, self.reader.as_deref(), &self.name)?;
        let id = BinaryId(state.binaries.len() as u32);
        // Levels split before this file was added don't apply to it.
        let unsplit = Label::single(ctx.interner.intern("[not split]"));
        for (i, chunk) in bin.chunks.chunks().iter().enumerate() {
            let mut f = Fragment::new(id, i, chunk, chunk.unified);
            f.labels
                .extend(state.set.levels.iter().map(|_| unsplit.clone()));
            state.set.fragments.push(f);
        }
        state.binaries.push(bin);
        Ok(state)
    }
}
