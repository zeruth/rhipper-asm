# Builds the `clang` variant of the rhtest programs: clang-cl for x86 at /O2 /MD over the MSVC C
# runtime, so the program's own code is clang's while the startup and the helpers it links
# (`_alldvrm`, `_ftol2`, the security cookie) are MSVC's, as in the MSVC builds (`<name>-O2.exe`,
# `<name>-Od.exe`), which need `cl` and are built elsewhere. The `static` variant of `core` and
# `int64` is the same at /MT: the C runtime linked into the image with no PDB, as a VS2010 `/MT`
# program ships, so the runtime's recognition by the map alone is covered. /Brepro keeps a rebuild
# byte-identical for the same LLVM; the linker map goes beside the binary, as for every fixture.
#
#   ./testdata/rhtest/build.ps1            build every program
#   ./testdata/rhtest/build.ps1 misc crt   build some
[CmdletBinding()]
param([string[]]$Only)

$ErrorActionPreference = 'Continue'
$src = Join-Path $PSScriptRoot 'src'
$programs = @(
  @{ Name = 'core'; Ext = 'c' }, @{ Name = 'calls'; Ext = 'c' }, @{ Name = 'int64'; Ext = 'c' }, @{ Name = 'float'; Ext = 'c' },
  @{ Name = 'fpenv'; Ext = 'c' }, @{ Name = 'misc'; Ext = 'c' }, @{ Name = 'crt'; Ext = 'c' }, @{ Name = 'sys'; Ext = 'c' },
  @{ Name = 'win'; Ext = 'c' }, @{ Name = 'seh'; Ext = 'c' }, @{ Name = 'eh'; Ext = 'cpp' }, @{ Name = 'cpp'; Ext = 'cpp' },
  @{ Name = 'com'; Ext = 'cpp' }, @{ Name = 'stl'; Ext = 'cpp' }, @{ Name = 'stlio'; Ext = 'cpp' }, @{ Name = 'engine'; Ext = 'cpp' }
)
$static = @('core', 'int64')
# The libraries any of them names: user32 and shell32 (misc, sys, win), ws2_32 (win), COM (com).
$libs = @('kernel32.lib', 'user32.lib', 'shell32.lib', 'ws2_32.lib', 'ole32.lib', 'oleaut32.lib', 'uuid.lib')

foreach ($p in $programs) {
  if ($Only -and -not ($Only -contains $p.Name)) { continue }
  $name = $p.Name
  $variants = @(@{ Tag = 'clang'; Crt = '/MD' })
  if ($static -contains $name) { $variants += @{ Tag = 'static'; Crt = '/MT' } }
  foreach ($v in $variants) {
    $out = Join-Path $PSScriptRoot "$name-$($v.Tag).exe"
    $map = Join-Path $PSScriptRoot "$name-$($v.Tag).map"
    # Compiled and linked in one step, clang-cl keeps no object files.
    $args = @('--target=i686-pc-windows-msvc', '/nologo', '/O2', $v.Crt, '/Brepro', '/W3', '/EHsc',
      (Join-Path $src "$name.$($p.Ext)"), (Join-Path $src 'rhtest_main.c'), "/Fe:$out",
      '/link', "/map:$map", '/Brepro', '/subsystem:console') + $libs
    Write-Host "clang-cl $name-$($v.Tag)" -ForegroundColor Cyan
    & clang-cl @args
    if ($LASTEXITCODE -ne 0) { throw "clang-cl failed for $name-$($v.Tag)" }
  }
}

Get-ChildItem $PSScriptRoot -Include *-clang.exe, *-static.exe -Recurse -Depth 0 | Select-Object Name, Length
