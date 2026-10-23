// SPDX-License-Identifier: GPL-2.0-only
//! Project files (`*.bloui.toml`): a pipeline, stored as TOML.

use crate::param::ParamKind;
use crate::param::{ParamValue, ParamValues};
use crate::pipeline::{Pipeline, StageConfig};
use crate::registry::Registry;
use crate::{Error, Result};
use std::path::{Path, PathBuf};

/// Whether a path names a project file rather than an input.
pub fn is_project(path: &Path) -> bool {
    path.to_string_lossy().ends_with(".bloui.toml")
}

/// The pipeline for a newly opened input: memory regions with their free
/// space, then sections and symbols. Stages that can't run on the input
/// (no linker map: no memory regions) are skipped by the runner.
pub fn for_input(path: &Path) -> Pipeline {
    let split = |dim: &str| {
        StageConfig::new(
            "split",
            ParamValues::new().with("dimension", ParamValue::Text(dim.into())),
        )
    };
    let path = ParamValue::Text(path.to_string_lossy().into_owned());
    Pipeline {
        stages: vec![
            StageConfig::new("open-file", ParamValues::new().with("path", path)),
            split("memory-region"),
            StageConfig::new("free-space", ParamValues::new()),
            split("section"),
            split("symbol"),
        ],
    }
}

pub fn from_str(text: &str) -> Result<Pipeline> {
    toml::from_str(text).map_err(|e| Error::Project(e.to_string()))
}

pub fn to_string(pipeline: &Pipeline) -> Result<String> {
    toml::to_string(pipeline).map_err(|e| Error::Project(e.to_string()))
}

pub fn load(path: &Path) -> Result<Pipeline> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    from_str(&text).map_err(|e| Error::Project(format!("{}: {e}", path.display())))
}

pub fn save(path: &Path, pipeline: &Pipeline) -> Result<()> {
    std::fs::write(path, to_string(pipeline)?).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })
}

/// Rewrite the relative path parameters of a pipeline whose paths are
/// relative to `from`, so they work relative to `to` (e.g. when saving a
/// project in another directory). Paths below `to` stay relative, others
/// become absolute.
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
            let path = PathBuf::from(text);
            if path.is_absolute() || text.is_empty() {
                continue;
            }
            let abs = from.join(&path);
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
        let text = to_string(&p).unwrap();
        assert_eq!(from_str(&text).unwrap(), p, "{text}");
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
    }

    #[test]
    fn hand_written() {
        let p = from_str(
            r#"
            [[stage]]
            id = "open-file"
            path = "fw.elf"

            [[stage]]
            id = "split"
            dimension = "section"
            "#,
        )
        .unwrap();
        assert_eq!(p.stages.len(), 2);
        assert!(p.stages[1].enabled);
        assert_eq!(
            p.stages[1].params.get("dimension"),
            Some(&ParamValue::Text("section".into()))
        );
    }
}
