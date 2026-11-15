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
//! pointer compares as a value.
//!
//! The four steps that carry the levels (`promote`, `abi`, `stack`, `frame`) take functions over
//! the x64 `%State`, and a call of theirs is a state: `; call: @F_401000(rcx 7, xmm1 0x4000000000000000, [40] 9)`
//! sets registers and the qwords above the return address, the run sets the stack pointer and
//! pushes the return address. The module goes through the raise of the ABI level as far as the
//! step (`abi` as far as the promotion that applies its signatures), and each call agrees with
//! the raised function called with what its signature reads of the state, as the replayer reads
//! a vector: the result against the register it returns in, the host's calls, the globals, and of
//! the run over the state, the stack pointer it leaves and the registers the signature keeps. A
//! `; signature: @F_401000 i64 (i64, i64)` line says what a function takes once raised. A
//! `stack` case rewrites nothing: the analysis of each function a call reaches holds against the
//! run, the bytes it touched, the stack arguments it read and what it popped.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{FnType, Global, Module, Type};
use ra_asm_lift::raise::Target;
use ra_asm_lift::raise::abi::{Kind, Signature};
use ra_asm_lift::raise::liveness::Slot;
use ra_asm_lift::raise::stack::StackInfo;
use ra_asm_lift::raise::steps::{Context, Pass, Step, step};

/// The machine of the cases over the state.
const TARGET: Target = Target::WIN64;

/// The memory of a run: the statics in its first half, the frames in its second.
const ARENA_LEN: usize = 2 << 20;

/// The instructions a call may take; a case is a few lines, and one that loops is a broken case.
const LIMIT: u64 = 1_000_000;

fn cases_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/cases")
}

/// The general registers of the x64 state, in its order.
const REGISTERS: [&str; 16] =
    ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"];

/// The number of the stack pointer among [`REGISTERS`].
const RSP: usize = 4;

/// The bytes of the x64 `%State`: the general registers, `rip`, the flags, the vector registers.
const STATE_LEN: u64 = 16 * 8 + 8 + 8 + 32 * 16;

/// The offset of `xmm0` in the state.
const XMM0: u64 = 16 * 8 + 8 + 8;

/// The address a function called over the state returns to, which the run pushes for it.
const RETURN_ADDRESS: u64 = 0x7fff_0000_1000;

/// An argument of a call: a value, or the address of a global, which only a placed module knows;
/// or, for a function over the state, what the state and the stack hold at the entry.
#[derive(Debug, Clone, PartialEq)]
enum Arg {
    Value(Value),
    Global(String),
    /// A general register by its number in the state: `rcx 7`.
    Register(usize, u64),
    /// A vector register by its number, its low bits: `xmm1 0x3ff0000000000000`.
    Vector(usize, u128),
    /// A qword above the entry stack pointer, by its offset: `[40] 9`, the first stack argument.
    Stack(i64, u64),
}

impl Arg {
    /// `T literal`, as a call of the IR spells an argument, or `register literal` and
    /// `[offset] literal` for a function over the state.
    fn parse(text: &str) -> Result<Self, String> {
        let (ty, literal) = text.split_once(char::is_whitespace).ok_or_else(|| format!("`{text}` is not a type and a value"))?;
        if let Some(state) = Self::state(ty, literal.trim()) {
            return state.map_err(|e| format!("`{text}`: {e}"));
        }
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

    /// A register or a stack qword at the entry, when `place` names one. The stack pointer and
    /// the return address are the run's to set.
    fn state(place: &str, literal: &str) -> Option<Result<Self, String>> {
        let number = match literal.strip_prefix("0x") {
            Some(hex) => u128::from_str_radix(hex, 16).ok(),
            None => literal.parse::<i128>().ok().map(|v| v as u128),
        };
        let number = number.ok_or_else(|| format!("`{literal}` is not a number"));
        if let Some(n) = REGISTERS.iter().position(|r| *r == place) {
            if n == RSP {
                return Some(Err("the stack pointer is the run's; a stack argument is `[offset] value`".to_owned()));
            }
            return Some(number.map(|v| Self::Register(n, v as u64)));
        }
        if let Some(n) = place.strip_prefix("xmm").and_then(|n| n.parse::<usize>().ok()).filter(|n| *n < 16) {
            return Some(number.map(|v| Self::Vector(n, v)));
        }
        let offset = place.strip_prefix('[')?.strip_suffix(']')?.parse::<i64>().ok()?;
        if offset < 8 || offset % 8 != 0 {
            return Some(Err("a stack qword lies past the return address, at a multiple of 8".to_owned()));
        }
        Some(number.map(|v| Self::Stack(offset, v as u64)))
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
pub(crate) struct Case {
    /// The path under `testdata/cases` without its extension: `simplify/identities`.
    name: String,
    pass: String,
    calls: Vec<Call>,
    /// The file down to its `; after:` line, header and all, so a line of a parse error is a
    /// line of the file.
    source: String,
    /// The text under `; after:`.
    after: Option<String>,
    /// The types the functions over the state take once raised, by name: `; signature: @F_401000
    /// i64 (i64, i64)`, what the inference said they take, as the promotion applied it.
    signatures: Vec<(String, String)>,
}

impl Case {
    pub(crate) fn parse(name: &str, text: &str) -> Result<Self, String> {
        let (mut pass, mut calls, mut source, mut after) = (None, Vec::new(), String::new(), None::<String>);
        let mut signatures = Vec::new();
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
            } else if let Some(signature) = header.strip_prefix("signature:") {
                let shape = signature.trim().strip_prefix('@').and_then(|rest| rest.split_once(char::is_whitespace));
                let (function, ty) =
                    shape.ok_or_else(|| format!("{name}: a signature is `@name ret (params)`, not `{}`", signature.trim()))?;
                signatures.push((function.to_owned(), ty.trim().to_owned()));
            }
        }
        let pass = pass.ok_or_else(|| format!("{name}: the case names no pass: `; pass: <step>`"))?;
        if calls.is_empty() {
            return Err(format!("{name}: the case has no call: `; call: @f(i32 7)`"));
        }
        Ok(Self { name: name.to_owned(), pass, calls, source, after, signatures })
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
    /// The value returned, when it did return.
    value: Option<Value>,
    /// The calls the host saw, in order.
    host: Vec<String>,
    /// The bytes of each global when it ended.
    globals: Vec<(String, Vec<u8>)>,
}

/// What a call of a function over the state did, as its caller sees it.
struct Machine {
    run: Run,
    /// The general registers at the entry and at the end.
    entry: [u64; 16],
    exit: [u64; 16],
    /// `xmm0` at the end, where a float result lies.
    xmm0: u128,
    /// The stack bytes the call read and wrote, by their offset from the entry stack pointer.
    reads: Vec<i64>,
    writes: Vec<i64>,
}

/// The memory of a run in `arena`: the statics in its first quarter, the stack of a function
/// over the state in the second, the interpreter's frames in the second half.
struct Arena {
    base: u64,
    quarter: u64,
}

impl Arena {
    fn of(arena: &mut [u8]) -> Self {
        Self { base: arena.as_mut_ptr() as u64, quarter: (arena.len() / 4) as u64 }
    }

    fn stack(&self) -> std::ops::Range<u64> {
        self.base + self.quarter..self.base + 2 * self.quarter
    }

    /// The stack pointer a function over the state enters with: past a 16-byte boundary by the
    /// return address, as after a native call, with room above for its arguments.
    fn entry_stack_pointer(&self) -> u64 {
        ((self.stack().end - 256) & !15) - 8
    }
}

/// `module` placed in `arena` from zeroed memory, to call `function` of it.
fn placed<'m>(module: &'m Module, function: &str, arena: &mut [u8]) -> Result<Vm<'m>, String> {
    if module.function(function).is_none() {
        return Err(format!("the module has no function @{function}"));
    }
    arena.fill(0);
    let len = arena.len() as u64;
    let Arena { base, quarter } = Arena::of(arena);
    let mut memory = Memory::new();
    memory.add_region(Region::new(base, len, "case"));
    let mut vm = Vm::new(module, memory);
    vm.limit = LIMIT;
    vm.set_static_region(base, quarter);
    vm.set_frame_region(base + 2 * quarter, len - 2 * quarter);
    vm.place().map_err(|stop| format!("the module is not placed: {stop:?}"))?;
    Ok(vm)
}

/// Calls `function` with `args` and reads what the call did.
fn complete(vm: &mut Vm<'_>, module: &Module, function: &str, args: Vec<Value>, host: &mut Recorder) -> Result<Run, String> {
    let (outcome, value) = match vm.call_named(host, function, args) {
        Ok(value) => (format!("returns {}", render(&value)), Some(value)),
        Err(Stop::Exit(code)) => (format!("exits with {code}"), None),
        Err(Stop::Fault(message)) => (format!("stops ({message})"), None),
        Err(Stop::Limit(_)) => return Err(format!("the call does not end within {LIMIT} instructions")),
        Err(Stop::Unpacked(address)) => return Err(format!("the call entered {address:#x}, which it wrote")),
    };
    let mut globals = Vec::with_capacity(module.globals.len());
    for g in &module.globals {
        let address = vm.symbol(&g.name).ok_or_else(|| format!("@{} is not placed", g.name))?;
        let size = vm.layout().size(&g.ty);
        let bytes = vm.memory.read_vec(address, size).map_err(|stop| format!("@{} is not read: {stop:?}", g.name))?;
        globals.push((g.name.clone(), bytes));
    }
    Ok(Run { outcome, value, host: std::mem::take(&mut host.calls), globals })
}

/// Interprets `call` on `module`, placed in `arena` from zeroed memory.
fn interpret(module: &Module, call: &Call, arena: &mut [u8]) -> Result<Run, String> {
    let mut vm = placed(module, &call.function, arena)?;
    let mut args = Vec::with_capacity(call.args.len());
    for arg in &call.args {
        args.push(match arg {
            Arg::Value(value) => value.clone(),
            Arg::Global(name) => {
                Value::Int(u128::from(vm.symbol(name).ok_or_else(|| format!("the module has no @{name} to pass the address of"))?))
            }
            Arg::Register(..) | Arg::Vector(..) | Arg::Stack(..) => {
                return Err(format!("@{} takes arguments, and the call gives it a state", call.function));
            }
        });
    }
    complete(&mut vm, module, &call.function, args, &mut Recorder::default())
}

/// What a call over the state enters with: its registers, and the qwords above the return
/// address by their offset from the stack pointer.
struct Entry {
    gprs: [u64; 16],
    xmms: [u128; 16],
    stack: Vec<(i64, u64)>,
}

/// The entry of `call`, the stack pointer `rsp`, which is the run's.
fn entry_of(call: &Call, rsp: u64) -> Result<Entry, String> {
    let mut entry = Entry { gprs: [0; 16], xmms: [0; 16], stack: Vec::new() };
    entry.gprs[RSP] = rsp;
    for arg in &call.args {
        match arg {
            Arg::Register(n, v) => entry.gprs[*n] = *v,
            Arg::Vector(n, v) => entry.xmms[*n] = *v,
            Arg::Stack(offset, v) => entry.stack.push((*offset, *v)),
            Arg::Value(_) | Arg::Global(_) => return Err(format!("@{} takes the state, and the call gives it arguments", call.function)),
        }
    }
    Ok(entry)
}

/// Interprets `call` on `module` over a state its arguments fill, the stack pointer at a stack
/// of the arena with the return address on top.
fn interpret_state(module: &Module, call: &Call, arena: &mut [u8]) -> Result<Machine, String> {
    let layout = Arena::of(arena);
    let rsp = layout.entry_stack_pointer();
    let mut vm = placed(module, &call.function, arena)?;
    let f = module.function(&call.function).expect("a placed module has the function");
    if f.ty.params != [Type::Ptr] || f.ty.ret != Type::Void {
        return Err(format!("@{} takes no state: a call of registers is a machine function's", call.function));
    }
    let Entry { gprs: entry, xmms, stack } = entry_of(call, rsp)?;
    let fault = |stop: Stop| format!("the state is not set up: {stop:?}");
    let state = vm.allocate_static(STATE_LEN, 16).map_err(fault)?;
    for (n, v) in entry.iter().enumerate() {
        vm.memory.write(state + 8 * n as u64, &v.to_le_bytes()).map_err(fault)?;
    }
    for (n, v) in xmms.iter().enumerate() {
        vm.memory.write(state + XMM0 + 16 * n as u64, &v.to_le_bytes()).map_err(fault)?;
    }
    vm.memory.write(rsp, &RETURN_ADDRESS.to_le_bytes()).map_err(fault)?;
    for (offset, v) in &stack {
        vm.memory.write(rsp.wrapping_add_signed(*offset), &v.to_le_bytes()).map_err(fault)?;
    }
    vm.memory.arm();
    let run = complete(&mut vm, module, &call.function, vec![Value::Int(u128::from(state))], &mut Recorder::default());
    let access = vm.memory.disarm();
    let run = run?;
    let snapshot = vm.memory.read_vec(state, STATE_LEN).map_err(|stop| format!("the state is not read: {stop:?}"))?;
    let mut exit = [0u64; 16];
    for (n, register) in exit.iter_mut().enumerate() {
        *register = u64::from_le_bytes(snapshot[8 * n..8 * n + 8].try_into().unwrap());
    }
    let at = XMM0 as usize;
    let xmm0 = u128::from_le_bytes(snapshot[at..at + 16].try_into().unwrap());
    let range = layout.stack();
    let offsets = |runs: &[(u64, Vec<u8>)]| -> Vec<i64> {
        runs.iter()
            .flat_map(|(start, bytes)| *start..*start + bytes.len() as u64)
            .filter(|address| range.contains(address))
            .map(|address| address.wrapping_sub(rsp) as i64)
            .collect()
    };
    Ok(Machine { run, entry, exit, xmm0, reads: offsets(&access.reads), writes: offsets(&access.writes) })
}

/// The arguments a function of `signature` takes for the state `call` enters with, as the
/// replayer reads a vector: a register parameter from its register, a stack parameter from its
/// slot above the return address.
fn arguments(signature: &Signature, call: &Call) -> Result<Vec<Value>, String> {
    let Entry { gprs, xmms, stack } = entry_of(call, 0)?;
    if signature.variadic {
        return Err("the signature is variadic, which a case over the x64 state cannot be".to_owned());
    }
    let mut args = Vec::with_capacity(signature.param_count());
    for (index, kind) in signature.params.iter().enumerate() {
        let bits = match TARGET.param_slot(&signature.params, index) {
            Some(Slot::Gpr(n)) => u128::from(gprs[usize::from(n)]),
            Some(Slot::Xmm(n)) => xmms[usize::from(n)],
            slot => return Err(format!("parameter {} of the signature is in {slot:?}", index + 1)),
        };
        args.push(as_kind(*kind, bits));
    }
    for (kind, offset) in signature.stack_params.iter().zip(signature.stack_offsets(TARGET)) {
        let at = TARGET.first_stack_arg() + offset;
        let word = stack.iter().find(|(offset, _)| *offset == at).map_or(0, |(_, v)| *v);
        args.push(as_kind(*kind, u128::from(word)));
    }
    Ok(args)
}

/// The value of a parameter of `kind` whose register or slot holds `bits`.
fn as_kind(kind: Kind, bits: u128) -> Value {
    match kind {
        Kind::Int | Kind::Wide => Value::Int(bits & u128::from(u64::MAX)),
        Kind::Float => Value::F32(f32::from_bits(bits as u32)),
        Kind::Double => Value::F64(f64::from_bits(bits as u64)),
    }
}

/// The bits of a value, a float's as it lies in a register.
fn bits_of(value: &Value) -> u128 {
    match value {
        Value::F32(f) => u128::from(f.to_bits()),
        Value::F64(f) => u128::from(f.to_bits()),
        other => other.as_u128(),
    }
}

/// Where the raised function, called with the arguments `signature` reads of the state, parts
/// from the function over the state, if it does: how the two ended, the result against the
/// register it returns in, the calls and the globals; and where the run over the state parts from
/// the signature itself: the stack pointer it leaves and the registers the signature keeps.
fn through(signature: &Signature, machine: &Machine, raised: &Run) -> Option<String> {
    let before = &machine.run;
    if let (Some(_), Some(value)) = (&before.value, &raised.value) {
        let result = match signature.returns {
            None => None,
            Some(Kind::Int | Kind::Wide) => Some(("rax", u128::from(machine.exit[0]), 64)),
            Some(Kind::Double) => Some(("xmm0", machine.xmm0, 64)),
            Some(Kind::Float) => Some(("xmm0", machine.xmm0, 32)),
        };
        if let Some((register, held, bits)) = result {
            let mask = u128::MAX >> (128 - bits);
            if held & mask != bits_of(value) & mask {
                return Some(format!(
                    "{register} is {:#x} over the state, the raised function returns {:#x}",
                    held & mask,
                    bits_of(value) & mask
                ));
            }
        }
    } else if before.outcome != raised.outcome {
        return Some(format!("{} over the state, {} raised", before.outcome, raised.outcome));
    }
    let effects = |run: &Run| Run { outcome: String::new(), value: None, host: run.host.clone(), globals: run.globals.clone() };
    if let Some(what) = difference(&effects(before), &effects(raised)) {
        return Some(what);
    }
    if before.value.is_none() {
        return None;
    }
    let left = machine.entry[RSP].wrapping_add(TARGET.pushed_return() as u64).wrapping_add(signature.pops);
    if machine.exit[RSP] != left {
        return Some(format!("rsp leaves at {:#x} over the state, the signature pops to {left:#x}", machine.exit[RSP]));
    }
    for (n, name) in REGISTERS.iter().enumerate().filter(|(n, _)| *n != RSP) {
        let slot = Slot::Gpr(n as u8);
        if (TARGET.callee_saved(slot) || signature.preserves[slot.index()]) && machine.entry[n] != machine.exit[n] {
            return Some(format!(
                "the signature keeps {name}, which enters as {:#x} and leaves as {:#x}",
                machine.entry[n], machine.exit[n]
            ));
        }
    }
    None
}

/// Where the stack analysis of a function parts from a run of it, if it does: a byte the run
/// touched outside the frame the analysis gives (below it only when the function calls none of
/// the module, whose frames lie below), a stack argument it read that the analysis did not
/// list, a stack pointer it left that the pops do not say.
fn agrees(info: &StackInfo, machine: &Machine, alone: bool) -> Option<String> {
    // The return address is the run's: the `ret` that pops it reads it into a value nothing uses.
    let returns = 0..TARGET.pushed_return();
    for &offset in machine.reads.iter().chain(&machine.writes).filter(|offset| !returns.contains(offset)) {
        if offset >= info.max_offset || (alone && offset < info.min_offset) {
            return Some(format!(
                "the run touches the stack at {offset:+}, outside the frame of the analysis, {:+} to {:+}",
                info.min_offset, info.max_offset
            ));
        }
    }
    let slot = Kind::Int.stack_width(TARGET);
    for &offset in machine.reads.iter().filter(|offset| **offset >= TARGET.first_stack_arg() && **offset < info.max_offset) {
        if !info.argument_reads.iter().any(|(&at, &bits)| at <= offset && offset < at + i64::from(bits / 8).max(slot)) {
            return Some(format!(
                "the run reads the stack argument at {offset:+}, which the analysis does not list: {:?}",
                info.argument_reads
            ));
        }
    }
    if machine.run.value.is_some() {
        let moved = machine.exit[RSP].wrapping_sub(machine.entry[RSP]) as i64;
        if moved != TARGET.pushed_return() + info.pops {
            return Some(format!("the run leaves rsp {moved:+} from the entry, the analysis pops {}", info.pops));
        }
    }
    None
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
    printed_as(case, &after)
}

/// The functions of `after` print as the case's `after`, when it has one, and take the types its
/// signatures say.
fn printed_as(case: &Case, after: &Module) -> Result<(), String> {
    for (function, expected) in &case.signatures {
        let f = after.function(function).ok_or_else(|| format!("{}: the signature names @{function}, which is no function", case.name))?;
        let actual = f.ty.to_string();
        if actual != *expected {
            return Err(format!("{}: @{function} takes `{actual}` after `{}`, not `{expected}`", case.name, case.pass));
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

/// Runs `case`, whose functions are over the state, through the raise of the ABI level up to and
/// with its pass, stopped ahead of `before`: each call over the state agrees, through the
/// signature of its function, with the raised function called with the arguments the signature
/// reads of the state, as the replayer reads a vector.
fn check_states(case: &Case, roots: &BTreeSet<String>, before: &str) -> Result<(), String> {
    let module = ra_asm_ir::parse::parse(&case.source).map_err(|e| format!("{}: {e}", case.name))?;
    let mut raised = module.clone();
    let signatures = ra_asm_lift::raise::raise_before(&mut raised, roots, ra_asm_lift::RaiseOptions::default(), Some(before));
    let mut arena = vec![0u8; ARENA_LEN];
    for call in &case.calls {
        let fail = |what: String| format!("{}: `{}` through `{}`: {what}", case.name, call.text, case.pass);
        let signature = signatures.get(&call.function).ok_or_else(|| fail("the inference gave the function no signature".to_owned()))?;
        let machine = interpret_state(&module, call, &mut arena).map_err(|e| fail(format!("over the state, {e}")))?;
        let args = arguments(signature, call).map_err(&fail)?;
        let mut vm = placed(&raised, &call.function, &mut arena).map_err(|e| fail(format!("raised, {e}")))?;
        let run = complete(&mut vm, &raised, &call.function, args, &mut Recorder::default()).map_err(|e| fail(format!("raised, {e}")))?;
        if let Some(what) = through(signature, &machine, &run) {
            return Err(fail(what));
        }
    }
    printed_as(case, &raised)
}

/// Runs `case`, a stack case: the analysis of each function a call reaches agrees with the run
/// of the call over the state. The functions are read as the inference reads them, once the
/// `simplify` step ahead of it ran, and told of their callees what the signatures say.
fn check_stack(case: &Case, roots: &BTreeSet<String>) -> Result<(), String> {
    let module = ra_asm_ir::parse::parse(&case.source).map_err(|e| format!("{}: {e}", case.name))?;
    let mut simplified = module.clone();
    apply(step("simplify").expect("a step"), &mut simplified, roots);
    let options = ra_asm_lift::RaiseOptions::default();
    let none = HashMap::new();
    let Pass::Signatures(infer) = step("abi").expect("a step").pass else { unreachable!("`abi` infers the signatures") };
    let signatures = infer(&simplified, &Context::new(roots, &options, &none));
    let cx = Context::new(roots, &options, &signatures);
    let Pass::Stack(analyze) = step("stack").expect("a step").pass else { unreachable!("`stack` is the analysis") };
    let mut arena = vec![0u8; ARENA_LEN];
    for call in &case.calls {
        let fail = |what: String| format!("{}: `{}` through `{}`: {what}", case.name, call.text, case.pass);
        let f = simplified.function(&call.function).ok_or_else(|| fail(format!("the module has no function @{}", call.function)))?;
        let info = analyze(f, &cx).ok_or_else(|| fail("the function takes no state".to_owned()))?;
        let machine = interpret_state(&module, call, &mut arena).map_err(|e| fail(format!("over the state, {e}")))?;
        if let Some(what) = agrees(&info, &machine, module.functions.len() == 1) {
            return Err(fail(what));
        }
    }
    printed_as(case, &simplified)
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

/// Runs `case` with the step it names; the functions it calls are the roots. The four steps that
/// carry the levels take functions over the state: `promote` and `frame` run in the raise as far
/// as themselves, `abi` as far as the promotion that applies its signatures, and `stack` is
/// held against the runs.
pub(crate) fn run(case: &Case) -> Result<(), String> {
    let step = step(&case.pass).ok_or_else(|| format!("{}: there is no pass `{}`", case.name, case.pass))?;
    let roots: BTreeSet<String> = case.calls.iter().map(|call| call.function.clone()).collect();
    match case.pass.as_str() {
        "abi" | "promote" => check_states(case, &roots, "conditions"),
        "frame" => check_states(case, &roots, "known"),
        "stack" => check_stack(case, &roots),
        _ => check(case, &|module| apply(step, module, &roots)),
    }
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

/// The tests, one file per group under `tests/cases`.
#[path = "cases/mod.rs"]
mod groups;
