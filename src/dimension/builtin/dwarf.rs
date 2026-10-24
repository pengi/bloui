// SPDX-License-Identifier: GPL-2.0-only
//! Dimensions from DWARF debug information: compile unit, source file.

use crate::Result;
use crate::dimension::{Dimension, DimensionFactory, Labeling};
use crate::model::debuginfo::{SourceFile, Variable};
use crate::model::{AddrSpace, Availability, Binary, Interner, Label, Overlap, Span, Sym};
use crate::param::{ParamSpec, ParamValues};
use crate::registry::Registry;
use std::collections::HashMap;

pub struct CompileUnitDim;

impl DimensionFactory for CompileUnitDim {
    fn id(&self) -> &'static str {
        "compile-unit"
    }
    fn name(&self) -> &'static str {
        "compile unit"
    }
    fn description(&self) -> &'static str {
        "Compile unit from the DWARF debug info: the source file given to the \
         compiler, with the code and variables it produced"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![paths_param()]
    }
    fn create(&self, params: &ParamValues, _registry: &Registry) -> Result<Box<dyn Dimension>> {
        Ok(Box::new(CompileUnits {
            paths: Paths::from_params(params),
        }))
    }
}

struct CompileUnits {
    paths: Paths,
}

impl Dimension for CompileUnits {
    fn label(&self, bin: &Binary, interner: &mut Interner) -> Result<Labeling> {
        let info = crate::debuginfo::get(bin)?;
        let mut l = new_labeling(bin, interner);
        let labels: Vec<Label> = info
            .units
            .iter()
            .map(|u| {
                // Rust names a unit `src/lib.rs/@/crate.hash-cgu.0`.
                let text = match u.name.split_once("/@/") {
                    Some((path, cgu)) => {
                        let krate = cgu.split('.').next().unwrap_or(cgu);
                        format!("{krate}: {}", self.paths.show(path, u.comp_dir.as_deref()))
                    }
                    None => self.paths.show(&u.name, u.comp_dir.as_deref()),
                };
                Label::single(interner.intern(&text))
            })
            .collect();
        for (u, label) in info.units.iter().zip(&labels) {
            for &r in &u.ranges {
                l.push(AddrSpace::Vma, r, label.clone());
            }
        }
        for v in &info.variables {
            if let Some(span) = variable_span(bin, v) {
                l.push(AddrSpace::Vma, span, labels[v.unit as usize].clone());
            }
        }
        Ok(l)
    }

    fn availability(&self, bin: &Binary) -> Availability {
        needs_debug_info(bin)
    }
}

pub struct SourceFileDim;

impl DimensionFactory for SourceFileDim {
    fn id(&self) -> &'static str {
        "source-file"
    }
    fn name(&self) -> &'static str {
        "source file"
    }
    fn description(&self) -> &'static str {
        "Source file of each byte from the DWARF line table (code inlined \
         from a header is the header's), and of each variable"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            paths_param(),
            ParamSpec::choice(
                "lines",
                "Lines",
                &[("off", "Files only"), ("on", "Split files by source line")],
            ),
        ]
    }
    fn create(&self, params: &ParamValues, _registry: &Registry) -> Result<Box<dyn Dimension>> {
        Ok(Box::new(SourceFiles {
            paths: Paths::from_params(params),
            lines: params.text("lines") == "on",
        }))
    }
}

struct SourceFiles {
    paths: Paths,
    lines: bool,
}

impl SourceFiles {
    fn file_label(&self, f: &SourceFile, interner: &mut Interner) -> Vec<Sym> {
        let text = self.paths.show(&f.path, f.comp_dir.as_deref());
        let mut parts = Vec::new();
        if text.starts_with('/') {
            parts.push(interner.intern("/"));
        }
        parts.extend(
            text.split('/')
                .filter(|p| !p.is_empty())
                .map(|p| interner.intern(p)),
        );
        parts
    }
}

impl Dimension for SourceFiles {
    fn label(&self, bin: &Binary, interner: &mut Interner) -> Result<Labeling> {
        let info = crate::debuginfo::get(bin)?;
        let mut l = new_labeling(bin, interner);
        let files: Vec<Vec<Sym>> = info
            .files
            .iter()
            .map(|f| self.file_label(f, interner))
            .collect();
        let mut labels: HashMap<(u32, u32), Label> = HashMap::new();
        let mut label = |file: u32, line: u32, interner: &mut Interner| {
            let line = if self.lines { line } else { 0 };
            labels
                .entry((file, line))
                .or_insert_with(|| {
                    let mut parts = files[file as usize].clone();
                    if self.lines {
                        parts.push(interner.intern(&format!(":{line}")));
                    }
                    Label::from_path(parts)
                })
                .clone()
        };
        for r in &info.lines {
            l.push(AddrSpace::Vma, r.span, label(r.file, r.line, interner));
        }
        for v in &info.variables {
            if let (Some(span), Some(file)) = (variable_span(bin, v), v.file) {
                l.push(AddrSpace::Vma, span, label(file, v.line, interner));
            }
        }
        Ok(l)
    }

    fn availability(&self, bin: &Binary) -> Availability {
        needs_debug_info(bin)
    }
}

fn needs_debug_info(bin: &Binary) -> Availability {
    if bin.attachments.debug.is_some() {
        Availability::Available
    } else {
        Availability::Unavailable("no DWARF debug info".into())
    }
}

/// Bytes without debug info are labeled by section, like `[.text]`.
fn new_labeling(bin: &Binary, interner: &mut Interner) -> Labeling {
    let mut l = Labeling::new(
        Overlap::SmallestWins,
        Label::single(interner.intern("[no debug info]")),
    );
    super::push_section_fallback(&mut l, bin, interner);
    l
}

/// A variable's bytes: the size of the symbol at its address if there is
/// one, else of its type.
fn variable_span(bin: &Binary, v: &Variable) -> Option<Span> {
    let size = bin
        .symbols
        .iter()
        .filter(|s| s.addr == v.addr && s.size > 0)
        .map(|s| s.size)
        .max()
        .or(v.size)?;
    Some(Span::at(v.addr, size))
}

fn paths_param() -> ParamSpec {
    ParamSpec::choice(
        "paths",
        "Paths",
        &[
            (
                "relative",
                "Relative to the compile directory, if inside it",
            ),
            ("name", "File names only"),
            ("full", "Full paths"),
        ],
    )
}

#[derive(Clone, Copy)]
enum Paths {
    Relative,
    Name,
    Full,
}

impl Paths {
    fn from_params(p: &ParamValues) -> Self {
        match p.text("paths") {
            "name" => Paths::Name,
            "full" => Paths::Full,
            _ => Paths::Relative,
        }
    }

    /// `path` (relative to `comp_dir` or absolute) as shown.
    fn show(self, path: &str, comp_dir: Option<&str>) -> String {
        let full = match comp_dir {
            Some(dir) if !path.starts_with('/') => format!("{}/{path}", dir.trim_end_matches('/')),
            _ => path.to_owned(),
        };
        let full = normalize(&full);
        match self {
            Paths::Full => full,
            Paths::Name => full.rsplit('/').next().unwrap_or(&full).to_owned(),
            Paths::Relative => match comp_dir.map(normalize) {
                Some(dir) if dir != "." => full
                    .strip_prefix(&dir)
                    .and_then(|p| p.strip_prefix('/'))
                    .map_or_else(|| full.clone(), str::to_owned),
                _ => full,
            },
        }
    }
}

/// Drops `.` parts and resolves `..` where possible.
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for p in path.split('/') {
        match p {
            "" | "." => {}
            ".." if parts.last().is_some_and(|l| *l != "..") => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    let joined = parts.join("/");
    match (path.starts_with('/'), joined.is_empty()) {
        (true, _) => format!("/{joined}"),
        (false, true) => ".".into(),
        (false, false) => joined,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths() {
        let p = |mode, path, dir| Paths::show(mode, path, dir);
        assert_eq!(p(Paths::Relative, "hal/gpio.c", Some(".")), "hal/gpio.c");
        assert_eq!(p(Paths::Relative, "./hal/../gpio.c", Some(".")), "gpio.c");
        assert_eq!(
            p(Paths::Full, "hal/gpio.c", Some("/src/fw")),
            "/src/fw/hal/gpio.c"
        );
        assert_eq!(p(Paths::Name, "hal/gpio.c", Some("/src/fw")), "gpio.c");
        assert_eq!(
            p(Paths::Relative, "/usr/include/stdio.h", Some("/src/fw")),
            "/usr/include/stdio.h"
        );
        assert_eq!(
            p(
                Paths::Relative,
                "/rustc/abc/library/core/src/lib.rs",
                Some("/rustc/abc")
            ),
            "library/core/src/lib.rs"
        );
    }
}
