// SPDX-License-Identifier: GPL-2.0-only
//! Symbol dimensions on the C++ and Rust fixtures: demangling, hashes.

mod common;
use bloui::model::Metric;
use bloui::param::{ParamValue, ParamValues};
use common::*;

const CXX: &str = "cxx/fw.elf";
const RUST: &str = "rust/fw.elf";

/// A split by a dimension with parameters.
fn split_with(dim: &str, params: &[(&str, &str)]) -> bloui::pipeline::StageConfig {
    let mut map = ParamValues::new().with("id", text(dim));
    for (k, v) in params {
        map.set(k, text(v));
    }
    stage("split", &[("dimension", ParamValue::Map(map))])
}

fn load_size(t: &bloui::engine::AnalysisTree, path: &[&str]) -> Option<u64> {
    find(t, path).map(|n| n.sizes.load)
}

#[test]
fn cxx_demangled() {
    let t = tree(vec![open(CXX), split("symbol")], Metric::Load);
    // From `nm -S`.
    let cases = [
        ("main", 0x68),
        ("hw::uart::Port::operator<<(char const*)", 8),
        ("hw::uart::Port::write(unsigned char)", 6),
        ("hw::uart::Port::write(char const*)", 0x10),
        (
            "hw::RingBuffer<unsigned char, (unsigned int)64>::push(unsigned char) [clone .isra.0]",
            0x16,
        ),
        ("app::(anonymous namespace)::banner", 0x1a),
        // The weak alias HardFault_Handler loses against the global name.
        ("board::default_handler()", 2),
    ];
    for (name, size) in cases {
        assert_eq!(load_size(&t, &[name]), Some(size), "{name}");
    }
    assert!(find(&t, &["HardFault_Handler"]).is_none());
}

#[test]
fn cxx_raw_names() {
    let t = tree(
        vec![open(CXX), split_with("symbol", &[("demangle", "off")])],
        Metric::Load,
    );
    assert_eq!(load_size(&t, &["_ZN2hw4uart4PortlsEPKc"]), Some(8));
    assert!(find(&t, &["main"]).is_some());
}

#[test]
fn rust_demangled() {
    let t = tree(vec![open(RUST), split("symbol")], Metric::Memory);
    let cases = [
        // Legacy mangling (the app crate).
        ("fw::main", 0x96),
        ("<fw::uart::Port as core::fmt::Write>::write_str", 0x2a),
        ("fw::RX", 0x48),
        // An LLVM-split global keeps its suffix.
        ("fw::TICKS.0", 4),
        // v0 mangling (core).
        ("core::str::converts::from_utf8", 0x228),
        ("<core::fmt::Formatter>::pad_integral", 0x266),
    ];
    for (name, size) in cases {
        assert_eq!(
            find(&t, &[name]).map(|n| n.sizes.memory),
            Some(size),
            "{name}"
        );
    }
    // Generic instances get the same name without hashes.
    let pushes: Vec<_> = t
        .children(bloui::engine::AnalysisTree::ROOT)
        .iter()
        .filter(|&&c| &*t.node(c).name == "fw::queue::Ring<T,_>::push")
        .collect();
    assert_eq!(pushes.len(), 1);
    assert_eq!(t.node(*pushes[0]).sizes.memory, 2 * 0x3a);
}

#[test]
fn rust_hashes_kept() {
    let t = tree(
        vec![open(RUST), split_with("symbol", &[("hash", "keep")])],
        Metric::Memory,
    );
    assert!(find(&t, &["fw::main::he66410050b005348"]).is_some());
    assert!(find(&t, &["fw::queue::Ring<T,_>::push::hc73aff96842b8754"]).is_some());
}

#[test]
fn demangling_keeps_totals() {
    for input in [CXX, RUST] {
        let on = tree(vec![open(input), split("symbol")], Metric::File);
        let off = tree(
            vec![open(input), split_with("symbol", &[("demangle", "off")])],
            Metric::File,
        );
        let root = bloui::engine::AnalysisTree::ROOT;
        assert_eq!(on.node(root).sizes, off.node(root).sizes, "{input}");
    }
}

/// Every node's children add up to it (with `[self]` holding own bytes).
fn assert_children_sum(t: &bloui::engine::AnalysisTree) {
    for n in &t.nodes {
        if n.children.is_empty() {
            continue;
        }
        let mut sum = bloui::model::Sizes::default();
        for &c in &n.children {
            sum += t.node(c).sizes;
        }
        assert_eq!(sum, n.sizes, "{}", n.name);
    }
}

#[test]
fn cxx_namespaces() {
    let t = tree(vec![open(CXX), split("symbol-path")], Metric::Load);
    assert_eq!(
        load_size(&t, &["hw", "uart", "Port", "operator<<"]),
        Some(8)
    );
    assert_eq!(
        load_size(&t, &["hw", "uart", "Port", "write"]),
        Some(6 + 0x10)
    );
    assert_eq!(
        load_size(&t, &["app", "(anonymous namespace)", "banner"]),
        Some(0x1a)
    );
    // Both instances of the template.
    assert_eq!(
        t.children(find_id(&t, &["hw"])).len(),
        3,
        "uart and two RingBuffer instances"
    );
    assert_children_sum(&t);
}

#[test]
fn rust_crates() {
    let t = tree(
        vec![open(RUST), split_with("symbol-path", &[("depth", "1")])],
        Metric::Load,
    );
    let all = tree(vec![open(RUST), split("symbol")], Metric::Load);
    let root = bloui::engine::AnalysisTree::ROOT;
    assert_eq!(t.node(root).sizes, all.node(root).sizes);
    // core and the app crate, plus the unmangled C-ABI functions.
    for name in [
        "core",
        "fw",
        "Reset_Handler",
        "SysTick_Handler",
        "[.rodata]",
    ] {
        assert!(find(&t, &[name]).is_some(), "{name}");
    }
    assert!(t.children(find_id(&t, &["core"])).is_empty());
    // The trait impl goes to the type's crate.
    let fw = tree(vec![open(RUST), split("symbol-path")], Metric::Load);
    assert_eq!(
        load_size(&fw, &["fw", "uart", "Port", "write_str"]),
        Some(0x2a)
    );
    assert_children_sum(&fw);
}

#[test]
fn leaf_dropped_gives_self_nodes() {
    let t = tree(
        vec![open(RUST), split_with("symbol-path", &[("leaf", "drop")])],
        Metric::Load,
    );
    // core::fmt::write is in core › fmt itself, next to core::fmt::Formatter.
    assert_eq!(load_size(&t, &["core", "fmt", "[self]"]), Some(0x10c));
    assert!(find(&t, &["core", "fmt", "Formatter"]).is_some());
    // C names have no namespace.
    assert!(find(&t, &["[no namespace]"]).is_some());
    assert_children_sum(&t);
}

#[test]
fn split_by_chars_and_regex() {
    let t = tree(
        vec![open(FW), split_with("symbol-path", &[("split", "chars")])],
        Metric::Load,
    );
    assert_eq!(load_size(&t, &["uart", "put", "hex"]), Some(0x2c));
    assert_eq!(load_size(&t, &["udivmoddi4"]), Some(0x310));
    assert_children_sum(&t);
    let t = tree(
        vec![
            open(FW),
            split_with(
                "symbol-path",
                &[("split", "regex"), ("regex", "^(uart|gpio|clock)_(.*)")],
            ),
        ],
        Metric::Load,
    );
    assert_eq!(load_size(&t, &["uart", "put_hex"]), Some(0x2c));
    assert_eq!(load_size(&t, &["gpio", "toggle"]), Some(0x1c));
    // Names that don't match stay whole.
    assert_eq!(load_size(&t, &["__udivmoddi4"]), Some(0x310));
}

#[test]
fn kind_and_binding() {
    let t = tree(vec![open(CXX), split("symbol-binding")], Metric::Load);
    // The two RingBuffer::pop instances are weak (template instances); the
    // weak HardFault_Handler alias loses against its global target.
    assert_eq!(load_size(&t, &["weak"]), Some(2 * 0x1e));
    let t = tree(vec![open(CXX), split("symbol-kind")], Metric::Memory);
    // rx, events, ticks and the banner; vector_table is an object too.
    assert_eq!(
        find(&t, &["object"]).map(|n| n.sizes.memory),
        Some(0x48 + 0x48 + 4 + 0x1a + 0x40)
    );
    assert!(find(&t, &["function"]).is_some());
}
