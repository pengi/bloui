# Stages and dimensions

`bloui-cli --list` prints this list from the code, with every parameter's
type and default.

## Stages

### `open-file` (input)

Loads a binary and adds all its bytes.

| Parameter | Default | Meaning |
|-----------|---------|---------|
| `path` | | the file; relative paths are relative to the project file |
| `name` | file name | name shown in the tree (`file` dimension) |
| `reader` | `auto` | `auto` or `elf` |

Levels split before this stage get `[not split]` for this file's bytes.

### `split` (split)

Cuts the bytes at a dimension's boundaries and adds a tree level.

| Parameter | Default | Meaning |
|-----------|---------|---------|
| `dimension` | `section` | the dimension, with its own parameters |
| `name` | dimension name | the level's name |

### `filter-label` (filter)

Keeps or removes bytes by their label. The label comes from a dimension
(which doesn't have to be a tree level: "only `.text`" while splitting by
symbol), or from an existing tree level.

| Parameter | Default | Meaning |
|-----------|---------|---------|
| `source` | `dimension` | `dimension` or `level` |
| `dimension` | `section` | used with `source=dimension` |
| `level` | last | level name or index, with `source=level` |
| `mode` | `exclude` | `exclude` (remove matches) or `include` (keep only matches) |
| `match` | `regex` | `regex` (matches anywhere; anchor with `^`/`$`) or `exact` |
| `patterns` | | list of patterns; a label matching any of them matches |

### `filter-space` (filter)

Keeps bytes by where they are.

| Parameter | Default | Meaning |
|-----------|---------|---------|
| `keep` | `any` | `any`, `file` (in the file), `memory` (allocated at runtime), `load` (loaded: flash image) |
| `space` | `vma` | the space of the range: `vma`, `lma` or `file` |
| `start`, `end` | empty | the range, e.g. `0x20000000`, `64K`; empty means unbounded |

## Dimensions

| Id | Labels | Unattributed bytes |
|----|--------|--------------------|
| `file` | the input's name | |
| `segment` | loadable segment, `LOAD #<phdr index> [RWX]`, by runtime address | `[no segment]` |
| `section` | output section; also `[ELF header]`, `[program headers]`, `[section headers]` | `[no section]` (padding) |
| `category` | `code`, `rodata`, `data`, `bss`, `unwind`, `debug`, `symtab`, `reloc`, `meta`, `headers` | `[unmapped]` |
| `symbol` | symbol name; the smallest symbol wins where symbols overlap | `[<section>]`, or `[no symbol]` with `fallback=none` |

Categories come from the section type and flags: `unwind` is `.ARM.exidx`,
`.ARM.extab`, `.eh_frame*` and `.gcc_except_table`; `debug` is `.debug*`,
`.zdebug*`, `.stab*` and `.line`; `symtab` includes string tables; `meta` is
notes, `.comment`, `.ARM.attributes` and other non-allocated sections.
