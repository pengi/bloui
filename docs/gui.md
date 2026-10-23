# The GUI (`bloui`)

```sh
bloui [FILE.elf | PROJECT.bloui.toml]
```

The window starts empty. Open an ELF file or a project file with the open
button (Ctrl+O), from the command line, or by dropping it on the window.

An ELF file gets a default pipeline: split by memory region, add free
space, split by section, then by symbol. Without a linker map (`fw.map`
next to `fw.elf` is found automatically) the memory-region and free-space
stages are skipped; the status bar then says "2 notes", and its tooltip
says why. A project file brings its own pipeline (see
[project files](project-files.md)). Opening another file replaces the
current one.

## Layout

- **Header bar**: open, save (Ctrl+S; Ctrl+Shift+S saves under a new
  name), zoom out, the **metric** (file, memory or load, see
  [concepts](concepts.md)), **color by** (the tree level whose labels pick
  the treemap colors), and reload (F5), which reads the input files again.
- **Tree list**: name, size in the chosen metric, percentage of the total,
  a bar with the share of the parent, the number of fragments, and the
  lowest runtime address.
- **Treemap**: cushion treemap of the tree, or of the zoomed-in node.
- **Status bar**: the node under the mouse (or the selected node): path,
  size, share, address. When zoomed in, the zoom node comes first in
  brackets.

## Mouse and keys

| Where     | Input                       | Does                               |
|-----------|-----------------------------|------------------------------------|
| Treemap   | click                       | select (also in the tree list)     |
| Treemap   | double-click                | zoom in one level, towards the tile |
| Treemap   | right click, back button, Backspace | zoom out                   |
| Treemap   | Home                        | zoom out all the way               |
| Treemap   | Enter                       | zoom to the selected node          |
| Treemap   | Up                          | select the parent                  |
| Tree list | select a row                | select it in the treemap           |
| Tree list | double-click, Enter         | zoom to that node                  |

Selection, zoom and expanded rows are kept by path when the result
changes (reload, metric).

Saving a project writes the pipeline; relative paths in it are rewritten
to be relative to the project's directory.
