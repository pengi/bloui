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
| `debug` | `auto` | `auto`: use the file's DWARF debug info (for the DWARF dimensions), or else a separate debug file (see `debug-file`); `off` |

Levels split before this stage are applied to the new file's bytes too.
Filters before it are not.

### `linker-map` (attach)

Attaches a GNU ld map file to an input. Fails if the map doesn't match the
binary.

| Parameter | Default | Meaning |
|-----------|---------|---------|
| `path` | next to the input | the map file |
| `binary` | last input | the input's name |

### `debug-file` (attach)

Takes an input's DWARF debug info from a separate file, as made by
`objcopy --only-keep-debug`. The file must match: the same build id if
both have one, else the same address and size for every allocated
section. `open-file` already looks for one when the input has no DWARF of
its own, in this order: by build id in
`/usr/lib/debug/.build-id/xx/rest.debug`; by the name in
`.gnu_debuglink`, next to the input, in `.debug/` there, and under
`/usr/lib/debug/<input's directory>/`; then `fw.debug` and
`fw.elf.debug` next to `fw.elf`. Files found that don't match are
reported and skipped.

| Parameter | Default | Meaning |
|-----------|---------|---------|
| `path` | look for it | the debug file |
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

### `relabel` (transform)

Renames a tree level's labels by rules (see [Rules](#rules)). Labels no
rule matches stay as they are. Later inputs and free space get the new
labels too: the level's dimension becomes a `rules` dimension over the old
one.

| Parameter | Default | Meaning |
|-----------|---------|---------|
| `level` | last | level name or index |
| `rules` | | the rules |

### `group-small` (transform)

Within each parent, merges a level's small labels into one (`[other]`).
A label is kept only if it passes every limit that is set. `[free]` is
never grouped, and neither is a single small label (that would only hide
its name). Path labels are grouped whole. Inputs and free space added
after this stage are not grouped.

| Parameter | Default | Meaning |
|-----------|---------|---------|
| `level` | last | level name or index |
| `min-size` | `0` | group labels smaller than this (`1K`); 0 for no limit |
| `min-percent` | `0` | group labels smaller than this share of their parent, 0–100 |
| `top` | `0` | keep at most this many labels per parent, the biggest; 0 for no limit |
| `metric` | `view` | sizes by `file`, `memory`, `load`, or `view`: the metric the result is shown in (`-m`, or the GUI's Metric), so the stage reruns when that changes |
| `label` | `[other]` | the group's label |

### `flatten` (transform)

Drops a tree level, or cuts its path labels to a depth
(`libhal.a` › `gpio.o` → `libhal.a`). Later inputs get cut labels too.

| Parameter | Default | Meaning |
|-----------|---------|---------|
| `level` | last | level name or index |
| `mode` | `drop` | `drop` the level, or `depth` |
| `depth` | `1` | path parts to keep, with `mode=depth` |

## Dimensions

| Id | Labels | Unattributed bytes |
|----|--------|--------------------|
| `file` | the input's name | |
| `segment` | loadable segment, `LOAD #<phdr index> [RWX]`, by runtime address | `[no segment]` |
| `section` | output section; also `[ELF header]`, `[program headers]`, `[section headers]` | `[no section]` (padding) |
| `category` | `code`, `rodata`, `data`, `bss`, `unwind`, `debug`, `symtab`, `reloc`, `meta`, `headers` | `[unmapped]` |
| `symbol` | symbol name, demangled (Rust, C++); the smallest symbol wins where symbols overlap | `[<section>]`, or `[no symbol]` with `fallback=none` |
| `symbol-path` | the demangled symbol name as a path: `hw` › `uart` › `Port` › `write` | like `symbol` |
| `symbol-kind` | `function`, `object`, `tls`, `other` | like `symbol` |
| `symbol-binding` | `global`, `weak`, `local` | like `symbol` |
| `memory-region` | memory region, from the linker map or a `memory-regions` stage | `[no region]` |
| `object-file` | object file from the linker map; archive members as `libhal.a` › `gpio.o` | `[<section>]`, `[fill]` for padding |
| `compile-unit` | DWARF compile unit: `main.c`, `hal/gpio.c`; Rust units as `crate: path`. Code by the unit's ranges, variables by the unit they are defined in | `[<section>]`, `[no debug info]` |
| `source-file` | file from the DWARF line table (code inlined from a header is the header's), variables by where they are declared; as a path: `hal` › `gpio.c`, absolute paths under `/` | `[<section>]`, `[no debug info]` |
| `function` | function from DWARF, demangled from its linkage name; inlined code by the function it came from (`inlines`); variables by name | `[<section>]`, `[no debug info]` |
| `rules` | another dimension's labels, renamed by [rules](#rules) | the other dimension's, renamed too |
| `input-section` | input section from the linker map, like `.text.uart_init` | `[<section>]`, `[fill]` for padding |

Parameters:

- `symbol`: `demangle` = `auto` (Rust legacy and v0, C++ Itanium) or
  `off` (raw names); `hash` = `strip` (drop Rust hashes like
  `::h0123…` and crate disambiguators) or `keep`; `fallback` = `section`
  or `none`. C++ template arguments are shown as `cpp_demangle` prints
  them (`(unsigned int)64` where `c++filt` prints `64u`).
- `symbol-path`: `split` = `namespace` (at `::` outside brackets; C++
  argument lists and return types are dropped; a Rust `<Type as Trait>::f`
  goes under the type's path, or the trait's when the type has none, like
  `<u32 as core::fmt::Display>`), `chars` (at any of the characters in
  `chars`, default `_`) or `regex` (the capture groups of `regex` that
  match are the parts; names that don't match stay whole); `depth` = at
  most this many parts, 0 for all (`depth=1`: crate or top namespace);
  `leaf` = `keep` or `drop` (group by namespace and type only; names
  without one are `[no namespace]`); and `demangle`, `hash`, `fallback`
  as for `symbol`.
- `symbol-kind`, `symbol-binding`: `fallback` as for `symbol`.
- `compile-unit`, `source-file`: `paths` = `relative` (relative to the
  compile directory if inside it, else absolute), `name` (file name only)
  or `full`. `source-file`: `lines` = `off` or `on` (a last level
  `:<line>` per source line; still bytes, so it shows which lines cost
  the most).
- `function`: `inlines` = `inner` (inlined code by the inlined function:
  whose source the bytes are), `outer` (by the function it is inlined
  into; like `symbol`, but static functions without a symbol count too)
  or `chain` (a path: function › inlined › …, the function's own bytes as
  `[self]`); `demangle`, `hash` as for `symbol`.
- `rules`: `dimension` = the dimension whose labels are renamed (default
  `symbol`), with its own parameters; `rules`; `unmatched` = `keep` (the
  label as it is) or `other` (`[other]`). Available where that dimension
  is.
- `memory-region`: `by` = `auto` (load address for bytes with content, so
  `.data` is in flash; runtime address otherwise), `vma` or `lma`.
- `object-file`: `archives` = `nest` (`libhal.a` › `gpio.o`), `flat`
  (`libhal.a(gpio.o)`) or `archive` (`libhal.a` only); `paths` = `short`
  (file names) or `full` (as in the map).

`memory-region`, `object-file` and `input-section` are unavailable (the
stage is skipped) for inputs without a linker map or memory regions;
`compile-unit`, `source-file` and `function` for inputs without DWARF
debug info.

DWARF entries outside the binary's allocated sections are ignored: with
`--gc-sections`, the debug info of dropped functions stays, at address 0.
A variable's size is its symbol's, or its type's if it has no symbol.
Rust code inlined from the standard library is named by the local
`rust-src` path in the app's units and by `library/core/...` in core's
own; `rules` can merge them, e.g.
`rules=.*/(library/.*)=>$1` over `source-file`.

Categories come from the section type and flags: `unwind` is `.ARM.exidx`,
`.ARM.extab`, `.eh_frame*` and `.gcc_except_table`; `debug` is `.debug*`,
`.zdebug*`, `.stab*` and `.line`; `symtab` includes string tables; `meta` is
notes, `.comment`, `.ARM.attributes` and other non-allocated sections.

## Rules

`relabel` and the `rules` dimension take an ordered list of rules: a regex
`pattern` and a `label`. The first rule whose pattern matches a label (path
labels as their parts joined by `/`, like `libhal.a/gpio.o`) gives the new
label, which replaces the whole old one:

- `$1`, `${name}` insert capture groups; `$$` is a `$`.
- A `/` in the label makes a path: `drivers/$1` gives `drivers` › `uart`.
- A label that comes out empty keeps the old label.

On the command line, a rule is `PATTERN=>LABEL`, e.g.
`rules=^libhal\.a/(.*)=>hal/$1`. In a project file, rules are either such
rows or a table:

```toml
[[stage]]
id = "relabel"
rules = ['^libhal\.a/(.*)=>hal/$1']

# or
[[stage.rules]]
pattern = '^libhal\.a/(.*)'
label = "hal/$1"
```
