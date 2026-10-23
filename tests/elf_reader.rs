// SPDX-License-Identifier: GPL-2.0-only
//! ELF reader checks against facts from readelf/nm on the fixtures.

mod common;
use bloui::model::{FormatInfo, SectionKind, SegmentKind, SymbolBinding, SymbolKind};
use common::*;

#[test]
fn cortex_m_sections() {
    let b = read(FW);
    assert!(b.format.is_arm());
    let sec = |n: &str| b.sections.iter().find(|s| s.name == n).unwrap();
    // readelf -S
    let text = sec(".text");
    assert_eq!((text.vma, text.size), (0x0800_00a0, 0x528));
    assert_eq!(text.kind, SectionKind::Code);
    assert_eq!(text.lma, Some(0x0800_00a0));
    let data = sec(".data");
    assert_eq!((data.vma, data.size), (0x2000_0000, 8));
    assert_eq!(data.kind, SectionKind::Data);
    // Loaded from flash, right after .ARM.exidx.
    assert_eq!(data.lma, Some(0x0800_067c));
    let bss = sec(".bss");
    assert_eq!(
        (bss.kind, bss.nobits, bss.size),
        (SectionKind::Bss, true, 0x19c)
    );
    assert_eq!(bss.lma, None);
    assert_eq!(sec(".ARM.exidx").kind, SectionKind::Unwind);
    assert_eq!(sec(".debug_info").kind, SectionKind::Debug);
    assert!(!sec(".ARM.attributes").alloc);
    let loads = b
        .segments
        .iter()
        .filter(|s| s.kind == SegmentKind::Load)
        .count();
    assert_eq!(loads, 3);
}

#[test]
fn cortex_m_symbols() {
    let b = read(FW);
    let sym = |n: &str| b.symbols.iter().find(|s| s.name == n).unwrap();
    // nm: main is at 0x080000f4 (the ELF has 0x080000f5, the Thumb bit).
    let main = sym("main");
    assert_eq!(
        (main.addr, main.size, main.kind),
        (0x0800_00f4, 0x88, SymbolKind::Function)
    );
    assert_eq!(main.binding, SymbolBinding::Global);
    assert_eq!(b.sections[main.section.unwrap()].name, ".text");
    assert_eq!(sym("Reset_Handler").addr, 0x0800_00a0);
    assert_eq!(sym("crc_table").addr, 0x0800_05c8);
    assert_eq!(sym("scratch").binding, SymbolBinding::Local);
    assert_eq!(sym("NMI_Handler").binding, SymbolBinding::Weak);
    // ARM mapping symbols are dropped.
    assert!(
        !b.symbols
            .iter()
            .any(|s| s.name.starts_with("$t") || s.name.starts_with("$d"))
    );
}

#[test]
fn cortex_m_chunks() {
    let b = read(FW);
    let total_file: u64 = b
        .chunks
        .chunks()
        .iter()
        .filter(|c| c.has_file())
        .map(|c| c.len())
        .sum();
    assert_eq!(total_file, b.data.len() as u64);
    let load: u64 = b
        .chunks
        .chunks()
        .iter()
        .filter(|c| c.is_load())
        .map(|c| c.len())
        .sum();
    // readelf -l: filesz 0x67c + 0x8.
    assert_eq!(load, 0x67c + 8);
    let memory: u64 = b
        .chunks
        .chunks()
        .iter()
        .filter(|c| c.is_memory())
        .map(|c| c.len())
        .sum();
    assert_eq!(memory, 0x67c + 0x1a4 + 0x404);
}

#[test]
fn host_elf() {
    let b = read(HOST);
    assert!(matches!(
        b.format,
        FormatInfo::Elf {
            class64: true,
            machine: 62,
            ..
        }
    ));
    let main = b.symbols.iter().find(|s| s.name == "main").unwrap();
    assert_eq!(main.kind, SymbolKind::Function);
    assert_eq!(b.sections[main.section.unwrap()].name, ".text");
    let sec = |n: &str| b.sections.iter().find(|s| s.name == n).unwrap();
    assert_eq!(sec(".eh_frame").kind, SectionKind::Unwind);
    assert_eq!(sec(".rela.dyn").kind, SectionKind::Relocation);
    assert_eq!(sec(".dynsym").kind, SectionKind::SymbolTable);
}

#[test]
fn not_an_elf() {
    let reg = bloui::registry::Registry::builtin();
    let err = bloui::reader::read_file(&reg.readers, &fixture("cortexm/fw.ld"), None, "x".into());
    assert!(err.is_err());
}
