# `bloui-cli`

`bloui-cli` runs a pipeline and prints the result tree as text or CSV. It
has the same backend as the GUI.

```
bloui-cli [OPTIONS] [PROJECT]
```

The pipeline is built from a project file (`PROJECT`), if given, followed by
the stages given as options, **in command-line order**:

| Option | Stage it adds |
|--------|---------------|
| `-i, --input FILE` | `open-file` |
| `-s, --split DIM[:K=V,..]` | `split` by a dimension, e.g. `-s section`, `-s symbol:fallback=none` |
| `--include DIM=REGEX` | `filter-label`, keep matching bytes |
| `--exclude DIM=REGEX` | `filter-label`, remove matching bytes |
| `-k, --keep file\|memory\|load` | `filter-space` |
| `--map FILE` | `linker-map` for the last input (a map next to the input is attached anyway) |
| `--region NAME:ORIGIN:LENGTH` | `memory-regions`; repeat for more, they form one stage |
| `--free` | `free-space` |
| `-S, --stage 'ID K=V ..'` | any stage, see below |
| `-p, --preset NAME` | all stages of a preset (see `--list-presets` and [project files](project-files.md#presets)) |

Output options:

| Option | Meaning |
|--------|---------|
| `-m, --sort METRIC` | sort by `file`, `memory` or `load`; the `%` column uses it too. Default: the project's or preset's `[view]`, else `memory` |
| `-d, --depth N` | show at most N levels |
| `-n, --limit N` | show at most N children per node, the rest as `[k more]` (default 20, 0 = all) |
| `--bytes` | exact byte counts instead of `1.23Ki` |
| `--csv` | CSV, one row per leaf, one column per level |
| `--stats` | what each stage did, before the tree |
| `--save FILE` | save the pipeline (and `-m`, `--color-by`) as a project file; relative paths are rewritten for the file's directory |
| `--save-preset NAME` | save the pipeline without its inputs as a user preset |
| `--list-presets` | list the built-in and user presets |
| `--list` | list all stages and dimensions with their parameters |
| `--treemap FILE.png` | also render a cushion treemap (by the `--sort` metric) |
| `--size WxH` | treemap size in pixels (default `1024x640`) |
| `--layout squarified\|slice` | treemap layout (default `squarified`) |
| `--color-by L` | color treemap tiles by a tree level (name or index, default 0) or any dimension (`object-file:archives=flat`) |
| `--legend` | print the colors' legend: labels with their sizes (up to 20, the rest as `[other]`) |

If a stage fails, `bloui-cli` prints the error, shows the result without that
stage, and exits with status 1. Warnings (a linker map that doesn't match)
go to stderr; `--stats` also shows the other notes ("attached fw.map").

## Generic stages

`-S` takes a stage id and `key=value` words, the parameters listed by
`--list`:

```sh
# Remove [ELF header] etc. after splitting by section.
bloui-cli -i fw.elf -s section \
    -S 'filter-label source=level match=regex patterns=^\['

# Only RAM, by runtime address.
bloui-cli -i fw.elf -S 'filter-space space=vma start=0x20000000 end=0x20004000' -s symbol
```

- Repeat a key to give a list: `patterns=^\.text patterns=^\.data`.
- Set a dimension's own parameters with a dot:
  `dimension=symbol dimension.fallback=none`. This nests, for dimensions
  built on dimensions: `dimension=rules dimension.dimension=symbol-path
  dimension.dimension.depth=2`.
- Tables take one row per value, columns joined by the separator `--list`
  shows: `regions=FLASH:0x08000000:64K`, `rules=^hw/uart=>drivers`.
- Values can't contain spaces; use a project file for those.

`-s DIM:K=V,..` and `--color-by` take the same keys, separated by commas
(so values can't contain commas there):

```sh
# Group symbols by namespace two levels deep, then rename some groups.
bloui-cli -i fw.elf -s 'rules:dimension=symbol-path,dimension.depth=2,rules=^hw/uart=>drivers/uart,rules=^(app)=>$1,unmatched=other'

# Rename memory regions; free space added later gets the new names too.
bloui-cli -i fw.elf -s memory-region -S 'relabel rules=^FLASH$=>flash rules=^RAM$=>ram' --free
```

## Examples

```sh
# Flash usage by section and symbol.
bloui-cli -i fw.elf -k load -s section -s symbol -m load

# RAM and flash per segment and kind of content, without debug info.
bloui-cli -i fw.elf --exclude 'section=^\.debug' -s segment -s category

# Everything in the file, exact sizes.
bloui-cli -i fw.elf -s section -m file --bytes -n 0

# How full are FLASH and RAM, and what's in them by object file.
bloui-cli -i fw.elf -s memory-region --free -s object-file -d 2

# Regions by hand, when there's no map.
bloui-cli -i fw.elf --region FLASH:0x08000000:64K --region RAM:0x20000000:16K \
    -s memory-region --free

# Flash usage per object file and input section.
bloui-cli -i fw.elf -k load -s object-file -s input-section -m load

# A treemap of the flash image, colored by object file.
bloui-cli -i fw.elf -k load -s object-file -s input-section -m load -n 0 \
    --treemap flash.png

# Two files side by side.
bloui-cli -i old.elf -i new.elf -s file -s section

# Save the pipeline, and run it again later with extra stages.
bloui-cli -i fw.elf -k load -s section --save flash.bloui.toml
bloui-cli flash.bloui.toml -s symbol
```
