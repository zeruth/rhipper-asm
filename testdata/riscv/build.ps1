# Rebuilds the RISC-V fixtures from `src/rv64.c`, freestanding: `rv64.elf` for RV64IM, with no
# compressed instruction, and `rv64c.elf` for RV64IMC, with them. Beside each: the disassembler's
# listing with no alias taken (`.txt`), which the decoder's tests read, and what the program
# prints and its exit code under qemu's user-mode emulator in WSL (`.out`: the output, and
# `exit <code>` as its last line), which the lifted program must repeat.
#
# Usage: powershell -File testdata/riscv/build.ps1
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
foreach ($fixture in @(@('rv64', 'rv64im'), @('rv64c', 'rv64imc'))) {
    $name = $fixture[0]
    $elf = Join-Path $here "$name.elf"
    & clang --target=riscv64-unknown-elf "-march=$($fixture[1])" -mabi=lp64 -O1 -fno-unroll-loops -fno-vectorize -nostdlib -ffreestanding `
        -fuse-ld=lld '-Wl,--build-id=none' (Join-Path $here 'src\rv64.c') -o $elf
    if ($LASTEXITCODE -ne 0) { throw "clang failed for $name.elf" }
    $listing = & llvm-objdump -d -M no-aliases $elf
    [IO.File]::WriteAllText((Join-Path $here "$name.txt"), (($listing -join "`n") + "`n"))
    $unix = (& wsl wslpath -a ($elf -replace '\\', '/')).Trim()
    $printed = & wsl -e qemu-riscv64-static $unix
    $code = $LASTEXITCODE
    [IO.File]::WriteAllText((Join-Path $here "$name.out"), ((@($printed) + "exit $code") -join "`n") + "`n")
    Write-Host "$name.elf: $((Get-Item $elf).Length) bytes, $($listing.Count) lines listed; under qemu $(@($printed).Count) lines, exit $code"
}
