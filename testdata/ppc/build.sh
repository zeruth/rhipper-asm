#!/bin/sh
# Rebuilds the PowerPC fixtures in WSL, with clang and qemu's user-mode emulator: `src/ppc.c`
# into `ppc.elf` (32-bit big-endian PowerPC, no C library, integer code only) and `src/ppcfp.c`
# into `ppcfp.elf` (the same machine's floating-point unit), the disassembler's listing of each
# into its `.txt`, and what each prints and its exit code under the emulator into its `.out`
# (the output, and `exit <code>` as its last line).
#
# Usage, from the repository's root: wsl -e sh testdata/ppc/build.sh
set -e
here=$(dirname "$0")
# build <name> <emulator> <flags...>
build() {
    name=$1
    emulator=$2
    shift 2
    clang "$@" -O1 -fno-unroll-loops -fno-vectorize -nostdlib -ffreestanding -static \
        -fuse-ld=lld -Wl,--build-id=none -Wl,-e,_start "$here/src/$name.c" -o "$here/$name.elf"
    llvm-objdump -d "$here/$name.elf" > "$here/$name.txt"
    set +e
    "$emulator" "$here/$name.elf" > "$here/$name.out"
    code=$?
    set -e
    echo "exit $code" >> "$here/$name.out"
    echo "$name.elf: $(wc -c < "$here/$name.elf") bytes, $(wc -l < "$here/$name.txt") lines listed, exit $code"
}
build ppc qemu-ppc-static --target=powerpc-unknown-linux-gnu -mcpu=750 -msoft-float
build ppcfp qemu-ppc-static --target=powerpc-unknown-linux-gnu -mcpu=750 -mhard-float
# The integer program again as a DOL file, the GameCube's executable: linked where the console
# has its programs, in the mapping of its memory at 0x80000000, run under the emulator as the
# ELF file it is linked as (which is not kept), and written as a DOL by `dol.py`. Its system
# calls are still Linux's, which no console answers: the fixture is of the container and of
# the console's addresses.
cp "$here/src/ppc.c" "$here/src/gc.c"
build gc qemu-ppc-static --target=powerpc-unknown-linux-gnu -mcpu=750 -msoft-float -Wl,--image-base=0x80010000
python3 "$here/dol.py" "$here/gc.elf" "$here/gc.dol"
rm "$here/src/gc.c" "$here/gc.elf"
# The 64-bit machine of the later consoles, big-endian, its integer code (`ppc64`) and its
# floating-point unit (`ppc64fp`): the linker here links only the newer of its two calling
# conventions, which has no function descriptors.
build ppc64 qemu-ppc64-static --target=powerpc64-unknown-linux-gnu -mabi=elfv2 -mcpu=970 -mno-altivec
build ppc64fp qemu-ppc64-static --target=powerpc64-unknown-linux-gnu -mabi=elfv2 -mcpu=970 -mno-altivec
