# Rebuilds the RISC-V fixtures from `src/rv64.c`, freestanding: `rv64.elf` for RV64IM, with no
# compressed instruction, `rv64c.elf` for RV64IMC, with them, and `rv32c.elf` for RV32IMC, the
# narrow machine (where the source's `long` is thirty-two bits, so it prints other numbers).
# Beside each: the disassembler's listing with no alias taken (`.txt`), which the decoder's tests
# read, and what the program prints and its exit code under qemu's user-mode emulator in WSL
# (`.out`: the output, and `exit <code>` as its last line), which the lifted program must repeat.
#
# Usage: powershell -File testdata/riscv/build.ps1
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
foreach ($fixture in @(@('rv64', 'riscv64', 'rv64im', 'lp64'), @('rv64c', 'riscv64', 'rv64imc', 'lp64'), @('rv32c', 'riscv32', 'rv32imc', 'ilp32'))) {
    $name = $fixture[0]
    $elf = Join-Path $here "$name.elf"
    & clang "--target=$($fixture[1])-unknown-elf" "-march=$($fixture[2])" "-mabi=$($fixture[3])" -O1 -fno-unroll-loops -fno-vectorize -nostdlib `
        -ffreestanding -w -fuse-ld=lld '-Wl,--build-id=none' (Join-Path $here 'src\rv64.c') -o $elf
    if ($LASTEXITCODE -ne 0) { throw "clang failed for $name.elf" }
    $listing = & llvm-objdump -d -M no-aliases $elf
    [IO.File]::WriteAllText((Join-Path $here "$name.txt"), (($listing -join "`n") + "`n"))
    $unix = (& wsl wslpath -a ($elf -replace '\\', '/')).Trim()
    $printed = & wsl -e "qemu-$($fixture[1])-static" $unix
    $code = $LASTEXITCODE
    [IO.File]::WriteAllText((Join-Path $here "$name.out"), ((@($printed) + "exit $code") -join "`n") + "`n")
    Write-Host "$name.elf: $((Get-Item $elf).Length) bytes, $($listing.Count) lines listed; under qemu $(@($printed).Count) lines, exit $code"
}
