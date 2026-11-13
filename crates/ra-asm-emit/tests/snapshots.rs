//! The C of small functions against the text kept under `testdata/cases/emit`, one file per
//! case: what the backend writes for a shape is the snapshot, line for line, and a change to it
//! shows as the first line that differs.
//!
//! The checked build has one case per kind of site: the saved register compared after a call,
//! the index against the table's length before a `switch`, the stack pointer before a return.

use std::path::PathBuf;

use ra_asm_emit::{COptions, Check, Unit, Watch, emit_unit};
use ra_asm_ir::{BinOp, Callee, FnType, Function, FunctionBuilder, Module, Operand, Terminator, Type};

/// Compares `actual` with the snapshot `name`; a failure names the first line that differs and
/// prints the whole of what was written.
fn snapshot(name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cases/emit").join(format!("{name}.c"));
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}\n{actual}", path.display()));
    if let Err(line) = ra_asm_testing::digest::compare(&format!("emit-snapshots-{name}"), &expected, actual) {
        panic!("{line}\n{actual}");
    }
}

fn check(id: u32, function: &Function, kind: &str, site: u64, watch: Watch) -> Check {
    Check { id, function: function.name.clone(), kind: kind.to_owned(), site, watch }
}

/// The unit of `functions` written with `checks`.
fn unit_of(functions: Vec<Function>, checks: Vec<Check>) -> Unit {
    let mut module = Module::x86_64_windows("checked");
    module.functions = functions;
    emit_unit(&module, &COptions { checks, ..Default::default() })
}

/// The C of the one function `f`, written with its check.
fn c_of(f: Function, check: Check) -> String {
    let unit = unit_of(vec![f], vec![check]);
    assert_eq!(unit.unwatched, Vec::<u32>::new(), "the check has its site");
    unit.functions[0].text.clone()
}

/// A call at 0x1004 to a function that hands `rbx` back: the register before the call and the
/// one after it are the two values the check compares.
fn call() -> (Function, Check) {
    let mut b = FunctionBuilder::new("F_00001000", FnType::new(Type::I64, vec![Type::I64]), &["rbx"]);
    b.set_address(0x1000);
    let rbx = b.param(0);
    let entry = b.block("entry");
    b.address = Some(0x1004);
    let callee = Callee::Direct("F_00002000".into());
    let after = b.call(entry, FnType::new(Type::I64, vec![Type::I64]), callee, vec![(Type::I64, rbx.into())], "rbx.after").unwrap();
    b.terminate(entry, Terminator::Ret(Some((Type::I64, after.into()))));
    let f = b.finish();
    let watch = Watch::Saved(vec![(Type::I64, rbx.into(), after.into())]);
    let check = check(0, &f, "callee-saved", 0x1004, watch);
    (f, check)
}

/// A jump table of two entries, as the lift writes one: a `switch` over the index whose default
/// is the trap that names the jump at 0x1108.
fn switch() -> (Function, Check) {
    let mut b = FunctionBuilder::new("F_00001100", FnType::new(Type::I64, vec![Type::I64]), &["index"]);
    b.set_address(0x1100);
    let index = b.param(0);
    let entry = b.block("entry");
    let zero = b.block("zero");
    let one = b.block("one");
    let bad = b.block("switch.bad");
    b.terminate(entry, Terminator::Switch { ty: Type::I64, value: index.into(), default: bad, cases: vec![(0, zero), (1, one)] });
    b.terminate(zero, Terminator::Ret(Some((Type::I64, Operand::Int(10)))));
    b.terminate(one, Terminator::Ret(Some((Type::I64, Operand::Int(20)))));
    b.address = Some(0x1108);
    let trap = FnType::new(Type::Void, vec![Type::I64, Type::I64]);
    b.call(bad, trap, Callee::Direct("ra_trap".into()), vec![(Type::I64, Operand::Int(0x1108)), (Type::I64, Operand::Int(7))], "");
    let f = b.finish();
    let check = check(1, &f, "table-index", 0x1108, Watch::Index);
    (f, check)
}

/// A function that moves its stack pointer down by a run-time amount and back up before its
/// return at 0x1208: the stack pointer there against the one it was entered with.
fn stack() -> (Function, Check) {
    let mut b = FunctionBuilder::new("F_00001200", FnType::new(Type::I64, vec![Type::I64, Type::I64]), &["rsp", "n"]);
    b.set_address(0x1200);
    let (rsp, n) = (b.param(0), b.param(1));
    let entry = b.block("entry");
    b.address = Some(0x1204);
    let low = b.bin(entry, BinOp::Sub, Type::I64, rsp, n, "rsp.low");
    b.address = Some(0x1208);
    let end = b.bin(entry, BinOp::Add, Type::I64, low, n, "rsp.end");
    b.terminate(entry, Terminator::Ret(Some((Type::I64, n.into()))));
    let f = b.finish();
    let check = check(2, &f, "balanced-stack", 0x1208, Watch::Stack(Type::I64, end.into(), rsp.into()));
    (f, check)
}

#[test]
fn checked_call_compares_the_saved_register_after_it() {
    let (f, check) = call();
    snapshot("checked_call", &c_of(f, check));
}

#[test]
fn checked_switch_tests_the_index_against_the_table() {
    let (f, check) = switch();
    snapshot("checked_switch", &c_of(f, check));
}

#[test]
fn checked_return_compares_the_stack_pointer() {
    let (f, check) = stack();
    snapshot("checked_return", &c_of(f, check));
}

/// The table the runtime names a failed check from: a row per check, at its number.
#[test]
fn checked_unit_defines_the_table_of_its_assumptions() {
    let ((call, saved), (switch, index), (stack, balanced)) = (call(), switch(), stack());
    let unit = unit_of(vec![call, switch, stack], vec![saved, index, balanced]);
    assert_eq!(unit.unwatched, Vec::<u32>::new());
    snapshot("checked_table", &unit.global_defs);
}

/// A check whose site is no call, `switch` or return of its function is written nowhere, and the
/// unit says so; a number no check has is an empty row.
#[test]
fn checked_unit_names_a_check_without_a_site() {
    let (f, saved) = call();
    let stray = check(2, &f, "table-index", 0x9999, Watch::Index);
    let unit = unit_of(vec![f], vec![saved, stray]);
    assert_eq!(unit.unwatched, [2]);
    assert!(unit.global_defs.contains("    {0, 0, 0},\n    {\"F_00001000\", \"table-index\", 0x9999ull},\n"), "{}", unit.global_defs);
    assert_eq!(unit.functions[0].text.matches("ra_assume(").count(), 1, "{}", unit.functions[0].text);
}

/// Without checks the translation names neither the check nor the table.
#[test]
fn unchecked_unit_writes_no_check() {
    let (f, _) = call();
    let text = unit_of(vec![f], Vec::new()).text();
    assert!(!text.contains("ra_assum"), "{text}");
}
