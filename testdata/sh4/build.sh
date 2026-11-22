#!/bin/sh
# Rebuilds the SH-4 fixtures in WSL, with the Dreamcast toolchain's compiler and qemu's user-mode
# emulator: `src/sh4.c` into `sh4.elf` (little-endian SH-4, no C library, no FPU code) and
# `src/sh4fp.c` into `sh4fp.elf` (the floating-point unit, every number a single), the
# disassembler's listing of each into its `.txt`, and what each prints and its exit code under
# the emulator into its `.out` (the output, and `exit <code>` as its last line).
#
# Usage, from the repository's root: wsl -e sh testdata/sh4/build.sh
set -e
here=$(dirname "$0")
bin=/opt/toolchains/dc/sh-elf/bin
build() {
    name=$1
    shift
    "$bin/sh-elf-gcc" "$@" -ml -O1 -fno-unroll-loops -nostdlib -ffreestanding -static -Wl,--build-id=none \
        -Wl,-T,"$here/src/sh4.ld" -Wl,-z,max-page-size=0x1000 \
        "$here/src/$name.c" -lgcc -o "$here/$name.elf"
    "$bin/sh-elf-objdump" -d "$here/$name.elf" > "$here/$name.txt"
    set +e
    qemu-sh4-static "$here/$name.elf" > "$here/$name.out"
    code=$?
    set -e
    echo "exit $code" >> "$here/$name.out"
    echo "$name.elf: $(wc -c < "$here/$name.elf") bytes, $(wc -l < "$here/$name.txt") lines listed, exit $code"
}
build sh4 -m4-single
build sh4fp -m4-single-only -Wl,-e,_fp_start
