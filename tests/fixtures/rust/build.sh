#!/bin/sh
# Rebuild the Rust Cortex-M test firmware. The outputs (fw.elf, fw.map) are
# committed, so tests don't need the toolchain. Needs the
# thumbv7em-none-eabihf target (rustup target add thumbv7em-none-eabihf) and
# arm-none-eabi-gcc with GNU ld as the linker (rustc asks for lld; the later
# -fuse-ld=bfd wins), so the map is in GNU ld format.
set -eu
cd "$(dirname "$0")"

rustc --edition 2021 --target thumbv7em-none-eabihf --crate-type bin \
    --crate-name fw -C opt-level=s -C panic=abort -g \
    -C linker=arm-none-eabi-gcc -C linker-flavor=gcc \
    -C link-arg=-fuse-ld=bfd -C link-arg=-mcpu=cortex-m4 -C link-arg=-mthumb \
    -C link-arg=-nostdlib \
    -C link-arg=-Tfw.ld -C link-arg=-Wl,--gc-sections \
    -C link-arg=-Wl,-Map=fw.map -C link-arg=-Wl,--cref \
    --remap-path-prefix "$(pwd)=." \
    "$@" main.rs -o fw.elf
