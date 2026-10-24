// SPDX-License-Identifier: GPL-2.0-only
//! Declarative parameters for stages and dimensions.
//!
//! Each stage and dimension describes its parameters with [`ParamSpec`]s.
//! Frontends build forms (or command-line syntax) from them, and project
//! files store the values as TOML.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChoiceItem {
    pub value: &'static str,
    pub label: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParamKind {
    Bool,
    Int {
        min: i64,
        max: i64,
    },
    /// A byte count: `4096`, `0x1000`, `4K`, `1M`.
    Size,
    /// An address like [`ParamKind::Size`], or empty for "not set".
    Address,
    Text,
    Regex,
    Path,
    Choice(Vec<ChoiceItem>),
    TextList,
    /// A dimension and its nested parameters.
    Dimension,
    /// An existing tree level, by name or index.
    Level,
    /// Rows of values, e.g. memory regions.
    Table(Vec<ParamSpec>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParamSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: ParamKind,
    pub default: ParamValue,
    /// Only relevant when another parameter (key) has this value.
    pub visible_when: Option<(&'static str, &'static str)>,
}

impl ParamSpec {
    pub fn new(
        key: &'static str,
        label: &'static str,
        kind: ParamKind,
        default: ParamValue,
    ) -> Self {
        ParamSpec {
            key,
            label,
            help: "",
            kind,
            default,
            visible_when: None,
        }
    }

    pub fn help(mut self, help: &'static str) -> Self {
        self.help = help;
        self
    }

    /// Show this parameter only when `key` is set to `value`.
    pub fn visible_when(mut self, key: &'static str, value: &'static str) -> Self {
        self.visible_when = Some((key, value));
        self
    }

    /// Whether the parameter matters with these values (`specs` gives the
    /// defaults of values that aren't set).
    pub fn is_visible(&self, values: &ParamValues, specs: &[ParamSpec]) -> bool {
        let Some((key, want)) = self.visible_when else {
            return true;
        };
        let value = values
            .get(key)
            .or_else(|| specs.iter().find(|s| s.key == key).map(|s| &s.default));
        matches!(value, Some(ParamValue::Text(t)) if t == want)
    }

    pub fn bool(key: &'static str, label: &'static str, default: bool) -> Self {
        Self::new(key, label, ParamKind::Bool, ParamValue::Bool(default))
    }

    pub fn text(key: &'static str, label: &'static str, default: &str) -> Self {
        Self::new(
            key,
            label,
            ParamKind::Text,
            ParamValue::Text(default.into()),
        )
    }

    pub fn int(key: &'static str, label: &'static str, min: i64, max: i64, default: i64) -> Self {
        Self::new(
            key,
            label,
            ParamKind::Int { min, max },
            ParamValue::Int(default),
        )
    }

    pub fn regex(key: &'static str, label: &'static str, default: &str) -> Self {
        Self::new(
            key,
            label,
            ParamKind::Regex,
            ParamValue::Text(default.into()),
        )
    }

    pub fn path(key: &'static str, label: &'static str) -> Self {
        Self::new(key, label, ParamKind::Path, ParamValue::Text(String::new()))
    }

    pub fn address(key: &'static str, label: &'static str) -> Self {
        Self::new(
            key,
            label,
            ParamKind::Address,
            ParamValue::Text(String::new()),
        )
    }

    pub fn list(key: &'static str, label: &'static str) -> Self {
        Self::new(
            key,
            label,
            ParamKind::TextList,
            ParamValue::List(Vec::new()),
        )
    }

    /// A choice; the default is the first item.
    pub fn choice(
        key: &'static str,
        label: &'static str,
        items: &[(&'static str, &'static str)],
    ) -> Self {
        let items: Vec<ChoiceItem> = items
            .iter()
            .map(|&(value, label)| ChoiceItem { value, label })
            .collect();
        let default = ParamValue::Text(items[0].value.into());
        Self::new(key, label, ParamKind::Choice(items), default)
    }

    pub fn dimension(key: &'static str, label: &'static str, default: &str) -> Self {
        let mut map = ParamValues::default();
        map.set("id", ParamValue::Text(default.into()));
        Self::new(key, label, ParamKind::Dimension, ParamValue::Map(map))
    }

    pub fn table(key: &'static str, label: &'static str, columns: Vec<ParamSpec>) -> Self {
        Self::new(
            key,
            label,
            ParamKind::Table(columns),
            ParamValue::Table(Vec::new()),
        )
    }

    pub fn size(key: &'static str, label: &'static str, default: u64) -> Self {
        Self::new(key, label, ParamKind::Size, ParamValue::Int(default as i64))
    }

    pub fn level(key: &'static str, label: &'static str) -> Self {
        Self::new(key, label, ParamKind::Level, ParamValue::Int(-1))
    }
}

/// A parameter value, as stored in project files.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ParamValue {
    Bool(bool),
    Int(i64),
    Text(String),
    List(Vec<String>),
    Table(Vec<ParamValues>),
    Map(ParamValues),
}

/// Parameter values by key.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ParamValues(pub BTreeMap<String, ParamValue>);

/// A reference to a dimension with its parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct DimRef {
    pub id: String,
    pub params: ParamValues,
}

impl DimRef {
    pub fn new(id: &str) -> Self {
        DimRef {
            id: id.into(),
            params: ParamValues::default(),
        }
    }
}

/// A reference to a tree level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LevelRef {
    Index(usize),
    Name(String),
    /// The last level.
    Last,
}

impl ParamValues {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, key: &str, value: ParamValue) -> Self {
        self.set(key, value);
        self
    }

    pub fn set(&mut self, key: &str, value: ParamValue) {
        self.0.insert(key.into(), value);
    }

    pub fn get(&self, key: &str) -> Option<&ParamValue> {
        self.0.get(key)
    }

    pub fn remove(&mut self, key: &str) -> Option<ParamValue> {
        self.0.remove(key)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Check the values against a schema, coerce text (as typed on a command
    /// line) to the right types, and fill in defaults. The getters below
    /// expect resolved values.
    pub fn resolve(&self, specs: &[ParamSpec]) -> Result<ParamValues> {
        if let Some(key) = self.0.keys().find(|k| !specs.iter().any(|s| s.key == *k)) {
            return Err(Error::param(key.as_str(), "unknown parameter"));
        }
        let mut out = ParamValues::default();
        for spec in specs {
            let value = match self.0.get(spec.key) {
                Some(v) => coerce(spec, v)?,
                None => spec.default.clone(),
            };
            out.set(spec.key, value);
        }
        Ok(out)
    }

    /// A stable string for cache keys.
    pub fn canonical(&self) -> String {
        format!("{:?}", self.0)
    }

    pub fn bool(&self, key: &str) -> bool {
        matches!(self.0.get(key), Some(ParamValue::Bool(true)))
    }

    pub fn int(&self, key: &str) -> i64 {
        match self.0.get(key) {
            Some(ParamValue::Int(v)) => *v,
            _ => 0,
        }
    }

    /// A set [`ParamKind::Address`] or [`ParamKind::Size`] value.
    pub fn opt_u64(&self, key: &str) -> Option<u64> {
        match self.0.get(key) {
            Some(ParamValue::Int(v)) => Some(*v as u64),
            _ => None,
        }
    }

    pub fn text(&self, key: &str) -> &str {
        match self.0.get(key) {
            Some(ParamValue::Text(v)) => v,
            _ => "",
        }
    }

    pub fn list(&self, key: &str) -> &[String] {
        match self.0.get(key) {
            Some(ParamValue::List(v)) => v,
            _ => &[],
        }
    }

    pub fn table(&self, key: &str) -> &[ParamValues] {
        match self.0.get(key) {
            Some(ParamValue::Table(v)) => v,
            _ => &[],
        }
    }

    pub fn dimension(&self, key: &str) -> DimRef {
        match self.0.get(key) {
            Some(ParamValue::Map(m)) => {
                let mut params = m.clone();
                let id = match params.0.remove("id") {
                    Some(ParamValue::Text(id)) => id,
                    _ => String::new(),
                };
                DimRef { id, params }
            }
            _ => DimRef::new(""),
        }
    }

    pub fn level(&self, key: &str) -> LevelRef {
        match self.0.get(key) {
            Some(ParamValue::Int(i)) if *i >= 0 => LevelRef::Index(*i as usize),
            Some(ParamValue::Text(t)) if !t.is_empty() => LevelRef::Name(t.clone()),
            _ => LevelRef::Last,
        }
    }
}

/// Check one value against its spec, as typed in a form or on a command
/// line, and return it coerced to the right type.
pub fn check_value(spec: &ParamSpec, v: &ParamValue) -> Result<ParamValue> {
    coerce(spec, v)
}

/// A value as text for an entry field: sizes in decimal, addresses in
/// hex, lists one per line, "last" levels empty.
pub fn value_text(spec: &ParamSpec, v: &ParamValue) -> String {
    match (&spec.kind, v) {
        (ParamKind::Address, ParamValue::Int(n)) => format!("{n:#x}"),
        (ParamKind::Level, ParamValue::Int(n)) if *n < 0 => String::new(),
        (_, ParamValue::Int(n)) => n.to_string(),
        (_, ParamValue::Bool(b)) => b.to_string(),
        (_, ParamValue::Text(t)) => t.clone(),
        (_, ParamValue::List(l)) => l.join("\n"),
        (_, ParamValue::Map(m)) => m.text("id").to_owned(),
        (_, ParamValue::Table(rows)) => format!("{} rows", rows.len()),
    }
}

fn coerce(spec: &ParamSpec, v: &ParamValue) -> Result<ParamValue> {
    use ParamValue as V;
    let bad = |what: &str| Err(Error::param(spec.key, format!("expected {what}")));
    match (&spec.kind, v) {
        (ParamKind::Bool, V::Bool(_)) => Ok(v.clone()),
        (ParamKind::Bool, V::Text(t)) => match t.as_str() {
            "true" | "yes" | "on" | "1" => Ok(V::Bool(true)),
            "false" | "no" | "off" | "0" => Ok(V::Bool(false)),
            _ => bad("true or false"),
        },
        (ParamKind::Int { min, max }, _) => {
            let n = match v {
                V::Int(n) => *n,
                V::Text(t) => t
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| Error::param(spec.key, "expected an integer"))?,
                _ => return bad("an integer"),
            };
            if n < *min || n > *max {
                return Err(Error::param(spec.key, format!("must be in {min}..={max}")));
            }
            Ok(V::Int(n))
        }
        (ParamKind::Size | ParamKind::Address, V::Int(n)) if *n >= 0 => Ok(v.clone()),
        (ParamKind::Address, V::Text(t)) if t.trim().is_empty() => Ok(V::Text(String::new())),
        (ParamKind::Size | ParamKind::Address, V::Text(t)) => match parse_size(t) {
            Some(n) if n <= i64::MAX as u64 => Ok(V::Int(n as i64)),
            _ => bad("a size like 4096, 0x1000 or 4K"),
        },
        (ParamKind::Text | ParamKind::Path, V::Text(_)) => Ok(v.clone()),
        (ParamKind::Regex, V::Text(t)) => {
            regex::Regex::new(t).map_err(|e| Error::param(spec.key, e.to_string()))?;
            Ok(v.clone())
        }
        (ParamKind::Choice(items), V::Text(t)) => {
            if items.iter().any(|i| i.value == t) {
                Ok(v.clone())
            } else {
                let names: Vec<_> = items.iter().map(|i| i.value).collect();
                bad(&format!("one of: {}", names.join(", ")))
            }
        }
        (ParamKind::TextList, V::List(_)) => Ok(v.clone()),
        (ParamKind::TextList, V::Text(t)) => Ok(V::List(vec![t.clone()])),
        (ParamKind::Dimension, V::Text(id)) => {
            Ok(V::Map(ParamValues::new().with("id", V::Text(id.clone()))))
        }
        (ParamKind::Dimension, V::Map(m)) => match m.get("id") {
            Some(V::Text(_)) => Ok(v.clone()),
            _ => bad("a dimension id"),
        },
        (ParamKind::Level, V::Int(_) | V::Text(_)) => {
            if let V::Text(t) = v
                && let Ok(i) = t.parse::<i64>()
            {
                return Ok(V::Int(i));
            }
            Ok(v.clone())
        }
        (ParamKind::Table(_), V::List(l)) if l.is_empty() => Ok(V::Table(Vec::new())),
        // Rows as text, columns separated by ':', as typed on a command
        // line: `FLASH:0x08000000:64K`.
        (ParamKind::Table(cols), V::Text(t)) => Ok(V::Table(vec![text_row(spec.key, cols, t)?])),
        (ParamKind::Table(cols), V::List(rows)) => rows
            .iter()
            .map(|t| text_row(spec.key, cols, t))
            .collect::<Result<Vec<_>>>()
            .map(V::Table),
        (ParamKind::Table(cols), V::Table(rows)) => rows
            .iter()
            .map(|r| r.resolve(cols))
            .collect::<Result<Vec<_>>>()
            .map(V::Table),
        _ => bad(kind_name(&spec.kind)),
    }
}

fn text_row(key: &str, cols: &[ParamSpec], text: &str) -> Result<ParamValues> {
    let parts: Vec<&str> = text.split(':').collect();
    if parts.len() > cols.len() {
        return Err(Error::param(
            key,
            format!("too many ':'-separated values in '{text}'"),
        ));
    }
    let mut row = ParamValues::new();
    for (col, part) in cols.iter().zip(parts) {
        row.set(col.key, ParamValue::Text(part.to_owned()));
    }
    row.resolve(cols)
}

fn kind_name(kind: &ParamKind) -> &'static str {
    match kind {
        ParamKind::Bool => "true or false",
        ParamKind::Int { .. } => "an integer",
        ParamKind::Size | ParamKind::Address => "a size or address",
        ParamKind::Text | ParamKind::Path | ParamKind::Regex | ParamKind::Choice(_) => "text",
        ParamKind::TextList => "a list of text",
        ParamKind::Dimension => "a dimension",
        ParamKind::Level => "a level name or index",
        ParamKind::Table(_) => "a table",
    }
}

/// Parse `4096`, `0x1000`, `4K`, `4KiB`, `1M`, `1.5K`.
pub fn parse_size(text: &str) -> Option<u64> {
    let t = text.trim().replace('_', "");
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u64::from_str_radix(hex, 16).ok();
    }
    let split = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(t.len());
    let (num, unit) = t.split_at(split);
    let mult: u64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" | "kib" => 1 << 10,
        "m" | "mb" | "mib" => 1 << 20,
        "g" | "gb" | "gib" => 1 << 30,
        _ => return None,
    };
    if num.contains('.') {
        let f: f64 = num.parse().ok()?;
        Some((f * mult as f64).round() as u64)
    } else {
        num.parse::<u64>().ok()?.checked_mul(mult)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(parse_size("4096"), Some(4096));
        assert_eq!(parse_size("0x1000"), Some(4096));
        assert_eq!(parse_size("4K"), Some(4096));
        assert_eq!(parse_size("4 KiB"), Some(4096));
        assert_eq!(parse_size("1.5k"), Some(1536));
        assert_eq!(parse_size("2M"), Some(2 << 20));
        assert_eq!(parse_size("x"), None);
        assert_eq!(parse_size("4Q"), None);
    }

    fn specs() -> Vec<ParamSpec> {
        vec![
            ParamSpec::bool("b", "B", false),
            ParamSpec::choice("c", "C", &[("one", "One"), ("two", "Two")]),
            ParamSpec::address("a", "A"),
            ParamSpec::list("l", "L"),
            ParamSpec::dimension("d", "D", "section"),
        ]
    }

    #[test]
    fn resolve_defaults_and_coercion() {
        let v = ParamValues::new()
            .with("b", ParamValue::Text("yes".into()))
            .with("a", ParamValue::Text("0x10".into()))
            .with("l", ParamValue::Text("x".into()))
            .with("d", ParamValue::Text("symbol".into()));
        let r = v.resolve(&specs()).unwrap();
        assert!(r.bool("b"));
        assert_eq!(r.text("c"), "one");
        assert_eq!(r.opt_u64("a"), Some(16));
        assert_eq!(r.list("l"), ["x"]);
        assert_eq!(r.dimension("d").id, "symbol");
    }

    #[test]
    fn visibility_and_text() {
        let specs = vec![
            ParamSpec::choice("src", "S", &[("dim", "D"), ("level", "L")]),
            ParamSpec::level("level", "L").visible_when("src", "level"),
        ];
        let v = ParamValues::new();
        assert!(!specs[1].is_visible(&v, &specs));
        let v = v.with("src", ParamValue::Text("level".into()));
        assert!(specs[1].is_visible(&v, &specs));
        assert_eq!(value_text(&specs[1], &ParamValue::Int(-1)), "");
        let a = ParamSpec::address("a", "A");
        let typed = check_value(&a, &ParamValue::Text("4K".into())).unwrap();
        assert_eq!(value_text(&a, &typed), "0x1000");
        assert!(check_value(&a, &ParamValue::Text("4Q".into())).is_err());
    }

    #[test]
    fn resolve_errors() {
        let unknown = ParamValues::new().with("zz", ParamValue::Bool(true));
        assert!(unknown.resolve(&specs()).is_err());
        let bad = ParamValues::new().with("c", ParamValue::Text("three".into()));
        assert!(bad.resolve(&specs()).is_err());
        let empty_addr = ParamValues::new().with("a", ParamValue::Text(" ".into()));
        assert_eq!(empty_addr.resolve(&specs()).unwrap().opt_u64("a"), None);
    }
}
