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

### Load images and runtime copies

On a microcontroller, `.data` is stored in flash (its LMA) and copied to RAM
(its VMA) at startup, so it takes room in both. Bloui models that as two
chunks:

- the **load image**: the bytes in the file, at the LMA in flash;
- the **runtime copy**: at the VMA in RAM, with no content of its own.

Both take memory. So with a split by memory region, `.data` shows up in
FLASH and in RAM, and the regions add up to their real use. This is done
for every segment whose LMA differs from its VMA. Turn it off with
`open-file`'s `load_copies = false` for binaries where the physical address
means something else (an OS kernel, say).

Dimensions label ranges in whichever space suits them, and Bloui translates
the labels through the chunk table. That's why a split by section (labeled by
file offset) and a filter by RAM address range (labeled by VMA) work
together.

## Size metrics

Every node in the tree has three sizes:

| Metric   | What it counts |
|----------|----------------|
| `file`   | bytes in the ELF file |
| `memory` | address space taken in any memory: code and constants in flash, `.data`'s image in flash *and* its copy in RAM, `.bss`, stack, free space |
| `load`   | bytes with content that take memory: the flash image |

So for firmware, `load` is the flash image, and `memory` split by memory
region shows how full each region is. `.data` counts twice in `memory`
(see above), once in `load` and once in `file`.

## Memory regions and free space

Memory regions (`FLASH`, `RAM`, …) come from the linker map's "Memory
Configuration", or from a `memory-regions` stage. The `memory-region`
dimension places each byte by its runtime address, or its load address for
a load image.

The `free-space` stage adds the unused part of each region as `[free]`
bytes, so a region's `memory` size is its full length:

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

## Symbols

For ARM, the Thumb bit (bit 0) of function addresses is cleared, and the
mapping symbols `$t`, `$a` and `$d` are ignored. Where symbols overlap, the
smallest one wins. For aliases with the same range, a global name is
preferred over a weak one, then a local one, then the first by name.

Bytes in a section that no symbol covers are labeled `[<section>]` (for
example `[.text]` for padding or assembly code without a size), or
`[no symbol]` with `fallback=none`.
