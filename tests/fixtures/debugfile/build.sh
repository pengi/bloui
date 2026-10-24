#!/bin/sh
# Stripped copies of a Cortex-M firmware with separate debug files, one per
# way of finding them. Built from the cortexm sources with a build id. The
# outputs are committed, so tests don't need the toolchain.
#   linked.elf    .gnu_debuglink to linked.dbg (only found that way)
#   plain.elf     plain.debug next to it (found by name)
#   buildid.elf   root/.build-id/xx/rest.debug (found by build id, with
#                 root as the debug directory)
set -eu
cd "$(dirname "$0")"

CC=${CC:-arm-none-eabi-gcc}
AR=${AR:-arm-none-eabi-ar}
OBJCOPY=${OBJCOPY:-arm-none-eabi-objcopy}
SRC=../cortexm
OUT=$(pwd)/build

rm -rf build root ./*.elf ./*.dbg ./*.debug
mkdir -p build
# Compile in the source directory, so the debug info names files like the
# cortexm firmware's (main.c, hal/gpio.c).
(
    cd $SRC
    CFLAGS="-mcpu=cortex-m4 -mthumb -Os -g -ffunction-sections -fdata-sections \
        -fno-asynchronous-unwind-tables -ffreestanding -Wall -Wextra \
        -fdebug-prefix-map=$(pwd)=."
    $CC $CFLAGS -c hal/clock.c -o $OUT/clock.o
    $CC $CFLAGS -c hal/gpio.c -o $OUT/gpio.o
    for f in vectors main uart; do
        $CC $CFLAGS -c $f.c -o $OUT/$f.o
    done
    $CC -mcpu=cortex-m4 -mthumb -c startup.S -o $OUT/startup.o
)
$AR rcs build/libhal.a build/clock.o build/gpio.o
$CC -mcpu=cortex-m4 -mthumb -nostdlib -T $SRC/fw.ld -Wl,--gc-sections \
    -Wl,--build-id=sha1 \
    build/startup.o build/vectors.o build/main.o build/uart.o \
    -Lbuild -lhal -lgcc -o build/full.elf

$OBJCOPY --only-keep-debug build/full.elf linked.dbg
$OBJCOPY --strip-debug --add-gnu-debuglink=linked.dbg build/full.elf linked.elf
cp linked.dbg plain.debug
$OBJCOPY --strip-debug build/full.elf plain.elf
$OBJCOPY --strip-debug build/full.elf buildid.elf
id=$(${READELF:-arm-none-eabi-readelf} -n build/full.elf | sed -n 's/.*Build ID: *//p')
mkdir -p root/.build-id/$(echo $id | cut -c1-2)
cp linked.dbg root/.build-id/$(echo $id | cut -c1-2)/$(echo $id | cut -c3-).debug

rm -rf build
