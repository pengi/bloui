# Project files

A project file (`*.bloui.toml`) stores a pipeline: an array of `[[stage]]`
tables, each with an `id`, an optional `enabled = false`, and the stage's
parameters.

```toml
# Flash usage of the firmware, by section and symbol.

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
