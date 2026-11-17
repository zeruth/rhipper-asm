#!/bin/sh
# The listings of the MIPS decoder's check, in WSL from the repository's root:
#   sh testdata/mips/listings/build.sh
# `testdata/mips/control.c` at four optimisation levels in both byte orders, built by the command at
# the top of the source with `-mno-gpopt` (clang 18 fails to select a `$gp`-relative address at
# -O0 and big-endian), and llvm-objdump's text of each `.text` beside it. The test reads the words
# from the executable and the text from the listing, so it runs no tool.
set -eu
out=testdata/mips/listings
for level in O0 O1 O2 Os; do
  for order in el be; do
    if [ "$order" = el ]; then target=mipsel-linux-gnu; else target=mips-linux-gnu; fi
    name=control-$level.$order
    clang --target=$target -march=mips2 -$level -static -nostdlib -fno-pic -mno-abicalls -mno-gpopt \
      -fuse-ld=lld -Wl,--entry=_start -Wl,--build-id=none testdata/mips/control.c -o $out/$name
    llvm-objdump -d --section=.text $out/$name | sed "s|$out/||" > $out/$name.objdump
  done
done
