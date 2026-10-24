// SPDX-License-Identifier: GPL-2.0-only
use crate::Result;
use crate::dimension::{Dimension, DimensionFactory, Labeling};
use crate::model::{Availability, Binary, InputSection, Interner, Label, LinkMap, Overlap};
use crate::param::{ParamSpec, ParamValues};
use crate::registry::Registry;
use std::path::Path;

fn needs_map(bin: &Binary) -> Availability {
    if bin.attachments.linkmap.is_some() {
        Availability::Available
    } else {
        Availability::Unavailable("needs a linker map".into())
    }
}

/// Label each input section of the map with `label`, at its place in the
/// binary. Bytes the map doesn't cover fall back to `[<section>]`.
fn label_inputs(
    bin: &Binary,
    interner: &mut Interner,
    fallback: &str,
    mut label: impl FnMut(&LinkMap, &InputSection, &mut Interner) -> Label,
) -> Labeling {
    let mut l = Labeling::new(Overlap::FirstWins, Label::single(interner.intern(fallback)));
    let Some(map) = &bin.attachments.linkmap else {
        return l;
    };
    for input in map.inputs.iter().filter(|i| i.size > 0) {
        let out = &map.outputs[input.output];
        let Some(sec) = bin.section(&out.name) else {
            continue;
        };
        let Some(offset) = input.addr.checked_sub(out.addr) else {
            continue;
        };
        let lab = label(map, input, interner);
        for (space, span) in bin.places_in_section(sec, offset, input.size) {
            l.push(space, span, lab.clone());
        }
    }
    super::push_section_fallback(&mut l, bin, interner);
    l
}

pub struct ObjectFileDim;

impl DimensionFactory for ObjectFileDim {
    fn id(&self) -> &'static str {
        "object-file"
    }
    fn name(&self) -> &'static str {
        "object file"
    }
    fn description(&self) -> &'static str {
        "Object file the bytes came from, like libhal.a(gpio.o), from the linker map"
    }
    fn params(&self) -> Vec<ParamSpec> {
        vec![
            ParamSpec::choice(
                "archives",
                "Archive members",
                &[
                    ("nest", "As a level below the archive: libhal.a / gpio.o"),
                    ("flat", "As one name: libhal.a(gpio.o)"),
                    ("archive", "Only the archive: libhal.a"),
                ],
            ),
            ParamSpec::choice(
                "paths",
                "Paths",
                &[("short", "File names only"), ("full", "As in the map")],
            ),
        ]
    }
    fn create(&self, params: &ParamValues, _registry: &Registry) -> Result<Box<dyn Dimension>> {
        Ok(Box::new(Objects {
            archives: params.text("archives").to_owned(),
            short: params.text("paths") == "short",
        }))
    }
}

struct Objects {
    archives: String,
    short: bool,
}

impl Objects {
    fn path<'a>(&self, p: &'a str) -> &'a str {
        if self.short {
            Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(p)
        } else {
            p
        }
    }
}

impl Dimension for Objects {
    fn availability(&self, bin: &Binary) -> Availability {
        needs_map(bin)
    }
    fn label(&self, bin: &Binary, interner: &mut Interner) -> Result<Labeling> {
        Ok(label_inputs(
            bin,
            interner,
            "[no object]",
            |map, input, interner| {
                let Some(o) = input.object.map(|i| &map.objects[i]) else {
                    return Label::single(interner.intern("[fill]"));
                };
                match (&o.archive, self.archives.as_str()) {
                    (Some(a), "nest") => Label::from_path([
                        interner.intern(self.path(a)),
                        interner.intern(&o.member),
                    ]),
                    (Some(a), "archive") => Label::single(interner.intern(self.path(a))),
                    (Some(a), _) => {
                        Label::single(interner.intern(&format!("{}({})", self.path(a), o.member)))
                    }
                    (None, _) => Label::single(interner.intern(self.path(&o.member))),
                }
            },
        ))
    }
}

pub struct InputSectionDim;

impl DimensionFactory for InputSectionDim {
    fn id(&self) -> &'static str {
        "input-section"
    }
    fn name(&self) -> &'static str {
        "input section"
    }
    fn description(&self) -> &'static str {
        "Input section from the linker map, like .text.uart_init; padding is [fill]"
    }
    super::simple_create!();
}

impl Dimension for InputSectionDim {
    fn availability(&self, bin: &Binary) -> Availability {
        needs_map(bin)
    }
    fn label(&self, bin: &Binary, interner: &mut Interner) -> Result<Labeling> {
        Ok(label_inputs(
            bin,
            interner,
            "[no input section]",
            |_, input, interner| {
                let name = if input.is_fill() {
                    "[fill]"
                } else {
                    &input.name
                };
                Label::single(interner.intern(name))
            },
        ))
    }
}
