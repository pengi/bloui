// SPDX-License-Identifier: GPL-2.0-only
//! `bloui-cli`: run a pipeline and print the result tree as text or CSV.

mod output;

use bloui::engine::{AnalysisTree, ColorLabels, Runner, StageStatus};
use bloui::model::Metric;
use bloui::param::{ParamKind, ParamValue, ParamValues};
use bloui::pipeline::StageConfig;
use bloui::project::{Project, ViewSettings, preset};
use bloui::registry::Registry;
use bloui::render::ColorBy;
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

    /// Attach a linker map to the last input (linker-map stage). A map
    /// next to the input (fw.map) is attached automatically.
    #[arg(long, value_name = "FILE")]
    map: Vec<String>,

    /// Set a memory region by hand: `FLASH:0x08000000:64K`. Repeat for
    /// more; all of them form one memory-regions stage.
    #[arg(long, value_name = "NAME:ORIGIN:LENGTH")]
    region: Vec<String>,

    /// Add the free part of each memory region (free-space stage).
    #[arg(long)]
    free: bool,

    /// Add any stage: `'filter-label source=level patterns=^\.debug'`.
    /// Repeat a key for a list; `dimension.fallback=none` sets a nested
    /// parameter.
    #[arg(short = 'S', long, value_name = "'ID K=V ..'")]
    stage: Vec<String>,

    /// Add the stages of a preset (see --list-presets).
    #[arg(short, long, value_name = "NAME")]
    preset: Vec<String>,

    /// Sort by, and show percentages of, this metric. Default: the
    /// project's or preset's, else memory.
    #[arg(short = 'm', long, value_name = "file|memory|load")]
    sort: Option<String>,

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

    /// Also render a cushion treemap to this PNG file.
    #[arg(long, value_name = "FILE.png")]
    treemap: Option<PathBuf>,

    /// Treemap size in pixels.
    #[arg(long, default_value = "1024x640", value_name = "WxH")]
    size: String,

    /// Treemap layout.
    #[arg(long, default_value = "squarified", value_name = "squarified|slice")]
    layout: String,

    /// Color treemap tiles (and the legend) by a tree level, by name or
    /// index, or by any dimension: `section`, `1`, `object-file:archives=flat`.
    /// Default: the project's or preset's, else the first level.
    #[arg(long, value_name = "LEVEL|DIM[:K=V,..]")]
    color_by: Option<String>,

    /// Print a legend for the colors: labels with their sizes.
    #[arg(long)]
    legend: bool,

    /// Save the pipeline (and -m, --color-by) as a project file.
    #[arg(long, value_name = "FILE")]
    save: Option<PathBuf>,

    /// Save the pipeline without its inputs as a user preset.
    #[arg(long, value_name = "NAME")]
    save_preset: Option<String>,

    /// List the presets.
    #[arg(long)]
    list_presets: bool,
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
    let preset_dir = preset::user_dir();
    if cli.list_presets {
        output::presets(&preset::all(preset_dir.as_deref()), &registry);
        return Ok(ExitCode::SUCCESS);
    }

    let mut project = Project::default();
    let mut base_dir = PathBuf::from(".");
    if let Some(p) = &cli.project {
        project = bloui::project::load(p).map_err(|e| e.to_string())?;
        base_dir = p.parent().map_or_else(|| ".".into(), Path::to_path_buf);
    }
    let mut view = project.view.clone();
    let mut pipeline = project.pipeline;
    let (stages, preset_view) = inline_stages(
        cli,
        matches,
        &registry,
        preset_dir.as_deref(),
        cli.project.is_some(),
    )?;
    pipeline.stages.extend(stages);
    // Options win over presets, presets over the project file.
    for v in [Some(preset_view), cli_view(cli)?].into_iter().flatten() {
        view.metric = v.metric.or(view.metric);
        view.color_by = v.color_by.or(view.color_by);
    }
    let sort = view.metric.unwrap_or_default();
    let project = Project {
        description: project.description,
        view: view.clone(),
        pipeline,
    };
    if let Some(path) = &cli.save {
        // Paths are relative to `base_dir`; in the file, to its directory.
        let abs = |p: &Path| std::path::absolute(p).map_err(|e| e.to_string());
        let dir = abs(path.parent().unwrap_or(Path::new(".")))?;
        let mut saved = project.clone();
        bloui::project::rebase_paths(&mut saved.pipeline, &registry, &abs(&base_dir)?, &dir);
        bloui::project::save(path, &saved).map_err(|e| e.to_string())?;
    }
    if let Some(name) = &cli.save_preset {
        let dir = preset_dir.ok_or("no config directory: set HOME or XDG_CONFIG_HOME")?;
        let path = preset::save(&dir, name, &project, &registry).map_err(|e| e.to_string())?;
        eprintln!("bloui-cli: saved preset '{name}' as {}", path.display());
    }
    let pipeline = project.pipeline;
    if !pipeline.stages.iter().any(|s| s.id == "open-file") {
        return Err("no input: use -i FILE or a project file (see --help)".into());
    }

    let mut runner = Runner::new(registry.clone());
    runner.ctx.base_dir = base_dir;
    let result = runner.run(&pipeline);
    let mut failed = false;
    for (i, report) in result.reports.iter().enumerate() {
        let id = &pipeline.stages[i].id;
        for w in report.notes.iter().filter(|n| n.starts_with("warning: ")) {
            eprintln!("bloui-cli: stage {} ({id}): {w}", i + 1);
        }
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
        output::stats(&mut out, &pipeline, &result.reports, &registry, &opts);
    }
    if cli.csv {
        output::csv(&mut out, &tree, &opts);
    } else {
        output::tree(&mut out, &tree, &opts);
    }
    let color_by = if cli.legend || cli.treemap.is_some() {
        let text = view.color_by.as_deref().unwrap_or("0");
        color_by(text, &tree, &mut runner, &result.state)?
    } else {
        ColorBy::default()
    };
    if cli.legend {
        let entries = bloui::render::legend(&tree, &color_by, AnalysisTree::ROOT, sort, 20);
        output::legend(&mut out, &entries, tree.root().sizes.get(sort), &opts);
    }
    print!("{out}");
    if let Some(path) = &cli.treemap {
        treemap(cli, &tree, &color_by, sort, path)?;
    }
    Ok(if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// View settings given as options.
fn cli_view(cli: &Cli) -> Result<Option<ViewSettings>, String> {
    let metric = match &cli.sort {
        Some(m) => Some(Metric::from_name(m).ok_or(format!("unknown metric '{m}'"))?),
        None => None,
    };
    Ok(Some(ViewSettings {
        metric,
        color_by: cli.color_by.clone(),
    }))
}

/// The stages given as options, in command-line order, and the view
/// settings of the presets among them.
fn inline_stages(
    cli: &Cli,
    matches: &ArgMatches,
    registry: &Registry,
    preset_dir: Option<&Path>,
    absolute_inputs: bool,
) -> Result<(Vec<StageConfig>, ViewSettings), String> {
    let mut stages: Vec<(usize, StageConfig)> = Vec::new();
    let mut add =
        |id: &str, values: &[String], f: &mut dyn FnMut(&str) -> Result<StageConfig, String>| {
            let indices = matches.indices_of(id).into_iter().flatten();
            for (v, i) in values.iter().zip(indices) {
                stages.push((i, f(v)?));
            }
            Ok::<_, String>(())
        };
    // With a project file, relative paths there are relative to the
    // project, so make paths given here absolute.
    let path_value = |v: &str| -> Result<ParamValue, String> {
        let mut path = PathBuf::from(v);
        if absolute_inputs && path.is_relative() {
            path = std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(path);
        }
        Ok(ParamValue::Text(path.to_string_lossy().into_owned()))
    };
    add("input", &cli.input, &mut |v| {
        Ok(StageConfig::new(
            "open-file",
            ParamValues::new().with("path", path_value(v)?),
        ))
    })?;
    add("map", &cli.map, &mut |v| {
        Ok(StageConfig::new(
            "linker-map",
            ParamValues::new().with("path", path_value(v)?),
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
    if let Some(first) = matches.indices_of("region").and_then(|mut i| i.next()) {
        let regions = ParamValue::List(cli.region.clone());
        stages.push((
            first,
            StageConfig::new(
                "memory-regions",
                ParamValues::new().with("regions", regions),
            ),
        ));
    }
    if let Some(i) = matches.index_of("free").filter(|_| cli.free) {
        stages.push((i, StageConfig::new("free-space", ParamValues::new())));
    }
    let mut view = ViewSettings::default();
    let indices = matches.indices_of("preset").into_iter().flatten();
    for (name, i) in cli.preset.iter().zip(indices) {
        let p = preset::find(name, preset_dir)
            .ok_or(format!("unknown preset '{name}' (see --list-presets)"))?;
        let p_stages = preset::strip_inputs(&p.project.pipeline, registry).stages;
        stages.extend(p_stages.into_iter().map(|s| (i, s)));
        view.metric = p.project.view.metric.or(view.metric);
        view.color_by = p.project.view.color_by.or(view.color_by);
    }
    // Stable: a preset's stages keep their order.
    stages.sort_by_key(|(i, _)| *i);
    Ok((stages.into_iter().map(|(_, s)| s).collect(), view))
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

/// A level by index or name, or else a dimension.
fn color_by(
    text: &str,
    tree: &AnalysisTree,
    runner: &mut Runner,
    state: &bloui::pipeline::PipelineState,
) -> Result<ColorBy, String> {
    if let Ok(i) = text.parse::<u32>() {
        return Ok(ColorBy::Level(i));
    }
    if let Some(i) = tree.levels.iter().position(|l| l.name == text) {
        return Ok(ColorBy::Level(i as u32));
    }
    let dim = ParamValues::new().with("d", dim_spec(text)).dimension("d");
    let labels = ColorLabels::compute(&mut runner.ctx, state, tree, &dim)
        .map_err(|e| format!("--color-by: {e}"))?;
    Ok(ColorBy::Labels(Arc::new(labels)))
}

fn treemap(
    cli: &Cli,
    tree: &AnalysisTree,
    color_by: &ColorBy,
    sort: Metric,
    path: &Path,
) -> Result<(), String> {
    use bloui::render::{self, LayoutOptions, Rect, TreemapLayout};
    let (w, h) = cli
        .size
        .split_once('x')
        .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))
        .filter(|&(w, h)| w > 0 && h > 0 && w <= 16384 && h <= 16384)
        .ok_or(format!("--size expects WxH, got '{}'", cli.size))?;
    let layout: &dyn TreemapLayout = match cli.layout.as_str() {
        "squarified" => &render::Squarified,
        "slice" => &render::SliceAndDice,
        l => return Err(format!("unknown layout '{l}'")),
    };
    let bounds = Rect::new(0.0, 0.0, w as f64, h as f64);
    let map = layout.layout(
        tree,
        AnalysisTree::ROOT,
        sort,
        bounds,
        &LayoutOptions::default(),
    );
    let colors = color_by.scheme(sort);
    let img = render::cushion::render(&map, tree, &colors, w, h, &Default::default());
    img.write_png(path).map_err(|e| e.to_string())
}
