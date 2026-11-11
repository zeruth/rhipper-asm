//! Signature inference against the control fixture, whose source says what each function takes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ra_asm_lift::raise::abi::{Kind, infer};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin").join(name)
}

/// The linker map: name -> address, as the analysis tests read it.
fn map_functions(name: &str) -> BTreeMap<String, u64> {
    let text = std::fs::read_to_string(fixture(name)).unwrap();
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 || !parts[0].starts_with("0001:") || parts[1].ends_with('H') || parts[3].starts_with("kernel32:") {
            continue;
        }
        let address = u64::from_str_radix(parts[2], 16).unwrap();
        out.insert(parts[1].trim_start_matches('_').split('@').next().unwrap().to_owned(), address);
    }
    out
}

#[test]
fn control64_signatures_match_the_source() {
    let image = ra_asm_pe::load(&fixture("control64.exe")).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let lifted = ra_asm_lift::lift(&image, &analysis);
    let signatures = infer(&lifted.module, &lifted.roots);
    let map = map_functions("control64.map");
    let sig = |name: &str| signatures.get(&ra_asm_lift::function_name(map[name])).unwrap_or_else(|| panic!("{name} has a signature"));

    // int fib(int n), int classify(int x): one integer in, one out.
    for name in ["fib", "classify"] {
        let s = sig(name);
        assert_eq!((s.params.as_slice(), s.stack_params.len(), s.returns), ([Kind::Int].as_slice(), 0, Some(Kind::Int)), "{name}: {s:?}");
    }
    // int collatz(int n) is only called with 27: clang folded the argument in, and the binary's
    // function takes nothing. The signature follows the binary, not the source.
    let s = sig("collatz");
    assert_eq!((s.params.len(), s.returns), (0, Some(Kind::Int)), "collatz: {s:?}");
    // int op_add(int a, int b) and its kin: two in, one out; they are called through a table, so
    // the return comes from the caller reading rax after the indirect call... which the inference
    // cannot attribute. They are leaf functions that write rax; the table makes them roots.
    for name in ["op_add", "op_sub", "op_mul", "op_div"] {
        let s = sig(name);
        assert_eq!(s.params.as_slice(), [Kind::Int, Kind::Int].as_slice(), "{name}: {s:?}");
    }
    // void print_str(const char *): one in, nothing out.
    let s = sig("print_str");
    assert_eq!((s.params.as_slice(), s.returns), ([Kind::Int].as_slice(), None), "print_str: {s:?}");
    // void fail(int): never returns.
    let s = sig("fail");
    assert!(s.noreturn && s.params.len() == 1, "fail: {s:?}");
    // int sum_table(void): nothing in, one out.
    let s = sig("sum_table");
    assert_eq!((s.params.len(), s.returns), (0, Some(Kind::Int)), "sum_table: {s:?}");
    // void sort_table(void).
    let s = sig("sort_table");
    assert_eq!((s.params.len(), s.returns), (0, None), "sort_table: {s:?}");
    // The entry: no parameters, never returns (it calls ExitProcess).
    let s = sig("mainCRTStartup");
    assert_eq!(s.params.len(), 0, "entry: {s:?}");
    assert!(s.frame_below >= 0x28, "entry has a frame: {s:?}");
}

#[test]
fn float64_signatures_see_the_double_parameters() {
    let image = ra_asm_pe::load(&fixture("float64.exe")).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let lifted = ra_asm_lift::lift(&image, &analysis);
    let signatures = infer(&lifted.module, &lifted.roots);
    let map = map_functions("float64.map");
    let sig = |name: &str| signatures.get(&ra_asm_lift::function_name(map[name])).unwrap_or_else(|| panic!("{name} has a signature"));
    // long long milli(double x): a double in, an integer out.
    let s = sig("milli");
    assert_eq!((s.params.as_slice(), s.returns), ([Kind::Double].as_slice(), Some(Kind::Int)), "milli: {s:?}");
    // double mean(const double *values, int count): the count is always 5, so clang folded it in
    // and the pointer is the one parameter left.
    let s = sig("mean");
    assert_eq!(s.params.as_slice(), [Kind::Int].as_slice(), "mean: {s:?}");
    // int classify(double x).
    let s = sig("classify");
    assert_eq!((s.params.as_slice(), s.returns), ([Kind::Double].as_slice(), Some(Kind::Int)), "classify: {s:?}");
    // double newton_sqrt(double x): called with two different values, so the parameter stays.
    let s = sig("newton_sqrt");
    assert_eq!(s.params.as_slice(), [Kind::Double].as_slice(), "newton_sqrt: {s:?}");
}
