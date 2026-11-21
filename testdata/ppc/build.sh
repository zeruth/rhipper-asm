#!/bin/sh
# Rebuilds the PowerPC fixture in WSL, with clang and qemu's user-mode emulator: `src/ppc.c`
# into `ppc.elf` (32-bit big-endian PowerPC, no C library), the disassembler's listing of it into
# `ppc.txt`, and what the program prints and its exit code under the emulator into `ppc.out` (the
# output, and `exit <code>` as its last line).
#
# Usage, from the repository's root: wsl -e sh testdata/ppc/build.sh
set -e
here=$(dirname "$0")
clang --target=powerpc-unknown-linux-gnu -mcpu=750 -msoft-float -O1 -fno-unroll-loops -fno-vectorize -nostdlib -ffreestanding -static \
    -fuse-ld=lld -Wl,--build-id=none -Wl,-e,_start "$here/src/ppc.c" -o "$here/ppc.elf"
llvm-objdump -d "$here/ppc.elf" > "$here/ppc.txt"
set +e
qemu-ppc-static "$here/ppc.elf" > "$here/ppc.out"
code=$?
set -e
echo "exit $code" >> "$here/ppc.out"
echo "ppc.elf: $(wc -c < "$here/ppc.elf") bytes, $(wc -l < "$here/ppc.txt") lines listed, exit $code"
