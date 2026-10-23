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
| `-S, --stage 'ID K=V ..'` | any stage, see below |

Output options:

| Option | Meaning |
|--------|---------|
| `-m, --sort METRIC` | sort by `file`, `memory` (default) or `load`; the `%` column uses it too |
| `-d, --depth N` | show at most N levels |
| `-n, --limit N` | show at most N children per node, the rest as `[k more]` (default 20, 0 = all) |
| `--bytes` | exact byte counts instead of `1.23Ki` |
| `--csv` | CSV, one row per leaf, one column per level |
| `--stats` | what each stage did, before the tree |
| `--save FILE` | save the pipeline as a project file |
| `--list` | list all stages and dimensions with their parameters |

If a stage fails, `bloui-cli` prints the error, shows the result without that
stage, and exits with status 1.

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
  `dimension=symbol dimension.fallback=none`.
- Values can't contain spaces; use a project file for those.

## Examples

```sh
# Flash usage by section and symbol.
bloui-cli -i fw.elf -k load -s section -s symbol -m load

# RAM and flash per segment and kind of content, without debug info.
bloui-cli -i fw.elf --exclude 'section=^\.debug' -s segment -s category

# Everything in the file, exact sizes.
bloui-cli -i fw.elf -s section -m file --bytes -n 0

# Two files side by side.
bloui-cli -i old.elf -i new.elf -s file -s section

# Save the pipeline, and run it again later with extra stages.
bloui-cli -i fw.elf -k load -s section --save flash.bloui.toml
bloui-cli flash.bloui.toml -s symbol
```
