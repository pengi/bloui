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
