// SPDX-License-Identifier: GPL-2.0-only
//! Project files (`*.bloui.toml`): a pipeline, stored as TOML.

use crate::pipeline::Pipeline;
use crate::{Error, Result};
use std::path::Path;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::param::{ParamValue, ParamValues};
    use crate::pipeline::StageConfig;

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
