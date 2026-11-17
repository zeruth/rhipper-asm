# Compiles the port `ras emit --out out/<name>` wrote, with its runtime, into out/<name>/<name>.exe,
# and prints one line: `built <exe>` or `failed: <first error>`, so a hand port needs no pipe or
# chain. The flags are the hand port's: -O1 -g -std=c23 -gcodeview with lld, not large-address
# aware, so a 32-bit program's port keeps its 32-bit pointers. Every argument after the name is
# passed to clang as it is (`-DRA_TRACE` and the like); `-Out <dir>` reads the port from
# <dir>/<name> in place of the checkout's out/.
#
#   scripts/build-port.ps1 control32 -DRA_TRACE
#
# The parameters carry no attributes, so an argument the script does not declare reaches $args
# rather than failing to bind.
param([string]$Name, [string]$Out)

$ErrorActionPreference = 'Continue'

if (-not $Name) {
    Write-Output 'failed: usage: scripts/build-port.ps1 <name> [-Out <dir>] [-D<macro>...]'
    exit 2
}

if ($Out) {
    $dir = Join-Path $Out $Name
    $shown = "$Out/$Name/$Name.exe" -replace '\\', '/'
} else {
    $dir = Join-Path (Join-Path (Split-Path -Parent $PSScriptRoot) 'out') $Name
    $shown = "out/$Name/$Name.exe"
}

if (-not (Test-Path -LiteralPath $dir -PathType Container)) {
    Write-Output "failed: no port at $dir`: run ras emit <exe> --out $dir first"
    exit 1
}

$sources = @(Get-ChildItem -LiteralPath $dir -File | Where-Object { $_.Extension -eq '.c' } | Sort-Object Name | ForEach-Object { $_.FullName })
if ($sources.Count -eq 0) {
    Write-Output "failed: no .c file in $dir"
    exit 1
}

# The runtime the emit wrote beside the port, else the checkout's own.
$runtime = Join-Path $dir 'runtime'
if (-not (Test-Path -LiteralPath (Join-Path $runtime 'driver.c'))) {
    $runtime = Join-Path (Split-Path -Parent $PSScriptRoot) 'runtime'
}

if (-not (Get-Command clang -ErrorAction SilentlyContinue)) {
    Write-Output 'failed: clang is not on the PATH'
    exit 1
}

$exe = Join-Path $dir "$Name.exe"
$clang = @('-O1', '-g', '-std=c23', '-gcodeview', '-I', $dir, '-I', $runtime)
$clang += $sources
$clang += (Join-Path $runtime 'driver.c')
$resources = Join-Path $dir 'resources.res'
if (Test-Path -LiteralPath $resources) {
    $clang += $resources
}
$clang += @('-fuse-ld=lld', '-Wl,/largeaddressaware:no', '-lkernel32', '-luser32')
$clang += $args
$clang += @('-o', $exe)

# Windows PowerShell wraps each line a native command writes to stderr in an ErrorRecord
# (NativeCommandError); its exception's message is clang's line as written, without the host's
# `At line:` decoration, so the first error below is clang's own.
$lines = @(& clang @clang 2>&1 | ForEach-Object {
        if ($_ -is [System.Management.Automation.ErrorRecord]) { $_.Exception.Message } else { "$_" }
    })
$code = $LASTEXITCODE

if ($code -eq 0 -and (Test-Path -LiteralPath $exe)) {
    Write-Output "built $shown"
    exit 0
}

$first = $lines | Where-Object { $_ -match 'error:' } | Select-Object -First 1
if (-not $first) {
    $first = $lines | Where-Object { $_.Trim() } | Select-Object -Last 1
}
if (-not $first) {
    $first = "clang exited with $code"
}
Write-Output "failed: $($first.Trim())"
exit 1
