#!/bin/sh
# Rebuild the host (x86-64) test binary. The output is committed.
set -eu
cd "$(dirname "$0")"
${CC:-gcc} -O1 -g -fdebug-prefix-map="$(pwd)"=. -o hello.elf hello.c
