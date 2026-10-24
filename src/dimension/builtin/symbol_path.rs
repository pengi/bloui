// SPDX-License-Identifier: GPL-2.0-only
use super::symbol::{NameStyle, demangle_params, fallback_param, label_symbols, section_fallback};
use crate::dimension::{Dimension, DimensionFactory, Labeling};
use crate::model::{Binary, Interner, Label};
use crate::param::{ParamSpec, ParamValues};
use crate::registry::Registry;
use crate::symname;
use crate::{Error, Result};
use regex::Regex;

pub struct SymbolPathDim;

impl DimensionFactory for SymbolPathDim {
    fn id(&self) -> &'static str {
        "symbol-path"
    }
    fn name(&self) -> &'static str {
        "symbol path"
    }
    fn description(&self) -> &'static str {
        "Symbol name split into a path: namespaces, crates, modules and types, \
         then the function"
    }
    fn params(&self) -> Vec<ParamSpec> {
        let mut params = vec![
            ParamSpec::choice(
                "split",
                "Split at",
                &[
                    ("namespace", "Namespaces (::)"),
                    ("chars", "Any of some characters"),
                    ("regex", "Capture groups of a regex"),
                ],
            ),
            ParamSpec::text("chars", "Characters", "_")
                .help("Each one separates two parts: _ splits uart_init_pins")
                .visible_when("split", "chars"),
            ParamSpec::regex("regex", "Regex", "")
                .help(
                    "Each capture group that matches is a part; names that don't match stay whole",
                )
                .visible_when("split", "regex"),
            ParamSpec::int("depth", "Depth", 0, 64, 0).help(
                "Keep at most this many parts; 0 keeps all. 1 gives the crate or top namespace",
            ),
            ParamSpec::choice(
                "leaf",
                "Last part",
                &[
                    ("keep", "Keep it (the function or variable)"),
                    ("drop", "Drop it: group by namespace and type only"),
                ],
            ),
        ];
        params.extend(demangle_params());
        params.push(fallback_param());
        params
    }
    fn create(&self, params: &ParamValues, _registry: &Registry) -> Result<Box<dyn Dimension>> {
        let split = match params.text("split") {
            "chars" => Split::Chars(params.text("chars").chars().collect()),
            "regex" => Split::Regex(
                Regex::new(params.text("regex"))
                    .map_err(|e| Error::param("regex", e.to_string()))?,
            ),
            _ => Split::Namespace,
        };
        Ok(Box::new(SymbolPath {
            split,
            depth: params.int("depth") as usize,
            drop_leaf: params.text("leaf") == "drop",
            names: NameStyle::from_params(params),
            section_fallback: section_fallback(params),
        }))
    }
}

enum Split {
    Namespace,
    Chars(Vec<char>),
    Regex(Regex),
}

struct SymbolPath {
    split: Split,
    depth: usize,
    drop_leaf: bool,
    names: NameStyle,
    section_fallback: bool,
}

impl SymbolPath {
    fn parts<'a>(&self, name: &'a str) -> Vec<&'a str> {
        let mut parts = match &self.split {
            Split::Namespace => symname::split_path(name),
            Split::Chars(chars) => name
                .split(chars.as_slice())
                .filter(|p| !p.is_empty())
                .collect(),
            Split::Regex(re) => match re.captures(name) {
                Some(caps) => caps
                    .iter()
                    .skip(1)
                    .flatten()
                    .map(|m| m.as_str())
                    .filter(|p| !p.is_empty())
                    .collect(),
                None => Vec::new(),
            },
        };
        if parts.is_empty() {
            parts.push(name);
        }
        if self.drop_leaf {
            parts.pop();
            if parts.is_empty() {
                parts.push("[no namespace]");
            }
        }
        if self.depth > 0 {
            parts.truncate(self.depth);
        }
        parts
    }
}

impl Dimension for SymbolPath {
    fn label(&self, bin: &Binary, interner: &mut Interner) -> Result<Labeling> {
        Ok(label_symbols(
            bin,
            interner,
            self.section_fallback,
            |sym, interner| {
                let name = self.names.name(sym);
                let parts = self.parts(&name);
                Some(Label::from_path(
                    parts.into_iter().map(|p| interner.intern(p)),
                ))
            },
        ))
    }
}
