// SPDX-License-Identifier: GPL-2.0-only
//! Debug information (from DWARF), independent of the debug format. The
//! parser is in `crate::debuginfo`.

use super::{Bytes, Span};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

/// Where a binary's debug information is: the binary itself or a
/// separate debug file. Parsed on first use.
#[derive(Debug)]
pub struct DebugSource {
    pub path: PathBuf,
    pub data: Arc<Bytes>,
    pub little_endian: bool,
    /// The debug sections in `data`, by name (`.debug_info`, …).
    pub sections: Vec<(String, Span)>,
    /// Runtime ranges of the binary's allocated sections. Debug entries
    /// outside them (code the linker dropped, left at address 0) are
    /// ignored.
    pub valid: Vec<Span>,
    pub parsed: OnceLock<Result<Arc<DebugInfo>, String>>,
}

impl DebugSource {
    pub fn section(&self, name: &str) -> &[u8] {
        self.sections
            .iter()
            .find(|(n, _)| n == name)
            .map_or(&[], |(_, s)| &self.data[s.start as usize..s.end as usize])
    }
}

/// All runtime addresses are VMAs.
#[derive(Debug, Default)]
pub struct DebugInfo {
    pub units: Vec<CompUnit>,
    pub files: Vec<SourceFile>,
    /// Code ranges by source line, sorted by start; adjacent rows of the
    /// same line merged.
    pub lines: Vec<LineRange>,
    pub functions: Vec<Function>,
    /// Ranges of functions (depth 0) and of code inlined into them
    /// (depth 1 and deeper, each inside the one above).
    pub scopes: Vec<Scope>,
    /// Variables with a fixed address.
    pub variables: Vec<Variable>,
}

#[derive(Debug, Clone)]
pub struct CompUnit {
    /// As the compiler was given it: `hal/gpio.c`.
    pub name: String,
    pub comp_dir: Option<String>,
    pub producer: Option<String>,
    pub language: Option<String>,
    pub ranges: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceFile {
    /// As in the debug info: relative to `comp_dir`, or absolute.
    pub path: String,
    pub comp_dir: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    pub span: Span,
    /// Index into [`DebugInfo::files`].
    pub file: u32,
    /// 0: no source line (compiler-generated code).
    pub line: u32,
    /// Index into [`DebugInfo::units`].
    pub unit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Function {
    pub name: String,
    pub linkage_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scope {
    pub span: Span,
    pub depth: u32,
    /// Index into [`DebugInfo::functions`].
    pub function: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    pub name: String,
    pub linkage_name: Option<String>,
    pub addr: u64,
    /// From the variable's type, if it could be worked out.
    pub size: Option<u64>,
    pub unit: u32,
    /// Where it is declared: index into [`DebugInfo::files`].
    pub file: Option<u32>,
}
