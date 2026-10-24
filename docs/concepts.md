# Concepts

## The pipeline

An analysis is a list of **stages** that run in order. The pipeline starts
empty, with no bytes at all:

1. An **input** stage (`open-file`) loads a binary and adds all its bytes.
2. **Split** stages cut the bytes by a *dimension*, such as section or
   symbol. Each split adds a level to the result tree.
3. **Filter** stages remove bytes: by label (`.debug*` sections) or by space
   (only bytes that are loaded into flash, or in an address range).
4. **Attach** stages add data from outside the binary: a linker map, or
   memory regions entered by hand. They make more dimensions available
   (object file, memory region).
5. **Add** stages add bytes that aren't in the file: the free part of each
   memory region.
6. Later milestones add **transform** stages (group small items, rename).

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
match path labels as text joined with `/`. When one label is a prefix of
another (`fw › main` and `fw › main › {{closure}}`), the shorter one's
bytes go to a `[self]` node under it, so children always add up to their
parent.

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

### Where `.data` counts

On a microcontroller, `.data` is stored in flash (its LMA) and copied to RAM
(its VMA) at startup. Bloui counts it **once, where it is stored: in flash**.
A split by section still shows it as `.data`, and a filter by RAM address
range still finds it. Its runtime copy in RAM is not counted as used (or as
free), so RAM adds up to its size minus `.data`. A stage that adds the RAM
copy as its own bytes may come later.

Dimensions label ranges in whichever space suits them, and Bloui translates
the labels through the chunk table. That's why a split by section (labeled by
file offset) and a filter by RAM address range (labeled by VMA) work
together.

## Size metrics

Every node in the tree has three sizes:

| Metric   | What it counts |
|----------|----------------|
| `file`   | bytes in the ELF file |
| `memory` | bytes allocated at runtime: code, constants, `.data`, `.bss`, stack (and free space, when added) |
| `load`   | bytes with content that are allocated: the flash image |

So for firmware, `load` is the flash image, and `memory` split by memory
region shows how full each region is. Dynamically allocated memory (heap)
isn't visible yet.

## Memory regions and free space

Memory regions (`FLASH`, `RAM`, …) come from the linker map's "Memory
Configuration", or from a `memory-regions` stage. The `memory-region`
dimension places each byte where it is stored: by its load address if it
has content, otherwise (`.bss`, stack) by its runtime address.

The `free-space` stage adds the unused part of each region as `[free]`
bytes, so a region's `memory` size is its full length (RAM less the
runtime copy of `.data`):

```console
$ bloui-cli -i fw.elf -s memory-region --free -d 1
     FILE    MEMORY      LOAD      %M  memory region
   18.5Ki    80.0Ki    1.63Ki  100.0%  TOTAL
   1.63Ki    64.0Ki    1.63Ki   80.0%  ├─ FLASH
        0    16.0Ki         0   20.0%  ├─ RAM
   16.9Ki         0         0    0.0%  └─ [no region]
```

Free space is labeled `[free]` by every dimension except `memory-region`.

## Linker maps

A GNU ld map file (`-Wl,-Map=fw.map`) adds what the ELF file doesn't say:
which object file and archive member each byte came from, the input
sections (`.text.uart_init` with `-ffunction-sections`), padding (`*fill*`),
and the memory regions. It works without debug info, so it also covers
startup assembly and precompiled libraries.

`open-file` attaches `fw.map` or `fw.elf.map` automatically when it sits
next to `fw.elf`. A `linker-map` stage attaches one from elsewhere. Bloui
checks that the map matches the binary (every section in both must have the
same address and size), so a map from an older build isn't used by mistake.

## Treemap

The treemap shows the result tree as nested rectangles, each with an area
proportional to its size in the chosen metric. It's drawn WinDirStat-style,
with cushion shading: each level adds a "bump", so the nesting is visible
without borders.

- Layouts: *squarified* (Bruls et al.; tiles as square as possible, the
  default) or *slice and dice* (alternating rows and columns per level).
- Tiles smaller than a pixel are folded into their parent.
- Colors are taken from the label at one tree level (by default the first
  level) and hashed, so a label keeps its color when the pipeline changes.
  Bracketed labels such as `[no symbol]` are grey; free space is flat light
  grey, so how full a memory region is shows at a glance.

## Symbols

For ARM, the Thumb bit (bit 0) of function addresses is cleared, and the
mapping symbols `$t`, `$a` and `$d` are ignored. Where symbols overlap, the
smallest one wins. For aliases with the same range, a global name is
preferred over a weak one, then a local one, then the first by name.

Bytes in a section that no symbol covers are labeled `[<section>]` (for
example `[.text]` for padding or assembly code without a size), or
`[no symbol]` with `fallback=none`.
