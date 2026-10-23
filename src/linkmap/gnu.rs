// SPDX-License-Identifier: GPL-2.0-only
//! GNU ld map files (`-Wl,-Map=fw.map`).
//!
//! The format is line based. The parts used here:
//!
//! ```text
//! Memory Configuration
//!
//! Name             Origin             Length             Attributes
//! FLASH            0x08000000         0x00010000         xr
//!
//! Linker script and memory map
//!
//! .text           0x080000a0      0x528            <- output section
//!  *(.text .text.*)                                <- script, ignored
//!  .text.Reset_Handler                             <- input section, name only
//!                 0x080000a0       0x40 build/startup.o
//!                 0x080000a0                Reset_Handler   <- symbol, ignored
//!  *fill*         0x080000e2        0x2            <- padding
//!  .text          0x08000284       0x30 lib.a(obj.o)
//! .data           0x20000000        0x8 load address 0x0800067c
//! ```
//!
//! Long names push the numbers to the next line, for output and input
//! sections alike.

use crate::model::{InputSection, LinkMap, MemoryRegion, ObjectFile, OutputSection};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Other,
    Discarded,
    Memory,
    Map,
}

fn hex(s: &str) -> Option<u64> {
    u64::from_str_radix(s.strip_prefix("0x")?, 16).ok()
}

struct Parser {
    map: LinkMap,
    objects: HashMap<String, usize>,
    current_output: Option<usize>,
    pending_output: Option<String>,
    pending_input: Option<String>,
}

impl Parser {
    fn object(&mut self, text: &str) -> Option<usize> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        if let Some(&i) = self.objects.get(text) {
            return Some(i);
        }
        let (archive, member) = match text.strip_suffix(')').and_then(|t| t.split_once('(')) {
            Some((a, m)) if !a.is_empty() => (Some(a.to_owned()), m.to_owned()),
            _ => (None, text.to_owned()),
        };
        let i = self.map.objects.len();
        self.map.objects.push(ObjectFile {
            full: text.to_owned(),
            archive,
            member,
        });
        self.objects.insert(text.to_owned(), i);
        Some(i)
    }

    /// An input section with its numbers: `name addr size [object]`, where
    /// `rest` is everything after the name.
    fn input(&mut self, part: Part, name: &str, rest: &[&str]) {
        let (Some(addr), Some(size)) = (
            rest.first().and_then(|t| hex(t)),
            rest.get(1).and_then(|t| hex(t)),
        ) else {
            return;
        };
        let object = if name == "*fill*" {
            None
        } else {
            self.object(&rest[2..].join(" "))
        };
        let output = match part {
            Part::Map => match self.current_output {
                Some(o) => o,
                None => return,
            },
            _ => usize::MAX,
        };
        let sec = InputSection {
            output,
            name: name.to_owned(),
            addr,
            size,
            object,
        };
        if part == Part::Map {
            self.map.inputs.push(sec);
        } else if object.is_some() {
            self.map.discarded.push(sec);
        }
    }

    fn output(&mut self, name: &str, rest: &[&str]) {
        let (Some(addr), Some(size)) = (
            rest.first().and_then(|t| hex(t)),
            rest.get(1).and_then(|t| hex(t)),
        ) else {
            // Not an output section after all (an assignment or similar).
            self.current_output = None;
            return;
        };
        let load_addr = match rest.get(2..5) {
            Some(["load", "address", a]) => hex(a),
            _ => None,
        };
        self.current_output = Some(self.map.outputs.len());
        self.map.outputs.push(OutputSection {
            name: name.to_owned(),
            addr,
            size,
            load_addr,
        });
    }

    fn line(&mut self, part: Part, line: &str) {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.is_empty() {
            return;
        }
        match part {
            Part::Memory => {
                if let [name, origin, length, rest @ ..] = &tokens[..]
                    && *name != "*default*"
                    && let (Some(origin), Some(length)) = (hex(origin), hex(length))
                {
                    self.map.regions.push(MemoryRegion {
                        name: name.to_string(),
                        origin,
                        length,
                        attrs: rest.join(" "),
                    });
                }
            }
            Part::Map | Part::Discarded => {
                let starts_at_col0 = !line.starts_with([' ', '\t']);
                let one_space = line.starts_with(' ') && !line[1..].starts_with([' ', '\t']);
                if starts_at_col0 && part == Part::Map {
                    self.pending_input = None;
                    self.pending_output = None;
                    let name = tokens[0];
                    if ["LOAD", "OUTPUT", "START", "END", "INCLUDE", "OUTPUT_FORMAT"]
                        .iter()
                        .any(|k| name == *k || name.starts_with(&format!("{k}(")))
                    {
                        return;
                    }
                    if tokens.len() == 1 {
                        self.pending_output = Some(name.to_owned());
                    } else {
                        self.output(name, &tokens[1..]);
                    }
                } else if one_space && !tokens[0].contains('(') {
                    self.pending_input = None;
                    if tokens.len() == 1 {
                        self.pending_input = Some(tokens[0].to_owned());
                    } else {
                        self.input(part, tokens[0], &tokens[1..]);
                    }
                } else if tokens.len() >= 2 && hex(tokens[0]).is_some() && hex(tokens[1]).is_some()
                {
                    // Numbers for a name on the line before.
                    if let Some(name) = self.pending_input.take() {
                        self.input(part, &name, &tokens);
                    } else if let Some(name) = self.pending_output.take() {
                        self.output(&name, &tokens);
                    }
                } else {
                    // Symbols, assignments, script statements.
                    self.pending_input = None;
                    if !starts_at_col0 && self.pending_output.is_some() {
                        self.pending_output = None;
                    }
                }
            }
            Part::Other => {}
        }
    }
}

pub fn parse(text: &str) -> Result<LinkMap, String> {
    let mut p = Parser {
        map: LinkMap::default(),
        objects: HashMap::new(),
        current_output: None,
        pending_output: None,
        pending_input: None,
    };
    let mut part = Part::Other;
    let mut seen_map = false;
    for line in text.lines() {
        let next = match line.trim_end() {
            "Archive member included to satisfy reference by file (symbol)"
            | "Archive member included because of file (symbol)"
            | "Allocating common symbols"
            | "Cross Reference Table" => Some(Part::Other),
            "Discarded input sections" => Some(Part::Discarded),
            "Memory Configuration" => Some(Part::Memory),
            "Linker script and memory map" => {
                seen_map = true;
                Some(Part::Map)
            }
            _ => None,
        };
        match next {
            Some(n) => {
                part = n;
                p.pending_input = None;
                p.pending_output = None;
            }
            None => p.line(part, line),
        }
    }
    if !seen_map {
        return Err("not a GNU ld map file (no \"Linker script and memory map\")".into());
    }
    Ok(p.map)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP: &str = "\
Discarded input sections

 .text.unused   0x00000000        0xc build/main.o
 .text          0x00000000        0x0 build/main.o

Memory Configuration

Name             Origin             Length             Attributes
FLASH            0x08000000         0x00010000         xr
RAM              0x20000000         0x00004000         xrw
*default*        0x00000000         0xffffffff

Linker script and memory map

LOAD build/main.o
                0x20004000                        _estack = (ORIGIN (RAM) + LENGTH (RAM))

.text           0x08000000       0x40
 *(.text .text.*)
 .text.Reset_Handler
                0x08000000       0x20 build/startup.o
                0x08000000                Reset_Handler
 *fill*         0x08000020        0x4 
 .text          0x08000024       0x1c lib/libhal.a(gpio.o)
                0x08000024                gpio_init

.data           0x20000000        0x8 load address 0x08000040
 .data.x        0x20000000        0x8 build/main.o

.ARM.attributes
                0x00000000       0x2e
 .ARM.attributes
                0x00000000       0x2e build/main.o
OUTPUT(fw.elf elf32-littlearm)
";

    #[test]
    fn gnu_map() {
        let m = parse(MAP).unwrap();
        assert_eq!(m.regions.len(), 2);
        assert_eq!(m.regions[1].name, "RAM");
        assert_eq!(
            (m.regions[1].origin, m.regions[1].length),
            (0x2000_0000, 0x4000)
        );
        assert_eq!(m.regions[1].attrs, "xrw");

        let names: Vec<_> = m.outputs.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(names, [".text", ".data", ".ARM.attributes"]);
        assert_eq!(m.outputs[1].load_addr, Some(0x0800_0040));

        let ins: Vec<_> = m
            .inputs
            .iter()
            .map(|i| {
                (
                    i.output,
                    i.name.as_str(),
                    i.addr,
                    i.size,
                    i.object.map(|o| m.objects[o].full.as_str()),
                )
            })
            .collect();
        assert_eq!(
            ins,
            [
                (
                    0,
                    ".text.Reset_Handler",
                    0x0800_0000,
                    0x20,
                    Some("build/startup.o")
                ),
                (0, "*fill*", 0x0800_0020, 4, None),
                (0, ".text", 0x0800_0024, 0x1c, Some("lib/libhal.a(gpio.o)")),
                (1, ".data.x", 0x2000_0000, 8, Some("build/main.o")),
                (2, ".ARM.attributes", 0, 0x2e, Some("build/main.o")),
            ]
        );
        let gpio = m.objects.iter().find(|o| o.member == "gpio.o").unwrap();
        assert_eq!(gpio.archive.as_deref(), Some("lib/libhal.a"));
        assert_eq!(m.discarded.len(), 2);
        assert_eq!(m.discarded[0].name, ".text.unused");
    }

    #[test]
    fn not_a_map() {
        assert!(parse("hello\nworld\n").is_err());
    }
}
