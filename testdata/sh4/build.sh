#!/bin/sh
# Rebuilds the SH-4 fixtures in WSL, with the Dreamcast toolchain's compiler and qemu's user-mode
# emulator: `src/sh4.c` into `sh4.elf` (little-endian SH-4, no C library, no FPU code) and into
# `sh4eb.elf` (the same program big-endian, the byte order of the Saturn's and the 32X's
# processors; the toolchain has no library of that order, and the program needs none),
# `src/sh4fp.c` into `sh4fp.elf` (the floating-point unit, every number a single), the
# disassembler's listing of each into its `.txt`, and what each prints and its exit code under
# the emulator into its `.out` (the output, and `exit <code>` as its last line).
#
# Usage, from the repository's root: wsl -e sh testdata/sh4/build.sh
set -e
here=$(dirname "$0")
bin=/opt/toolchains/dc/sh-elf/bin
# build <name> <source> <emulator> <libraries> <flags...>
build() {
    name=$1
    source=$2
    emulator=$3
    libraries=$4
    shift 4
    "$bin/sh-elf-gcc" "$@" -O1 -fno-unroll-loops -nostdlib -ffreestanding -static -Wl,--build-id=none \
        -Wl,-T,"$here/src/sh4.ld" -Wl,-z,max-page-size=0x1000 \
        "$here/src/$source.c" $libraries -o "$here/$name.elf"
    "$bin/sh-elf-objdump" -d "$here/$name.elf" > "$here/$name.txt"
    set +e
    "$emulator" "$here/$name.elf" > "$here/$name.out"
    code=$?
    set -e
    echo "exit $code" >> "$here/$name.out"
    echo "$name.elf: $(wc -c < "$here/$name.elf") bytes, $(wc -l < "$here/$name.txt") lines listed, exit $code"
}
build sh4 sh4 qemu-sh4-static -lgcc -m4-single -ml
build sh4eb sh4 qemu-sh4eb-static "" -m4-single-only -mb
build sh4fp sh4fp qemu-sh4-static -lgcc -m4-single-only -ml -Wl,-e,_fp_start
