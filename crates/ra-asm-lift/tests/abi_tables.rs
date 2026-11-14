//! The answers of `Target`'s methods, written down: every method of each target over its whole
//! argument range (parameter positions 0 to 15 for lists of integers, of doubles and of both
//! alternating; every slot of the state for `slots` and `callee_saved`), recomputed here and
//! compared with `testdata/cases/abi_tables.txt`. A change to the conventions, or their move into
//! data, shows up as a diff of that file; `RA_UPDATE_SNAPSHOT=1` rewrites it.

use std::path::{Path, PathBuf};

use ra_asm_lift::raise::Target;
use ra_asm_lift::raise::abi::Kind;
use ra_asm_lift::raise::liveness::Slot;

const TARGETS: [(&str, Target); 4] = [("WIN64", Target::WIN64), ("X86", Target::X86), ("ARM64", Target::ARM64), ("ARM32", Target::ARM32)];
const KINDS: [Kind; 4] = [Kind::Int, Kind::Wide, Kind::Float, Kind::Double];
/// The parameter positions the snapshot covers.
const POSITIONS: usize = 16;

fn snapshot_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cases/abi_tables.txt")
}

/// The answers of every method of `target`, one per line.
fn describe(name: &str, target: Target) -> Vec<String> {
    let mut lines = vec![format!("# {name} bits={} isa={:?}", target.bits, target.isa)];
    lines.push(format!("is_64 = {}", target.is_64()));
    lines.push(format!("is_arm64 = {}", target.is_arm64()));
    lines.push(format!("is_arm32 = {}", target.is_arm32()));
    lines.push(format!("shares_positions = {}", target.shares_positions()));
    lines.push(format!("ptr = {}", target.ptr()));
    lines.push(format!("int_ty = {:?}", target.int_ty()));
    lines.push(format!("sp = {:?}", target.sp()));
    lines.push(format!("pushed_return = {}", target.pushed_return()));
    lines.push(format!("gprs = {}", target.gprs()));
    lines.push(format!("xmms = {}", target.xmms()));
    lines.push(format!("first_stack_arg = {}", target.first_stack_arg()));
    lines.push(format!("outgoing_at_call = {}", target.outgoing_at_call()));
    lines.push(format!("int_param_slots = {:?}", target.int_param_slots()));
    lines.push(format!("float_params = {}", target.float_params()));
    lines.push(format!("entry_misalignment = {}", target.entry_misalignment()));
    for kind in KINDS {
        lines.push(format!("stack_width {kind:?} = {}", kind.stack_width(target)));
    }
    let ints = vec![Kind::Int; POSITIONS];
    let doubles = vec![Kind::Double; POSITIONS];
    let mixed: Vec<Kind> = (0..POSITIONS).map(|i| if i % 2 == 0 { Kind::Int } else { Kind::Double }).collect();
    for (list, params) in [("int", &ints), ("double", &doubles), ("mixed", &mixed)] {
        for index in 0..POSITIONS {
            lines.push(format!("param_slot {list} {index} = {:?}", target.param_slot(params, index)));
        }
    }
    let in_state: Vec<Slot> = target.slots().collect();
    for slot in Slot::all() {
        lines.push(format!("slot {slot:?} in_state={} callee_saved={}", in_state.contains(&slot), target.callee_saved(slot)));
    }
    lines
}

#[test]
fn snapshot() {
    let mut text = String::new();
    for (name, target) in TARGETS {
        for line in describe(name, target) {
            text.push_str(&line);
            text.push('\n');
        }
        text.push('\n');
    }
    let path = snapshot_path();
    if std::env::var_os("RA_UPDATE_SNAPSHOT").is_some() {
        std::fs::write(&path, &text).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).replace("\r\n", "\n");
    if committed != text {
        let (number, (want, got)) = committed
            .lines()
            .zip(text.lines())
            .enumerate()
            .find(|(_, (a, b))| a != b)
            .unwrap_or((committed.lines().count().min(text.lines().count()), ("<end>", "<end>")));
        panic!(
            "{} differs from the computed answers at line {}:\n  committed: {want}\n  computed:  {got}\nrerun with RA_UPDATE_SNAPSHOT=1 to rewrite it",
            path.display(),
            number + 1
        );
    }
}

/// The methods of `Target` read its convention instead of asking for its instruction set: only
/// `is_arm64` and `is_arm32`, which are that question, name the ISA.
#[test]
fn target_reads_its_abi() {
    const ISA_TESTS: [&str; 5] = ["self.isa", "Isa::", "is_arm64(", "is_arm32(", "is_arm("];
    const PREDICATES: [&str; 2] = ["is_arm64", "is_arm32"];
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/raise/mod.rs");
    let source = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).replace("\r\n", "\n");
    let start = source.find("\nimpl Target {\n").unwrap_or_else(|| panic!("{} has no `impl Target`", path.display()));
    let body = &source[start + 1..];
    let body = &body[..body.find("\n}\n").expect("the end of `impl Target`")];
    let mut method: Option<&str> = None;
    let mut offenders = Vec::new();
    for line in body.lines() {
        let code = line.trim();
        if code.starts_with("//") {
            continue;
        }
        if let Some(rest) = code.strip_prefix("pub fn ").or_else(|| code.strip_prefix("fn ")) {
            method = rest.split('(').next();
        }
        let Some(name) = method else { continue };
        if !PREDICATES.contains(&name) && ISA_TESTS.iter().any(|test| code.contains(test)) {
            offenders.push(format!("{name}: {code}"));
        }
    }
    assert!(method.is_some(), "no method found in `impl Target`");
    assert!(offenders.is_empty(), "methods of `Target` that test the ISA:\n  {}", offenders.join("\n  "));
}
