# Development

## Building

```sh
cargo build                          # lib, bloui-cli, bloui (GUI)
cargo build --no-default-features    # without GTK: lib and bloui-cli only
cargo test
```

The GUI (`bloui`, the `gui` feature, on by default) will need GTK 4
development files: `sudo apt install libgtk-4-dev`.

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
  dimension/   Dimension trait; builtin/ has file, segment, section,
               category, symbol
  pipeline/    Stage trait, PipelineCtx (interner and caches), configs;
               builtin/ has open-file, split, filter-label, filter-space
  engine/      label translation, fragment refinement, result tree,
               runner with per-stage cache
  registry/    the lists of readers, dimensions and stages
  project/     *.bloui.toml load/save
  bin/bloui/       GTK frontend
  bin/bloui-cli/   text frontend
```

Dependencies go one way: `model`, `param` → `reader`, `dimension` →
`engine`, `pipeline` → `registry`, `project` → binaries.

### Adding a dimension or stage

Write a factory and the implementation in one file under `builtin/`, and
add it to the list in that directory's `mod.rs`. Parameters are declared as
`ParamSpec`s; `ParamValues::resolve` checks and fills them in before
`create` is called, so `create` can read them with the typed getters.

A dimension returns a `Labeling`: label entries in any address space, an
overlap policy, and a fallback label. Use tiers for fallbacks that should
lose against real labels (the symbol dimension's `[.text]`).

## Tests

- Unit tests next to the code (range maps, chunk tables, params, cutting).
- `tests/elf_reader.rs`: reader output against facts from `readelf`/`nm`.
- `tests/engine.rs`: invariants: totals don't depend on split order,
  children sum to their parent, filter statistics add up, flash/RAM by
  address.
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
- `host/`: a small x86-64 PIE built with `gcc`. `build.sh` rebuilds it.

After rebuilding a fixture, update the numbers in the tests and the
snapshots.

### Testing the GUI

Run the GUI on a nested X server, not your own desktop:

```sh
Xephyr :7 -screen 1200x800 -br -ac &
DISPLAY=:7 GDK_BACKEND=x11 HOME=/tmp/bloui-home cargo run --bin bloui
DISPLAY=:7 import -window root shot.png
```

Keyboard input via `xdotool` needs `xdotool windowfocus --sync <window>`
first, as there is no window manager.
