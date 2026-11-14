#!/bin/bash
# Builds the freestanding fixtures of a foreign Linux machine under testdata/bin and checks each
# against its expected output in the machine's emulator, from WSL (Ubuntu 24.04), where clang and
# lld carry every target and qemu-user-static runs the result:
#
#   wsl -d Ubuntu-24.04 -- bash -lc /mnt/c/.../testdata/src/freestanding/build.sh          every machine
#   wsl -d Ubuntu-24.04 -- bash -lc "/mnt/c/.../testdata/src/freestanding/build.sh mipsel" one
#
# Each program is a fixture source of testdata/src built against this directory alone: no C
# library, no PIC, no FPU, the loops left as the source wrote them, the functions nothing calls
# dropped. The build is static and without a build id, so a rebuild with the same LLVM gives the
# same bytes; the script prints each binary's SHA-256 for MANIFEST.toml and ends with the tools'
# versions. A program whose emulated output is not its expected file fails the build.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
testdata=$(cd "$here/../.." && pwd)
only=${1:-}

common=(-O2 -static -fno-pic -nostdlib -nostdlibinc -ffreestanding -fno-builtin
  -fno-unroll-loops -fno-vectorize -fno-slp-vectorize -ffunction-sections -fdata-sections
  -fuse-ld=lld -Wl,--gc-sections -Wl,--build-id=none -I "$here" "$here/printf.c" "$here/windows.c")

# One machine: the clang target, the syscall file, the emulator, and the flags of its code model.
build_machine() {
  local suffix=$1 target=$2 sys=$3 emulator=$4
  shift 4
  for program in hello control; do
    local out="$testdata/bin/${program}_$suffix"
    echo "clang ${program}_$suffix"
    clang --target="$target" "$@" "${common[@]}" "$here/$sys" "$testdata/src/$program.c" -o "$out"
    local actual="$testdata/../out/freestanding/${program}_$suffix.txt"
    mkdir -p "$(dirname "$actual")"
    # The sources write CRLF as their Windows builds do; the expected files are LF, and the
    # oracles compare across line endings, so the check here does too.
    "$emulator" "$out" | tr -d '\r' > "$actual"
    cmp "$actual" "$testdata/expected/$program.txt"
    sha256sum "$out"
  done
}

if [ -z "$only" ] || [ "$only" = mipsel ]; then
  # No abicalls and no gp-relative small data: the start routine sets no $gp, so every address
  # is absolute (and clang 18 cannot select a gp-relative address under -fdata-sections anyway).
  build_machine mipsel mipsel-linux-gnu sys_mipsel.c qemu-mipsel-static -mno-abicalls -mno-gpopt -msoft-float
fi
echo "tools: $(clang --version | head -1); $(ld.lld --version | head -1); $(qemu-mipsel-static --version | head -1)"
