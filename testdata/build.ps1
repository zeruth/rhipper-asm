# Builds the fixtures under testdata/bin from the sources under testdata/src.
#
# Each program is built for x86-64 and for x86 with clang and lld, without a C runtime, so the
# binaries stay small and hold only the code of the source; crt is the exception, built over the
# static C runtime with its PDB. /Brepro makes the output deterministic, so a rebuild from the same
# sources and the same LLVM gives the same bytes. Every build also writes the linker map beside the
# binary: the tests compare the functions the analysis finds with it.
#
#   ./testdata/build.ps1            build every fixture
#   ./testdata/build.ps1 control    build one
[CmdletBinding()]
param([string[]]$Only)

$ErrorActionPreference = 'Continue'
$src = Join-Path $PSScriptRoot 'src'
$bin = Join-Path $PSScriptRoot 'bin'
New-Item -ItemType Directory -Force $bin | Out-Null

# a64 is the ARM64 build of a fixture, for the second architecture: only the programs that ask
# for it get one, and it cannot run here, so its expected output is the x64 build's.
$targets = @(
  @{ Suffix = '64'; Triple = 'x86_64-pc-windows-msvc'; Entry = 'mainCRTStartup' },
  @{ Suffix = '32'; Triple = 'i686-pc-windows-msvc'; Entry = 'mainCRTStartup' },
  @{ Suffix = 'a64'; Triple = 'aarch64-pc-windows-msvc'; Entry = 'mainCRTStartup' }
)

function Build-Exe([string]$Name, [string[]]$Extra, [string]$Ext = 'c', [string[]]$Extra32 = @(), [string[]]$Suffixes = @('64', '32')) {
  foreach ($t in ($targets | Where-Object { $Suffixes -contains $_.Suffix })) {
    $out = Join-Path $bin "$Name$($t.Suffix).exe"
    $map = Join-Path $bin "$Name$($t.Suffix).map"
    # -ffreestanding would drop the x64 unwind tables, and the loader tests need .pdata.
    $args = @('-target', $t.Triple, '-O2', '-nostdlib', '-fno-builtin', '-funwind-tables', '-fuse-ld=lld',
      (Join-Path $src "$Name.$Ext"), '-o', $out, '-lkernel32',
      "-Wl,/entry:$($t.Entry)", '-Wl,/subsystem:console', '-Wl,/Brepro', "-Wl,/map:$map") + $Extra
    if ($t.Suffix -eq '32') { $args += $Extra32 }
    Write-Host "clang $Name$($t.Suffix)" -ForegroundColor Cyan
    & clang @args
    if ($LASTEXITCODE -ne 0) { throw "clang failed for $Name$($t.Suffix)" }
  }
}

function Build-Dll([string]$Name) {
  foreach ($t in ($targets | Where-Object { $_.Suffix -ne 'a64' })) {
    $out = Join-Path $bin "$Name$($t.Suffix).dll"
    $map = Join-Path $bin "$Name$($t.Suffix).map"
    $args = @('-target', $t.Triple, '-O2', '-nostdlib', '-fno-builtin', '-funwind-tables', '-shared', '-fuse-ld=lld',
      (Join-Path $src "$Name.c"), '-o', $out, '-lkernel32',
      '-Wl,/entry:DllMainCRTStartup', '-Wl,/Brepro', "-Wl,/map:$map", "-Wl,/def:$(Join-Path $src "$Name.def")")
    Write-Host "clang $Name$($t.Suffix)" -ForegroundColor Cyan
    & clang @args
    if ($LASTEXITCODE -ne 0) { throw "clang failed for $Name$($t.Suffix)" }
  }
}

# A program over MSVC's static C runtime (/MT), x64 only: clang-cl compiles the program and lld
# links libcmt and libucrt into it. /debug writes the PDB, which is part of the fixture: it names
# the functions and the library each came from, and the engine reads it beside the binary. /Z7
# puts the program's own procedures (the static functions) in it; the runtime's names come from
# the libraries' symbol tables. _NO_CRT_STDIO_INLINE keeps printf a function of the runtime, not
# an inline copy in the program; /GS- keeps the security cookie out of the program's code.
function Build-Crt([string]$Name) {
  $out = Join-Path $bin "$Name`64.exe"
  $map = Join-Path $bin "$Name`64.map"
  $pdb = Join-Path $bin "$Name`64.pdb"
  $args = @('--target=x86_64-pc-windows-msvc', '/nologo', '/O2', '/MT', '/GS-', '/Z7', '/Brepro', '/D_NO_CRT_STDIO_INLINE', '/D_CRT_SECURE_NO_WARNINGS',
    (Join-Path $src "$Name.c"), "/Fe:$out",
    '/link', '/debug', "/pdb:$pdb", "/pdbaltpath:$Name`64.pdb", "/map:$map", '/subsystem:console', '/Brepro')
  Write-Host "clang-cl $Name`64" -ForegroundColor Cyan
  & clang-cl @args
  if ($LASTEXITCODE -ne 0) { throw "clang-cl failed for $Name`64" }
}

function Wanted([string]$Name) { return -not $Only -or ($Only -contains $Name) }

# hello is also built for ARM64: the fixture of the AArch64 decoder, exception table and lift.
if (Wanted 'hello') { Build-Exe 'hello' @() 'c' @() @('64', '32', 'a64') }
# control carries a CodeView debug record, for the debug directory.
if (Wanted 'control') { Build-Exe 'control' @('-g', '-gcodeview', '-Wl,/debug', '-Wl,/pdbaltpath:control.pdb') }
if (Wanted 'float') { Build-Exe 'float' @() }
# extended does its floating point on the x87, for the exact unit; x64 code has no x87 arithmetic.
if (Wanted 'extended') { Build-Exe 'extended' @('-mno-sse') 'c' @() @('32') }
# classes is C++ with RTTI, for the vtable analysis; no exceptions, so no runtime handler is named.
# The program defines type_info's vtable symbol itself; on x86 clang prefixes its label with an
# underscore, and the linker maps the name the type descriptors use to it.
if (Wanted 'classes') {
  Build-Exe 'classes' @('-std=c++20', '-frtti', '-fno-exceptions') 'cpp' @('-Wl,/alternatename:??_7type_info@@6B@=_??_7type_info@@6B@')
}
# throws is C++ with exceptions: vcruntime supplies the throw and the frame handler the unwind
# data names, nothing else of a runtime. x64 only: the x86 frame handler's registration frames
# are not lifted yet.
if (Wanted 'throws') { Build-Exe 'throws' @('-std=c++20', '-fno-rtti', '-lvcruntime') 'cpp' @() @('64') }
if (Wanted 'exports') { Build-Dll 'exports' }
# tls has a TLS directory of its own making: a __declspec(thread) variable, the index, the template
# bounds and a callback, which a program without the C runtime must define itself.
if (Wanted 'tls') { Build-Exe 'tls' @() }
# structs reads and writes a record of mixed fields through a pointer, for the type recovery.
if (Wanted 'structs') { Build-Exe 'structs' @() }
# crt links the static C runtime; its PDB stays, the engine reads the names from it.
if (Wanted 'crt') { Build-Crt 'crt' }
# window registers a class, shows a popup, paints once and quits through the message loop: user32
# calls the program's window procedure back.
if (Wanted 'window') { Build-Exe 'window' @('-luser32') }

# The PDBs (crt's excepted) and the import libraries are build leftovers, not fixtures.
Get-ChildItem $bin -Include *.pdb, *.lib, *.exp -Recurse | Where-Object { $_.Name -ne 'crt64.pdb' } | Remove-Item -Force
Get-ChildItem $bin | Select-Object Name, Length
