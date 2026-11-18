#!/bin/bash
# Builds the vector harness's guest (guest.c with the machine's trampoline, <machine>.S) with
# clang and lld in WSL (Ubuntu 24.04) into <dir>/guest_<machine> and, given a cases file and a
# results file, runs it under the machine's qemu-user with the cases on standard input and the
# results on standard output:
#
#   wsl -d Ubuntu-24.04 -- bash /mnt/c/.../testdata/vectors/harness/run.sh mipsel <dir>
#   wsl -d Ubuntu-24.04 -- bash /mnt/c/.../testdata/vectors/harness/run.sh mipsel <dir> cases.bin results.bin
#
# The guest needs no C library: no PIC, no FPU, every address absolute, and the image at
# 0x0a000000, clear of the case page at 0x30000000 and the data region at 0x40000000.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
machine=$1
dir=$2

common=(-ffreestanding -fno-builtin -nostdlib -static -fno-pic -fno-stack-protector
  -fno-asynchronous-unwind-tables -O1 -fuse-ld=lld -Wl,--image-base=0x0a000000 -Wl,--build-id=none)

case "$machine" in
  mipsel)
    # No abicalls and no gp-relative small data: the start routine sets no $gp.
    flags=(--target=mipsel-linux-gnu -mno-abicalls -mno-gpopt -msoft-float)
    emulator=qemu-mipsel-static
    ;;
  mips64el)
    # n64 without abicalls: absolute addresses, which the image below 2 GB lets the trampoline
    # build with lui and an add.
    flags=(--target=mips64el-linux-gnuabi64 -mno-abicalls -mno-gpopt -msoft-float)
    emulator=qemu-mips64el-static
    ;;
  *)
    echo "run.sh: no trampoline for $machine" >&2
    exit 2
    ;;
esac

mkdir -p "$dir"
guest="$dir/guest_$machine"
clang "${flags[@]}" "${common[@]}" "$here/guest.c" "$here/$machine.S" -o "$guest"
if [ $# -ge 4 ]; then
  "$emulator" "$guest" < "$3" > "$4"
fi
