# Concepts

## The pipeline

An analysis is a list of **stages** that run in order. The pipeline starts
empty, with no bytes at all:

1. An **input** stage (`open-file`) loads a binary and adds all its bytes.
2. **Split** stages cut the bytes by a *dimension*, such as section or
   symbol. Each split adds a level to the result tree.
3. **Filter** stages remove bytes: by label (`.debug*` sections) or by space
   (only bytes that are loaded into flash, or in an address range).
4. Later milestones add **attach** stages (linker map, memory regions),
   **transform** stages (group small items, rename) and **add** stages (free
   space in a memory region).

The final bytes are aggregated into a tree. Each stage reports the bytes it
got and the bytes it passed on, so you can see what a filter removed.

Stages that can't run on their input (a level filter before any split, say)
are skipped with a reason, and the pipeline continues.

Reruns are cheap: each stage's output is cached, and editing stage *k* only
reruns stages *k* and later. A dimension's labels are cached per binary.

## Dimensions

A **dimension** labels the bytes of a binary: `section` gives `.text`,
`.data`, …; `symbol` gives function and object names. Dimensions are used by
split and filter stages. Bytes a dimension can't attribute get an explicit
label in brackets, like `[no symbol]` or `[ELF header]`.

A label can be a path (`a/b/c`), which becomes several tree levels. Filters
match path labels as text joined with `/`.

See [the reference](reference.md) for all dimensions.

## Address spaces

A byte can live in up to three address spaces:

| Space | Meaning | Example on Cortex-M |
|-------|---------|---------------------|
| file  | offset in the ELF file | everything, including debug info |
| VMA   | address at runtime | `.data` in RAM at `0x20000000` |
| LMA   | address it is loaded from (ELF `p_paddr`) | `.data`'s initial values in flash |

Bloui cuts a binary into **chunks**, runs of bytes that have the same length
in every space they're in. The chunks come from the loadable segments
(program headers). A file range outside the segments, like debug info, has no
VMA or LMA. The memory-only part of a segment (`.bss`, `.stack`) has a VMA
but no file offset and no LMA, as nothing is loaded for it.

Dimensions label ranges in whichever space suits them, and Bloui translates
the labels through the chunk table. That's why a split by section (labeled by
file offset) and a filter by RAM address range (labeled by VMA) work
together.

## Size metrics

Every node in the tree has three sizes:

| Metric   | What it counts |
|----------|----------------|
| `file`   | bytes in the ELF file |
| `memory` | bytes allocated at runtime (RAM and flash, including `.bss`) |
| `load`   | bytes with content that are allocated: what goes into flash |

So for firmware, `load` is the flash image and `memory` minus `load` is
`.bss`-like RAM. `.data` counts in both: its content is loaded from flash and
it occupies RAM at runtime.

## Symbols

For ARM, the Thumb bit (bit 0) of function addresses is cleared, and the
mapping symbols `$t`, `$a` and `$d` are ignored. Where symbols overlap, the
smallest one wins. For aliases with the same range, a global name is
preferred over a weak one, then a local one, then the first by name.

Bytes in a section that no symbol covers are labeled `[<section>]` (for
example `[.text]` for padding or assembly code without a size), or
`[no symbol]` with `fallback=none`.
