#!/bin/sh
# Two builds of a Cortex-M firmware for diff tests: v1.elf, and v2.elf
# built with -DV2 (see main.c: a buffer grows, the banner changes,
# report() is new, uart_put_hex is gone). The outputs are committed, so
# tests don't need the toolchain.
set -eu
cd "$(dirname "$0")"

CC=${CC:-arm-none-eabi-gcc}
AR=${AR:-arm-none-eabi-ar}
CFLAGS="-mcpu=cortex-m4 -mthumb -Os -g -ffunction-sections -fdata-sections \
    -fno-asynchronous-unwind-tables -ffreestanding -Wall -Wextra \
    -fdebug-prefix-map=$(pwd)=."

for v in v1 v2; do
    defs=""
    [ $v = v2 ] && defs="-DV2"
    rm -rf build
    mkdir -p build
    $CC $CFLAGS $defs -c hal/clock.c -o build/clock.o
    $CC $CFLAGS $defs -c hal/gpio.c -o build/gpio.o
    $AR rcs build/libhal.a build/clock.o build/gpio.o
    for f in vectors main uart; do
        $CC $CFLAGS $defs -c $f.c -o build/$f.o
    done
    $CC -mcpu=cortex-m4 -mthumb -c startup.S -o build/startup.o
    $CC -mcpu=cortex-m4 -mthumb -nostdlib -T fw.ld -Wl,--gc-sections \
        -Wl,-Map=$v.map \
        build/startup.o build/vectors.o build/main.o build/uart.o \
        -Lbuild -lhal -lgcc -o $v.elf
done
rm -rf build
