#!/bin/sh
# Rebuilds the SH-4 fixture in WSL, with the Dreamcast toolchain's compiler and qemu's user-mode
# emulator: `src/sh4.c` into `sh4.elf` (little-endian SH-4, no C library, no FPU code), the
# disassembler's listing of it into `sh4.txt`, and what the program prints and its exit code
# under the emulator into `sh4.out` (the output, and `exit <code>` as its last line).
#
# Usage, from the repository's root: wsl -e sh testdata/sh4/build.sh
set -e
here=$(dirname "$0")
bin=/opt/toolchains/dc/sh-elf/bin
"$bin/sh-elf-gcc" -m4-single -ml -O1 -fno-unroll-loops -nostdlib -ffreestanding -static -Wl,--build-id=none \
    -Wl,-T,"$here/src/sh4.ld" -Wl,-z,max-page-size=0x1000 \
    "$here/src/sh4.c" -lgcc -o "$here/sh4.elf"
"$bin/sh-elf-objdump" -d "$here/sh4.elf" > "$here/sh4.txt"
set +e
qemu-sh4-static "$here/sh4.elf" > "$here/sh4.out"
code=$?
set -e
echo "exit $code" >> "$here/sh4.out"
echo "sh4.elf: $(wc -c < "$here/sh4.elf") bytes, $(wc -l < "$here/sh4.txt") lines listed, exit $code"
