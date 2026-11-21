# Rebuilds the Java fixtures: `src/Opcodes.java` compiled for every release the installed JDK
# targets (8 to 25 with JDK 25), each into `<release>/Opcodes.class` with `javap -c -p`'s listing
# beside it as `Opcodes.txt`, and what the program prints as `Opcodes.out`.
#
# Usage: powershell -File testdata/java/build.ps1
#
# `src/Thrown.java`, the fixture of the machine's exceptions, is compiled once, for Java 8, into
# `thrown/`, with what it prints before it throws as `Thrown.out`.
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$thrown = Join-Path $here 'thrown'
New-Item -ItemType Directory -Force $thrown | Out-Null
& javac --release 8 -nowarn -Xlint:-options -d $thrown (Join-Path $here 'src\Thrown.java') 2>$null
$printed = & java -cp $thrown Thrown 2>$null
[IO.File]::WriteAllText((Join-Path $thrown 'Thrown.out'), (($printed -join "`n") + "`n"))
Write-Host "thrown: prints $($printed.Count) lines"
#
# `src/Plain.java`, the fixture of the releases before Java 8, is compiled once, for Java 8, into
# `plain/`, with what it prints as `Plain.out`: it uses nothing the first class files had not, and
# the tests set its classes' version back release by release.
$plain = Join-Path $here 'plain'
New-Item -ItemType Directory -Force $plain | Out-Null
& javac --release 8 -nowarn -Xlint:-options -d $plain (Join-Path $here 'src\Plain.java') 2>$null
$printed = & java -cp $plain Plain
[IO.File]::WriteAllText((Join-Path $plain 'Plain.out'), (($printed -join "`n") + "`n"))
Write-Host "plain: prints $($printed.Count) lines"
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
