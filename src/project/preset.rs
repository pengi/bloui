// SPDX-License-Identifier: GPL-2.0-only
//! Presets: pipelines without inputs, to apply to any input. Built-in
//! ones, and the user's in `~/.config/bloui/presets/NAME.bloui.toml`.

use super::{Project, from_str, load, save as save_project};
use crate::pipeline::{Pipeline, StageConfig, StageKind};
use crate::registry::Registry;
use crate::{Error, Result};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    /// A built-in's name, or the file name without `.bloui.toml`.
    pub name: String,
    /// The stages (no inputs), a description, and view settings.
    pub project: Project,
    pub builtin: bool,
}

const BUILTIN: &[(&str, &str)] = &[
    (
        "default",
        r#"
description = "Memory regions with free space, then sections and symbols"

[[stage]]
id = "split"
dimension = "memory-region"

[[stage]]
id = "free-space"

[[stage]]
id = "split"
dimension = "section"

[[stage]]
id = "split"
dimension = "symbol"
"#,
    ),
    (
        "flash-by-object-file",
        r#"
description = "What is in flash, by object file and symbol"

[view]
metric = "load"

[[stage]]
id = "filter-space"
keep = "load"

[[stage]]
id = "split"
dimension = "object-file"

[[stage]]
id = "split"
dimension = "symbol"
"#,
    ),
    (
        "sections",
        r#"
description = "Sections and symbols"

[[stage]]
id = "split"
dimension = "section"

[[stage]]
id = "split"
dimension = "symbol"
"#,
    ),
    (
        "ram",
        r#"
description = "RAM by section and symbol: regions other than flash/ROM, with .data at its runtime address"

[[stage]]
id = "split"
dimension = { id = "memory-region", by = "vma" }

[[stage]]
id = "free-space"

[[stage]]
id = "filter-label"
source = "level"
patterns = ['(?i)flash|rom']

[[stage]]
id = "split"
dimension = "section"

[[stage]]
id = "split"
dimension = "symbol"
"#,
    ),
];

pub fn builtin() -> Vec<Preset> {
    BUILTIN
        .iter()
        .map(|(name, text)| Preset {
            name: (*name).into(),
            project: from_str(text).expect("built-in presets parse"),
            builtin: true,
        })
        .collect()
}

/// The preset for newly opened inputs.
pub fn default_preset() -> Preset {
    builtin().swap_remove(0)
}

/// `$XDG_CONFIG_HOME/bloui/presets`, or `~/.config/bloui/presets`.
pub fn user_dir() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(config.join("bloui").join("presets"))
}

/// The presets in a directory, by name; files that don't load are
/// returned as errors.
pub fn list(dir: &Path) -> (Vec<Preset>, Vec<Error>) {
    let (mut presets, mut errors) = (Vec::new(), Vec::new());
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (presets, errors);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file = path.file_name().map(|n| n.to_string_lossy().into_owned());
        let Some(name) = file.as_deref().and_then(|f| f.strip_suffix(".bloui.toml")) else {
            continue;
        };
        match load(&path) {
            Ok(project) => presets.push(Preset {
                name: name.to_owned(),
                project,
                builtin: false,
            }),
            Err(e) => errors.push(e),
        }
    }
    presets.sort_by(|a, b| a.name.cmp(&b.name));
    (presets, errors)
}

/// The built-in presets, then the user's; a user preset with a built-in's
/// name replaces it.
pub fn all(dir: Option<&Path>) -> (Vec<Preset>, Vec<Error>) {
    let mut presets = builtin();
    let (user, errors) = dir.map(list).unwrap_or_default();
    for p in user {
        match presets.iter_mut().find(|b| b.name == p.name) {
            Some(b) => *b = p,
            None => presets.push(p),
        }
    }
    (presets, errors)
}

pub fn find(name: &str, dir: Option<&Path>) -> Option<Preset> {
    all(dir).0.into_iter().find(|p| p.name == name)
}

fn is_input(stage: &StageConfig, registry: &Registry) -> bool {
    registry
        .stage(&stage.id)
        .is_some_and(|f| f.kind() == StageKind::Input)
}

/// The pipeline without its input stages.
pub fn strip_inputs(pipeline: &Pipeline, registry: &Registry) -> Pipeline {
    Pipeline {
        stages: pipeline
            .stages
            .iter()
            .filter(|s| !is_input(s, registry))
            .cloned()
            .collect(),
    }
}

/// The input stages of `current`, then the preset's stages.
pub fn apply(current: &Pipeline, preset: &Pipeline, registry: &Registry) -> Pipeline {
    let mut stages: Vec<StageConfig> = current
        .stages
        .iter()
        .filter(|s| is_input(s, registry))
        .cloned()
        .collect();
    stages.extend(strip_inputs(preset, registry).stages);
    Pipeline { stages }
}

/// Save a project's pipeline, without inputs, as a preset in `dir`.
pub fn save(dir: &Path, name: &str, project: &Project, registry: &Registry) -> Result<PathBuf> {
    let valid = !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\'])
        && name.trim() == name;
    if !valid {
        return Err(Error::Project(format!("bad preset name '{name}'")));
    }
    std::fs::create_dir_all(dir).map_err(|source| Error::Io {
        path: dir.to_owned(),
        source,
    })?;
    let path = dir.join(format!("{name}.bloui.toml"));
    let preset = Project {
        pipeline: strip_inputs(&project.pipeline, registry),
        ..project.clone()
    };
    save_project(&path, &preset)?;
    Ok(path)
}
