// SPDX-License-Identifier: GPL-2.0-only
//! The processing pipeline: stages, their configuration and context.

pub mod builtin;

use crate::Result;
use crate::dimension::Dimension;
use crate::engine::{ResolvedLabels, translate};
use crate::model::{Availability, Binary, BinaryId, Fragment, FragmentSet, Interner, Label};
use crate::param::{ParamSpec, ParamValues};
use crate::reader::ReadOptions;
use crate::registry::Registry;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StageKind {
    /// Adds a binary and all its bytes.
    Input,
    /// Adds data from outside the binary (linker map, …).
    Attach,
    /// Cuts fragments and adds a tree level.
    Split,
    /// Removes fragments.
    Filter,
    /// Regroups or renames.
    Transform,
    /// Adds fragments, such as free space.
    Add,
}

impl StageKind {
    pub fn name(self) -> &'static str {
        match self {
            StageKind::Input => "input",
            StageKind::Attach => "attach",
            StageKind::Split => "split",
            StageKind::Filter => "filter",
            StageKind::Transform => "transform",
            StageKind::Add => "add",
        }
    }
}

/// What flows between stages.
#[derive(Clone, Debug, Default)]
pub struct PipelineState {
    /// Indexed by [`BinaryId`].
    pub binaries: Vec<Arc<Binary>>,
    pub set: FragmentSet,
}

impl PipelineState {
    pub fn binary(&self, id: BinaryId) -> &Arc<Binary> {
        &self.binaries[id.0 as usize]
    }

    /// A binary by name; the last one opened if `name` is empty.
    pub fn find_binary(&self, name: &str) -> Option<BinaryId> {
        if name.is_empty() {
            return self
                .binaries
                .len()
                .checked_sub(1)
                .map(|i| BinaryId(i as u32));
        }
        self.binaries
            .iter()
            .position(|b| b.name == name)
            .map(|i| BinaryId(i as u32))
    }
}

/// Path, reader id, display name and options of a loaded file.
type FileKey = (PathBuf, String, String, ReadOptions);

/// State that lives across runs: the interner and caches.
pub struct PipelineCtx {
    pub registry: Arc<Registry>,
    pub interner: Interner,
    /// Relative input paths are resolved against this.
    pub base_dir: PathBuf,
    labels: HashMap<(u64, String), Arc<ResolvedLabels>>,
    files: HashMap<FileKey, (Option<SystemTime>, Arc<Binary>)>,
    derived: HashMap<(u64, String), Arc<Binary>>,
    notes: Vec<String>,
}

impl PipelineCtx {
    pub fn new(registry: Arc<Registry>) -> Self {
        PipelineCtx {
            registry,
            interner: Interner::new(),
            base_dir: PathBuf::from("."),
            labels: HashMap::new(),
            files: HashMap::new(),
            derived: HashMap::new(),
            notes: Vec::new(),
        }
    }

    /// Add a note to the running stage's report ("attached fw.map").
    pub fn note(&mut self, text: impl Into<String>) {
        self.notes.push(text.into());
    }

    /// A note that is a warning; frontends show these more prominently.
    pub fn warn(&mut self, text: impl Into<String>) {
        self.notes.push(format!("warning: {}", text.into()));
    }

    pub(crate) fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    /// A dimension's labels for a binary, cached by binary and `key`.
    pub fn labels(
        &mut self,
        bin: &Binary,
        key: &str,
        dim: &dyn Dimension,
    ) -> Result<Arc<ResolvedLabels>> {
        let k = (bin.uid, key.to_owned());
        if let Some(l) = self.labels.get(&k) {
            return Ok(l.clone());
        }
        let labeling = dim.label(bin, &mut self.interner)?;
        let resolved = Arc::new(translate::resolve(bin, labeling));
        self.labels.insert(k, resolved.clone());
        Ok(resolved)
    }

    /// Labels for every binary in a state, indexed by [`BinaryId`].
    pub fn labels_for_all(
        &mut self,
        state: &PipelineState,
        key: &str,
        dim: &dyn Dimension,
    ) -> Result<Vec<Arc<ResolvedLabels>>> {
        state
            .binaries
            .iter()
            .map(|b| self.labels(b, key, dim))
            .collect()
    }

    /// A modified binary, cached by the original's uid and `key`, so that
    /// reruns give the same binary (and keep label caches valid). `key`
    /// must cover everything `f` depends on.
    pub fn derive_binary(
        &mut self,
        bin: &Arc<Binary>,
        key: &str,
        f: impl FnOnce(&Binary) -> Result<Binary>,
    ) -> Result<Arc<Binary>> {
        let k = (bin.uid, key.to_owned());
        if let Some(b) = self.derived.get(&k) {
            return Ok(b.clone());
        }
        let b = Arc::new(f(bin)?);
        self.derived.insert(k, b.clone());
        Ok(b)
    }

    /// Drop cached data of binaries that are no longer in use.
    pub fn retain_binaries(&mut self, live: &HashSet<u64>) {
        self.labels.retain(|(uid, _), _| live.contains(uid));
        self.derived.retain(|_, b| live.contains(&b.uid));
    }

    pub fn resolve_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_owned()
        } else {
            self.base_dir.join(path)
        }
    }

    /// Open a file, reusing an earlier load while the file is unchanged.
    pub fn open_binary(
        &mut self,
        path: &Path,
        reader: Option<&str>,
        name: &str,
        opts: ReadOptions,
    ) -> Result<Arc<Binary>> {
        let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        let key = (
            path.to_owned(),
            reader.unwrap_or("").to_owned(),
            name.to_owned(),
            opts,
        );
        if let Some((t, b)) = self.files.get(&key)
            && *t == mtime
            && mtime.is_some()
        {
            return Ok(b.clone());
        }
        let bin = Arc::new(crate::reader::read_file(
            &self.registry.readers,
            path,
            reader,
            name.to_owned(),
            opts,
        )?);
        self.files.insert(key, (mtime, bin.clone()));
        Ok(bin)
    }

    pub fn forget_files(&mut self) {
        self.files.clear();
        self.derived.clear();
    }

    /// Cut every fragment by a dimension, and call `f` with each piece and
    /// its label. `f` also gets the interner, to look at label text.
    ///
    /// Free space is labeled `[free]`, unless the dimension labels free
    /// space itself (memory regions do).
    pub fn cut_all(
        &mut self,
        state: &PipelineState,
        fragments: &[Fragment],
        key: &str,
        dim: &dyn Dimension,
        mut f: impl FnMut(Fragment, &Label, &Interner),
    ) -> Result<()> {
        let labels = self.labels_for_all(state, key, dim)?;
        let free = Label::single(self.interner.intern("[free]"));
        let own_free = dim.labels_free_space();
        for frag in fragments {
            let bin = state.binary(frag.bin);
            if !own_free && bin.chunks.chunk(frag.chunk as usize).free {
                f(frag.clone(), &free, &self.interner);
                continue;
            }
            let l = &labels[frag.bin.0 as usize];
            crate::engine::refine::cut(frag, l, |piece, label| f(piece, label, &self.interner));
        }
        Ok(())
    }

    /// Label new fragments (of a file opened late, or free space) by the
    /// dimensions of the existing tree levels, as if they had been there
    /// when those levels were split. Filters before are not applied.
    pub fn apply_levels(
        &mut self,
        state: &PipelineState,
        mut fragments: Vec<Fragment>,
    ) -> Result<Vec<Fragment>> {
        let registry = self.registry.clone();
        for level in &state.set.levels {
            let dim = registry.create_dimension(&level.dim)?;
            let mut out = Vec::with_capacity(fragments.len());
            self.cut_all(state, &fragments, &dim.key, &*dim.dim, |mut f, label, _| {
                f.labels.push(label.clone());
                out.push(f);
            })?;
            fragments = out;
        }
        Ok(fragments)
    }
}

/// A stage in a pipeline, as stored in a project file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StageConfig {
    pub id: String,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub enabled: bool,
    #[serde(flatten)]
    pub params: ParamValues,
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

impl StageConfig {
    pub fn new(id: &str, params: ParamValues) -> Self {
        StageConfig {
            id: id.into(),
            enabled: true,
            params,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Pipeline {
    #[serde(default, rename = "stage")]
    pub stages: Vec<StageConfig>,
}

pub trait StageFactory: Send + Sync {
    /// Stable id, used in project files: `"split"`.
    fn id(&self) -> &'static str;
    fn kind(&self) -> StageKind;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn params(&self) -> Vec<ParamSpec>;
    /// `params` are already resolved against [`Self::params`].
    fn create(&self, params: &ParamValues, registry: &Registry) -> Result<Box<dyn Stage>>;
}

pub trait Stage: Send + Sync {
    /// Whether the stage can run on this input.
    fn check(&self, _ctx: &PipelineCtx, _input: &PipelineState) -> Availability {
        Availability::Available
    }
    fn apply(&self, ctx: &mut PipelineCtx, input: PipelineState) -> Result<PipelineState>;
}
