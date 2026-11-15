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

/// Whether `word` names a machine register as the raise's constants spell it: an x86 general
/// register of 64 or 32 bits, an ARM64 `X` or `W` register or a vector register.
fn is_register(word: &str) -> bool {
    const NAMED: [&str; 16] =
        ["RAX", "RCX", "RDX", "RBX", "RSP", "RBP", "RSI", "RDI", "EAX", "ECX", "EDX", "EBX", "ESP", "EBP", "ESI", "EDI"];
    let number = |rest: &str| if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) { rest.parse::<u32>().ok() } else { None };
    if NAMED.contains(&word) {
        return true;
    }
    if let Some(n) = word.strip_prefix("XMM").and_then(number) {
        return n <= 31;
    }
    if let Some(n) = word.strip_prefix('R').and_then(number) {
        return (8..=15).contains(&n);
    }
    word.strip_prefix(['X', 'W']).and_then(number).is_some_and(|n| n <= 30)
}

/// The ISA tests and register names on one line of code: the words `is_arm64`, `is_arm32` and
/// registers, and `Isa` as a path.
fn isa_tests(code: &str) -> usize {
    let word_byte = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let bytes = code.as_bytes();
    let mut count = 0;
    let mut i = 0;
    while i < bytes.len() {
        if !word_byte(bytes[i]) {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && word_byte(bytes[i]) {
            i += 1;
        }
        let word = &code[start..i];
        if word == "is_arm64" || word == "is_arm32" || (word == "Isa" && code[i..].starts_with("::")) || is_register(word) {
            count += 1;
        }
    }
    count
}

/// Every `.rs` file under `dir`, in a stable order.
fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<PathBuf> =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())).map(|entry| entry.unwrap().path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The raise asks for its instruction set, or names a register, in fewer places with every card
/// that moves an answer into `Target`: the count over `src/raise` (comment lines aside) may not
/// rise above `testdata/cases/isa_ratchet.txt`, and a fall lowers that number.
#[test]
fn ratchet() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/raise");
    let mut files = Vec::new();
    sources(&root, &mut files);
    let mut total = 0;
    let mut by_file = Vec::new();
    for path in &files {
        let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let count: usize = text.lines().filter(|line| !line.trim_start().starts_with("//")).map(isa_tests).sum();
        if count > 0 {
            let name = path.strip_prefix(&root).unwrap_or(path).display().to_string().replace('\\', "/");
            by_file.push(format!("{name}: {count}"));
        }
        total += count;
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cases/isa_ratchet.txt");
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let committed: usize = committed
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .and_then(|line| line.parse().ok())
        .unwrap_or_else(|| panic!("{} holds no count", path.display()));
    let files = by_file.join("\n  ");
    assert!(
        total <= committed,
        "src/raise tests the ISA or names a register {total} times, above the {committed} of {}:\n  {files}",
        path.display()
    );
    assert!(
        total == committed,
        "src/raise tests the ISA or names a register {total} times, below the {committed} of {}: lower it to {total}",
        path.display()
    );
}
