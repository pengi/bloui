# Project files

A project file (`*.bloui.toml`) stores a pipeline: an array of `[[stage]]`
tables, each with an `id`, an optional `enabled = false`, and the stage's
parameters. Optionally, a `description` and a `[view]` table with how to
look at the result.

```toml
description = "Flash usage of the firmware, by section and symbol"

[view]
metric = "load"          # file, memory or load
color_by = "section"     # a level name or index, or a dimension id

[[stage]]
id = "open-file"
path = "build/fw.elf"

[[stage]]
id = "filter-space"
keep = "load"

[[stage]]
id = "split"
dimension = "section"

[[stage]]
id = "split"
dimension = { id = "symbol", fallback = "none" }

[[stage]]
id = "filter-label"
enabled = false
source = "level"
patterns = ['^\[']
```

- Relative paths are relative to the project file's directory.
- A dimension parameter is either its id (`"section"`) or a table with `id`
  and the dimension's parameters.
- Sizes and addresses can be integers or text: `0x20000000`, `"64K"`.
- Unknown parameters are errors, so typos don't go unnoticed.

`bloui-cli --save FILE` writes the current pipeline as a project file.
Stages given on the command line after a project file are appended to it.
Options such as `-m` win over the project's `[view]`.

## Presets

A preset is a project file without input stages (`open-file`): a way of
looking at any input. Applying one keeps the current inputs and replaces
all other stages; its `[view]` is applied too.

Built in:

| Name | Stages |
|------|--------|
| `default` | memory regions with free space, sections, symbols (used for newly opened files) |
| `flash-by-object-file` | only flash contents (`keep = "load"`), object files, symbols; metric load |
| `sections` | sections, symbols |
| `ram` | memory regions by runtime address (`.data` in RAM), free space, regions named like flash or ROM removed, sections, symbols |

User presets live in `~/.config/bloui/presets/NAME.bloui.toml` (or under
`$XDG_CONFIG_HOME`); one with a built-in's name replaces it. Save one with
`bloui-cli --save-preset NAME` or the GUI's Presets menu, or write it by
hand.
