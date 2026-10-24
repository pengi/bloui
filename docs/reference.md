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
| `map` | `auto` | `auto`: attach `fw.map` or `fw.elf.map` next to `fw.elf` if there is one; `off` |

Levels split before this stage are applied to the new file's bytes too.
Filters before it are not.

### `linker-map` (attach)

Attaches a GNU ld map file to an input. Fails if the map doesn't match the
binary.

| Parameter | Default | Meaning |
|-----------|---------|---------|
| `path` | next to the input | the map file |
| `binary` | last input | the input's name |

### `memory-regions` (attach)

Sets memory regions by hand. They replace the linker map's.

| Parameter | Default | Meaning |
|-----------|---------|---------|
| `regions` | | table of `name`, `origin`, `length`, `attrs`; on the command line, `NAME:ORIGIN:LENGTH` rows |
| `binary` | all inputs | the input's name |

### `free-space` (add)

Adds the unused part of each memory region as `[free]` bytes (in the
`memory` metric). Used means taken by anything at its runtime or load
address.

| Parameter | Default | Meaning |
|-----------|---------|---------|
| `regions` | all | region names |
| `binary` | all inputs | the input's name |

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
| `symbol` | symbol name, demangled (Rust, C++); the smallest symbol wins where symbols overlap | `[<section>]`, or `[no symbol]` with `fallback=none` |
| `memory-region` | memory region, from the linker map or a `memory-regions` stage | `[no region]` |
| `object-file` | object file from the linker map; archive members as `libhal.a` › `gpio.o` | `[<section>]`, `[fill]` for padding |
| `input-section` | input section from the linker map, like `.text.uart_init` | `[<section>]`, `[fill]` for padding |

Parameters:

- `symbol`: `demangle` = `auto` (Rust legacy and v0, C++ Itanium) or
  `off` (raw names); `hash` = `strip` (drop Rust hashes like
  `::h0123…` and crate disambiguators) or `keep`; `fallback` = `section`
  or `none`. C++ template arguments are shown as `cpp_demangle` prints
  them (`(unsigned int)64` where `c++filt` prints `64u`).
- `memory-region`: `by` = `auto` (load address for bytes with content, so
  `.data` is in flash; runtime address otherwise), `vma` or `lma`.
- `object-file`: `archives` = `nest` (`libhal.a` › `gpio.o`), `flat`
  (`libhal.a(gpio.o)`) or `archive` (`libhal.a` only); `paths` = `short`
  (file names) or `full` (as in the map).

`memory-region`, `object-file` and `input-section` are unavailable (the
stage is skipped) for inputs without a linker map or memory regions.

Categories come from the section type and flags: `unwind` is `.ARM.exidx`,
`.ARM.extab`, `.eh_frame*` and `.gcc_except_table`; `debug` is `.debug*`,
`.zdebug*`, `.stab*` and `.line`; `symtab` includes string tables; `meta` is
notes, `.comment`, `.ARM.attributes` and other non-allocated sections.
