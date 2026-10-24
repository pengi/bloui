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
