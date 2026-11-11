//! The analysis against the fixtures and their linker maps.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ra_asm_analysis::{Analysis, Terminator, analyze};
use ra_asm_pe::Image;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin").join(name)
}

fn load(name: &str) -> Image {
    ra_asm_pe::load(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e:#}"))
}

/// The functions of the `.text` section as the linker map names them: `name -> address`. The
/// import thunks the linker adds (`kernel32:KERNEL32.dll`) are left out: nothing calls them.
fn map_functions(name: &str) -> BTreeMap<String, u64> {
    let text = std::fs::read_to_string(fixture(name)).unwrap();
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 || !parts[0].starts_with("0001:") {
            continue;
        }
        // The section line (` 0001:00000000 000005f0H .text CODE`) and the linker's import thunks.
        if parts[1].ends_with('H') || parts[3].starts_with("kernel32:") {
            continue;
        }
        let address = u64::from_str_radix(parts[2], 16).unwrap();
        // The 32-bit map decorates: `_print_int`, `_WriteFile@20`. A C++ name (`?area@Rect@@UEBAHXZ`)
        // stays whole.
        let raw = parts[1];
        let name = if raw.starts_with('?') { raw } else { raw.trim_start_matches('_').split('@').next().unwrap() };
        out.insert(name.to_owned(), address);
    }
    out
}

/// The functions of an rhtest map: `address -> name`. MSVC's `link` flags a function with `f`
/// and lists the statics under `Static symbols`; lld (the clang builds) flags nothing, so there
/// every decorated code symbol counts (`_name`, `@name@4`, `?name@@...`), and the labels the
/// assembler-written runtime objects leak (`L1`, `cs10`, `$ehgcr_4_31`) do not. The import thunks
/// (`ucrt:api-ms-win-crt-runtime-l1-1-0.dll`) are left out: the analysis knows them as thunks,
/// not as functions of the program. The exception funclets (`__catch$f$0`, `?dtor$3@?0?f@4HA`)
/// are pieces of their function, not functions.
fn rhtest_map_functions(path: &Path) -> BTreeMap<u64, String> {
    let text = std::fs::read_to_string(path).unwrap();
    let msvc = text.lines().any(|l| l.split_whitespace().nth(3) == Some("f"));
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 || !parts[0].starts_with("0001:") || parts[1].ends_with('H') {
            continue;
        }
        let object = *parts.last().unwrap();
        let symbol = parts[1];
        let decorated = symbol.starts_with(['_', '@', '?']);
        let funclet = ["__catch$", "__unwindfunclet$", "?catch$", "?dtor$"].iter().any(|p| symbol.starts_with(p));
        let thunk = Path::new(object).extension().is_some_and(|e| e.eq_ignore_ascii_case("dll"));
        if thunk || (msvc && parts[3] != "f") || (!msvc && !decorated) || funclet {
            continue;
        }
        let address = u64::from_str_radix(parts[2], 16).unwrap();
        out.entry(address).or_insert_with(|| parts[1].to_owned());
    }
    out
}

/// Every rhtest binary with its map: `<name>-<variant>.exe` beside `<name>-<variant>.map`.
fn rhtest_binaries() -> Vec<(PathBuf, PathBuf)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/rhtest");
    let mut out: Vec<(PathBuf, PathBuf)> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| {
            let path = e.ok()?.path();
            (path.extension()? == "exe").then(|| (path.clone(), path.with_extension("map")))
        })
        .collect();
    out.sort();
    assert!(!out.is_empty(), "rhtest binaries under {}", dir.display());
    out
}

fn by_name<'a>(analysis: &'a Analysis<'_>, map: &BTreeMap<String, u64>, name: &str) -> &'a ra_asm_analysis::Function {
    let address = map[name];
    analysis.function(address).unwrap_or_else(|| panic!("{name} at {address:#x} is a function"))
}

#[test]
fn finds_exactly_the_functions_of_the_linker_maps() {
    for (binary, map) in [
        ("hello64.exe", "hello64.map"),
        ("hello32.exe", "hello32.map"),
        ("control64.exe", "control64.map"),
        ("control32.exe", "control32.map"),
        ("classes64.exe", "classes64.map"),
        ("classes32.exe", "classes32.map"),
    ] {
        let image = load(binary);
        let analysis = analyze(&image);
        let expected = map_functions(map);
        let mut ours: Vec<u64> = analysis.functions.keys().copied().collect();
        let mut theirs: Vec<u64> = expected.values().copied().collect();
        ours.sort_unstable();
        theirs.sort_unstable();
        assert_eq!(ours, theirs, "{binary}: the functions ({})", expected.keys().cloned().collect::<Vec<_>>().join(", "));
        // classes has a virtual call in tail position: a jump through the vtable, which no
        // analysis of the image resolves.
        let unresolved: Vec<&String> = analysis.warnings.iter().filter(|w| !w.contains("indirect jump")).collect();
        assert!(unresolved.is_empty(), "{binary}: {:?}", analysis.warnings);
        // The map beside the fixture names its entry; the host's runtime owns the bare name.
        assert_eq!(analysis.name_of(image.entry_point().unwrap()), "mainCRTStartup_");
    }
}

/// The MSVC map beside `core-O2.exe` names its functions: each map function keeps its symbol as
/// the map has it, and its name is the source's.
#[test]
fn the_link_map_beside_the_image_names_its_functions() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/rhtest");
    let image = ra_asm_pe::load(&dir.join("core-O2.exe")).unwrap();
    assert!(!image.warnings.iter().any(|w| w.starts_with("symbols:")), "{:?}", image.warnings);
    let analysis = analyze(&image);
    let expected = rhtest_map_functions(&dir.join("core-O2.map"));
    for (&address, symbol) in &expected {
        let f = analysis.function(address).unwrap_or_else(|| panic!("{symbol} at {address:#x} is a function"));
        assert_eq!(f.symbol.as_deref(), Some(symbol.as_str()), "{symbol} at {address:#x} is named {}", f.name);
    }
    let run = analysis.functions.values().find(|f| f.symbol.as_deref() == Some("_rhtest_run")).expect("_rhtest_run");
    assert_eq!(run.name, "rhtest_run");
}

/// The MSVC shapes over the rhtest programs: `int 3` padding between functions, jump tables in
/// `.text`, `__security_check_cookie`, the hot and cold halves of one function, the CRT's own
/// code. A map function the analysis misses, one it splits (an entry of ours inside a map
/// function) and one it invents all fail by name.
#[test]
fn finds_the_functions_of_the_rhtest_maps() {
    let mut failures = Vec::new();
    for (binary, map) in rhtest_binaries() {
        let name = binary.file_name().unwrap().to_string_lossy().into_owned();
        let image = ra_asm_pe::load(&binary).unwrap_or_else(|e| panic!("{name}: {e:#}"));
        let analysis = analyze(&image);
        let expected = rhtest_map_functions(&map);
        let mut lines = Vec::new();
        for (&address, symbol) in &expected {
            if analysis.function(address).is_none() {
                let inside = analysis.containing(address).map(|f| format!(" (inside {} at {:#x})", f.name, f.entry)).unwrap_or_default();
                lines.push(format!("  missed {symbol} at {address:#x}{inside}"));
            }
        }
        for f in analysis.functions.values() {
            if f.is_thunk() || expected.contains_key(&f.entry) {
                continue;
            }
            match expected.range(..f.entry).next_back() {
                Some((&start, symbol)) => lines.push(format!("  split {symbol} at {start:#x}: an entry at {:#x}", f.entry)),
                None => lines.push(format!("  invented {} at {:#x}", f.name, f.entry)),
            }
        }
        if !lines.is_empty() {
            failures.push(format!("{name}: {} map functions, {} found\n{}", expected.len(), analysis.functions.len(), lines.join("\n")));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_rtti_names_the_classes_and_their_virtual_functions() {
    for (binary, map) in [("classes64.exe", "classes64.map"), ("classes32.exe", "classes32.map")] {
        let image = load(binary);
        let bare_image = Image::parse(std::fs::read(fixture(binary)).unwrap()).unwrap();
        let analysis = analyze(&image);
        let expected = map_functions(map);
        let by_name = |name: &str| analysis.classes.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("{binary}: class {name}"));

        // Shape is never instantiated: it has no vtable, and is known as a base of Square and Rect.
        let names: Vec<&str> = analysis.classes.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names.len(), 4, "{binary}: {names:?}");
        let shape = by_name("Shape");
        assert!(
            shape.bases.is_empty() && shape.vtables.is_empty(),
            "{binary}: Shape has {:?} and {} vtables",
            shape.bases,
            shape.vtables.len()
        );
        let square = by_name("Square");
        assert_eq!(square.bases, ["Shape"]);
        assert_eq!(square.direct_bases, ["Shape"]);
        assert_eq!(square.vtables.len(), 1);
        assert_eq!(square.vtables[0].functions.len(), 3, "{binary}: area, name and the inherited scale");
        let rect = by_name("Rect");
        assert_eq!(rect.bases, ["Shape", "Named"]);
        assert_eq!(rect.direct_bases, ["Shape", "Named"]);
        assert_eq!(rect.vtables.len(), 2, "{binary}: one vtable per base with virtual functions");
        assert_eq!(rect.vtables[0].offset, 0);
        assert_eq!(rect.vtables[0].functions.len(), 3);
        assert!(rect.vtables[1].offset > 0);
        assert_eq!(rect.vtables[1].base.as_deref(), Some("Named"));
        assert_eq!(rect.vtables[1].functions.len(), 2);
        let named = by_name("Named");
        assert!(named.bases.is_empty());
        assert_eq!(named.vtables[0].functions.len(), 2);

        // The map beside the fixture names the functions by their C++ names. Without it (the same
        // bytes parsed without a path), they take the names of their slots: the class that
        // introduced them first.
        let mangled = |name: &str| {
            let key = if image.is_64() { name.to_owned() } else { name.replace("@@UEBA", "@@UBE").replace("PEBD", "PBD") };
            expected[key.as_str()]
        };
        let name_of = |name: &str| analysis.name_of(mangled(name));
        assert_eq!(name_of("?scale@Shape@@UEBAHH@Z"), "Shape_scale");
        assert_eq!(name_of("?area@Rect@@UEBAHXZ"), "Rect_area");
        assert_eq!(name_of("?label@Rect@@UEBAPEBDXZ"), "Rect_label");
        assert_eq!(name_of("?weight@Named@@UEBAHXZ"), "Named_weight");
        let bare = analyze(&bare_image);
        let name_of = |name: &str| bare.name_of(mangled(name));
        assert_eq!(name_of("?scale@Shape@@UEBAHH@Z"), "Square_vf2");
        assert_eq!(name_of("?area@Rect@@UEBAHXZ"), "Rect_vf0");
        assert_eq!(name_of("?label@Rect@@UEBAPEBDXZ"), "Rect_Named_vf0");
        assert_eq!(name_of("?weight@Named@@UEBAHXZ"), "Named_vf1");
        assert_eq!(bare.name_of(bare_image.entry_point().unwrap()), "entry");
        let scale = analysis.function(expected[if image.is_64() { "?scale@Shape@@UEBAHH@Z" } else { "?scale@Shape@@UBEHH@Z" }]).unwrap();
        assert_eq!(scale.virtuals.len(), 1);
        assert_eq!((scale.virtuals[0].class.as_str(), scale.virtuals[0].slot), ("Square", 2));

        // The string literals each function addresses.
        let square_name =
            analysis.function(expected[if image.is_64() { "?name@Square@@UEBAPEBDXZ" } else { "?name@Square@@UBEPBDXZ" }]).unwrap();
        assert_eq!(square_name.strings.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>(), ["square"]);
        let describe_named = analysis
            .function(expected[if image.is_64() { "?describe_named@@YAHPEBUNamed@@@Z" } else { "?describe_named@@YAHPBUNamed@@@Z" }])
            .unwrap();
        assert_eq!(describe_named.strings.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>(), ["\r\n"]);
    }
}

#[test]
fn the_switch_of_classify_resolves_to_every_case() {
    for (binary, map) in [("control64.exe", "control64.map"), ("control32.exe", "control32.map")] {
        let image = load(binary);
        let analysis = analyze(&image);
        let expected = map_functions(map);
        let classify = by_name(&analysis, &expected, "classify");
        let switch = classify
            .blocks
            .iter()
            .find_map(|b| match &b.terminator {
                Terminator::Switch { targets, .. } => Some(targets),
                _ => None,
            })
            .unwrap_or_else(|| {
                panic!(
                    "{binary}: classify has a switch; terminators {:?}",
                    classify.blocks.iter().map(|b| &b.terminator).collect::<Vec<_>>()
                )
            });
        // Nine cases; the compiler may share a block between two of them.
        assert!(switch.len() >= 8 && switch.len() <= 9, "{binary}: {} switch targets", switch.len());
        for &target in switch {
            assert!(classify.block_at(target).is_some(), "{binary}: case block at {target:#x}");
        }
        // Two cases end in tail calls to collatz and fib.
        assert!(
            classify.tail_calls.contains(&expected["collatz"]) || classify.calls.contains(&expected["collatz"]),
            "{binary}: classify reaches collatz"
        );
    }
}

#[test]
fn loops_and_branches_make_the_expected_graphs() {
    let image = load("control64.exe");
    let analysis = analyze(&image);
    let expected = map_functions("control64.map");
    let collatz = by_name(&analysis, &expected, "collatz");
    // A loop: some block jumps back to an earlier block.
    assert!(collatz.blocks.iter().any(|b| b.successors().iter().any(|&s| s <= b.start)), "collatz has a back edge");
    assert!(collatz.blocks.iter().any(|b| b.terminator.is_return()));
    let fib = by_name(&analysis, &expected, "fib");
    assert!(fib.calls.contains(&fib.entry), "fib calls itself");
    assert!(fib.blocks.iter().any(|b| matches!(b.terminator, Terminator::CondJump { .. })));
}

#[test]
fn fail_never_returns_and_its_callers_stop_at_the_call() {
    for (binary, map) in [("control64.exe", "control64.map"), ("control32.exe", "control32.map")] {
        let image = load(binary);
        let analysis = analyze(&image);
        let expected = map_functions(map);
        let fail = by_name(&analysis, &expected, "fail");
        assert!(fail.noreturn, "{binary}: fail never returns: {:?}", fail.blocks.iter().map(|b| &b.terminator).collect::<Vec<_>>());
        assert!(fail.import_calls.iter().any(|i| i == "ExitProcess"));
        let op_div = by_name(&analysis, &expected, "op_div");
        let stops = op_div.blocks.iter().any(|b| b.terminator == Terminator::NoReturnCall(fail.entry));
        assert!(
            stops,
            "{binary}: op_div's call to fail ends its block: {:?}",
            op_div.blocks.iter().map(|b| &b.terminator).collect::<Vec<_>>()
        );
        let entry = analysis.function(image.entry_point().unwrap()).unwrap();
        assert!(entry.blocks.iter().any(|b| matches!(b.terminator, Terminator::NoReturnImportCall(ref i) if i == "ExitProcess")));
    }
}

#[test]
fn the_function_pointer_table_names_its_functions_by_origin() {
    let image = load("control64.exe");
    let analysis = analyze(&image);
    let expected = map_functions("control64.map");
    // op_add is only reached through the `ops` table, so it comes from a pointer or a call.
    let op_add = by_name(&analysis, &expected, "op_add");
    assert!(!op_add.origins.is_empty());
    assert_eq!(op_add.blocks.len(), 1);
    assert!(op_add.blocks[0].terminator.is_return());
}

#[test]
fn exports_name_their_functions() {
    let image = load("exports64.dll");
    let analysis = analyze(&image);
    let table = image.exports.as_ref().unwrap();
    let add = table.by_name("ra_add").unwrap().va.unwrap();
    assert_eq!(analysis.name_of(add), "ra_add");
    // clang inlined ra_add into ra_twice: one block, no call.
    let twice = table.by_name("ra_twice").unwrap().va.unwrap();
    let twice_fn = analysis.function(twice).unwrap();
    assert_eq!((twice_fn.name.as_str(), twice_fn.blocks.len()), ("ra_twice", 1));
    let hidden = table.exports.iter().find(|e| e.ordinal == 20).unwrap().va.unwrap();
    let hidden_fn = analysis
        .function(hidden)
        .unwrap_or_else(|| panic!("ordinal 20 at {hidden:#x} is a function; found {:?}", analysis.functions.keys().collect::<Vec<_>>()));
    assert_eq!(
        (hidden_fn.name.as_str(), &hidden_fn.origins),
        ("ordinal_20", &vec![ra_asm_analysis::Origin::Export("ordinal_20".to_owned())])
    );
}
