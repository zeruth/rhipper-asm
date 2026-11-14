# Builds the 32-bit x86 vector harness, x86_32.c, with clang and lld for i686 Windows into
# out/harness/x86_32.exe, for a run by hand; harness_runs_three_cases in
# crates/ra-asm-lift/tests/vectors_gen.rs builds the same program into its scratch directory.
#
#   ./testdata/vectors/harness/build.ps1
#   ./out/harness/x86_32.exe cases.bin results.bin
#
# No C runtime: the program talks to kernel32 alone, so WOW64 runs it with nothing else loaded
# below the arena at 0x20000000 and the code page at 0x30000000. The ARM guests beside it
# (guest.c with aarch64.S and arm.S) are built in WSL by the vector tests themselves.
[CmdletBinding()]
param()

$ErrorActionPreference = 'Continue'
$out = Join-Path $PSScriptRoot '..\..\..\out\harness'
New-Item -ItemType Directory -Force $out | Out-Null
$exe = Join-Path $out 'x86_32.exe'

$args = @('-target', 'i686-pc-windows-msvc', '-O1', '-nostdlib', '-fno-builtin', '-fuse-ld=lld',
  (Join-Path $PSScriptRoot 'x86_32.c'), '-o', $exe, '-lkernel32',
  '-Wl,/entry:mainCRTStartup', '-Wl,/subsystem:console')
Write-Host "clang x86_32 harness" -ForegroundColor Cyan
& clang @args
if ($LASTEXITCODE -ne 0) { throw "clang failed for the x86_32 harness" }
Get-Item $exe | Select-Object FullName, Length
