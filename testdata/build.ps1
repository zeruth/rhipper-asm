# Builds the fixtures under testdata/bin from the sources under testdata/src.
#
# Each program is built for x86-64 and for x86 with clang and lld, without a C runtime, so the
# binaries stay small and hold only the code of the source. /Brepro makes the output deterministic,
# so a rebuild from the same sources and the same LLVM gives the same bytes. Every build also writes
# the linker map beside the binary: the tests compare the functions the analysis finds with it.
#
#   ./testdata/build.ps1            build every fixture
#   ./testdata/build.ps1 control    build one
[CmdletBinding()]
param([string[]]$Only)

$ErrorActionPreference = 'Continue'
$src = Join-Path $PSScriptRoot 'src'
$bin = Join-Path $PSScriptRoot 'bin'
New-Item -ItemType Directory -Force $bin | Out-Null

$targets = @(
  @{ Suffix = '64'; Triple = 'x86_64-pc-windows-msvc'; Entry = 'mainCRTStartup' },
  @{ Suffix = '32'; Triple = 'i686-pc-windows-msvc'; Entry = 'mainCRTStartup' }
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
  foreach ($t in $targets) {
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

function Wanted([string]$Name) { return -not $Only -or ($Only -contains $Name) }

if (Wanted 'hello') { Build-Exe 'hello' @() }
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
if (Wanted 'exports') { Build-Dll 'exports' }
# tls has a TLS directory of its own making: a __declspec(thread) variable, the index, the template
# bounds and a callback, which a program without the C runtime must define itself.
if (Wanted 'tls') { Build-Exe 'tls' @() }
# structs reads and writes a record of mixed fields through a pointer, for the type recovery.
if (Wanted 'structs') { Build-Exe 'structs' @() }

# The PDBs and the import libraries are build leftovers, not fixtures.
Get-ChildItem $bin -Include *.pdb, *.lib, *.exp -Recurse | Remove-Item -Force
Get-ChildItem $bin | Select-Object Name, Length
