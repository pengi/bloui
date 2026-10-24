// SPDX-License-Identifier: GPL-2.0-only
//! Comparing two builds: base sizes, changes, sorting, pruning.

mod common;
use bloui::engine::{AnalysisTree, Change, SortKey};
use bloui::model::{Metric, Sizes};
use bloui::pipeline::StageConfig;
use common::*;

const V1: &str = "diff/v1.elf";
const V2: &str = "diff/v2.elf";

fn base(rel: &str) -> StageConfig {
    stage(
        "open-file",
        &[
            ("path", text(&fixture(rel).to_string_lossy())),
            ("role", text("base")),
        ],
    )
}

fn diff(splits: &[&str], metric: Metric) -> AnalysisTree {
    let mut stages = vec![open(V2), base(V1)];
    stages.extend(splits.iter().map(|d| split(d)));
    tree(stages, metric)
}

#[test]
fn no_base_no_diff() {
    let t = tree(vec![open(FW), split("section")], Metric::Load);
    assert!(!t.has_base);
    assert!(t.nodes.iter().all(|n| n.base == Sizes::default()));
}

#[test]
fn a_build_against_itself_has_no_changes() {
    let t = tree(vec![open(FW), base(FW), split("symbol")], Metric::Load);
    assert!(t.has_base);
    for n in &t.nodes {
        assert_eq!(n.sizes, n.base, "{}", n.name);
        assert_eq!(n.change(Metric::Load), Change::Same);
    }
    let changed = t.changed_only();
    assert_eq!(changed.reachable(), [AnalysisTree::ROOT]);
}

#[test]
fn totals_are_the_two_builds() {
    for metric in [Metric::File, Metric::Memory, Metric::Load] {
        let t = diff(&["section"], metric);
        let v1 = tree(vec![open(V1)], metric);
        let v2 = tree(vec![open(V2)], metric);
        assert_eq!(t.root().sizes, v2.root().sizes);
        assert_eq!(t.root().base, v1.root().sizes);
    }
}

#[test]
fn symbol_changes() {
    let t = diff(&["symbol"], Metric::Load);
    let node = |name: &str| find(&t, &[name]).unwrap();
    // From `nm -S` of v1.elf and v2.elf.
    let cases = [
        ("report", 0, 0x2c, Change::New),
        ("uart_put_hex", 0x2c, 0, Change::Removed),
        ("digits.0", 0x11, 0, Change::Removed),
        ("banner", 0x1b, 0x2d, Change::Grown),
        ("main", 0x88, 0x8c, Change::Grown),
        ("crc_table", 0x80, 0x80, Change::Same),
    ];
    for (name, old, new, change) in cases {
        let n = node(name);
        assert_eq!((n.base.load, n.sizes.load), (old, new), "{name}");
        assert_eq!(n.change(Metric::Load), change, "{name}");
        assert_eq!(n.delta(Metric::Load), new as i64 - old as i64);
    }
    // A removed symbol has no address in the current build.
    assert_eq!(node("uart_put_hex").vma, None);
}

#[test]
fn ram_and_object_files() {
    let t = diff(&["memory-region", "object-file"], Metric::Memory);
    let main = find(&t, &["RAM", "main.o"]).unwrap();
    // scratch grows from 256 to 512 bytes.
    assert_eq!(main.delta(Metric::Memory), 256);
    let uart = find(&t, &["FLASH", "uart.o"]).unwrap();
    assert_eq!(uart.change(Metric::Memory), Change::Shrunk);
}

#[test]
fn sort_by_delta_and_name() {
    let mut t = diff(&["symbol"], Metric::Load);
    t.sort_by(SortKey::Delta(Metric::Load));
    let names: Vec<&str> = t
        .children(AnalysisTree::ROOT)
        .iter()
        .take(3)
        .map(|&c| &*t.node(c).name)
        .collect();
    // Ties in change go by current size: report (44 now) first.
    assert_eq!(names, ["report", "uart_put_hex", "[.rodata]"]);
    let deltas: Vec<u64> = t
        .children(AnalysisTree::ROOT)
        .iter()
        .map(|&c| t.node(c).delta(Metric::Load).unsigned_abs())
        .collect();
    assert!(deltas.windows(2).all(|w| w[0] >= w[1]));
    t.sort_by(SortKey::Name);
    let names: Vec<&str> = t
        .children(AnalysisTree::ROOT)
        .iter()
        .map(|&c| &*t.node(c).name)
        .collect();
    assert!(names.windows(2).all(|w| w[0] <= w[1]));
}

#[test]
fn changed_only_keeps_changes_and_their_parents() {
    let t = diff(&["section", "symbol"], Metric::Load).changed_only();
    assert!(find(&t, &[".text", "report"]).is_some());
    assert!(find(&t, &[".text", "uart_put_hex"]).is_some());
    assert!(find(&t, &[".text", "gpio_toggle"]).is_none());
    let reachable = t.reachable();
    assert!(
        reachable[1..]
            .iter()
            .all(|&n| t.node(n).sizes != t.node(n).base)
    );
    // Ids are kept: a node's parent is still its parent.
    for &n in &reachable {
        for &c in t.children(n) {
            assert_eq!(t.node(c).parent, Some(n));
        }
    }
}

#[test]
fn change_colors_and_legend() {
    use bloui::render::ColorScheme;
    use bloui::render::color::{self, ColorBy};
    let t = diff(&["symbol"], Metric::Load);
    let id = |name: &str| find_id(&t, &[name]);
    let scheme = ColorBy::Change.scheme(&t, Metric::Load);
    let paint = |name: &str| scheme.paint(&t, id(name)).color;
    assert_eq!(paint("report"), color::NEW);
    assert_eq!(paint("crc_table"), color::UNCHANGED);
    // banner grew by 67%, main by 3%: banner is redder.
    let (banner, main) = (paint("banner"), paint("main"));
    assert!(
        banner.0 > main.0 && banner.1 < main.1,
        "{banner:?} {main:?}"
    );
    assert_eq!(
        ColorBy::Change
            .label_of(&t, id("uart_put_hex"), Metric::Load)
            .as_deref(),
        Some("removed")
    );
    let legend = bloui::render::legend(&t, &ColorBy::Change, AnalysisTree::ROOT, Metric::Load, 10);
    let entries: Vec<(&str, u64)> = legend.iter().map(|e| (&*e.label, e.size)).collect();
    assert_eq!(
        entries,
        [
            ("new", 0x2c + 27),
            ("grown", 18 + 4),
            ("removed", 0x2c + 0x11),
            ("unchanged", 1700 - 0x2c - 27 - 0x2d - 0x8c),
        ]
    );
}
