#!/bin/sh
# Writes crates/ra-asm-pe/src/iop_names.rs: the ordinal of every function the IOP libraries of
# ps2sdk export, with its library and its name. Each header of `$PS2SDK/iop/include` that a module
# imports from opens a table with `DECLARE_IMPORT_TABLE(<library>, ...)` and names each function
# with `DECLARE_IMPORT(<ordinal>, <name>)`.
# Run from the repository root in WSL, whose login shell sets PS2SDK:
#   wsl -d Ubuntu-24.04 -- bash -l testdata/ps2/iop_names.sh
set -eu
include="${PS2SDK:?PS2SDK is not set: run under the login shell}/iop/include"
out=crates/ra-asm-pe/src/iop_names.rs
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

find "$include" -name '*.h' | LC_ALL=C sort | while read -r header; do
    awk '
        function hex(s,    v, i) {
            v = 0
            s = tolower(s)
            for (i = 1; i <= length(s); i++) v = v * 16 + index("0123456789abcdef", substr(s, i, 1)) - 1
            return v
        }
        FNR == 1 { library = "" }
        match($0, /DECLARE_IMPORT_TABLE\( *[A-Za-z_][A-Za-z_0-9]* *,/) {
            library = substr($0, RSTART, RLENGTH)
            sub(/^DECLARE_IMPORT_TABLE\( */, "", library)
            sub(/ *,$/, "", library)
        }
        library != "" && match($0, /DECLARE_IMPORT\( *(0[xX][0-9a-fA-F]+|[0-9]+) *, *[A-Za-z_][A-Za-z_0-9]* *\)/) {
            call = substr($0, RSTART, RLENGTH)
            sub(/^DECLARE_IMPORT\( */, "", call)
            sub(/ *\)$/, "", call)
            split(call, parts, / *, */)
            ordinal = parts[1] ~ /^0[xX]/ ? hex(substr(parts[1], 3)) : parts[1] + 0
            printf "%s %d %s\n", library, ordinal, parts[2]
        }' "$header"
done | LC_ALL=C sort -s -u -k1,1 -k2,2n >"$work/table"   # one name per ordinal: the first header's

{
    echo '//! The functions of the `PlayStation 2` IOP'"'"'s libraries: an IRX imports each by its ordinal in a'
    echo '//! library, and this names it, `(library, ordinal, name)` in the order of the library and the'
    echo '//! ordinal. Written by `testdata/ps2/iop_names.sh` from the IOP headers of ps2sdk; not edited.'
    echo
    echo 'pub(crate) const NAMES: &[(&str, u16, &str)] = &['
    awk '{ printf "    (\"%s\", %s, \"%s\"),\n", $1, $2, $3 }' "$work/table"
    echo '];'
} >"$out"
echo "$(wc -l <"$work/table") functions written to $out"
