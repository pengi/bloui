# Bloui

A size analyzer for ELF files, like [bloaty](https://github.com/google/bloaty),
aimed at embedded (Cortex-M) firmware first. The analysis is a pipeline of
stages: open a file, split the bytes by section, symbol, memory region and
more, filter some away, and look at the result as a tree (and, later, a
treemap in a GTK GUI).

```console
$ bloui-cli -i fw.elf -k load -s section -s symbol -m load -n 3
     FILE    MEMORY      LOAD      %L  section › symbol
   1.63Ki    1.63Ki    1.63Ki  100.0%  TOTAL
   1.29Ki    1.29Ki    1.29Ki   79.1%  ├─ .text
      784       784       784   47.0%  │  ├─ __udivmoddi4
      136       136       136    8.2%  │  ├─ main
      ...
```

Documentation is in [docs/](docs/README.md).

License: GPL-2.0-only, see [LICENSE](LICENSE).
