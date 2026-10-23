#!/bin/sh
# Rebuild the Cortex-M test firmware. The outputs (fw.elf, fw.map) are
# committed, so tests don't need the toolchain.
set -eu
cd "$(dirname "$0")"

CC=${CC:-arm-none-eabi-gcc}
AR=${AR:-arm-none-eabi-ar}
CFLAGS="-mcpu=cortex-m4 -mthumb -Os -g -ffunction-sections -fdata-sections \
    -fno-asynchronous-unwind-tables -ffreestanding -Wall -Wextra \
    -fdebug-prefix-map=$(pwd)=."

rm -rf build
mkdir -p build

# hal as a static library, to get archive members in the map file.
$CC $CFLAGS -c hal/clock.c -o build/clock.o
$CC $CFLAGS -c hal/gpio.c -o build/gpio.o
$AR rcs build/libhal.a build/clock.o build/gpio.o

$CC $CFLAGS -c vectors.c -o build/vectors.o
$CC $CFLAGS -c main.c -o build/main.o
$CC $CFLAGS -c uart.c -o build/uart.o
# startup.S without -g: no DWARF for it.
$CC -mcpu=cortex-m4 -mthumb -c startup.S -o build/startup.o

$CC -mcpu=cortex-m4 -mthumb -nostdlib -T fw.ld -Wl,--gc-sections \
    -Wl,-Map=fw.map -Wl,--cref \
    build/startup.o build/vectors.o build/main.o build/uart.o \
    -Lbuild -lhal -lgcc -o fw.elf

rm -rf build
