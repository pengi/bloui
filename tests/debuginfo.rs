// SPDX-License-Identifier: GPL-2.0-only
//! DWARF reading, against facts from readelf/objdump/nm.

mod common;
use bloui::model::{DebugInfo, Span};
use common::*;
use std::sync::Arc;

fn info(rel: &str) -> Arc<DebugInfo> {
    let (result, _) = run(vec![open(rel)]);
    bloui::debuginfo::get(&result.state.binaries[0]).unwrap()
}

fn unit_names(d: &DebugInfo) -> Vec<&str> {
    d.units.iter().map(|u| u.name.as_str()).collect()
}

fn variable<'a>(d: &'a DebugInfo, name: &str) -> &'a bloui::model::debuginfo::Variable {
    d.variables.iter().find(|v| v.name == name).unwrap()
}

/// Line ranges are sorted and don't overlap; inlined scopes are inside a
/// scope one level up.
fn check_invariants(d: &DebugInfo) {
    for w in d.lines.windows(2) {
        assert!(w[0].span.end <= w[1].span.start, "{:x?}", w);
    }
    for s in d.scopes.iter().filter(|s| s.depth > 0) {
        assert!(
            d.scopes.iter().any(|p| p.depth == s.depth - 1
                && p.span.start <= s.span.start
                && s.span.end <= p.span.end),
            "{s:x?}"
        );
    }
}

#[test]
fn c_firmware() {
    let d = info(FW);
    assert_eq!(
        unit_names(&d),
        ["vectors.c", "main.c", "uart.c", "hal/clock.c", "hal/gpio.c"]
    );
    assert_eq!(d.units[0].language.as_deref(), Some("C11"));
    // uart.c's functions, uart_init to the end of USART1_IRQHandler.
    let uart = &d.units[2];
    assert_eq!(uart.ranges.first().unwrap().start, 0x0800_017c);
    assert_eq!(uart.ranges.last().unwrap().end, 0x0800_0248);
    // Reset_Handler (startup.S, no -g) and libgcc have no debug info.
    let covered = |a: u64| {
        d.units
            .iter()
            .any(|u| u.ranges.iter().any(|r| r.contains(a)))
    };
    assert!(!covered(0x0800_00a0));
    assert!(!covered(0x0800_02b4));
    assert!(covered(0x0800_00f4));
    // The first line row: Default_Handler, vectors.c:11.
    let first = d.lines[0];
    assert_eq!(first.span, Span::new(0x0800_00e0, 0x0800_00e2));
    assert_eq!(
        (d.files[first.file as usize].path.as_str(), first.line),
        ("vectors.c", 11)
    );
    // crc16 is inlined into main.
    assert!(
        d.scopes
            .iter()
            .any(|s| s.depth == 1 && d.functions[s.function as usize].name == "crc16")
    );
    let rx = variable(&d, "rx_buffer");
    assert_eq!((rx.addr, rx.size), (0x2000_0114, Some(128)));
    assert_eq!(d.files[rx.file.unwrap() as usize].path, "uart.c");
    assert_eq!(variable(&d, "crc_table").size, Some(128));
    check_invariants(&d);
}

#[test]
fn cxx_firmware() {
    let d = info("cxx/fw.elf");
    assert_eq!(unit_names(&d), ["startup.cpp", "main.cpp", "uart.cpp"]);
    // Template code is in the header.
    let drivers = d.files.iter().position(|f| f.path == "drivers.h").unwrap() as u32;
    let pop = 0x0800_00d4; // RingBuffer<unsigned long, 16>::pop
    assert!(
        d.lines
            .iter()
            .any(|l| l.file == drivers && l.span.contains(pop))
    );
    // Port::write(unsigned char) inlined into write(const char*).
    let write = d
        .scopes
        .iter()
        .find(|s| s.depth == 1 && s.span.start >= 0x0800_017e && s.span.end <= 0x0800_018e)
        .unwrap();
    assert_eq!(
        d.functions[write.function as usize].linkage_name.as_deref(),
        Some("_ZN2hw4uart4Port5writeEh")
    );
    // Sizes of class-typed variables, through their declarations.
    assert_eq!(variable(&d, "rx").size, Some(72));
    assert_eq!(variable(&d, "events").size, Some(72));
    check_invariants(&d);
}

#[test]
fn rust_firmware() {
    let d = info("rust/fw.elf");
    assert_eq!(d.units.len(), 2);
    assert!(d.units[0].name.starts_with("main.rs/@/fw."));
    assert!(
        d.units[1]
            .comp_dir
            .as_deref()
            .unwrap()
            .starts_with("/rustc/")
    );
    assert_eq!(d.units[1].language.as_deref(), Some("Rust"));
    // Deeply nested inlining in core's iterators.
    assert!(d.scopes.iter().any(|s| s.depth >= 3));
    assert_eq!(variable(&d, "GREETING").size, Some(8));
    assert_eq!(variable(&d, "RX").addr, 0x2000_0008);
    check_invariants(&d);
}

#[test]
fn host_and_debug_off() {
    // x86-64, DWARF 5 from the host gcc.
    let d = info(HOST);
    assert_eq!(unit_names(&d), ["hello.c"]);
    check_invariants(&d);
    let off = stage(
        "open-file",
        &[
            ("path", text(&fixture(HOST).to_string_lossy())),
            ("debug", text("off")),
        ],
    );
    let (result, _) = run(vec![off]);
    let host = &result.state.binaries[0];
    assert!(host.attachments.debug.is_none());
    assert!(bloui::debuginfo::get(host).is_err());
}

fn split_with(dim: &str, params: &[(&str, &str)]) -> bloui::pipeline::StageConfig {
    let mut map = bloui::param::ParamValues::new().with("id", text(dim));
    for (k, v) in params {
        map.set(k, text(v));
    }
    stage(
        "split",
        &[("dimension", bloui::param::ParamValue::Map(map))],
    )
}

#[test]
fn compile_units_match_object_files() {
    use bloui::model::Metric;
    for metric in [Metric::Load, Metric::Memory] {
        let cu = tree(vec![open(FW), split("compile-unit")], metric);
        let obj = tree(vec![open(FW), split("object-file")], metric);
        assert_eq!(cu.root().sizes, obj.root().sizes);
        for (unit, object) in [
            ("main.c", vec!["main.o"]),
            ("uart.c", vec!["uart.o"]),
            ("vectors.c", vec!["vectors.o"]),
            ("hal/gpio.c", vec!["libhal.a", "gpio.o"]),
            ("hal/clock.c", vec!["libhal.a", "clock.o"]),
        ] {
            assert_eq!(
                find(&cu, &[unit]).map(|n| n.sizes.get(metric)),
                find(&obj, &object).map(|n| n.sizes.get(metric)),
                "{unit} {metric:?}"
            );
        }
    }
}

#[test]
fn source_files_and_lines() {
    use bloui::model::Metric;
    let t = tree(vec![open("cxx/fw.elf"), split("source-file")], Metric::Load);
    // RingBuffer's push and pop instances.
    assert_eq!(
        find(&t, &["drivers.h"]).map(|n| n.sizes.load),
        Some(0x16 + 0x18 + 2 * 0x1e)
    );
    let t = tree(
        vec![open(FW), split_with("source-file", &[("lines", "on")])],
        Metric::Load,
    );
    // crc_table is declared on main.c line 11.
    assert_eq!(
        find(&t, &["main.c", ":11"]).map(|n| n.sizes.load),
        Some(128)
    );
    let main = find(&t, &["main.c"]).unwrap();
    let sum: u64 = main.children.iter().map(|&c| t.node(c).sizes.load).sum();
    assert_eq!(sum, main.sizes.load);
    let t = tree(
        vec![open(FW), split_with("source-file", &[("paths", "name")])],
        Metric::Load,
    );
    assert!(find(&t, &["gpio.c"]).is_some());
}

#[test]
fn rust_compile_units() {
    use bloui::model::Metric;
    let t = tree(
        vec![open("rust/fw.elf"), split("compile-unit")],
        Metric::Load,
    );
    assert!(find(&t, &["fw: main.rs"]).is_some());
    assert!(find(&t, &["core: library/core/src/lib.rs"]).is_some());
}

#[test]
fn dwarf_dimensions_need_debug_info() {
    let off = stage(
        "open-file",
        &[
            ("path", text(&fixture(FW).to_string_lossy())),
            ("debug", text("off")),
        ],
    );
    let mut runner = bloui::engine::Runner::new(Arc::new(bloui::registry::Registry::builtin()));
    let result = runner.run(&bloui::pipeline::Pipeline {
        stages: vec![off, split("compile-unit")],
    });
    let errors: Vec<_> = result.errors().map(|(i, e)| (i, e.to_string())).collect();
    assert_eq!(errors, [(1, "no DWARF debug info".to_owned())]);
}

fn read_debugfile(name: &str) -> bloui::model::Binary {
    read(&format!("debugfile/{name}"))
}

#[test]
fn finds_separate_debug_files() {
    use bloui::model::Metric;
    let full = tree(vec![open(FW), split("compile-unit")], Metric::Load);
    for name in ["linked.elf", "plain.elf"] {
        let rel = format!("debugfile/{name}");
        let (result, _) = run(vec![open(&rel), split("compile-unit")]);
        let notes = &result.reports[0].notes;
        assert!(
            notes.iter().any(|n| n.starts_with("debug info from")),
            "{notes:?}"
        );
        let t = tree(vec![open(&rel), split("compile-unit")], Metric::Load);
        // Same sources as the cortexm firmware.
        for unit in ["main.c", "uart.c", "hal/gpio.c"] {
            assert_eq!(
                find(&t, &[unit]).map(|n| n.sizes.load),
                find(&full, &[unit]).map(|n| n.sizes.load),
                "{name} {unit}"
            );
        }
    }
}

#[test]
fn build_id_and_debuglink() {
    let linked = read_debugfile("linked.elf");
    assert_eq!(
        bloui::debuginfo::debuglink(&linked).as_deref(),
        Some("linked.dbg")
    );
    let id = bloui::debuginfo::build_id(&linked).unwrap();
    assert_eq!(id.len(), 20);
    // By build id, under a debug directory.
    let buildid = read_debugfile("buildid.elf");
    let root = fixture("debugfile/root");
    let first = bloui::debuginfo::candidates(&buildid, std::slice::from_ref(&root))[0].clone();
    assert!(first.starts_with(root.join(".build-id")));
    assert!(first.is_file(), "{}", first.display());
    let debug = bloui::reader::read_file(
        &bloui::registry::Registry::builtin().readers,
        &first,
        None,
        "debug".into(),
    )
    .unwrap();
    bloui::debuginfo::check(&buildid, &debug).unwrap();
    // A debug file of another build doesn't match.
    let other = read("cxx/fw.elf");
    assert!(bloui::debuginfo::check(&buildid, &other).is_err());
}

#[test]
fn debug_file_stage() {
    use bloui::model::Metric;
    let root = fixture("debugfile/root");
    let id: String = bloui::debuginfo::build_id(&read_debugfile("buildid.elf"))
        .unwrap()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let path = root
        .join(".build-id")
        .join(&id[..2])
        .join(format!("{}.debug", &id[2..]));
    let with = |path: &str| {
        vec![
            open("debugfile/buildid.elf"),
            stage("debug-file", &[("path", text(path))]),
            split("compile-unit"),
        ]
    };
    let t = tree(with(&path.to_string_lossy()), Metric::Load);
    assert!(find(&t, &["main.c"]).is_some());
    // The wrong file fails the stage.
    let mut runner = bloui::engine::Runner::new(Arc::new(bloui::registry::Registry::builtin()));
    let result = runner.run(&bloui::pipeline::Pipeline {
        stages: with(&fixture("cxx/fw.elf").to_string_lossy()),
    });
    let errors: Vec<_> = result.errors().map(|(i, e)| (i, e.to_string())).collect();
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(errors[0].1.contains("doesn't match"), "{errors:?}");
}
