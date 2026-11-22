#!/bin/sh
# Rebuilds the MSP430 fixture in WSL, with clang and lld: `src/msp430.c` into `msp430.elf` (a
# 16-bit MSP430 program with no library), and the disassembler's listing of it into
# `msp430.txt`.
#
# No emulator of the machine is here to run it under. `msp430.out` is what the program prints
# and the code it ends with by its source, which is the SH-4 fixture's: the lines every other
# machine's build of that source printed under its emulator (`testdata/sh4/sh4.out`).
#
# Usage, from the repository's root: wsl -e sh testdata/msp430/build.sh
set -e
here=$(dirname "$0")
# The driver would call the vendor's linker, which is not here: lld links the object itself.
object=$(mktemp)
clang --target=msp430 -O1 -fno-unroll-loops -ffreestanding -fno-builtin -c "$here/src/msp430.c" -o "$object"
ld.lld --build-id=none -e _start --image-base=0x4000 "$object" -o "$here/msp430.elf"
rm "$object"
llvm-objdump -d "$here/msp430.elf" > "$here/msp430.txt"
cp "$here/../sh4/sh4.out" "$here/msp430.out"
echo "msp430.elf: $(wc -c < "$here/msp430.elf") bytes, $(wc -l < "$here/msp430.txt") lines listed"
