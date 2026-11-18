#!/bin/bash
# Builds the PlayStation fixtures under testdata/bin with PSn00bSDK from WSL (Ubuntu 24.04), where
# the SDK's mipsel-none-elf-gcc and elf2x are on the login shell's path:
#
#   wsl -d Ubuntu-24.04 -- bash -lc /mnt/c/.../testdata/src/psx/build.sh
#
# Each program is one source of this directory built the way the SDK's psn00bsdk_add_executable
# builds a GPREL release executable (lib/libpsn00b/cmake/flags.cmake, without -g: the PS-EXE
# carries no debug information and a rebuild with the same SDK gives the same bytes): the ELF is
# linked with the SDK's exe.ld and its static libraries, elf2x turns it into the PS-EXE the
# console's loader takes, and nm writes the symbol map beside it. No emulator is on the machine,
# so nothing runs the result here: the expected output of each program is derived from its source
# (MANIFEST.toml evidence `source`). The script prints each binary's SHA-256 for MANIFEST.toml and
# ends with the tools' versions.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
testdata=$(cd "$here/../.." && pwd)
# elf2x sits in the SDK package's bin, beside its lib and include; gcc also has a bin of its own
# under the toolchain's target directory, so it does not locate the package.
sdk=$(cd "$(dirname "$(command -v elf2x)")/.." && pwd)
libs=$sdk/lib/libpsn00b
elf_dir=$testdata/../out/psx
mkdir -p "$elf_dir"

flags=(-O2 -Wa,--strip-local-absolute -ffreestanding -fno-builtin -nostdlib -fdata-sections -ffunction-sections
  -fsigned-char -fno-strict-overflow -msoft-float -march=r3000 -mtune=r3000 -mabi=32 -mno-mt -mno-llsc
  -G8 -fno-pic -mno-abicalls -mgpopt -mno-extern-sdata -DPSN00BSDK=1 -DNDEBUG=1 -I "$sdk/include/libpsn00b"
  -static -Wl,-gc-sections -T "$libs/ldscripts/exe.ld")
# The SDK's library order (internal_setup.cmake), libgcc last.
archives=()
for lib in psxgpu psxgte psxspu psxcd psxpress psxsio psxetc psxapi smd lzp c; do
  archives+=("$libs/release/lib${lib}_exe_gprel.a")
done

# The programs named on the command line, or all of them.
programs=("$@")
[ ${#programs[@]} -eq 0 ] && programs=(hello gte draw)
for program in "${programs[@]}"; do
  elf="$elf_dir/${program}_psx.elf"
  exe="$testdata/bin/${program}_psx.exe"
  echo "mipsel-none-elf-gcc ${program}_psx"
  mipsel-none-elf-gcc "${flags[@]}" "$here/$program.c" -o "$elf" "${archives[@]}" -lgcc
  elf2x -q "$elf" "$exe"
  mipsel-none-elf-nm -f posix -n "$elf" > "$testdata/bin/${program}_psx.map"
  sha256sum "$exe"
done
echo "tools: PSn00bSDK $(grep -o '"version": *"[^"]*"' "$libs/build.json" | cut -d '"' -f 4); $(mipsel-none-elf-gcc --version | head -1)"
