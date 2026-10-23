// SPDX-License-Identifier: GPL-2.0-only
//! Snapshot tests of `bloui-cli` output. Run with `BLOUI_BLESS=1` to
//! update the files in `tests/snapshots/`.

use std::path::Path;
use std::process::Command;

fn cli(name: &str, args: &[&str]) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = Command::new(env!("CARGO_BIN_EXE_bloui-cli"))
        .args(args)
        .current_dir(root.join("tests/fixtures"))
        .output()
        .unwrap();
    let mut text = String::from_utf8(out.stdout).unwrap();
    let stderr = String::from_utf8(out.stderr).unwrap();
    if !stderr.is_empty() {
        text.push_str("--- stderr\n");
        text.push_str(&stderr);
    }
    let path = root.join("tests/snapshots").join(format!("{name}.txt"));
    if std::env::var_os("BLOUI_BLESS").is_some() {
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing snapshot {}; run with BLOUI_BLESS=1",
            path.display()
        )
    });
    assert!(
        text == want,
        "snapshot {name} differs:\n--- got\n{text}\n--- want\n{want}"
    );
}

#[test]
fn fw_sections() {
    cli(
        "fw_sections",
        &[
            "-i",
            "cortexm/fw.elf",
            "-s",
            "section",
            "-n",
            "0",
            "--bytes",
        ],
    );
}

#[test]
fn fw_flash_symbols() {
    cli(
        "fw_flash_symbols",
        &[
            "-i",
            "cortexm/fw.elf",
            "-k",
            "load",
            "-s",
            "section",
            "-s",
            "symbol",
            "-m",
            "load",
            "-n",
            "5",
        ],
    );
}

#[test]
fn fw_segments_stats() {
    cli(
        "fw_segments_stats",
        &[
            "-i",
            "cortexm/fw.elf",
            "--exclude",
            r"section=^\.debug",
            "-s",
            "segment",
            "-s",
            "category",
            "--stats",
        ],
    );
}

#[test]
fn fw_csv() {
    cli(
        "fw_csv",
        &[
            "-i",
            "cortexm/fw.elf",
            "-k",
            "memory",
            "-s",
            "section",
            "-s",
            "symbol:fallback=none",
            "--csv",
        ],
    );
}

#[test]
fn host_segments() {
    cli(
        "host_segments",
        &[
            "-i",
            "host/hello.elf",
            "-k",
            "memory",
            "-s",
            "segment",
            "-d",
            "1",
        ],
    );
}

#[test]
fn project_file() {
    cli(
        "project_file",
        &["project.bloui.toml", "-s", "symbol", "-d", "2", "-n", "3"],
    );
}

#[test]
fn errors() {
    cli("error_no_input", &["-s", "section"]);
    cli(
        "error_bad_stage",
        &["-i", "cortexm/fw.elf", "-S", "split dimension=nope"],
    );
}
