#!/bin/sh
# Rebuild the C++ Cortex-M test firmware. The outputs (fw.elf, fw.map) are
# committed, so tests don't need the toolchain.
set -eu
cd "$(dirname "$0")"

CXX=${CXX:-arm-none-eabi-g++}
CXXFLAGS="-std=c++17 -mcpu=cortex-m4 -mthumb -Os -g -ffunction-sections \
    -fdata-sections -fno-exceptions -fno-rtti -fno-asynchronous-unwind-tables \
    -fno-threadsafe-statics -ffreestanding -Wall -Wextra \
    -fdebug-prefix-map=$(pwd)=."

rm -rf build
mkdir -p build
for f in startup main uart; do
    $CXX $CXXFLAGS -c $f.cpp -o build/$f.o
done
$CXX -mcpu=cortex-m4 -mthumb -nostdlib -T fw.ld -Wl,--gc-sections \
    -Wl,-Map=fw.map -Wl,--cref \
    build/startup.o build/main.o build/uart.o -lgcc -o fw.elf

rm -rf build
