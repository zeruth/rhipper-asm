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
//! The printer's cases are `.ll` files in the same directory, each with the C written for it in
//! the `.c` beside it; `RA_BLESS=1` rewrites those snapshots from what the printer writes.

use std::path::PathBuf;

use ra_asm_emit::structure::{COPY_LIMIT, Graph, Stmt, structure};
use ra_asm_emit::tree::run_tree;
use ra_asm_emit::{ADAPTERS_FILE, COptions, Check, RustOptions, Unit, Watch, emit_rust, emit_unit};
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
    let mut f = b.finish();
    // The back edge's value is defined after the phi that reads it.
    for ins in &mut f.blocks[header.0 as usize].instrs {
        if ins.result == Some(i)
            && let Inst::Phi { incoming, .. } = &mut ins.inst
        {
            incoming.push((next.into(), latch));
        }
    }
    f
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

/// The C the printer writes for the functions of the case at `path`, in their order, in an x64
/// Windows module as the lift makes one.
fn printed(path: &std::path::Path) -> String {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let parsed = ra_asm_ir::parse::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let stem = path.file_stem().and_then(|s| s.to_str()).expect("a case's name");
    let mut module = Module::x86_64_windows(stem);
    module.types = parsed.types;
    module.globals = parsed.globals;
    module.declarations = parsed.declarations;
    module.functions = parsed.functions;
    let unit = emit_unit(&module, &COptions::default());
    unit.functions.iter().map(|f| f.text.as_str()).collect::<Vec<_>>().join("\n")
}

/// Each `.ll` case under `testdata/cases/emit` printed as C against the `.c` beside it: a change
/// of the printer is a line diff of the snapshots it moves. `RA_BLESS=1` writes what the printer
/// writes over them instead, so the review reads the change in the diff of the commit.
#[test]
fn printer_c99_writes_each_case_as_its_snapshot() {
    let bless = std::env::var("RA_BLESS").is_ok_and(|v| v == "1");
    let mut failures = Vec::new();
    let mut compared = 0;
    for case in printer_cases() {
        let actual = printed(&case);
        let path = case.with_extension("c");
        let name = case.file_stem().and_then(|s| s.to_str()).expect("a case's name").to_owned();
        if bless {
            std::fs::write(&path, &actual).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            compared += 1;
            continue;
        }
        let expected = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                failures.push((format!("{name}: {}: {e}; RA_BLESS=1 writes it", path.display()), actual));
                continue;
            }
        };
        if let Err(line) = ra_asm_testing::digest::compare(&format!("emit-printer-{name}"), &expected, &actual) {
            failures.push((line, actual));
        }
        compared += 1;
    }
    let lines: Vec<&str> = failures.iter().map(|(line, _)| line.as_str()).collect();
    let texts: Vec<&str> = failures.iter().map(|(_, text)| text.as_str()).collect();
    assert!(failures.is_empty(), "{} snapshots differ:\n{}\n\n{}", failures.len(), lines.join("\n"), texts.join("\n----\n"));
    assert!(compared >= 12, "{compared} snapshots compared, not 12");
}
