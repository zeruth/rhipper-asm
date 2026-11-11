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
        // The 32-bit map decorates: `_print_int`, `_WriteFile@20`.
        let name = parts[1].trim_start_matches('_').split('@').next().unwrap();
        out.insert(name.to_owned(), address);
    }
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
    ] {
        let image = load(binary);
        let analysis = analyze(&image);
        let expected = map_functions(map);
        let mut ours: Vec<u64> = analysis.functions.keys().copied().collect();
        let mut theirs: Vec<u64> = expected.values().copied().collect();
        ours.sort_unstable();
        theirs.sort_unstable();
        assert_eq!(ours, theirs, "{binary}: the functions ({})", expected.keys().cloned().collect::<Vec<_>>().join(", "));
        assert!(analysis.warnings.is_empty(), "{binary}: {:?}", analysis.warnings);
        assert_eq!(analysis.name_of(image.entry_point().unwrap()), "entry");
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
