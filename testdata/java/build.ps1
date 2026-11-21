# Rebuilds the Java fixtures: `src/Opcodes.java` compiled for every release the installed JDK
# targets (8 to 25 with JDK 25), each into `<release>/Opcodes.class` with `javap -c -p`'s listing
# beside it as `Opcodes.txt`, and what the program prints as `Opcodes.out`.
#
# Usage: powershell -File testdata/java/build.ps1
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
foreach ($release in 8..25) {
    $out = Join-Path $here $release
    New-Item -ItemType Directory -Force $out | Out-Null
    & javac --release $release -nowarn -Xlint:-options -d $out (Join-Path $here 'src\Opcodes.java') 2>$null
    if ($LASTEXITCODE -ne 0) { Write-Host "release ${release}: javac does not target it"; Remove-Item -Recurse -Force $out; continue }
    $listing = & javap -c -p (Join-Path $out 'Opcodes.class')
    [IO.File]::WriteAllText((Join-Path $out 'Opcodes.txt'), (($listing -join "`n") + "`n"))
    $printed = & java -cp $out Opcodes
    [IO.File]::WriteAllText((Join-Path $out 'Opcodes.out'), (($printed -join "`n") + "`n"))
    Write-Host "release ${release}: $((Get-Item (Join-Path $out 'Opcodes.class')).Length) bytes, prints $printed"
}
