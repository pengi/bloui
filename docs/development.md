# Development

## Building

```sh
cargo build                          # lib, bloui-cli, bloui (GUI)
cargo build --no-default-features    # without GTK: lib and bloui-cli only
cargo test
```

The GUI (`bloui`, the `gui` feature, on by default) needs GTK 4.14 or
later with its development files: `sudo apt install libgtk-4-dev`. It uses
gtk4-rs (MIT), plain GTK without libadwaita.

## Checks before each commit

```sh
cargo fmt --check
cargo clippy --all-targets           # must be warning-free
cargo test
scripts/check-licenses.py            # dependencies must be GPL-2.0-only compatible
```

## Licensing

Bloui is GPL-2.0-only. Every dependency must be compatible with that:
MIT, BSD, ISC, Zlib, Unicode, MPL-2.0, LGPL-2.1, or a dual license with one
of those. **Plain Apache-2.0 is not compatible with GPL-2.0**, so a crate
that is Apache-2.0 only can't be used. `scripts/check-licenses.py` checks all
packages in `Cargo.lock`. Each source file starts with an SPDX line.

## Code layout

One crate: a library with everything except the user interfaces, and two
binaries. **The library has no GUI code**; anything a GUI needs that isn't
widgets or painting (layouts, hit testing, view state) belongs in the
library, so `bloui-cli` can use it too.

```
src/
  model/       core types: address spaces, RangeMap, interner/labels,
               Binary and its chunk table, fragments, sizes
  param/       parameter schemas and values (stage/dimension settings)
  reader/      Reader trait; elf/ is the only format-specific code
  linkmap/     linker map parsers (GNU ld) and the map/binary check
  debuginfo/   DWARF reader (gimli) into model::DebugInfo: compile units,
               line ranges, functions and inlined scopes, variables
  symname/     symbol names: demangling (Rust, C++), splitting into paths
  rules/       regex relabeling rules (rules dimension, relabel stage)
  dimension/   Dimension trait; builtin/ has file, segment, section,
               category, symbol, memory-region, object-file, input-section
  pipeline/    Stage trait, PipelineCtx (interner and caches), configs;
               builtin/ has open-file, split, filter-label, filter-space,
               linker-map, memory-regions, free-space
  engine/      label translation, fragment refinement, result tree,
               runner with per-stage cache
  registry/    the lists of readers, dimensions and stages
  project/     *.bloui.toml load/save
  render/      treemap layouts and hit testing, cushion shading, colors,
               images (PNG), scenes (display lists for frontends)
  view/        view state (selection, hover, zoom, keeps them across
               reruns) and the cached treemap view; frontends forward
               input here and paint the scene
  bin/bloui/       GTK frontend: app (startup), window (layout, actions,
                   open/save, undo), pipeline/ (stage list; form/ builds
                   parameter forms from ParamSpecs), tree_view
                   (ColumnView over a lazy TreeListModel), treemap
                   (widget painting the scene), worker (runs the Runner
                   off the GUI thread)
  bin/bloui-cli/   text frontend
data/
  io.github.pengi.bloui.desktop   desktop entry (the app id is
                                  io.github.pengi.bloui)
  icons/hicolor/                  the logo, also the window icon; bloui
                                  adds data/icons to the icon theme when
                                  run from the source tree
docs/images/   the README's screenshot
```

Dependencies go one way: `model`, `param`, `symname`, `rules` → `reader`, `dimension` →
`engine`, `pipeline` → `registry`, `project` → binaries.

### Adding a dimension or stage

Write a factory and the implementation in one file under `builtin/`, and
add it to the list in that directory's `mod.rs`. Parameters are declared as
`ParamSpec`s; `ParamValues::resolve` checks and fills them in before
`create` is called, so `create` can read them with the typed getters.
The GUI builds its forms from the same specs: give each a label and help
text, and use `ParamSpec::visible_when(key, value)` for a parameter that
only matters for some value of another. A stage should also implement
`StageFactory::summary`, the one line shown in the pipeline list and in
`bloui-cli --stats`.

`DimensionFactory::create` gets the registry, so a dimension can be built
on another one (`rules` takes a `ParamSpec::dimension` parameter and
creates it with `registry.create_dimension`). A dimension that needs
something the binary may not have (a linker map) says so in
`Dimension::availability`; a split or filter by it is then skipped.
Symbol-based dimensions use `label_symbols` in `builtin/symbol.rs`.

A dimension returns a `Labeling`: label entries in any address space, an
overlap policy, and a fallback label. Use tiers for fallbacks that should
lose against real labels (the symbol dimension's `[.text]`). Place a range
in a section with `Binary::places_in_section`, which gives its file range
and its runtime range (the latter covers NOBITS sections like `.bss`).

A stage that changes a binary (attaches something) makes a copy with
`Binary::derive` through `PipelineCtx::derive_binary`, which caches it so
reruns keep the same binary and its cached labels. Stages report with
`ctx.note()` and `ctx.warn()`. A stage that depends on the metric the
result is shown in reads it with `ctx.metric()`; the runner then reruns it
(and the stages after it) when the metric changes, and the GUI does when
its Metric changes.

## Tests

- Unit tests next to the code (range maps, chunk tables, params, cutting).
- `tests/elf_reader.rs`: reader output against facts from `readelf`/`nm`.
- `tests/embedded.rs`: linker map parsing and checking, memory regions,
  free space, object files.
- `tests/engine.rs`: invariants: totals don't depend on split order,
  children sum to their parent, filter statistics add up, flash/RAM by
  address.
- `tests/render.rs`: layout invariants (inside bounds, areas proportional,
  no overlaps), hit testing against brute force, cushion rendering.
- `tests/view.rs`: zoom/selection, surviving a rerun, treemap view caching.
- `tests/cli.rs`: snapshots of `bloui-cli` output in `tests/snapshots/`.
  After an intended output change, run `BLOUI_BLESS=1 cargo test --test cli`
  and review the diff.

### Fixtures

`tests/fixtures/` has the sources and build scripts of the test binaries,
and the built files, so tests don't need the toolchains:

- `cortexm/`: Cortex-M4 firmware built with `arm-none-eabi-gcc` and its own
  linker script (FLASH/RAM, `.data` loaded from flash, NOLOAD `.stack`),
  with a static library (`libhal.a`), libgcc's 64-bit division, and
  startup code in assembly without debug info. `build.sh` rebuilds `fw.elf`
  and `fw.map`.
- `cxx/`: the same kind of firmware in C++ (`arm-none-eabi-g++`):
  namespaces, an anonymous namespace, a class template with two instances,
  an operator, a weak alias, and `.isra` clones.
- `rust/`: a `#![no_std]` Rust firmware for `thumbv7em-none-eabihf`, built
  with plain `rustc` and linked by `arm-none-eabi-gcc` with GNU ld, so the
  map is a GNU ld map. The app crate has legacy-mangled names and `core`
  v0-mangled ones; `core` brings its DWARF along, hence the file size.
- `debugfile/`: stripped builds of the cortexm sources (with a build id)
  and their separate debug files: `linked.elf` (found by debug link),
  `plain.elf` (`plain.debug` next to it), `buildid.elf` (under
  `root/.build-id/`, with `root` as the debug directory).
- `diff/`: two builds of a copy of the cortexm sources, `v1.elf` and
  `v2.elf` (`-DV2`: a function added, one removed, a buffer and a string
  grown), with maps, for diffs.
- `host/`: a small x86-64 PIE built with `gcc`. `build.sh` rebuilds it.

After rebuilding a fixture, update the numbers in the tests and the
snapshots.

### Testing the GUI

Run the GUI on a nested X server, not your own desktop:

```sh
Xephyr :7 -screen 1200x800 -br -ac &
DISPLAY=:7 GDK_BACKEND=x11 GDK_DEBUG=no-portals GTK_A11Y=none \
    DBUS_SESSION_BUS_ADDRESS=unix:path=/nonexistent HOME=/tmp/bloui-home \
    target/debug/bloui
DISPLAY=:7 import -window root shot.png
```

Without a session bus, file dialogs stay inside Xephyr (GTK uses its own
chooser); with yours, GTK asks the desktop portal and the dialog opens on
your own desktop. Keyboard input via `xdotool` needs `xdotool windowfocus --sync <window>`
first, as there is no window manager.
