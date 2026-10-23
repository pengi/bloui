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
current one. Either way, the pipeline can then be changed in the
pipeline pane, and the result follows every change.

## Layout

- **Pipeline pane** (left, F9 hides it): the stages, top to bottom.
  Each row has a checkbox to turn the stage on or off, an icon for its
  kind, a summary ("split by section", "exclude section ~ ^\.debug"), what
  it changed in the current metric ("−1.29Ki", "+62.4Ki"), and a warning
  or error icon (with the reason as tooltip) when the stage was skipped
  or failed. The toolbar has undo, redo, move up, move down, remove, and
  **+**, which adds a stage after the selected one. Below the list is a
  form with the selected stage's parameters. Fields with a bad value
  (like `4Q` for a size) turn red and say why in their tooltip; the
  pipeline runs again with every valid change. Sizes and addresses take
  `4096`, `0x1000`, `4K`, `1.5M`.
- **Header bar**: open, save (Ctrl+S; Ctrl+Shift+S saves under a new
  name), zoom out, the **metric** (file, memory or load, see
  [concepts](concepts.md)), **color by**, and reload (F5), which reads the
  input files again. Color by is either a tree level (tiles take the
  color of their label at that level) or any dimension ("dimension:
  object file"): then each tile takes the color of the label with the
  most bytes in it, without adding a level to the tree. A label has the
  same color whichever way it is chosen.
- **Tree list**: name, size in the chosen metric, percentage of the total,
  a bar with the share of the parent, the number of fragments, and the
  lowest runtime address.
- **Treemap**: cushion treemap of the tree, or of the zoomed-in node.
- **Legend** (right of the tree list, F8 hides it): the labels of the
  colors under the zoomed-in node, biggest first, with their sizes (the
  rest as `[other]` after 40). For a dimension, sizes are all bytes with
  that label, also where another label wins the tile. Hovering a label
  outlines its tiles; clicking it selects the biggest node with it.
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
| Anywhere  | Ctrl+Z, Ctrl+Shift+Z        | undo, redo pipeline edits          |
| Anywhere  | F9                          | show or hide the pipeline pane     |
| Anywhere  | F8                          | show or hide the legend            |

Selection, zoom and expanded rows are kept by path when the result
changes (reload, metric).

Saving a project writes the pipeline; relative paths in it are rewritten
to be relative to the project's directory.
