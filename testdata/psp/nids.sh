#!/bin/sh
# Writes crates/ra-asm-pe/src/psp_nids.rs: the NID of every function the stub archives of pspdev's
# SDK import, with its library and its name. Each stub object of `$PSPDEV/psp/sdk/lib/*.a` is one
# `STUB_FUNC 0x<nid>, <name>` of the SDK's stub sources: the function in `.sceStub.text`, its NID
# at the same index in `.rodata.sceNid`, its library the `__stub_module_<library>` it references.
# Run from the repository root in WSL, whose login shell sets PSPDEV:
#   wsl -d Ubuntu-24.04 -- bash -l testdata/psp/nids.sh
set -eu
lib="${PSPDEV:?PSPDEV is not set: run under the login shell}/psp/sdk/lib"
out=crates/ra-asm-pe/src/psp_nids.rs
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

for archive in "$lib"/*.a; do
    dir="$work/$(basename "$archive" .a)"
    mkdir -p "$dir"
    (cd "$dir" && psp-ar x "$archive")
    for object in "$dir"/*.o; do
        psp-objdump -t -s "$object" 2>/dev/null | awk '
            function hex(s,    v, i) {
                v = 0
                for (i = 1; i <= length(s); i++) v = v * 16 + index("0123456789abcdef", substr(s, i, 1)) - 1
                return v
            }
            # The library, from the undefined head of the stub table each stub points at.
            $NF ~ /^__stub_module_/ { library = substr($NF, 15) }
            # A function of the stub text: its offset over the stub size of 8 bytes is its index.
            $3 == "F" && $4 == ".sceStub.text" { names[n++] = $NF; offsets[$NF] = hex($1) / 8 }
            /^Contents of section / { section = $4; sub(/:$/, "", section); next }
            section == ".rodata.sceNid" && /^ [0-9a-f]+ / {
                # The hex columns, without the text column after them; a word is in file order.
                base = hex($1) / 4
                count = split(substr($0, 7, 35), words, " ")
                for (i = 1; i <= count; i++) {
                    w = words[i]
                    if (length(w) == 8) nids[base + i - 1] = substr(w, 7, 2) substr(w, 5, 2) substr(w, 3, 2) substr(w, 1, 2)
                }
            }
            END {
                if (library == "") exit
                for (k = 0; k < n; k++) {
                    name = names[k]
                    if ((offsets[name]) in nids) printf "%s 0x%s %s\n", library, nids[offsets[name]], name
                }
            }'
    done
done | LC_ALL=C sort -u -k1,1 -k2,2 >"$work/table"

{
    echo '//! The NIDs of the PSP'"'"'s system libraries: each function a PRX imports is the 32-bit NID of its'
    echo '//! name in a library, and this names it, `(library, nid, name)` in the order of the library and'
    echo '//! the NID. Written by `testdata/psp/nids.sh` from the stub archives of pspdev'"'"'s SDK; not edited.'
    echo
    echo '// the NIDs are printed as the SDK writes them, eight hex digits without separators'
    echo '#[allow(clippy::unreadable_literal)]'
    echo 'pub(crate) const NIDS: &[(&str, u32, &str)] = &['
    awk '{ printf "    (\"%s\", %s, \"%s\"),\n", $1, $2, $3 }' "$work/table"
    echo '];'
} >"$out"
echo "$(wc -l <"$work/table") NIDs written to $out"
