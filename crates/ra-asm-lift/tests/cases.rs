//! The pass cases: every `.ll` file under `testdata/cases` is an equivalence test of one step of
//! the raise. The file is a module in the printer's text under a header of comments:
//!
//! ```text
//! ; pass: simplify
//! ; call: @f(i32 7, i32 9)
//! ; call: @f(i32 -1, i32 0)
//! define i32 @f(i32 %a, i32 %b) {
//! …
//! }
//! ; after:
//! define i32 @f(i32 %a, i32 %b) {
//! …
//! }
//! ```
//!
//! `pass` names the step ([`ra_asm_lift::raise::steps::step`]); each `call` is an input, a
//! function of the module with its arguments (an integer in decimal or `0x` hexadecimal, a float
//! in decimal, `null`, or `@global` for the address of one); the text under `after`, when the case
//! has one, is what the functions print as once the pass ran. The runner interprets each call on
//! the module as written and on the module the pass left, and compares what the two runs did: how
//! the call ended and with which value, the calls the host saw (a declaration of the module is the
//! host's, which records the call and answers with a number made of its arguments), and the bytes
//! of every global when it ended. So a case asserts nothing itself: a pass that changes what a
//! function does fails it, with the case's name and the call.
//!
//! Each run starts from the same zeroed memory with the module placed at the same addresses, so a
//! pointer compares as a value. A step that reads the signatures (`promote` at the ABI level) gets
//! none, and the two steps that rewrite nothing (`abi`, `stack`) only run.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{BinOp, FnType, Global, Inst, Module};
use ra_asm_lift::raise::steps::{Context, Pass, Step, step};

/// The memory of a run: the statics in its first half, the frames in its second.
const ARENA_LEN: usize = 2 << 20;

/// The instructions a call may take; a case is a few lines, and one that loops is a broken case.
const LIMIT: u64 = 1_000_000;

fn cases_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cases")
}

/// An argument of a call: a value, or the address of a global, which only a placed module knows.
#[derive(Debug, Clone, PartialEq)]
enum Arg {
    Value(Value),
    Global(String),
}

impl Arg {
    /// `T literal`, as a call of the IR spells an argument.
    fn parse(text: &str) -> Result<Self, String> {
        let (ty, literal) = text.split_once(char::is_whitespace).ok_or_else(|| format!("`{text}` is not a type and a value"))?;
        let literal = literal.trim();
        let integer = |bits: u32| -> Option<Value> {
            let v = match literal {
                "true" => 1,
                "false" | "null" => 0,
                _ => match literal.strip_prefix("0x") {
                    Some(hex) => u128::from_str_radix(hex, 16).ok()?,
                    None => literal.parse::<i128>().ok()? as u128,
                },
            };
            Some(Value::int(v, bits))
        };
        let value = match ty {
            "ptr" => match literal.strip_prefix('@') {
                Some(global) => return Ok(Self::Global(global.to_owned())),
                None => integer(64),
            },
            "double" => literal.parse::<f64>().ok().map(Value::F64),
            "float" => literal.parse::<f32>().ok().map(Value::F32),
            _ => ty.strip_prefix('i').and_then(|bits| bits.parse::<u32>().ok()).filter(|bits| (1..=128).contains(bits)).and_then(integer),
        };
        value.map(Self::Value).ok_or_else(|| format!("`{text}` is not an argument a case can pass"))
    }
}

/// An input of a case: a function of its module and the arguments.
#[derive(Debug, Clone, PartialEq)]
struct Call {
    /// The call as the case spells it, which a failure names.
    text: String,
    function: String,
    args: Vec<Arg>,
}

impl Call {
    /// `@name(T a, U b)`.
    fn parse(text: &str) -> Result<Self, String> {
        let shape = text.strip_prefix('@').and_then(|rest| rest.strip_suffix(')')).and_then(|rest| rest.split_once('('));
        let (function, args) = shape.ok_or("a call is `@name(type value, …)`")?;
        let args: Vec<Arg> = if args.trim().is_empty() {
            Vec::new()
        } else {
            args.split(',').map(|a| Arg::parse(a.trim())).collect::<Result<Vec<Arg>, String>>()?
        };
        Ok(Self { text: text.to_owned(), function: function.trim().to_owned(), args })
    }
}

/// A case file, read.
#[derive(Debug, Clone, PartialEq)]
struct Case {
    /// The path under `testdata/cases` without its extension: `simplify/identities`.
    name: String,
    pass: String,
    calls: Vec<Call>,
    /// The file down to its `; after:` line, header and all, so a line of a parse error is a
    /// line of the file.
    source: String,
    /// The text under `; after:`.
    after: Option<String>,
}

impl Case {
    fn parse(name: &str, text: &str) -> Result<Self, String> {
        let (mut pass, mut calls, mut source, mut after) = (None, Vec::new(), String::new(), None::<String>);
        for line in text.lines() {
            if let Some(expected) = &mut after {
                expected.push_str(line);
                expected.push('\n');
                continue;
            }
            source.push_str(line);
            source.push('\n');
            let Some(header) = line.trim().strip_prefix(';').map(str::trim) else { continue };
            if header == "after:" {
                after = Some(String::new());
            } else if let Some(named) = header.strip_prefix("pass:") {
                if pass.replace(named.trim().to_owned()).is_some() {
                    return Err(format!("{name}: the case names two passes"));
                }
            } else if let Some(call) = header.strip_prefix("call:") {
                let call = call.trim();
                calls.push(Call::parse(call).map_err(|e| format!("{name}: `{call}`: {e}"))?);
            }
        }
        let pass = pass.ok_or_else(|| format!("{name}: the case names no pass: `; pass: <step>`"))?;
        if calls.is_empty() {
            return Err(format!("{name}: the case has no call: `; call: @f(i32 7)`"));
        }
        Ok(Self { name: name.to_owned(), pass, calls, source, after })
    }

    fn load(path: &Path) -> Result<Self, String> {
        let name = path.strip_prefix(cases_dir()).unwrap_or(path).with_extension("").to_string_lossy().replace('\\', "/");
        let text = std::fs::read_to_string(path).map_err(|e| format!("{name}: {e}"))?;
        Self::parse(&name, &text)
    }
}

/// The host of a case: every call of a declaration is recorded and answered with the sum of its
/// arguments and its place among the calls, so the answer is the same for the same calls in the
/// same order and no other.
#[derive(Default)]
struct Recorder {
    calls: Vec<String>,
}

impl Host for Recorder {
    fn call(&mut self, _vm: &mut Vm<'_>, name: &str, _ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        let spelled: Vec<String> = args.iter().map(render).collect();
        self.calls.push(format!("{name}({})", spelled.join(", ")));
        let sum = args.iter().fold(self.calls.len() as u128, |sum, a| sum.wrapping_add(a.as_u128()));
        Ok(Value::Int(sum))
    }
}

/// A value by its bits, so two runs compare a NaN as equal to itself.
fn render(value: &Value) -> String {
    match value {
        Value::Int(v) => format!("{v:#x}"),
        Value::F32(f) => format!("float {:#x}", f.to_bits()),
        Value::F64(f) => format!("double {:#x}", f.to_bits()),
        Value::Vector(lanes) | Value::Aggregate(lanes) => format!("<{}>", lanes.iter().map(render).collect::<Vec<_>>().join(", ")),
    }
}

/// What a call did.
#[derive(Debug, PartialEq)]
struct Run {
    /// How it ended: the value returned, the exit code, or the fault.
    outcome: String,
    /// The calls the host saw, in order.
    host: Vec<String>,
    /// The bytes of each global when it ended.
    globals: Vec<(String, Vec<u8>)>,
}

/// Interprets `call` on `module`, placed in `arena` from zeroed memory.
fn interpret(module: &Module, call: &Call, arena: &mut [u8]) -> Result<Run, String> {
    if module.function(&call.function).is_none() {
        return Err(format!("the module has no function @{}", call.function));
    }
    arena.fill(0);
    let (base, half) = (arena.as_mut_ptr() as u64, (arena.len() / 2) as u64);
    let mut memory = Memory::new();
    memory.add_region(Region::new(base, arena.len() as u64, "case"));
    let mut vm = Vm::new(module, memory);
    vm.limit = LIMIT;
    vm.set_static_region(base, half);
    vm.set_frame_region(base + half, half);
    vm.place().map_err(|stop| format!("the module is not placed: {stop:?}"))?;
    let mut args = Vec::with_capacity(call.args.len());
    for arg in &call.args {
        args.push(match arg {
            Arg::Value(value) => value.clone(),
            Arg::Global(name) => {
                Value::Int(u128::from(vm.symbol(name).ok_or_else(|| format!("the module has no @{name} to pass the address of"))?))
            }
        });
    }
    let mut host = Recorder::default();
    let outcome = match vm.call_named(&mut host, &call.function, args) {
        Ok(value) => format!("returns {}", render(&value)),
        Err(Stop::Exit(code)) => format!("exits with {code}"),
        Err(Stop::Fault(message)) => format!("stops ({message})"),
        Err(Stop::Limit(_)) => return Err(format!("the call does not end within {LIMIT} instructions")),
    };
    let mut globals = Vec::with_capacity(module.globals.len());
    for g in &module.globals {
        let address = vm.symbol(&g.name).ok_or_else(|| format!("@{} is not placed", g.name))?;
        let size = vm.layout().size(&g.ty);
        let bytes = vm.memory.read_vec(address, size).map_err(|stop| format!("@{} is not read: {stop:?}", g.name))?;
        globals.push((g.name.clone(), bytes));
    }
    Ok(Run { outcome, host: host.calls, globals })
}

/// Where the run after the pass parts from the run before it, if it does. A global only one of
/// the modules has is the pass's to add or drop; the ones both have hold the same bytes.
fn difference(before: &Run, after: &Run) -> Option<String> {
    if before.outcome != after.outcome {
        return Some(format!("{} before, {} after", before.outcome, after.outcome));
    }
    if before.host != after.host {
        return Some(format!("the host saw [{}] before, [{}] after", before.host.join("; "), after.host.join("; ")));
    }
    for (name, was) in &before.globals {
        let Some((_, is)) = after.globals.iter().find(|(other, _)| other == name) else { continue };
        if was.len() != is.len() {
            return Some(format!("@{name} is {} bytes before, {} after", was.len(), is.len()));
        }
        if let Some(at) = was.iter().zip(is).position(|(a, b)| a != b) {
            return Some(format!("@{name} differs at byte {at}: {:#04x} before, {:#04x} after", was[at], is[at]));
        }
    }
    None
}

/// The lines of an IR text that say something, without the blanks at their ends.
fn lines(text: &str) -> Vec<&str> {
    text.lines().map(str::trim_end).filter(|line| !line.is_empty()).collect()
}

/// Runs `case` with `pass` as its pass: each call agrees before and after it, and the functions
/// print as the case's `after` when it has one. The failure names the case, the call and the pass.
fn check(case: &Case, pass: &dyn Fn(&mut Module)) -> Result<(), String> {
    let before = ra_asm_ir::parse::parse(&case.source).map_err(|e| format!("{}: {e}", case.name))?;
    let mut after = before.clone();
    pass(&mut after);
    let mut arena = vec![0u8; ARENA_LEN];
    for call in &case.calls {
        let fail = |what: String| format!("{}: `{}` through `{}`: {what}", case.name, call.text, case.pass);
        let was = interpret(&before, call, &mut arena).map_err(|e| fail(format!("before the pass, {e}")))?;
        let is = interpret(&after, call, &mut arena).map_err(|e| fail(format!("after the pass, {e}")))?;
        if let Some(what) = difference(&was, &is) {
            return Err(fail(what));
        }
    }
    if let Some(expected) = &case.after {
        let actual: String = after.functions.iter().map(ra_asm_ir::printer::function).collect::<Vec<_>>().join("\n");
        let (expected, actual) = (lines(expected).join("\n"), lines(&actual).join("\n"));
        if expected != actual {
            let diff = ra_asm_ir::diff::unified(&expected, &actual, 3);
            return Err(format!("{}: the text after `{}` is not the case's (-) but (+):\n{diff}", case.name, case.pass));
        }
    }
    Ok(())
}

/// Runs `step` over `module` as the raise does a step of its kind, with `roots` as the functions
/// no caller of the module reaches and no signature inferred.
fn apply(step: Step, module: &mut Module, roots: &BTreeSet<String>) {
    let options = ra_asm_lift::RaiseOptions::default();
    let signatures = HashMap::new();
    let cx = Context::new(roots, &options, &signatures);
    match step.pass {
        Pass::Function(run) => {
            let globals: &[Global] = &module.globals;
            for f in &mut module.functions {
                run(f, &cx, globals);
            }
        }
        Pass::Module(run) => run(module, &cx),
        Pass::Signatures(run) => {
            run(module, &cx);
        }
        Pass::Stack(run) => {
            for f in &module.functions {
                run(f, &cx);
            }
        }
    }
}

/// Runs `case` with the step it names; the functions it calls are the roots.
fn run(case: &Case) -> Result<(), String> {
    let step = step(&case.pass).ok_or_else(|| format!("{}: there is no pass `{}`", case.name, case.pass))?;
    let roots: BTreeSet<String> = case.calls.iter().map(|call| call.function.clone()).collect();
    check(case, &|module| apply(step, module, &roots))
}

/// The `.ll` files under `dir`, at any depth, in the order of their paths.
fn case_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display())) {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|e| e == "ll") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

/// A seed case, by its name under `testdata/cases`.
fn seed(name: &str) -> Case {
    Case::load(&cases_dir().join(format!("{name}.ll"))).unwrap()
}

/// A pass that is wrong on purpose: every `add` becomes a `sub`.
fn subtracts(module: &mut Module) {
    for f in &mut module.functions {
        for b in &mut f.blocks {
            for i in &mut b.instrs {
                if let Inst::Bin { op, .. } = &mut i.inst
                    && *op == BinOp::Add
                {
                    *op = BinOp::Sub;
                }
            }
        }
    }
}

/// A pass that is wrong on purpose: no store is kept.
fn forgets(module: &mut Module) {
    for f in &mut module.functions {
        for b in &mut f.blocks {
            b.instrs.retain(|i| !matches!(i.inst, Inst::Store { .. }));
        }
    }
}

/// A case of two lines for the runner's own tests; `none` is no step, so `check` gets its pass.
const TWICE: &str = "\
; pass: none
; call: @twice(i32 4)
define i32 @twice(i32 %x) {
entry:
  %r = add i32 %x, %x
  ret i32 %r
}
";

/// Every case file under `testdata/cases` passes: adding a file adds a test.
#[test]
fn every_case_passes() {
    let files = case_files(&cases_dir());
    let mut names = Vec::new();
    let mut failures = Vec::new();
    for file in &files {
        match Case::load(file) {
            Ok(case) => {
                if let Err(failure) = run(&case) {
                    failures.push(failure);
                }
                names.push(case.name);
            }
            Err(failure) => failures.push(failure),
        }
    }
    assert!(failures.is_empty(), "{} of {} cases fail:\n{}", failures.len(), files.len(), failures.join("\n"));
    for seed in ["simplify/folds", "simplify/identities", "simplify/memory"] {
        assert!(names.iter().any(|name| name == seed), "the seed case {seed} did not run: {names:?}");
    }
}

/// A pass that changes what a function returns is caught, and the failure names the case, the
/// call that shows it and what each side returned.
#[test]
fn a_wrong_pass_is_caught_with_the_case_and_the_call() {
    let case = seed("simplify/identities");
    let failure = check(&case, &subtracts).expect_err("7 + 9 is not 7 - 9");
    assert!(failure.starts_with("simplify/identities: `@f(i32 7, i32 9)` through `simplify`: "), "{failure}");
    assert!(failure.ends_with("returns 0x10 before, returns 0xfffffffe after"), "{failure}");
}

/// A pass that returns the same value and leaves the memory different is caught by the global.
#[test]
fn a_pass_that_loses_a_store_is_caught_by_the_memory() {
    let case = seed("simplify/memory");
    let failure = check(&case, &forgets).expect_err("the stores are the function's effect");
    assert!(failure.starts_with("simplify/memory: `@fill(ptr @buf, i32 3)` through `simplify`: @buf differs at byte 0: "), "{failure}");
}

/// The text under `after` is compared with what the functions print as, whatever the calls say.
#[test]
fn the_text_after_the_pass_is_compared() {
    let module = ra_asm_ir::parse::parse(TWICE).unwrap();
    let printed = ra_asm_ir::printer::function(&module.functions[0]);
    let same = Case::parse("inline/twice", &format!("{TWICE}; after:\n\n{printed}")).unwrap();
    assert_eq!(same.calls.len(), 1);
    assert_eq!(check(&same, &|_| ()), Ok(()));

    let other = Case::parse("inline/twice", &format!("{TWICE}; after:\n{}", printed.replace("%r", "%q"))).unwrap();
    let failure = check(&other, &|_| ()).expect_err("the function does not print as the case says");
    assert!(failure.starts_with("inline/twice: the text after `none` is not the case's"), "{failure}");
    assert!(failure.contains("%q") && failure.contains("%r"), "{failure}");
}

/// A case the runner cannot run says why, by name: no pass, no call, a call it cannot spell, a
/// pass or a function there is none of.
#[test]
fn a_broken_case_is_refused_by_name() {
    let refusal = |text: &str| Case::parse("inline/broken", text).expect_err("the case is broken");
    assert!(refusal("; call: @f(i32 1)\n").contains("inline/broken: the case names no pass"));
    assert!(refusal("; pass: simplify\n").contains("inline/broken: the case has no call"));
    assert!(refusal("; pass: simplify\n; call: @f(i32 x)\n").contains("`i32 x` is not an argument"));
    assert!(refusal("; pass: simplify\n; call: f(i32 1)\n").contains("a call is `@name("));

    let unknown = Case::parse("inline/twice", TWICE).unwrap();
    assert_eq!(run(&unknown), Err("inline/twice: there is no pass `none`".to_owned()));
    let missing = Case::parse("inline/twice", &TWICE.replace("call: @twice", "call: @thrice")).unwrap();
    let failure = check(&missing, &|_| ()).expect_err("the module has no such function");
    assert!(failure.contains("`@thrice(i32 4)`") && failure.contains("no function @thrice"), "{failure}");
}

/// The arguments a call spells: integers at their width, floats, and the address of a global.
#[test]
fn a_call_reads_its_arguments() {
    let call = Call::parse("@f(i8 -1, i64 0x10, i1 true, double 1.5, ptr @buf, ptr null)").unwrap();
    assert_eq!(call.function, "f");
    assert_eq!(
        call.args,
        [
            Arg::Value(Value::Int(0xff)),
            Arg::Value(Value::Int(0x10)),
            Arg::Value(Value::Int(1)),
            Arg::Value(Value::F64(1.5)),
            Arg::Global("buf".to_owned()),
            Arg::Value(Value::Int(0)),
        ]
    );
    assert!(Call::parse("@g()").unwrap().args.is_empty());
}
