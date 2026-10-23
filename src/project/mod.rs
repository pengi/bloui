// SPDX-License-Identifier: GPL-2.0-only
//! Project files (`*.bloui.toml`): a pipeline and view settings, stored
//! as TOML. Presets are project files without inputs.

pub mod preset;

use crate::model::Metric;
use crate::param::ParamKind;
use crate::param::{ParamValue, ParamValues};
use crate::pipeline::{Pipeline, StageConfig};
use crate::registry::Registry;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// A project file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// What it is for; shown for presets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "ViewSettings::is_empty")]
    pub view: ViewSettings,
    #[serde(flatten)]
    pub pipeline: Pipeline,
}

impl From<Pipeline> for Project {
    fn from(pipeline: Pipeline) -> Self {
        Project {
            pipeline,
            ..Default::default()
        }
    }
}

/// How to look at the result. Unset fields keep the frontend's choice.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metric: Option<Metric>,
    /// A tree level by name or index, or a dimension, as for
    /// `bloui-cli --color-by`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_by: Option<String>,
}

impl ViewSettings {
    pub fn is_empty(&self) -> bool {
        *self == ViewSettings::default()
    }
}

/// Whether a path names a project file rather than an input.
pub fn is_project(path: &Path) -> bool {
    path.to_string_lossy().ends_with(".bloui.toml")
}

/// The pipeline for a newly opened input: the input, then the default
/// preset (memory regions with their free space, sections, symbols).
/// Stages that can't run on the input (no linker map: no memory regions)
/// are skipped by the runner.
pub fn for_input(path: &Path) -> Pipeline {
    let path = ParamValue::Text(path.to_string_lossy().into_owned());
    let mut stages = vec![StageConfig::new(
        "open-file",
        ParamValues::new().with("path", path),
    )];
    stages.extend(preset::default_preset().project.pipeline.stages);
    Pipeline { stages }
}

pub fn from_str(text: &str) -> Result<Project> {
    toml::from_str(text).map_err(|e| Error::Project(e.to_string()))
}

pub fn to_string(project: &Project) -> Result<String> {
    toml::to_string(project).map_err(|e| Error::Project(e.to_string()))
}

pub fn load(path: &Path) -> Result<Project> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    from_str(&text).map_err(|e| Error::Project(format!("{}: {e}", path.display())))
}

pub fn save(path: &Path, project: &Project) -> Result<()> {
    std::fs::write(path, to_string(project)?).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })
}

/// Rewrite the relative path parameters of a pipeline whose paths are
/// relative to `from`, so they work relative to `to` (e.g. when saving a
/// project in another directory). Paths below `to` become (or stay)
/// relative, others absolute.
pub fn rebase_paths(pipeline: &mut Pipeline, registry: &Registry, from: &Path, to: &Path) {
    for stage in &mut pipeline.stages {
        let Some(factory) = registry.stage(&stage.id) else {
            continue;
        };
        for spec in factory.params() {
            if spec.kind != ParamKind::Path {
                continue;
            }
            let Some(ParamValue::Text(text)) = stage.params.get(spec.key) else {
                continue;
            };
            if text.is_empty() {
                continue;
            }
            let abs = from.join(text);
            let new = abs.strip_prefix(to).map_or(abs.clone(), Path::to_path_buf);
            let new = ParamValue::Text(new.to_string_lossy().into_owned());
            stage.params.set(spec.key, new);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut dim = ParamValues::new().with("id", ParamValue::Text("symbol".into()));
        dim.set("fallback", ParamValue::Text("none".into()));
        let p = Pipeline {
            stages: vec![
                StageConfig::new(
                    "open-file",
                    ParamValues::new().with("path", ParamValue::Text("fw.elf".into())),
                ),
                StageConfig::new(
                    "split",
                    ParamValues::new().with("dimension", ParamValue::Map(dim)),
                ),
                StageConfig {
                    enabled: false,
                    ..StageConfig::new(
                        "filter-label",
                        ParamValues::new()
                            .with("patterns", ParamValue::List(vec!["^\\.debug".into()]))
                            .with("level", ParamValue::Int(0)),
                    )
                },
            ],
        };
        let p = Project {
            description: Some("Test".into()),
            view: ViewSettings {
                metric: Some(Metric::Load),
                color_by: Some("object-file".into()),
            },
            pipeline: p,
        };
        let text = to_string(&p).unwrap();
        assert_eq!(from_str(&text).unwrap(), p, "{text}");
        // Without view settings, there is no [view] table.
        let bare = to_string(&Project::from(p.pipeline.clone())).unwrap();
        assert!(!bare.contains("[view]"), "{bare}");
    }

    #[test]
    fn rebase() {
        let reg = Registry::builtin();
        let open = |p: &str| {
            StageConfig::new(
                "open-file",
                ParamValues::new().with("path", ParamValue::Text(p.into())),
            )
        };
        let mut p = Pipeline {
            stages: vec![open("fw.elf"), open("/abs/x.elf"), open("sub/y.elf")],
        };
        rebase_paths(&mut p, &reg, Path::new("/a/b"), Path::new("/a"));
        rebase_paths(&mut p, &reg, Path::new("/a"), Path::new("/c"));
        let paths: Vec<_> = p.stages.iter().map(|s| s.params.get("path")).collect();
        let text = |s: &str| ParamValue::Text(s.into());
        assert_eq!(
            paths,
            [
                Some(&text("/a/b/fw.elf")),
                Some(&text("/abs/x.elf")),
                Some(&text("/a/b/sub/y.elf"))
            ]
        );
        let mut p = Pipeline {
            stages: vec![open("fw.elf")],
        };
        rebase_paths(&mut p, &reg, Path::new("/a/b"), Path::new("/a"));
        assert_eq!(p.stages[0].params.get("path"), Some(&text("b/fw.elf")));
        // Absolute paths below the new directory become relative.
        let mut p = Pipeline {
            stages: vec![open("/a/b/fw.elf")],
        };
        rebase_paths(&mut p, &reg, Path::new("/x"), Path::new("/a"));
        assert_eq!(p.stages[0].params.get("path"), Some(&text("b/fw.elf")));
    }

    #[test]
    fn hand_written() {
        let p = from_str(
            r#"
            [view]
            metric = "load"

            [[stage]]
            id = "open-file"
            path = "fw.elf"

            [[stage]]
            id = "split"
            dimension = "section"
            "#,
        )
        .unwrap();
        assert_eq!(p.view.metric, Some(Metric::Load));
        let p = p.pipeline;
        assert_eq!(p.stages.len(), 2);
        assert!(p.stages[1].enabled);
        assert_eq!(
            p.stages[1].params.get("dimension"),
            Some(&ParamValue::Text("section".into()))
        );
    }
}
