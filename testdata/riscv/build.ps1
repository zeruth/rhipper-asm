# Rebuilds the RISC-V fixture: `src/rv64.c` for RV64IM, freestanding and with no compressed
# instructions, into `rv64.elf`, and the disassembler's listing of it, with no alias taken, into
# `rv64.txt`, which the decoder's test reads.
#
# Usage: powershell -File testdata/riscv/build.ps1
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$elf = Join-Path $here 'rv64.elf'
& clang --target=riscv64-unknown-elf -march=rv64im -mabi=lp64 -O1 -fno-unroll-loops -fno-vectorize -nostdlib -ffreestanding `
    -fuse-ld=lld '-Wl,--build-id=none' (Join-Path $here 'src\rv64.c') -o $elf
if ($LASTEXITCODE -ne 0) { throw 'clang failed for rv64.elf' }
$listing = & llvm-objdump -d -M no-aliases $elf
[IO.File]::WriteAllText((Join-Path $here 'rv64.txt'), (($listing -join "`n") + "`n"))
Write-Host "rv64.elf: $((Get-Item $elf).Length) bytes, $($listing.Count) lines listed"
