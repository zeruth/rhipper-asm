//! The C of small functions against the text kept under `testdata/cases/emit`, one file per
//! case: what the backend writes for a shape is the snapshot, line for line, and a change to it
//! shows as the first line that differs.
//!
//! The checked build has one case per kind of site: the saved register compared after a call,
//! the index against the table's length before a `switch`, the stack pointer before a return.
//!
//! A 32-bit program's imports have one case each for a structure the host converts both ways,
//! one it only widens, and an import that takes none.
//!
//! A tail two paths share, left from inside a loop, is a snapshot in C (a `goto` to its label)
//! and in Rust (a labeled block), and its tree replays the function's vectors as its graph does.
//!
//! A file of an output split by size declares what it names and nothing else.
//!
//! The printer's cases are `.ll` files in the same directory, each with the C, the C++ and the
//! Rust written for it in the `.c`, `.cpp` and `.rs` beside it; `RA_BLESS=1` rewrites those
//! snapshots from what the printers write. A snapshot can bless a mistake, so under the `oracle`
//! feature each one is built, with clang or rustc, and runs its case's `; call:` lines.

use std::path::{Path, PathBuf};

use ra_asm_emit::structure::{COPY_LIMIT, Graph, Stmt, structure};
use ra_asm_emit::tree::run_tree;
use ra_asm_emit::{ADAPTERS_FILE, COptions, Check, Dialect, RustOptions, Unit, Watch, emit_rust, emit_unit};
use ra_asm_ir::interp::{Host, Memory, Stop, Value, Vm};
use ra_asm_ir::{BinOp, CallConv, Callee, Declaration, FnType, Function, FunctionBuilder, ICond, Inst, Module, Operand, Terminator, Type};

/// Compares `actual` with the snapshot `name`; a failure names the first line that differs and
/// prints the whole of what was written.
fn snapshot(name: &str, actual: &str) {
    snapshot_of(name, "c", actual);
}

/// [`snapshot`] for a file of the language of `extension`.
fn snapshot_of(name: &str, extension: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cases/emit").join(format!("{name}.{extension}"));
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}\n{actual}", path.display()));
    let test = if extension == "c" { format!("emit-snapshots-{name}") } else { format!("emit-snapshots-{name}-{extension}") };
    if let Err(line) = ra_asm_testing::digest::compare(&test, &expected, actual) {
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

/// The unit of a 32-bit program that imports each of `imports`, by its name and arity, as the
/// raise declares an import: integers in, an integer out.
fn imports_unit(imports: &[(&str, usize)]) -> Unit {
    let mut module = Module::x86_64_windows("adapters");
    for &(name, arity) in imports {
        let ty = FnType::new(Type::I64, vec![Type::I64; arity]);
        module.declare(Declaration { name: name.to_owned(), ty, conv: CallConv::default(), attrs: Vec::new() });
    }
    emit_unit(&module, &COptions { guest32: true, ..Default::default() })
}

/// The calls keep the import's name, which the program's header points at the wrapper; the
/// wrappers go in their own file.
fn assert_wrapped(unit: &Unit, import: &str) {
    let define = format!("#define {import} ra_adapt_{import}\nextern uint64_t {import}(");
    assert!(unit.externs.contains(&define), "{}", unit.externs);
    let files = unit.files(ra_asm_emit::FUNCTIONS_PER_FILE);
    assert!(
        files.iter().any(|f| f.name == ADAPTERS_FILE && f.text == unit.adapters),
        "{:?}",
        files.iter().map(|f| &f.name).collect::<Vec<_>>()
    );
}

/// RegisterClassExA only reads the class: its wrapper widens the program's `WNDCLASSEXA` for the
/// host and writes nothing back.
#[test]
fn adapters_widen_the_class_of_register_class_ex() {
    let unit = imports_unit(&[("RegisterClassExA", 1)]);
    assert_wrapped(&unit, "RegisterClassExA");
    snapshot("adapters_register_class", &unit.adapters);
}

/// GetMessageA fills the message: its wrapper widens the program's `MSG` and narrows the host's
/// back, stopping on a value that does not fit.
#[test]
fn adapters_convert_the_message_of_get_message_both_ways() {
    let unit = imports_unit(&[("GetMessageA", 4)]);
    assert_wrapped(&unit, "GetMessageA");
    snapshot("adapters_get_message", &unit.adapters);
}

/// An import that takes no structure, or one whose layouts are the same at both widths (the
/// `RECT` of GetClientRect), is called directly: no wrapper and no `adapters.c`.
#[test]
fn adapters_leave_an_import_without_a_converted_structure_alone() {
    let unit = imports_unit(&[("ExitProcess", 1), ("GetClientRect", 2)]);
    assert_eq!(unit.adapters, "");
    assert!(unit.files(ra_asm_emit::FUNCTIONS_PER_FILE).iter().all(|f| f.name != ADAPTERS_FILE));
    snapshot("adapters_plain", &unit.externs);
}

/// A 64-bit program's structures are the host's already: its imports are called directly.
#[test]
fn adapters_are_a_32_bit_programs_only() {
    let mut module = Module::x86_64_windows("adapters");
    module.declare(Declaration {
        name: "GetMessageA".to_owned(),
        ty: FnType::new(Type::I64, vec![Type::I64; 4]),
        conv: CallConv::default(),
        attrs: Vec::new(),
    });
    let unit = emit_unit(&module, &COptions::default());
    assert_eq!(unit.adapters, "");
    assert!(!unit.externs.contains("ra_adapt_"), "{}", unit.externs);
}

/// `u32 shared_tail(u32 n, u32 *p)`: an early return when `n` is zero, then a loop over `*p` up
/// to `n` that leaves through two blocks (the count reached, a 7 found) to one return block,
/// which the early return shares and which is too long to copy.
fn shared_tail() -> Function {
    let mut b = FunctionBuilder::new("shared_tail", FnType::new(Type::I32, vec![Type::I32, Type::Ptr]), &["n", "p"]);
    let (n, p) = (b.param(0), b.param(1));
    let entry = b.block("entry");
    let header = b.block("header");
    let body = b.block("body");
    let latch = b.block("latch");
    let done = b.block("done");
    let found = b.block("found");
    let last = b.block("ret");
    let none = b.icmp(entry, ICond::Eq, Type::I32, n, 0i64, "none");
    b.terminate(entry, Terminator::CondBr { cond: none.into(), then: last, els: header });
    let i = b.load(header, Type::I32, p, "i");
    let more = b.icmp(header, ICond::Slt, Type::I32, i, n, "more");
    b.terminate(header, Terminator::CondBr { cond: more.into(), then: body, els: done });
    let hit = b.icmp(body, ICond::Eq, Type::I32, i, 7i64, "hit");
    b.terminate(body, Terminator::CondBr { cond: hit.into(), then: found, els: latch });
    let next = b.bin(latch, BinOp::Add, Type::I32, i, 1i64, "next");
    b.store(latch, Type::I32, next, p);
    b.terminate(latch, Terminator::Br(header));
    b.store(done, Type::I32, 0i64, p);
    b.terminate(done, Terminator::Br(last));
    b.store(found, Type::I32, 1i64, p);
    b.terminate(found, Terminator::Br(last));
    for k in 1..=COPY_LIMIT {
        b.store(last, Type::I32, i64::try_from(k).expect("a small count"), p);
    }
    let r = b.load(last, Type::I32, p, "r");
    b.terminate(last, Terminator::Ret(Some((Type::I32, r.into()))));
    b.finish()
}

/// C has no labeled block: the loop leaves the region by a `goto` to the tail's label, written
/// once after the region.
#[test]
fn shared_tail_in_c_goes_to_its_label_after_the_region() {
    let mut module = Module::x86_64_windows("tail");
    module.functions.push(shared_tail());
    let unit = emit_unit(&module, &COptions::default());
    snapshot("shared_tail", &unit.functions[0].text);
}

/// `uint64_t name(uint64_t x) { return x; }` at `address`.
fn identity(name: &str, address: u64) -> Function {
    let mut b = FunctionBuilder::new(name, FnType::new(Type::I64, vec![Type::I64]), &["x"]);
    b.set_address(address);
    let x = b.param(0);
    let entry = b.block("entry");
    b.terminate(entry, Terminator::Ret(Some((Type::I64, x.into()))));
    b.finish()
}

/// A file of a split output declares the functions it calls, and no other: one of its own it
/// defines before naming it is declared by its definition, and the header declares none.
#[test]
fn a_split_file_declares_only_the_functions_it_names() {
    let mut module = Module::x86_64_windows("split");
    module.functions = vec![call().0, identity("F_00002000", 0x2000), identity("F_00003000", 0x3000)];
    let unit = emit_unit(&module, &COptions::default());
    let files = unit.files(2);
    let text = |name: &str| files.iter().find(|f| f.name == name).map(|f| f.text.as_str()).unwrap_or_else(|| panic!("no {name}"));
    assert!(!text("program.h").contains("F_0000"), "{}", text("program.h"));
    let first = text("program_000.c");
    assert!(first.starts_with("#include \"program.h\"\n\nuint64_t F_00002000(uint64_t x);\n\n// 0x1000\n"), "{first}");
    assert!(first.contains("uint64_t F_00001000(uint64_t rbx) { RA_ENTER(\"F_00001000\");\n"), "{first}");
    assert!(!first.contains("F_00003000"), "{first}");
    assert_eq!(text("program_001.c").matches("F_00003000(uint64_t x)").count(), 1, "{}", text("program_001.c"));
}

/// Rust leaves the labeled block around the region: `'L_ret: { ... break 'L_ret; ... }`, then
/// the tail.
#[test]
fn shared_tail_in_rust_is_a_labeled_block() {
    let mut module = Module::x86_64_windows("tail");
    module.functions.push(shared_tail());
    let out = emit_rust(&module, &RustOptions::default());
    snapshot_of("shared_tail", "rs", &out.functions[0].text);
}

/// A host no function of these tests calls.
struct NoHost;

impl Host for NoHost {
    fn call(&mut self, _vm: &mut Vm<'_>, name: &str, _ty: &FnType, _args: &[Value]) -> Result<Value, Stop> {
        Err(Stop::fault(format!("no host function {name}")))
    }
}

/// [`shared_tail`] over values instead of memory: 100, 200 or 300 by the way into the return
/// block, which adds 1 to 16 to it.
fn shared_tail_of_values() -> Function {
    let mut b = FunctionBuilder::new("shared_tail_of_values", FnType::new(Type::I32, vec![Type::I32]), &["x"]);
    let x = b.param(0);
    let entry = b.block("entry");
    let header = b.block("header");
    let body = b.block("body");
    let latch = b.block("latch");
    let done = b.block("done");
    let found = b.block("found");
    let last = b.block("ret");
    let none = b.icmp(entry, ICond::Eq, Type::I32, x, 0i64, "none");
    b.terminate(entry, Terminator::CondBr { cond: none.into(), then: last, els: header });
    let i = b.value(header, Inst::Phi { ty: Type::I32, incoming: vec![(Operand::Int(0), entry)] }, "i");
    let more = b.icmp(header, ICond::Slt, Type::I32, i, x, "more");
    b.terminate(header, Terminator::CondBr { cond: more.into(), then: body, els: done });
    let hit = b.icmp(body, ICond::Eq, Type::I32, i, 7i64, "hit");
    b.terminate(body, Terminator::CondBr { cond: hit.into(), then: found, els: latch });
    let next = b.bin(latch, BinOp::Add, Type::I32, i, 1i64, "next");
    b.terminate(latch, Terminator::Br(header));
    b.terminate(done, Terminator::Br(last));
    b.terminate(found, Terminator::Br(last));
    let incoming = vec![(Operand::Int(100), entry), (Operand::Int(200), done), (Operand::Int(300), found)];
    let mut r = b.value(last, Inst::Phi { ty: Type::I32, incoming }, "r");
    for k in 1..=COPY_LIMIT {
        r = b.bin(last, BinOp::Add, Type::I32, r, i64::try_from(k).expect("a small count"), "r");
    }
    b.terminate(last, Terminator::Ret(Some((Type::I32, r.into()))));
    b.add_incoming(i, latch, next);
    b.finish()
}

/// The tree of a function with a shared tail replays its vectors: each way into the tail (the
/// early return, the count reached, a 7 found) gives what the graph gives.
#[test]
fn shared_tail_tree_replays_its_vectors() {
    let mut module = Module::default();
    module.functions.push(shared_tail_of_values());
    let f = &module.functions[0];
    let tree = structure(f, &Graph::of(f));
    assert!(tree.body.iter().any(|s| matches!(s, Stmt::Tail { .. })), "{:#?}", tree.body);
    assert_eq!(tree.gotos(), 0, "{:#?}", tree.body);
    let adds = u128::try_from((1..=COPY_LIMIT).sum::<usize>()).expect("a small sum");
    for (x, expected) in [(0, 100 + adds), (3, 200 + adds), (10, 300 + adds)] {
        let mut vm = Vm::new(&module, Memory::new());
        let graph = vm.call_named(&mut NoHost, &f.name, vec![Value::Int(x)]).expect("the graph runs");
        assert_eq!(graph, Value::Int(expected), "the graph of @{}({x})", f.name);
        let ran = run_tree(&mut vm, &mut NoHost, &f.name, &tree, vec![Value::Int(x)]);
        assert_eq!(ran, Ok(graph), "the tree of @{}({x}): {:#?}", f.name, tree.body);
    }
}

/// The printer's cases: every `.ll` file under `testdata/cases/emit`, in order.
fn printer_cases() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cases/emit");
    let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    let mut cases: Vec<PathBuf> = entries.map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|e| e == "ll")).collect();
    cases.sort();
    cases
}

/// The functions of the case at `path`, in their order, with what they reference, in an x64
/// Windows module as the lift makes one.
fn case_module(path: &Path) -> Module {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let parsed = ra_asm_ir::parse::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let stem = path.file_stem().and_then(|s| s.to_str()).expect("a case's name");
    let mut module = Module::x86_64_windows(stem);
    module.types = parsed.types;
    module.globals = parsed.globals;
    module.declarations = parsed.declarations;
    module.functions = parsed.functions;
    module
}

/// The C or C++ the printer writes for the functions of `module`, in their order.
fn printed_c(module: &Module, dialect: Dialect) -> String {
    let unit = emit_unit(module, &COptions { dialect, ..COptions::default() });
    unit.functions.iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join("\n")
}

/// The Rust the printer writes for the functions of `module`, in their order.
fn printed_rust(module: &Module) -> String {
    let out = emit_rust(module, &RustOptions::default());
    out.functions.iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join("\n")
}

/// Each `.ll` case under `testdata/cases/emit` printed by `print` against the snapshot of
/// `extension` beside it: a change of the printer is a line diff of the snapshots it moves.
/// `RA_BLESS=1` writes what the printer writes over them instead, so the review reads the change
/// in the diff of the commit. A case without a snapshot gets one written, and fails until the
/// review has read it in that diff.
fn printer_writes_each_case_as_its_snapshot(extension: &str, print: fn(&Module) -> String) {
    let bless = std::env::var("RA_BLESS").is_ok_and(|v| v == "1");
    let mut failures = Vec::new();
    let mut compared = 0;
    for case in printer_cases() {
        let actual = print(&case_module(&case));
        let path = case.with_extension(extension);
        let name = case.file_stem().and_then(|s| s.to_str()).expect("a case's name").to_owned();
        if bless {
            std::fs::write(&path, &actual).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            compared += 1;
            continue;
        }
        let Ok(expected) = std::fs::read_to_string(&path) else {
            let written = std::fs::write(&path, &actual).map_or_else(|e| format!("not written: {e}"), |()| "written now".to_owned());
            failures.push((format!("{name}: {} was missing, {written}; read it before committing it", path.display()), actual));
            continue;
        };
        let test = if extension == "c" { format!("emit-printer-{name}") } else { format!("emit-printer-{name}-{extension}") };
        if let Err(line) = ra_asm_testing::digest::compare(&test, &expected, &actual) {
            failures.push((line, actual));
        }
        compared += 1;
    }
    let lines: Vec<&str> = failures.iter().map(|(line, _)| line.as_str()).collect();
    let texts: Vec<&str> = failures.iter().map(|(_, text)| text.as_str()).collect();
    assert!(failures.is_empty(), "{} .{extension} snapshots differ:\n{}\n\n{}", failures.len(), lines.join("\n"), texts.join("\n----\n"));
    assert!(compared >= 12, "{compared} .{extension} snapshots compared, not 12");
}

#[test]
fn printer_c99_writes_each_case_as_its_snapshot() {
    printer_writes_each_case_as_its_snapshot("c", |module| printed_c(module, Dialect::C));
}

/// The C++ dialect writes a function as C does; its snapshots keep that, so a change that parts
/// the two shows in the diff of these alone.
#[test]
fn printer_cpp_writes_each_case_as_its_snapshot() {
    printer_writes_each_case_as_its_snapshot("cpp", |module| printed_c(module, Dialect::Cpp));
}

#[test]
fn printer_rust_writes_each_case_as_its_snapshot() {
    printer_writes_each_case_as_its_snapshot("rs", printed_rust);
}

/// The snapshots built and run: each language's snapshot of a case, in the unit or the package
/// the printer writes around it, with a `main` that makes the case's calls in order and prints a
/// line for each, which must be the line the call names.
///
/// A call is `; call: @f(<type> <value>, ...) = <result> [&<cell> ...]`: an integer argument by
/// its type (`i32 -1`), a `double 0.5`, a `ptr null`, or a `ptr &3`, the address of a 64-bit cell
/// holding 3. After `=` come the result (`void` for none) and each cell's value after the call. A
/// declaration the case calls is the harness's: its arguments read as decimal digits, the first
/// the highest (`report(4, 2)` is 42).
#[cfg(feature = "oracle")]
mod built {
    use std::fmt::Write as _;
    use std::path::Path;
    use std::process::Command;

    use ra_asm_emit::{COptions, Dialect, RustOptions, emit_rust, emit_unit};
    use ra_asm_ir::{Module, Type};

    use super::{case_module, printer_cases};

    /// An argument of a call.
    enum Arg {
        /// An integer of the width, as its bits.
        Int(u16, u128),
        Double(f64),
        Null,
        /// The address of a cell holding the value.
        Cell(u64),
    }

    /// One `; call:` line of a case.
    struct Call {
        function: String,
        args: Vec<Arg>,
        /// What the run prints for it, as the harness prints it.
        line: String,
    }

    /// `value` cut to `bits`, as its unsigned bits.
    fn masked(value: i128, bits: u16) -> u128 {
        let all = value.cast_unsigned();
        if bits >= 128 { all } else { all & ((1u128 << bits) - 1) }
    }

    fn integer(text: &str) -> i128 {
        text.parse().unwrap_or_else(|e| panic!("`{text}`: {e}"))
    }

    /// The `; call:` lines of the case `text`, against the functions of `module`.
    fn calls(text: &str, module: &Module) -> Vec<Call> {
        let mut calls = Vec::new();
        for header in text.lines().filter_map(|l| l.trim().strip_prefix(';')) {
            let Some(call) = header.trim().strip_prefix("call:") else { continue };
            let call = call.trim();
            let (head, expected) = call.split_once(" = ").unwrap_or_else(|| panic!("`{call}`: no ` = <result>`"));
            let (function, args) = head.strip_prefix('@').and_then(|h| h.split_once('(')).unwrap_or_else(|| panic!("`{call}`: no `@f(`"));
            let args = args.strip_suffix(')').unwrap_or_else(|| panic!("`{call}`: no `)` before ` = `"));
            let args: Vec<Arg> = args
                .split(',')
                .map(str::trim)
                .filter(|a| !a.is_empty())
                .map(|arg| {
                    let (ty, value) = arg.split_once(' ').unwrap_or_else(|| panic!("`{call}`: `{arg}` is no `<type> <value>`"));
                    match (ty, value.trim()) {
                        ("ptr", "null") => Arg::Null,
                        ("ptr", cell) => {
                            let cell = cell.strip_prefix('&').unwrap_or_else(|| panic!("`{call}`: a `ptr` is `null` or `&<value>`"));
                            Arg::Cell(u64::try_from(masked(integer(cell), 64)).expect("a 64-bit cell"))
                        }
                        ("double", value) => Arg::Double(value.parse().unwrap_or_else(|e| panic!("`{call}`: `{value}`: {e}"))),
                        (ty, value) => {
                            let bits: u16 =
                                ty.strip_prefix('i').and_then(|b| b.parse().ok()).unwrap_or_else(|| panic!("`{call}`: type `{ty}`"));
                            Arg::Int(bits, masked(integer(value), bits))
                        }
                    }
                })
                .collect();
            let f = module.functions.iter().find(|f| f.name == function).unwrap_or_else(|| panic!("`{call}`: no function @{function}"));
            let mut tokens = expected.split_whitespace();
            let result = tokens.next().unwrap_or_else(|| panic!("`{call}`: no result after ` = `"));
            let mut line = match &f.ty.ret {
                Type::Void => {
                    assert_eq!(result, "void", "`{call}`: @{function} returns nothing");
                    "void".to_owned()
                }
                Type::Int(bits) => masked(integer(result), *bits).to_string(),
                Type::Double => format!("0x{:x}", result.parse::<f64>().unwrap_or_else(|e| panic!("`{call}`: `{result}`: {e}")).to_bits()),
                other => panic!("`{call}`: the harness prints no {other}"),
            };
            let cells = args.iter().filter(|a| matches!(a, Arg::Cell(_))).count();
            let after: Vec<&str> = tokens.collect();
            assert_eq!(after.len(), cells, "`{call}`: {cells} cells, {} values after the result", after.len());
            for cell in after {
                let value = cell.strip_prefix('&').unwrap_or_else(|| panic!("`{call}`: a cell after the result is `&<value>`"));
                let _ = write!(line, " &{}", masked(integer(value), 64));
            }
            calls.push(Call { function: function.to_owned(), args, line });
        }
        calls
    }

    /// The C type of an integer the harness passes or returns.
    fn c_int(ty: &Type) -> String {
        match ty {
            Type::Int(bits @ (8 | 16 | 32 | 64)) => format!("uint{bits}_t"),
            other => panic!("the harness passes no {other}"),
        }
    }

    /// The Rust type of an integer the harness passes or returns.
    fn rust_int(ty: &Type) -> String {
        match ty {
            Type::Int(bits @ (8 | 16 | 32 | 64)) => format!("u{bits}"),
            other => panic!("the harness passes no {other}"),
        }
    }

    /// The C or C++ unit of `snapshot`: the printer's declarations of the case, the snapshot, the
    /// case's declarations defined and the `main` of its calls.
    fn c_source(module: &Module, snapshot: &str, calls: &[Call], dialect: Dialect) -> String {
        let unit = emit_unit(module, &COptions { dialect, ..COptions::default() });
        let mut out = String::from("#include <stdio.h>\n#include <string.h>\n");
        for part in [&unit.include, &unit.types, &unit.externs, &unit.prototypes, &unit.global_types, &unit.global_decls, &unit.global_defs]
        {
            out.push_str(part);
            out.push('\n');
        }
        out.push_str(snapshot);
        out.push('\n');
        let linkage = if dialect == Dialect::Cpp { "extern \"C\" " } else { "" };
        for d in &module.declarations {
            let params: Vec<String> = d.ty.params.iter().enumerate().map(|(i, t)| format!("{} a{i}", c_int(t))).collect();
            let params = if params.is_empty() { "void".to_owned() } else { params.join(", ") };
            let ret = c_int(&d.ty.ret);
            let body = (0..d.ty.params.len()).fold(format!("({ret})0"), |acc, i| format!("({ret})({acc} * 10u + a{i})"));
            let _ = writeln!(out, "{linkage}{ret} {}({params}) {{ return {body}; }}", d.name);
        }
        out.push_str("\nint main(void) {\n");
        for call in calls {
            out.push_str("    {\n");
            let mut args = Vec::new();
            let mut cells = Vec::new();
            for arg in &call.args {
                match arg {
                    Arg::Int(bits, value) => args.push(format!("({})UINT64_C({value})", c_int(&Type::Int(*bits)))),
                    Arg::Double(value) => args.push(format!("{value:?}")),
                    Arg::Null => args.push("(uint8_t *)0".to_owned()),
                    Arg::Cell(value) => {
                        let _ = writeln!(out, "        uint64_t cell{} = UINT64_C({value});", cells.len());
                        args.push(format!("(uint8_t *)&cell{}", cells.len()));
                        cells.push(cells.len());
                    }
                }
            }
            let call_text = format!("{}({})", call.function, args.join(", "));
            let ret = &module.functions.iter().find(|f| f.name == call.function).expect("a called function").ty.ret;
            match ret {
                Type::Void => {
                    let _ = writeln!(out, "        {call_text};\n        printf(\"void\");");
                }
                Type::Double => {
                    let _ = writeln!(
                        out,
                        "        double r = {call_text};\n        unsigned long long bits;\n        memcpy(&bits, &r, sizeof bits);\n        printf(\"0x%llx\", bits);"
                    );
                }
                _ => {
                    let _ = writeln!(out, "        printf(\"%llu\", (unsigned long long){call_text});");
                }
            }
            for cell in cells {
                let _ = writeln!(out, "        printf(\" &%llu\", (unsigned long long)cell{cell});");
            }
            out.push_str("        printf(\"\\n\");\n    }\n");
        }
        out.push_str("    return 0;\n}\n");
        out
    }

    /// The `main.rs` of the package the printer writes for `module`, up to its `main`, then the
    /// case's declarations defined and the `main` of its calls.
    fn rust_main(head: &str, module: &Module, calls: &[Call]) -> String {
        let mut out = head.to_owned();
        for d in &module.declarations {
            let params: Vec<String> = d.ty.params.iter().enumerate().map(|(i, t)| format!("a{i}: {}", rust_int(t))).collect();
            let ret = rust_int(&d.ty.ret);
            let body =
                (0..d.ty.params.len()).fold(format!("0{ret}"), |acc, i| format!("({acc}).wrapping_mul(10).wrapping_add(a{i} as {ret})"));
            let _ =
                writeln!(out, "#[unsafe(no_mangle)]\npub extern \"system\" fn {}({}) -> {ret} {{ {body} }}\n", d.name, params.join(", "));
        }
        out.push_str("fn main() {\n    unsafe {\n        ra_init();\n");
        for call in calls {
            out.push_str("        {\n");
            let mut args = Vec::new();
            let mut cells = Vec::new();
            for arg in &call.args {
                match arg {
                    Arg::Int(bits, value) => args.push(format!("{value}u{bits}")),
                    Arg::Double(value) => args.push(format!("{value:?}f64")),
                    Arg::Null => args.push("core::ptr::null_mut()".to_owned()),
                    Arg::Cell(value) => {
                        let _ = writeln!(out, "            let mut cell{}: u64 = {value};", cells.len());
                        args.push(format!("(&raw mut cell{}) as *mut u8", cells.len()));
                        cells.push(cells.len());
                    }
                }
            }
            let call_text = format!("{}({})", call.function, args.join(", "));
            let ret = &module.functions.iter().find(|f| f.name == call.function).expect("a called function").ty.ret;
            match ret {
                Type::Void => {
                    let _ = writeln!(out, "            {call_text};\n            print!(\"void\");");
                }
                Type::Double => {
                    let _ = writeln!(out, "            print!(\"0x{{:x}}\", {call_text}.to_bits());");
                }
                _ => {
                    let _ = writeln!(out, "            print!(\"{{}}\", {call_text});");
                }
            }
            for cell in cells {
                let _ = writeln!(out, "            print!(\" &{{}}\", cell{cell});");
            }
            out.push_str("            println!();\n        }\n");
        }
        out.push_str("    }\n}\n");
        out
    }

    /// Runs `exe` and compares what it prints with the calls' lines; the failure, if any.
    fn ran(test: &str, exe: &Path, calls: &[Call]) -> Option<String> {
        let run = match Command::new(exe).output() {
            Ok(run) => run,
            Err(e) => return Some(format!("{test}: {}: {e}", exe.display())),
        };
        if !run.status.success() {
            return Some(format!("{test}: {} exited with {}: {}", exe.display(), run.status, String::from_utf8_lossy(&run.stderr)));
        }
        let expected: String = calls.iter().map(|c| format!("{}\n", c.line)).collect();
        let actual = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
        ra_asm_testing::digest::compare(test, &expected, &actual).err()
    }

    /// The failure of a build, if any.
    fn built(test: &str, what: &str, build: std::io::Result<std::process::Output>) -> Option<String> {
        match build {
            Ok(out) if out.status.success() => None,
            Ok(out) => Some(format!("{test}: {what}: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))),
            Err(e) => Some(format!("{test}: {what}: {e}")),
        }
    }

    #[test]
    fn printer_snapshots_build_and_run_their_calls() {
        if ra_asm_testing::tool("clang").is_none() || ra_asm_testing::tool("rustc").is_none() {
            return;
        }
        let runtime = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime");
        let mut failures = Vec::new();
        let mut ran_calls = 0;
        for case in printer_cases() {
            let name = case.file_stem().and_then(|s| s.to_str()).expect("a case's name").to_owned();
            let text = std::fs::read_to_string(&case).unwrap_or_else(|e| panic!("{}: {e}", case.display()));
            let module = case_module(&case);
            let calls = calls(&text, &module);
            assert!(!calls.is_empty(), "{}: the case has no `; call:` line", case.display());
            let scratch = ra_asm_testing::Scratch::new("ra-asm-emit-printer", &name);
            for (extension, dialect) in [("c", Dialect::C), ("cpp", Dialect::Cpp)] {
                let test = format!("emit-printer-run-{name}-{extension}");
                let snapshot =
                    std::fs::read_to_string(case.with_extension(extension)).unwrap_or_else(|e| panic!("{name}.{extension}: {e}"));
                let source = scratch.join(format!("{name}.{extension}"));
                std::fs::write(&source, c_source(&module, &snapshot, &calls, dialect)).unwrap();
                let exe = scratch.join(format!("{name}-{extension}.exe"));
                let mut build = Command::new("clang");
                build.args(["-O1", "-w"]);
                if dialect == Dialect::C {
                    build.arg("-std=c23");
                }
                if cfg!(windows) {
                    build.arg("-fuse-ld=lld");
                }
                build.arg(&source).arg("-I").arg(&runtime).arg("-o").arg(&exe);
                let failure = built(&test, "clang", build.output()).or_else(|| ran(&test, &exe, &calls));
                failures.extend(failure);
                ran_calls += calls.len();
            }
            let test = format!("emit-printer-run-{name}-rs");
            let snapshot = std::fs::read_to_string(case.with_extension("rs")).unwrap_or_else(|e| panic!("{name}.rs: {e}"));
            let package = scratch.join("rust");
            let output = emit_rust(&module, &RustOptions::default());
            ra_asm_emit::write_files(&package, &output.files).unwrap();
            let main = output.files.iter().find(|f| f.name == "src/main.rs").expect("the package's main.rs");
            let head = &main.text[..main.text.find("\nfn main() {").expect("the package's main") + 1];
            std::fs::write(package.join("src/main.rs"), rust_main(head, &module, &calls)).unwrap();
            std::fs::write(package.join("src/program.rs"), format!("use super::*;\n\n{snapshot}\n")).unwrap();
            let exe = scratch.join(format!("{name}-rs.exe"));
            let build = Command::new("rustc")
                .args(["--edition", "2024", "-O", "--crate-name", "harness", "-A", "warnings"])
                .arg(package.join("src/main.rs"))
                .arg("-o")
                .arg(&exe)
                .output();
            let failure = built(&test, "rustc", build).or_else(|| ran(&test, &exe, &calls));
            failures.extend(failure);
            ran_calls += calls.len();
        }
        assert!(failures.is_empty(), "{} snapshots built or ran wrong:\n{}", failures.len(), failures.join("\n"));
        assert!(ran_calls >= 36, "{ran_calls} calls ran over the snapshots");
    }
}
