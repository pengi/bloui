// SPDX-License-Identifier: GPL-2.0-only
//! `bloui-cli`: run a pipeline and print the result tree as text or CSV.

mod output;

use bloui::engine::{AnalysisTree, Runner, StageStatus};
use bloui::model::Metric;
use bloui::param::{ParamKind, ParamValue, ParamValues};
use bloui::pipeline::{Pipeline, StageConfig};
use bloui::registry::Registry;
use clap::{ArgMatches, CommandFactory, FromArgMatches, Parser};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

/// Size analysis of ELF files, as a pipeline of stages.
///
/// Stages from a project file run first, then the stages given with -i, -s,
/// -k, --include, --exclude and -S, in command-line order.
#[derive(Parser)]
#[command(name = "bloui-cli", version)]
struct Cli {
    /// Project file (*.bloui.toml) with a pipeline.
    project: Option<PathBuf>,

    /// Add an input file (open-file stage).
    #[arg(short, long, value_name = "FILE")]
    input: Vec<String>,

    /// Split by a dimension, adding a tree level: `section`,
    /// `symbol:fallback=none`.
    #[arg(short, long, value_name = "DIM[:K=V,..]")]
    split: Vec<String>,

    /// Keep only bytes whose DIM label matches REGEX.
    #[arg(long, value_name = "DIM=REGEX")]
    include: Vec<String>,

    /// Remove bytes whose DIM label matches REGEX.
    #[arg(long, value_name = "DIM=REGEX")]
    exclude: Vec<String>,

    /// Keep only bytes in the file, in memory, or loaded (flash image).
    #[arg(short, long, value_name = "file|memory|load")]
    keep: Vec<String>,

    /// Add any stage: `'filter-label source=level patterns=^\.debug'`.
    /// Repeat a key for a list; `dimension.fallback=none` sets a nested
    /// parameter.
    #[arg(short = 'S', long, value_name = "'ID K=V ..'")]
    stage: Vec<String>,

    /// Sort by, and show percentages of, this metric.
    #[arg(
        short = 'm',
        long,
        default_value = "memory",
        value_name = "file|memory|load"
    )]
    sort: String,

    /// Show at most this many tree levels.
    #[arg(short, long)]
    depth: Option<usize>,

    /// Show at most this many children per node (0: all).
    #[arg(short = 'n', long, default_value_t = 20)]
    limit: usize,

    /// Print CSV: one row per leaf, one column per level.
    #[arg(long)]
    csv: bool,

    /// Print exact byte counts.
    #[arg(long)]
    bytes: bool,

    /// Print what each stage did.
    #[arg(long)]
    stats: bool,

    /// List the stages and dimensions, with their parameters.
    #[arg(long)]
    list: bool,

    /// Save the pipeline as a project file.
    #[arg(long, value_name = "FILE")]
    save: Option<PathBuf>,
}

fn main() -> ExitCode {
    let matches = Cli::command().get_matches();
    let cli = match Cli::from_arg_matches(&matches) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };
    match run(&cli, &matches) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("bloui-cli: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: &Cli, matches: &ArgMatches) -> Result<ExitCode, String> {
    let registry = Arc::new(Registry::builtin());
    if cli.list {
        output::list(&registry);
        return Ok(ExitCode::SUCCESS);
    }
    let sort = Metric::from_name(&cli.sort).ok_or(format!("unknown metric '{}'", cli.sort))?;

    let mut pipeline = Pipeline::default();
    let mut base_dir = PathBuf::from(".");
    if let Some(p) = &cli.project {
        pipeline = bloui::project::load(p).map_err(|e| e.to_string())?;
        base_dir = p.parent().map_or_else(|| ".".into(), Path::to_path_buf);
    }
    pipeline.stages.extend(inline_stages(
        cli,
        matches,
        &registry,
        cli.project.is_some(),
    )?);
    if let Some(path) = &cli.save {
        bloui::project::save(path, &pipeline).map_err(|e| e.to_string())?;
    }
    if !pipeline.stages.iter().any(|s| s.id == "open-file") {
        return Err("no input: use -i FILE or a project file (see --help)".into());
    }

    let mut runner = Runner::new(registry.clone());
    runner.ctx.base_dir = base_dir;
    let result = runner.run(&pipeline);
    let mut failed = false;
    for (i, report) in result.reports.iter().enumerate() {
        let id = &pipeline.stages[i].id;
        match &report.status {
            StageStatus::Failed(e) => {
                eprintln!("bloui-cli: stage {} ({id}) failed: {e}", i + 1);
                failed = true;
            }
            StageStatus::Unavailable(e) => {
                eprintln!("bloui-cli: stage {} ({id}) skipped: {e}", i + 1)
            }
            _ => {}
        }
    }
    let tree = AnalysisTree::build(&result.state, &runner.ctx.interner, sort);
    let opts = output::Options {
        sort,
        depth: cli.depth,
        limit: cli.limit,
        bytes: cli.bytes,
    };
    let mut out = String::new();
    if cli.stats {
        output::stats(&mut out, &pipeline, &result.reports, &opts);
    }
    if cli.csv {
        output::csv(&mut out, &tree, &opts);
    } else {
        output::tree(&mut out, &tree, &opts);
    }
    print!("{out}");
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// The stages given as options, in command-line order.
fn inline_stages(
    cli: &Cli,
    matches: &ArgMatches,
    registry: &Registry,
    absolute_inputs: bool,
) -> Result<Vec<StageConfig>, String> {
    let mut stages: Vec<(usize, StageConfig)> = Vec::new();
    let mut add =
        |id: &str, values: &[String], f: &mut dyn FnMut(&str) -> Result<StageConfig, String>| {
            let indices = matches.indices_of(id).into_iter().flatten();
            for (v, i) in values.iter().zip(indices) {
                stages.push((i, f(v)?));
            }
            Ok::<_, String>(())
        };
    add("input", &cli.input, &mut |v| {
        let mut path = PathBuf::from(v);
        // With a project file, relative paths there are relative to the
        // project, so make paths given here absolute.
        if absolute_inputs && path.is_relative() {
            path = std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(path);
        }
        let path = ParamValue::Text(path.to_string_lossy().into_owned());
        Ok(StageConfig::new(
            "open-file",
            ParamValues::new().with("path", path),
        ))
    })?;
    add("split", &cli.split, &mut |v| {
        Ok(StageConfig::new(
            "split",
            ParamValues::new().with("dimension", dim_spec(v)),
        ))
    })?;
    for (key, mode) in [("include", "include"), ("exclude", "exclude")] {
        let values = if key == "include" {
            &cli.include
        } else {
            &cli.exclude
        };
        add(key, values, &mut |v| {
            let (dim, re) = v
                .split_once('=')
                .ok_or(format!("--{key} expects DIM=REGEX, got '{v}'"))?;
            Ok(StageConfig::new(
                "filter-label",
                ParamValues::new()
                    .with("dimension", dim_spec(dim))
                    .with("mode", ParamValue::Text(mode.into()))
                    .with("patterns", ParamValue::List(vec![re.into()])),
            ))
        })?;
    }
    add("keep", &cli.keep, &mut |v| {
        Ok(StageConfig::new(
            "filter-space",
            ParamValues::new().with("keep", ParamValue::Text(v.into())),
        ))
    })?;
    add("stage", &cli.stage, &mut |v| generic_stage(v, registry))?;
    stages.sort_by_key(|(i, _)| *i);
    Ok(stages.into_iter().map(|(_, s)| s).collect())
}

/// `symbol:fallback=none,x=y` → a dimension value.
fn dim_spec(text: &str) -> ParamValue {
    let (id, rest) = text.split_once(':').unwrap_or((text, ""));
    let mut map = ParamValues::new().with("id", ParamValue::Text(id.into()));
    for kv in rest.split(',').filter(|s| !s.is_empty()) {
        let (k, v) = kv.split_once('=').unwrap_or((kv, "true"));
        map.set(k, ParamValue::Text(v.into()));
    }
    ParamValue::Map(map)
}

/// `ID key=value key=value ...`.
fn generic_stage(text: &str, registry: &Registry) -> Result<StageConfig, String> {
    let mut words = text.split_whitespace();
    let id = words.next().ok_or("empty --stage")?;
    let factory = registry
        .stage(id)
        .ok_or(format!("unknown stage '{id}' (see --list)"))?;
    let specs = factory.params();
    let mut params = ParamValues::new();
    for word in words {
        let (key, value) = word
            .split_once('=')
            .ok_or(format!("expected key=value, got '{word}'"))?;
        let value = ParamValue::Text(value.into());
        if let Some((base, sub)) = key.split_once('.') {
            let mut map = match params.get(base) {
                Some(ParamValue::Map(m)) => m.clone(),
                Some(ParamValue::Text(id)) => {
                    ParamValues::new().with("id", ParamValue::Text(id.clone()))
                }
                _ => ParamValues::new(),
            };
            map.set(sub, value);
            params.set(base, ParamValue::Map(map));
            continue;
        }
        let is_dim = specs
            .iter()
            .any(|s| s.key == key && s.kind == ParamKind::Dimension);
        let new = match (params.get(key), value) {
            (Some(ParamValue::Map(m)), ParamValue::Text(id)) if is_dim => {
                ParamValue::Map(m.clone().with("id", ParamValue::Text(id)))
            }
            (Some(ParamValue::Text(old)), ParamValue::Text(v)) => {
                ParamValue::List(vec![old.clone(), v])
            }
            (Some(ParamValue::List(old)), ParamValue::Text(v)) => {
                let mut l = old.clone();
                l.push(v);
                ParamValue::List(l)
            }
            (_, v) => v,
        };
        params.set(key, new);
    }
    Ok(StageConfig::new(id, params))
}
