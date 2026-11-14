//! The csmith programs under `testdata/csmith` (written by `testdata/csmith/build.ps1`): 32 seeds,
//! each built for x86-64 and i686 at -O0 and -O2, each binary an image the loader reads and each
//! with the one checksum line its native run printed.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ra_asm_pe::Machine;

const SEEDS: usize = 32;
const VARIANTS: [(&str, &str); 4] = [("64", "O0"), ("64", "O2"), ("32", "O0"), ("32", "O2")];

fn csmith() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/csmith")
}

/// The seed, the width and the level of `seed<NN>_<width>_<level>`.
fn parse(stem: &str) -> Option<(&str, &str, &str)> {
    let mut parts = stem.strip_prefix("seed")?.split('_');
    let (seed, width, level) = (parts.next()?, parts.next()?, parts.next()?);
    let numbered = !seed.is_empty() && seed.bytes().all(|b| b.is_ascii_digit());
    (numbered && parts.next().is_none()).then_some((seed, width, level))
}

/// The checksum of an expected file: `checksum = <hex>` and a line feed, nothing else.
fn checksum(text: &str) -> Option<&str> {
    let hex = text.strip_suffix('\n')?.strip_prefix("checksum = ")?;
    let digits = (1..=8).contains(&hex.len()) && hex.bytes().all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b));
    digits.then_some(hex)
}

#[test]
fn the_csmith_programs_load_and_have_a_checksum() {
    let bin = csmith().join("bin");
    let mut programs: Vec<PathBuf> = std::fs::read_dir(&bin)
        .unwrap_or_else(|e| panic!("{}: {e} (testdata/csmith/build.ps1 writes it)", bin.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "exe"))
        .collect();
    programs.sort();
    assert_eq!(programs.len(), SEEDS * VARIANTS.len(), "programs under {}", bin.display());

    // seed -> (width, level) -> checksum
    let mut seeds: BTreeMap<String, BTreeMap<(String, String), String>> = BTreeMap::new();
    for path in &programs {
        let stem = path.file_stem().unwrap().to_str().unwrap();
        let (seed, width, level) = parse(stem).unwrap_or_else(|| panic!("{stem}: not seed<NN>_<width>_<level>"));

        let image = ra_asm_pe::load(path).unwrap_or_else(|e| panic!("{stem}: {e:#}"));
        let machine = if width == "64" { Machine::Amd64 } else { Machine::I386 };
        assert_eq!(image.machine(), machine, "{stem}");
        assert_eq!(image.is_64(), width == "64", "{stem}");
        assert!(!image.headers.coff.is_dll(), "{stem} is a program");
        let entry = image.entry_point().unwrap_or_else(|| panic!("{stem} has no entry point"));
        assert!(image.is_code(entry), "{stem}: the entry point {entry:#x} lies in a code section");

        let expected = path.with_extension("txt");
        let text = std::fs::read_to_string(&expected).unwrap_or_else(|e| panic!("{}: {e}", expected.display()));
        let sum = checksum(&text).unwrap_or_else(|| panic!("{stem}: the expected file is {text:?}, not one checksum line"));
        seeds.entry(seed.to_owned()).or_default().insert((width.to_owned(), level.to_owned()), sum.to_owned());
    }

    assert_eq!(seeds.len(), SEEDS, "seeds: {:?}", seeds.keys().collect::<Vec<_>>());
    let wanted: BTreeSet<(String, String)> = VARIANTS.iter().map(|&(w, l)| (w.to_owned(), l.to_owned())).collect();
    for (seed, variants) in &seeds {
        let built: BTreeSet<_> = variants.keys().cloned().collect();
        assert_eq!(built, wanted, "seed{seed}: one program per width and level");
        // The build keeps a seed only when the two levels of a width print the same line.
        for width in ["64", "32"] {
            let (o0, o2) = (&variants[&(width.to_owned(), "O0".to_owned())], &variants[&(width.to_owned(), "O2".to_owned())]);
            assert_eq!(o0, o2, "seed{seed}_{width}: -O0 and -O2 print the same checksum");
        }
        let source = csmith().join("src").join(format!("seed{seed}.c"));
        assert!(source.is_file(), "{} is committed beside its programs", source.display());
    }

    // csmith's runtime is committed with its licence, so a program rebuilds without WSL.
    for file in ["csmith.h", "LICENSE"] {
        let path = csmith().join("src/runtime").join(file);
        assert!(path.is_file(), "{} is committed", path.display());
    }
}
