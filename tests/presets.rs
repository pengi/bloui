// SPDX-License-Identifier: GPL-2.0-only
//! Presets: built-in ones run, apply keeps inputs, saving and listing.

mod common;
use bloui::model::Metric;
use bloui::pipeline::Pipeline;
use bloui::project::preset;
use bloui::project::{Project, ViewSettings};
use bloui::registry::Registry;
use common::*;

fn with_input(p: &preset::Preset) -> Vec<bloui::pipeline::StageConfig> {
    let mut stages = vec![open(FW)];
    stages.extend(p.project.pipeline.stages.clone());
    stages
}

#[test]
fn builtin_presets_run_on_the_firmware() {
    for p in preset::builtin() {
        assert!(p.project.description.is_some(), "{}", p.name);
        // `run` panics on any failed or skipped stage.
        let (result, _) = run(with_input(&p));
        assert!(result.state.set.total().memory > 0, "{}", p.name);
    }
}

#[test]
fn ram_preset_leaves_out_flash() {
    let ram = preset::find("ram", None).unwrap();
    let t = tree(with_input(&ram), Metric::Memory);
    assert!(find(&t, &["FLASH"]).is_none());
    // .data counts at its runtime address here.
    assert_eq!(find(&t, &["RAM", ".data"]).unwrap().sizes.memory, 8);
    assert_eq!(t.root().sizes.memory, 16 * 1024);
}

#[test]
fn apply_keeps_the_inputs() {
    let reg = Registry::builtin();
    let current = Pipeline {
        stages: vec![open(FW), split("segment"), open(HOST)],
    };
    let sections = preset::find("sections", None).unwrap();
    let p = preset::apply(&current, &sections.project.pipeline, &reg);
    let ids: Vec<&str> = p.stages.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["open-file", "open-file", "split", "split"]);
    assert_eq!(p.stages[0], current.stages[0]);
    assert_eq!(p.stages[1], current.stages[2]);
}

#[test]
fn save_and_list_user_presets() {
    let reg = Registry::builtin();
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("presets-test");
    let _ = std::fs::remove_dir_all(&dir);
    let project = Project {
        description: Some("Mine".into()),
        view: ViewSettings {
            metric: Some(Metric::Load),
            color_by: None,
        },
        pipeline: Pipeline {
            stages: vec![open(FW), split("category")],
        },
    };
    let path = preset::save(&dir, "mine", &project, &reg).unwrap();
    assert!(path.ends_with("mine.bloui.toml"));
    // Same name as a built-in: replaces it.
    preset::save(&dir, "sections", &project, &reg).unwrap();
    std::fs::write(dir.join("broken.bloui.toml"), "[[stage]]\nid = 3\n").unwrap();
    assert!(preset::save(&dir, "a/b", &project, &reg).is_err());

    let (all, errors) = preset::all(Some(&dir));
    assert_eq!(errors.len(), 1);
    let mine = all.iter().find(|p| p.name == "mine").unwrap();
    assert!(!mine.builtin);
    // Saved without the input.
    assert_eq!(mine.project.pipeline.stages, [split("category")]);
    assert_eq!(mine.project.view.metric, Some(Metric::Load));
    let sections = all.iter().find(|p| p.name == "sections").unwrap();
    assert!(!sections.builtin);
    assert_eq!(all.iter().filter(|p| p.name == "sections").count(), 1);
}
