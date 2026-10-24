// SPDX-License-Identifier: GPL-2.0-only
//! Relabeling rules: an ordered list of `regex → label`, first match wins.
//! Used by the `rules` dimension and the `relabel` stage.

use crate::model::{Interner, Label};
use crate::param::{ParamSpec, ParamValues};
use crate::{Error, Result};
use regex::Regex;
use std::collections::HashMap;

struct Rule {
    pattern: Regex,
    label: String,
}

pub struct Rules {
    rules: Vec<Rule>,
}

impl Rules {
    /// The table parameter: rows of `pattern` and `label`, `PATTERN=>LABEL`
    /// as text.
    pub fn param(key: &'static str, label: &'static str) -> ParamSpec {
        ParamSpec::table(
            key,
            label,
            vec![
                ParamSpec::regex("pattern", "Pattern", ""),
                ParamSpec::text("label", "Label", "")
                    .help("$1 or ${name} for capture groups; / makes a path"),
            ],
        )
        .row_separator("=>")
        .help(
            "The first rule whose pattern matches gives the label. \
             On the command line: PATTERN=>LABEL",
        )
    }

    /// From the rows of a [`Rules::param`] table.
    pub fn from_table(key: &str, rows: &[ParamValues]) -> Result<Self> {
        let rules = rows
            .iter()
            .map(|row| {
                let pattern = Regex::new(row.text("pattern"))
                    .map_err(|e| Error::param(key, e.to_string()))?;
                Ok(Rule {
                    pattern,
                    label: row.text("label").to_owned(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Rules { rules })
    }

    /// The label of the first rule that matches `text`, with capture
    /// groups filled in.
    pub fn apply(&self, text: &str) -> Option<String> {
        self.rules.iter().find_map(|r| {
            let caps = r.pattern.captures(text)?;
            let mut out = String::new();
            caps.expand(&r.label, &mut out);
            Some(out)
        })
    }
}

/// Applies rules to labels, remembering each label's result.
pub struct Relabeler<'a> {
    rules: &'a Rules,
    /// For labels no rule matches; `None` keeps them.
    unmatched: Option<Label>,
    cache: HashMap<Label, Label>,
}

impl<'a> Relabeler<'a> {
    pub fn new(rules: &'a Rules, unmatched: Option<Label>) -> Self {
        Relabeler {
            rules,
            unmatched,
            cache: HashMap::new(),
        }
    }

    /// Path labels match as their text joined by `/`, and a `/` in the
    /// result makes a path. A result without any text keeps the label.
    pub fn relabel(&mut self, label: &Label, interner: &mut Interner) -> Label {
        if let Some(l) = self.cache.get(label) {
            return l.clone();
        }
        let new = match self.rules.apply(&label.text(interner)) {
            Some(text) => {
                let parts: Vec<_> = text
                    .split('/')
                    .filter(|p| !p.is_empty())
                    .map(|p| interner.intern(p))
                    .collect();
                if parts.is_empty() {
                    label.clone()
                } else {
                    Label::from_path(parts)
                }
            }
            None => self.unmatched.clone().unwrap_or_else(|| label.clone()),
        };
        self.cache.insert(label.clone(), new.clone());
        new
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::param::ParamValue;

    fn rules(rows: &[(&str, &str)]) -> Rules {
        let rows: Vec<_> = rows
            .iter()
            .map(|(p, l)| {
                ParamValues::new()
                    .with("pattern", ParamValue::Text((*p).into()))
                    .with("label", ParamValue::Text((*l).into()))
            })
            .collect();
        Rules::from_table("rules", &rows).unwrap()
    }

    #[test]
    fn first_match_wins() {
        let r = rules(&[
            (r"^hw::uart", "drivers/uart"),
            (r"^hw::(\w+)", "drivers/$1"),
            (r"^(?<krate>\w+)::", "${krate}"),
        ]);
        assert_eq!(
            r.apply("hw::uart::Port::write").as_deref(),
            Some("drivers/uart")
        );
        assert_eq!(
            r.apply("hw::RingBuffer<u8>::pop").as_deref(),
            Some("drivers/RingBuffer")
        );
        assert_eq!(r.apply("core::fmt::write").as_deref(), Some("core"));
        assert_eq!(r.apply("main"), None);
    }

    #[test]
    fn relabels_paths() {
        let r = rules(&[(r"^libhal\.a/(.*)", "hal/$1"), ("^x$", "")]);
        let mut i = Interner::new();
        let other = Label::single(i.intern("[other]"));
        let mut kept = Relabeler::new(&r, None);
        let mut grouped = Relabeler::new(&r, Some(other.clone()));
        let gpio = Label::from_path([i.intern("libhal.a"), i.intern("gpio.o")]);
        assert_eq!(kept.relabel(&gpio, &mut i).text(&i), "hal/gpio.o");
        let main = Label::single(i.intern("main.o"));
        assert_eq!(kept.relabel(&main, &mut i), main);
        assert_eq!(grouped.relabel(&main, &mut i), other);
        // An empty result keeps the label.
        let x = Label::single(i.intern("x"));
        assert_eq!(kept.relabel(&x, &mut i), x);
    }
}
