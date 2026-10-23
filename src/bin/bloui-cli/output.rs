// SPDX-License-Identifier: GPL-2.0-only
//! Text and CSV output.

use bloui::engine::{AnalysisTree, NodeId, StageReport, StageStatus};
use bloui::model::{Metric, Sizes};
use bloui::param::{ParamKind, ParamSpec, ParamValue};
use bloui::pipeline::Pipeline;
use bloui::registry::Registry;
use std::fmt::Write;

pub struct Options {
    pub sort: Metric,
    pub depth: Option<usize>,
    pub limit: usize,
    pub bytes: bool,
}

impl Options {
    fn size(&self, v: u64) -> String {
        if self.bytes { v.to_string() } else { human(v) }
    }
}

/// `999`, `1.00Ki`, `12.3Ki`, `123Ki`, `1.20Mi`.
pub fn human(v: u64) -> String {
    if v < 1024 {
        return v.to_string();
    }
    let units = ["Ki", "Mi", "Gi", "Ti"];
    let mut f = v as f64 / 1024.0;
    let mut u = 0;
    while f >= 1024.0 && u + 1 < units.len() {
        f /= 1024.0;
        u += 1;
    }
    if f < 10.0 {
        format!("{f:.2}{}", units[u])
    } else if f < 100.0 {
        format!("{f:.1}{}", units[u])
    } else {
        format!("{f:.0}{}", units[u])
    }
}

fn percent(part: u64, total: u64) -> String {
    if total == 0 {
        "-".into()
    } else {
        format!("{:.1}%", part as f64 * 100.0 / total as f64)
    }
}

fn row(out: &mut String, o: &Options, sizes: Sizes, total: u64, name: &str) {
    let _ = writeln!(
        out,
        "{:>9} {:>9} {:>9} {:>7}  {name}",
        o.size(sizes.file),
        o.size(sizes.memory),
        o.size(sizes.load),
        percent(sizes.get(o.sort), total),
    );
}

pub fn tree(out: &mut String, tree: &AnalysisTree, o: &Options) {
    let levels: Vec<_> = tree.levels.iter().map(|l| l.name.as_str()).collect();
    let _ = writeln!(
        out,
        "{:>9} {:>9} {:>9} {:>7}  {}",
        "FILE",
        "MEMORY",
        "LOAD",
        format!(
            "%{}",
            o.sort.name().chars().next().unwrap().to_ascii_uppercase()
        ),
        if levels.is_empty() {
            "NAME".to_owned()
        } else {
            levels.join(" › ")
        }
    );
    let total = tree.root().sizes.get(o.sort);
    row(out, o, tree.root().sizes, total, "TOTAL");
    children(out, tree, o, AnalysisTree::ROOT, "", total);
}

fn children(
    out: &mut String,
    tree: &AnalysisTree,
    o: &Options,
    id: NodeId,
    prefix: &str,
    total: u64,
) {
    let node = tree.node(id);
    if o.depth.is_some_and(|d| node.depth as usize >= d) {
        return;
    }
    let kids = tree.children(id);
    let shown = if o.limit == 0 {
        kids.len()
    } else {
        kids.len().min(o.limit)
    };
    let rest = &kids[shown..];
    for (i, &kid) in kids[..shown].iter().enumerate() {
        let last = i + 1 == shown && rest.is_empty();
        let n = tree.node(kid);
        let name = format!("{prefix}{}{}", if last { "└─ " } else { "├─ " }, n.name);
        row(out, o, n.sizes, total, &name);
        let deeper = format!("{prefix}{}", if last { "   " } else { "│  " });
        children(out, tree, o, kid, &deeper, total);
    }
    if !rest.is_empty() {
        let sizes = rest
            .iter()
            .fold(Sizes::default(), |a, &k| a + tree.node(k).sizes);
        let name = format!("{prefix}└─ [{} more]", rest.len());
        row(out, o, sizes, total, &name);
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

/// One row per leaf (or node at the depth limit).
pub fn csv(out: &mut String, tree: &AnalysisTree, o: &Options) {
    let levels = o
        .depth
        .map_or(tree.levels.len(), |d| d.min(tree.levels.len()));
    let mut header: Vec<String> = tree.levels[..levels]
        .iter()
        .map(|l| csv_field(&l.name))
        .collect();
    header.extend(["file".into(), "memory".into(), "load".into()]);
    let _ = writeln!(out, "{}", header.join(","));
    let mut stack = vec![AnalysisTree::ROOT];
    while let Some(id) = stack.pop() {
        let node = tree.node(id);
        let at_limit = o.depth.is_some_and(|d| node.depth as usize >= d);
        if node.children.is_empty() || at_limit {
            let path = tree.path(id);
            let mut cols: Vec<String> = path[1..]
                .iter()
                .map(|&p| csv_field(&tree.node(p).name))
                .collect();
            cols.resize(levels.max(cols.len()), String::new());
            let s = node.sizes;
            cols.extend([s.file.to_string(), s.memory.to_string(), s.load.to_string()]);
            let _ = writeln!(out, "{}", cols.join(","));
        } else {
            stack.extend(node.children.iter().rev());
        }
    }
}

pub fn stats(out: &mut String, pipeline: &Pipeline, reports: &[StageReport], o: &Options) {
    let _ = writeln!(out, "Pipeline ({}):", o.sort.name());
    for (i, (stage, r)) in pipeline.stages.iter().zip(reports).enumerate() {
        let status = match &r.status {
            StageStatus::Ok => "ok".to_owned(),
            StageStatus::Disabled => "disabled".to_owned(),
            StageStatus::Unavailable(w) => format!("skipped: {w}"),
            StageStatus::Failed(e) => format!("failed: {e}"),
        };
        let (input, output) = (r.stats.input.get(o.sort), r.stats.output.get(o.sort));
        let delta = if output >= input {
            format!("+{}", o.size(output - input))
        } else {
            format!("-{}", o.size(input - output))
        };
        let _ = writeln!(
            out,
            "  {:>2}. {:<14} {:>9} → {:<9} {:>9}  {status}",
            i + 1,
            stage.id,
            o.size(input),
            o.size(output),
            delta
        );
    }
    out.push('\n');
}

fn describe_param(p: &ParamSpec) -> String {
    let kind = match &p.kind {
        ParamKind::Bool => "bool".to_owned(),
        ParamKind::Int { min, max } => format!("int {min}..{max}"),
        ParamKind::Size => "size".to_owned(),
        ParamKind::Address => "address".to_owned(),
        ParamKind::Text => "text".to_owned(),
        ParamKind::Regex => "regex".to_owned(),
        ParamKind::Path => "path".to_owned(),
        ParamKind::Choice(items) => items.iter().map(|i| i.value).collect::<Vec<_>>().join("|"),
        ParamKind::TextList => "list".to_owned(),
        ParamKind::Dimension => "dimension".to_owned(),
        ParamKind::Level => "level".to_owned(),
        ParamKind::Table(_) => "table".to_owned(),
    };
    let default = match &p.default {
        ParamValue::Bool(b) => b.to_string(),
        ParamValue::Int(i) if p.kind == ParamKind::Level && *i < 0 => "last".into(),
        ParamValue::Int(i) => i.to_string(),
        ParamValue::Text(t) => t.clone(),
        ParamValue::Map(m) => match m.get("id") {
            Some(ParamValue::Text(t)) => t.clone(),
            _ => String::new(),
        },
        _ => String::new(),
    };
    let mut s = format!("      {:<10} {:<28} {}", p.key, kind, p.label);
    if !default.is_empty() {
        let _ = write!(s, " (default: {default})");
    }
    if !p.help.is_empty() {
        let _ = write!(s, ". {}", p.help);
    }
    s
}

pub fn list(registry: &Registry) {
    println!("Stages:");
    for s in &registry.stages {
        println!("  {:<14} [{}] {}", s.id(), s.kind().name(), s.description());
        for p in s.params() {
            println!("{}", describe_param(&p));
        }
    }
    println!("\nDimensions:");
    for d in &registry.dimensions {
        println!("  {:<14} {}", d.id(), d.description());
        for p in d.params() {
            println!("{}", describe_param(&p));
        }
    }
}
