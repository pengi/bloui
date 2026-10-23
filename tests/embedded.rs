// SPDX-License-Identifier: GPL-2.0-only
//! Linker maps, memory regions and free space on the Cortex-M fixture.

mod common;
use bloui::engine::AnalysisTree;
use bloui::model::Metric;
use bloui::param::ParamValue;
use common::*;

fn map() -> bloui::model::LinkMap {
    bloui::linkmap::load(&fixture("cortexm/fw.map")).unwrap()
}

#[test]
fn parse_fixture_map() {
    let m = map();
    let regions: Vec<_> = m
        .regions
        .iter()
        .map(|r| (r.name.as_str(), r.origin, r.length))
        .collect();
    assert_eq!(
        regions,
        [
            ("FLASH", 0x0800_0000, 0x10000),
            ("RAM", 0x2000_0000, 0x4000)
        ]
    );
    let text = m.output(".text").unwrap();
    assert_eq!((text.addr, text.size), (0x0800_00a0, 0x528));
    assert_eq!(m.output(".data").unwrap().load_addr, Some(0x0800_067c));
    // Input sections of .text add up to it, fill included.
    let sum: u64 = m
        .inputs
        .iter()
        .filter(|i| m.outputs[i.output].name == ".text")
        .map(|i| i.size)
        .sum();
    assert_eq!(sum, 0x528);
    let gpio = m.objects.iter().find(|o| o.member == "gpio.o").unwrap();
    assert_eq!(gpio.archive.as_deref(), Some("build/libhal.a"));
    assert!(m.objects.iter().any(|o| o.member == "_udivmoddi4.o"));
    assert!(
        m.discarded
            .iter()
            .any(|d| d.name == ".text.unused_function")
    );
}

#[test]
fn map_check() {
    let m = map();
    assert!(bloui::linkmap::check(&m, &read(FW)).is_ok());
    assert!(bloui::linkmap::check(&m, &read(HOST)).is_err());
    let mut stale = m.clone();
    stale
        .outputs
        .iter_mut()
        .find(|o| o.name == ".text")
        .unwrap()
        .size += 4;
    let err = bloui::linkmap::check(&stale, &read(FW))
        .unwrap_err()
        .to_string();
    assert!(err.contains(".text"), "{err}");
}

#[test]
fn map_is_attached_automatically() {
    let (result, _) = run(vec![open(FW)]);
    assert!(result.state.binaries[0].attachments.linkmap.is_some());
    assert!(result.reports[0].notes[0].starts_with("attached "));
    let off = stage(
        "open-file",
        &[
            ("path", text(fixture(FW).to_str().unwrap())),
            ("map", text("off")),
        ],
    );
    let (result, _) = run(vec![off]);
    assert!(result.state.binaries[0].attachments.linkmap.is_none());
}

#[test]
fn regions_add_up_with_free_space() {
    let t = tree(
        vec![
            open(FW),
            split("memory-region"),
            stage("free-space", &[]),
            split("section"),
        ],
        Metric::Memory,
    );
    assert_eq!(find(&t, &["FLASH"]).unwrap().sizes.memory, 64 * 1024);
    // .data counts where it is stored, in flash. Its runtime copy in RAM
    // isn't shown, and isn't free either, so RAM adds up to 8 bytes less.
    assert_eq!(find(&t, &["RAM"]).unwrap().sizes.memory, 16 * 1024 - 8);
    assert_eq!(find(&t, &["FLASH", ".data"]).unwrap().sizes.load, 8);
    assert!(find(&t, &["RAM", ".data"]).is_none());
    let flash_free = find(&t, &["FLASH", "[free]"]).unwrap().sizes.memory;
    assert_eq!(flash_free, 0x10000 - 0x67c - 8);
    let ram_free = find(&t, &["RAM", "[free]"]).unwrap().sizes.memory;
    assert_eq!(ram_free, 0x4000 - 0x1a4 - 0x404);
}

#[test]
fn free_space_before_the_split() {
    // Free space added first is labeled by later splits.
    let t = tree(
        vec![open(FW), stage("free-space", &[]), split("memory-region")],
        Metric::Memory,
    );
    assert_eq!(find(&t, &["FLASH"]).unwrap().sizes.memory, 64 * 1024);
    let t = tree(
        vec![open(FW), stage("free-space", &[]), split("section")],
        Metric::Memory,
    );
    assert_eq!(
        find(&t, &["[free]"]).unwrap().sizes.memory,
        0x10000 - 0x67c - 8 + 0x4000 - 0x1a4 - 0x404
    );
}

#[test]
fn object_files_cover_flash() {
    let t = tree(
        vec![
            open(FW),
            stage("filter-space", &[("keep", text("load"))]),
            split("object-file"),
        ],
        Metric::Load,
    );
    let total = t.root().sizes.load;
    assert_eq!(total, 0x67c + 8);
    assert_eq!(
        find(&t, &["libhal.a", "gpio.o"]).unwrap().sizes.load,
        0x14 + 0x1c
    );
    assert_eq!(
        find(&t, &["libgcc.a", "_udivmoddi4.o"]).unwrap().sizes.load,
        0x310 + 8
    );
    // Reset_Handler with its literal pool, though the symbol is smaller.
    assert_eq!(find(&t, &["startup.o"]).unwrap().sizes.load, 0x40);
    assert_eq!(find(&t, &["[fill]"]).unwrap().sizes.load, 2);
    assert!(find(&t, &["[no object]"]).is_none());
    assert!(find(&t, &["[.text]"]).is_none());
}

#[test]
fn input_sections() {
    let t = tree(vec![open(FW), split("input-section")], Metric::Memory);
    assert_eq!(
        find(&t, &[".text.startup.main"]).unwrap().sizes.memory,
        0x88
    );
    assert_eq!(find(&t, &[".bss.scratch"]).unwrap().sizes.memory, 0x100);
}

#[test]
fn manual_regions_replace_the_maps() {
    let regions = ParamValue::List(vec![
        "CODE:0x08000000:1K".into(),
        "REST:0x08000400:63K".into(),
    ]);
    let t = tree(
        vec![
            open(FW),
            stage("memory-regions", &[("regions", regions)]),
            split("memory-region"),
        ],
        Metric::Memory,
    );
    let names: Vec<_> = t
        .children(AnalysisTree::ROOT)
        .iter()
        .map(|&c| &*t.node(c).name)
        .collect();
    assert!(names.contains(&"CODE") && names.contains(&"REST") && !names.contains(&"FLASH"));
    assert_eq!(find(&t, &["CODE"]).unwrap().sizes.memory, 1024);
}

#[test]
fn region_unavailable_without_map() {
    let off = stage(
        "open-file",
        &[
            ("path", text(fixture(FW).to_str().unwrap())),
            ("map", text("off")),
        ],
    );
    let mut runner =
        bloui::engine::Runner::new(std::sync::Arc::new(bloui::registry::Registry::builtin()));
    let r = runner.run(&bloui::pipeline::Pipeline {
        stages: vec![off, split("memory-region"), stage("free-space", &[])],
    });
    assert!(matches!(
        r.reports[1].status,
        bloui::engine::StageStatus::Unavailable(_)
    ));
    assert!(matches!(
        r.reports[2].status,
        bloui::engine::StageStatus::Unavailable(_)
    ));
}

#[test]
fn late_input_gets_existing_levels() {
    let t = tree(vec![open(FW), split("section"), open(HOST)], Metric::File);
    assert!(find(&t, &[".text"]).unwrap().sizes.file > 0x528);
    assert!(find(&t, &[".dynsym"]).is_some());
    assert!(find(&t, &["[not split]"]).is_none());
}

#[test]
fn stale_map_stage_fails() {
    let dir = std::env::temp_dir().join(format!("bloui-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let text_map = std::fs::read_to_string(fixture("cortexm/fw.map")).unwrap();
    let stale = text_map.replace(
        ".text           0x080000a0      0x528",
        ".text           0x080000a0      0x52c",
    );
    assert_ne!(stale, text_map);
    let path = dir.join("stale.map");
    std::fs::write(&path, stale).unwrap();
    let mut runner =
        bloui::engine::Runner::new(std::sync::Arc::new(bloui::registry::Registry::builtin()));
    let r = runner.run(&bloui::pipeline::Pipeline {
        stages: vec![
            stage(
                "open-file",
                &[
                    ("path", text(fixture(FW).to_str().unwrap())),
                    ("map", text("off")),
                ],
            ),
            stage("linker-map", &[("path", text(path.to_str().unwrap()))]),
        ],
    });
    std::fs::remove_dir_all(&dir).unwrap();
    match &r.reports[1].status {
        bloui::engine::StageStatus::Failed(e) => assert!(e.contains("doesn't match"), "{e}"),
        s => panic!("{s:?}"),
    }
}
