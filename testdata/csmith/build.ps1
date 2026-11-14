# Generates the csmith programs under testdata/csmith: the sources under src, the binaries, their
# linker maps and their expected output under bin.
#
# Csmith writes a C program that computes a checksum of its own globals and prints it as one line,
# `checksum = <hex>`, so a program is its own oracle: a lift that prints another line is wrong.
# The script walks the seeds from 1 and keeps the first 32 that are usable: csmith (in WSL, where
# it is installed) writes src/seed<NN>.c with pointers, structs, bitfields and 64-bit arithmetic
# on; clang builds it for x86-64 and i686 at -O0 and -O2 as bin/seed<NN>_<width>_<level>.exe; each
# build runs here, and the line it prints is bin/seed<NN>_<width>_<level>.txt. A seed is dropped,
# and the next one above takes its place, when one of its builds does not end within the limit
# (csmith does not promise that a program terminates) or when the two levels of a width disagree
# (the program is not an oracle then); the script names every seed it drops and why.
#
# The programs are built like the other fixtures, without a C library: src/platform holds the
# entry, the memory functions and the printf, and stands in for the four hosted headers csmith's
# runtime includes; libcmt gives the compiler's helpers (the stack probe, x86's 64-bit division).
# csmith's own runtime headers are copied from the WSL package into src/runtime with their
# licence. /Brepro keeps a rebuild from the same sources and the same LLVM byte-identical.
#
#   ./testdata/csmith/build.ps1                 seeds from 1, the first 32 usable
#   ./testdata/csmith/build.ps1 -Limit 5        allow each run five seconds instead of two
#
# Needs in WSL: `apt-get install csmith libcsmith-dev`.
[CmdletBinding()]
param(
  # How many seeds to keep.
  [int]$Count = 32,
  # The seconds a build may run before its seed is dropped.
  [int]$Limit = 2,
  # The last seed tried before the script gives up.
  [int]$Last = 128,
  [string]$Distro = 'Ubuntu-24.04'
)

$ErrorActionPreference = 'Continue'
$src = Join-Path $PSScriptRoot 'src'
$bin = Join-Path $PSScriptRoot 'bin'
$platform = Join-Path $src 'platform'
$runtime = Join-Path $src 'runtime'
New-Item -ItemType Directory -Force $bin, $runtime | Out-Null

& wsl -d $Distro -- which csmith | Out-Null
if ($LASTEXITCODE -ne 0) { throw "csmith is not installed in WSL $Distro`: apt-get install csmith libcsmith-dev" }

# The runtime headers and the licence, from where the package puts them.
$include = $null
foreach ($dir in @('/usr/include/csmith', '/usr/include/csmith-2.3.0', '/usr/local/include/csmith', '/usr/local/include/csmith-2.3.0')) {
  & wsl -d $Distro -- test -f "$dir/csmith.h"
  if ($LASTEXITCODE -eq 0) { $include = $dir; break }
}
if (-not $include) { throw "csmith.h is not under /usr/include or /usr/local/include in WSL $Distro`: apt-get install libcsmith-dev" }
& wsl -d $Distro --cd $src -- cp -r "$include/." runtime/
if ($LASTEXITCODE -ne 0) { throw "cannot copy csmith's runtime headers from $include" }
$licence = $null
foreach ($file in @('/usr/share/doc/csmith/copyright', '/usr/share/doc/libcsmith-dev/copyright', '/usr/local/share/doc/csmith/COPYING')) {
  & wsl -d $Distro -- test -f $file
  if ($LASTEXITCODE -eq 0) { $licence = $file; break }
}
if (-not $licence) { throw "csmith's licence is not under /usr/share/doc in WSL $Distro" }
& wsl -d $Distro --cd $src -- cp $licence runtime/LICENSE
if ($LASTEXITCODE -ne 0) { throw "cannot copy csmith's licence from $licence" }

# The set is this run's alone: what an earlier run kept is rebuilt or gone.
Get-ChildItem $bin -Include seed*.exe, seed*.map, seed*.txt -Recurse | Remove-Item -Force
Get-ChildItem $src -Filter seed*.c | Remove-Item -Force

$targets = @(
  @{ Width = '64'; Triple = 'x86_64-pc-windows-msvc' },
  @{ Width = '32'; Triple = 'i686-pc-windows-msvc' }
)
$levels = @('O0', 'O2')

# The program's standard output, or $null when it does not end within the limit or ends with a
# status other than zero.
function Invoke-Program([string]$Exe) {
  $info = New-Object System.Diagnostics.ProcessStartInfo
  $info.FileName = $Exe
  $info.UseShellExecute = $false
  $info.RedirectStandardOutput = $true
  $info.CreateNoWindow = $true
  $process = [System.Diagnostics.Process]::Start($info)
  $output = $process.StandardOutput.ReadToEndAsync()
  if (-not $process.WaitForExit($Limit * 1000)) {
    $process.Kill()
    $process.WaitForExit()
    return $null
  }
  $process.WaitForExit()
  if ($process.ExitCode -ne 0) { return $null }
  return $output.Result
}

function Remove-Seed([string]$Name) {
  Get-ChildItem $bin -Filter "$Name`_*" | Remove-Item -Force
  Remove-Item -Force (Join-Path $src "$Name.c") -ErrorAction SilentlyContinue
}

# Builds and runs the four programs of a seed; the reason it is dropped, or $null when it is kept.
function Build-Seed([int]$Seed) {
  $name = 'seed{0:d2}' -f $Seed
  # --no-argc: `main` takes nothing, so the program needs no command line and no strcmp. Floats
  # stay off (csmith's default): their checksum would depend on the unit the build picks.
  & wsl -d $Distro --cd $src -- csmith --seed $Seed --no-argc --pointers --structs --bitfields --longlong --math64 --no-float --output "$name.c" | Out-Host
  if ($LASTEXITCODE -ne 0) { throw "csmith failed for seed $Seed" }
  foreach ($t in $targets) {
    $lines = @()
    foreach ($level in $levels) {
      $stem = "$name`_$($t.Width)_$level"
      $out = Join-Path $bin "$stem.exe"
      # -ffreestanding takes limits.h and stdint.h from clang alone; -U_MSC_VER keeps csmith's
      # runtime on <stdint.h>, not on its copy of a header for old MSVC; -w, a generated program
      # warns on every line.
      $args = @('-target', $t.Triple, "-$level", '-ffreestanding', '-funwind-tables', '-nostdlib', '-fuse-ld=lld', '-U_MSC_VER', '-w',
        '-I', $platform, '-I', $runtime, (Join-Path $src "$name.c"), (Join-Path $platform 'windows.c'), '-o', $out, '-lkernel32', '-llibcmt',
        '-Wl,/entry:mainCRTStartup', '-Wl,/subsystem:console', '-Wl,/Brepro', "-Wl,/map:$(Join-Path $bin "$stem.map")")
      # Out-Host: what a tool prints is not the function's result.
      # A seed whose build fails is dropped, not fatal: some programs need a helper no C library
      # here gives (seed 22 calls __atomic_load for a struct copy).
      & clang @args | Out-Host
      if ($LASTEXITCODE -ne 0) { return "clang failed for $stem" }
      $output = Invoke-Program $out
      if ($null -eq $output) { return "$stem did not end with status 0 within $Limit s" }
      $line = $output.TrimEnd("`r", "`n")
      if ($line -cnotmatch '^checksum = [0-9A-F]{1,8}$') { return "$stem printed '$line', not a checksum line" }
      [System.IO.File]::WriteAllText((Join-Path $bin "$stem.txt"), "$line`n")
      $lines += $line
    }
    if ($lines[0] -ne $lines[1]) { return "$name`_$($t.Width) prints $($lines[0]) at -O0 and $($lines[1]) at -O2" }
  }
  return $null
}

$kept = @()
$dropped = @()
for ($seed = 1; $kept.Count -lt $Count; $seed++) {
  if ($seed -gt $Last) { throw "only $($kept.Count) of $Count usable seeds up to $Last" }
  Write-Host "seed $seed" -ForegroundColor Cyan
  $reason = Build-Seed $seed
  if ($reason) {
    Remove-Seed ('seed{0:d2}' -f $seed)
    $dropped += "seed $seed`: $reason"
    Write-Host "  dropped: $reason" -ForegroundColor Yellow
  } else {
    $kept += $seed
  }
}

Write-Host "kept $($kept.Count) seeds: $($kept -join ' ')"
foreach ($line in $dropped) { Write-Host "dropped $line" -ForegroundColor Yellow }
Write-Host "tools: $(& wsl -d $Distro -- csmith --version | Select-Object -First 1); $(& clang --version | Select-Object -First 1)"
Write-Host "programs: $((Get-ChildItem $bin -Filter seed*.exe).Count)"
