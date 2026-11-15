//! Every runtime function the lift declares in `state.rs` has its definition in each runtime a
//! lifted program runs on: the C runtime `runtime/driver.c`, the Rust runtime `runtime/rhipper.rs`
//! and the interpreter's host `crates/ra-asm-lift/src/run/host.rs`. A shim that lands in fewer than
//! the four places fails here with its name and the file that lacks it, rather than at the link of a
//! port that happens to call it.

use std::path::Path;

/// The shims a runtime does not define on purpose, with the reason; a gap the runtime fills fails
/// the test until it leaves this list.
const GAPS: &[(&str, &str, &str)] = &[
    ("ra_svc", "runtime/driver.c", "a Thumb-2 program runs only in the interpreter"),
    ("ra_svc", "runtime/rhipper.rs", "a Thumb-2 program runs only in the interpreter"),
    ("ra_call", "runtime/rhipper.rs", "the Rust runtime has no engine for a machine-level call"),
    ("ra_eh_throw_destructor", "runtime/driver.c", "the x86 frame handler's protocol runs only in the interpreter"),
    ("ra_eh_throw_destructor", "runtime/rhipper.rs", "the x86 frame handler's protocol runs only in the interpreter"),
    ("ra_eh_throw_copy", "runtime/driver.c", "the x86 frame handler's protocol runs only in the interpreter"),
    ("ra_eh_throw_copy", "runtime/rhipper.rs", "the x86 frame handler's protocol runs only in the interpreter"),
    ("ra_eh_ending_object", "runtime/driver.c", "the x86 frame handler's protocol runs only in the interpreter"),
    ("ra_eh_ending_object", "runtime/rhipper.rs", "the x86 frame handler's protocol runs only in the interpreter"),
    ("ra_eh_ending_destructor", "runtime/driver.c", "the x86 frame handler's protocol runs only in the interpreter"),
    ("ra_eh_ending_destructor", "runtime/rhipper.rs", "the x86 frame handler's protocol runs only in the interpreter"),
    ("ra_eh_catch_sp", "runtime/driver.c", "the x86 frame handler's protocol runs only in the interpreter"),
    ("ra_eh_catch_sp", "runtime/rhipper.rs", "the x86 frame handler's protocol runs only in the interpreter"),
    ("ra_eh_ending_sp", "runtime/driver.c", "the x86 frame handler's protocol runs only in the interpreter"),
    ("ra_eh_ending_sp", "runtime/rhipper.rs", "the x86 frame handler's protocol runs only in the interpreter"),
];

/// The `ra_*` functions the lift declares, from every declaring function of `state.rs`.
fn shims() -> Vec<String> {
    let mut module = ra_asm_ir::Module::default();
    ra_asm_lift::state::declare_runtime(&mut module);
    ra_asm_lift::state::declare_eh_runtime(&mut module);
    ra_asm_lift::state::declare_eh32_runtime(&mut module);
    ra_asm_lift::state::declare_seh_runtime(&mut module);
    ra_asm_lift::state::declare_svc(&mut module);
    module.declarations.into_iter().map(|d| d.name).filter(|n| n.starts_with("ra_")).collect()
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// A C definition: a line at the left margin that names the function before its parameters and
/// opens its body, as `ras doctor` reads them.
fn defined_in_c(source: &str, name: &str) -> bool {
    let call = format!("{name}(");
    source
        .lines()
        .filter(|l| !l.starts_with(char::is_whitespace) && !l.starts_with("//") && l.contains('{'))
        .any(|l| l.match_indices(&call).any(|(at, _)| !l[..at].ends_with(is_ident)))
}

/// A Rust definition: `fn name(`.
fn defined_in_rust(source: &str, name: &str) -> bool {
    let item = format!("fn {name}(");
    source.lines().any(|l| l.match_indices(&item).any(|(at, _)| !l[..at].ends_with(is_ident)))
}

/// The host's definition: a match arm on the name, alone, among alternatives or under a guard.
fn defined_in_host(source: &str, name: &str) -> bool {
    let literal = format!("\"{name}\"");
    source.match_indices(&literal).any(|(at, _)| {
        let rest = source[at + literal.len()..].trim_start();
        rest.starts_with("=>") || rest.starts_with('|') || rest.starts_with("if ")
    })
}

#[test]
fn every_runtime_shim_is_defined_in_every_runtime() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let runtimes: [(&str, fn(&str, &str) -> bool); 3] = [
        ("runtime/driver.c", defined_in_c),
        ("runtime/rhipper.rs", defined_in_rust),
        ("crates/ra-asm-lift/src/run/host.rs", defined_in_host),
    ];
    let shims = shims();
    assert!(shims.iter().any(|s| s == "ra_trap"), "state.rs declares no ra_trap: the declaring functions changed");
    let mut failures = Vec::new();
    for (file, defined) in runtimes {
        let source = std::fs::read_to_string(root.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
        for shim in &shims {
            let gap = GAPS.iter().find(|(s, f, _)| s == shim && *f == file);
            match (defined(&source, shim), gap) {
                (false, None) => failures.push(format!("{shim}: no definition in {file}")),
                (true, Some((_, _, why))) => {
                    failures.push(format!("{shim}: {file} defines it now, so it leaves GAPS (\"{why}\")"));
                }
                _ => {}
            }
        }
    }
    for (shim, file, _) in GAPS {
        if !shims.iter().any(|s| s == shim) {
            failures.push(format!("{shim}: in GAPS for {file}, but state.rs no longer declares it"));
        }
    }
    assert!(failures.is_empty(), "the runtime shims and their definitions disagree:\n{}", failures.join("\n"));
}
