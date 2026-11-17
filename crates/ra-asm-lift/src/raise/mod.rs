//! The raising passes: from machine IR over `%State` to source-shaped IR.
//!
//! Promotion turns the registers and the flags into SSA values inside each function, guided by
//! liveness, so a flag no branch reads is never computed and a register travels in a value, not
//! through memory. In ABI mode the inferred signatures apply too: parameters, a frame, calls with
//! arguments, a returned value, and no `%State` at all. Simplification and dead-code elimination
//! clean up after either.

pub mod abi;
pub mod assume;
pub mod check;
pub mod conditions;
pub mod dce;
pub mod flatten;
pub mod frame;
pub mod held;
pub mod known;
pub mod liveness;
pub mod locals;
pub mod outline;
pub mod promote;
pub mod prototypes;
pub mod simplify;
pub mod stack;
pub mod steps;
pub mod stores;
pub mod strings;
pub mod stubs;
pub mod types;
pub mod widths;
pub mod x87;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use liveness::Slot;
use ra_asm_analysis::rtti::Vtable;
use ra_asm_arch::abi::{self as convention, Abi, Reg};
use ra_asm_ir::{Module, Type};

use crate::state::Isa;

/// Whether the stages print their times, as `ras emit --times` asks (see [`timed`]).
static TIMES: AtomicBool = AtomicBool::new(false);

/// Has [`timed`] print the seconds of every stage from now on, as `RA_STEP_TIMES` does.
pub fn print_times(on: bool) {
    TIMES.store(on, Ordering::Relaxed);
}

/// Whether the stages print their times: [`print_times`] asked for them, or `RA_STEP_TIMES` is set.
pub fn times() -> bool {
    TIMES.load(Ordering::Relaxed) || std::env::var_os("RA_STEP_TIMES").is_some()
}

/// Runs `run` and, when the times print ([`times`]), writes its seconds on stderr as one line,
/// `ra: time: <name>: <seconds>s`: a slow stretch of a large emit shows by its stage, not by
/// watching the process.
pub fn timed<T>(name: &str, run: impl FnOnce() -> T) -> T {
    if !times() {
        return run();
    }
    let start = Instant::now();
    let out = run();
    eprintln!("ra: time: {name}: {:.3}s", start.elapsed().as_secs_f64());
    out
}

/// The moment past which a raise stops, and what stops it (see [`stop_at`]).
struct Deadline {
    at: Instant,
    stop: fn(function: &str, step: &str),
}

static DEADLINE: OnceLock<Deadline> = OnceLock::new();

/// Has every raise from now on call `stop` with the function and the step it is at once `at` has
/// passed, as `ras --deadline` asks: the raise looks between its steps and between its functions,
/// so a raise that runs on is stopped where it is and named, not waited out. `stop` ends the
/// process; a raise it returns to goes on. The first deadline set holds.
pub fn stop_at(at: Instant, stop: fn(function: &str, step: &str)) {
    let _ = DEADLINE.set(Deadline { at, stop });
}

/// Calls the deadline's stop when [`stop_at`] set one and it has passed, with the function and
/// the step the raise is at.
fn check_deadline(function: &str, step: &str) {
    if let Some(deadline) = DEADLINE.get()
        && Instant::now() >= deadline.at
    {
        (deadline.stop)(function, step);
    }
}

/// What a deadline names for the function of a step over the whole module.
const THE_MODULE: &str = "the module";

/// The machine the IR came from, by its instruction set and pointer width, which set the calling
/// convention the raise applies: Win64 for a 64-bit x86 image, the x86 conventions (`cdecl`,
/// `stdcall`, `fastcall`, `thiscall`, told apart by the registers a function reads and the bytes
/// it pops) for a 32-bit one, AAPCS64 as Windows on ARM keeps it for an ARM64 one, and o32 or
/// n32 for MIPS, both with 32-bit pointers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub bits: u32,
    pub isa: Isa,
}

/// The slot of `x8`, where an AAPCS64 caller passes the address of a returned aggregate.
pub const X8: Slot = Slot::Gpr(8);
/// The MIPS high result register, `v1`.
const MIPS_V1: Slot = Slot::Gpr(3);
/// The MIPS globals pointer, `gp`.
pub const MIPS_GP: Slot = Slot::Gpr(28);

/// What the raise reads of a target besides its pointer width: its calling convention and the
/// registers its state holds.
struct Machine {
    abi: &'static Abi,
    /// The convention's integer parameter registers as slots.
    int_params: &'static [Slot],
    /// The registers a function only its own code calls may take arguments in, the convention's
    /// first: MSVC passes a static function's arguments in any register it likes on x86 (the C
    /// runtime's `parse_cmdline` takes its count's address in `edi`). Elsewhere the convention's.
    private_ints: &'static [Slot],
    gprs: u8,
    xmms: u8,
    /// Whether an unknown callee takes its first integer arguments in registers.
    register_arguments: bool,
    /// The high half of a 64-bit result where the convention's own register is read: `v1` on
    /// MIPS; elsewhere `edx`, where the x86 helpers (`_allmul`, `_allrem`) leave it.
    high_result: Option<Slot>,
}

/// The slot of a convention's register: a general or a vector register's own.
const fn slot(reg: Reg) -> Slot {
    match reg {
        Reg::Gpr(n) => Slot::Gpr(n),
        Reg::Vector(n) => Slot::Xmm(n),
        Reg::X87(_) => panic!("an x87 register has no slot"),
    }
}

/// `regs` as slots, so a table of them lives as long as the convention does.
const fn slots<const N: usize>(regs: &[Reg]) -> [Slot; N] {
    let mut out = [Slot::Gpr(0); N];
    let mut i = 0;
    while i < N {
        out[i] = slot(regs[i]);
        i += 1;
    }
    out
}

const WIN64_INTS: [Slot; convention::WIN64.int_params.len()] = slots(convention::WIN64.int_params);
const X86_INTS: [Slot; convention::X86.int_params.len()] = slots(convention::X86.int_params);
/// `ecx` and `edx`, then `eax`, `ebx`, `esi` and `edi`.
const X86_PRIVATE_INTS: [Slot; 6] = [Slot::Gpr(1), Slot::Gpr(2), Slot::Gpr(0), Slot::Gpr(3), Slot::Gpr(6), Slot::Gpr(7)];
const AAPCS64_INTS: [Slot; convention::AAPCS64.int_params.len()] = slots(convention::AAPCS64.int_params);
const AAPCS32_INTS: [Slot; convention::AAPCS32.int_params.len()] = slots(convention::AAPCS32.int_params);
const O32_INTS: [Slot; convention::O32.int_params.len()] = slots(convention::O32.int_params);
const N32_INTS: [Slot; convention::N32.int_params.len()] = slots(convention::N32.int_params);

const WIN64_MACHINE: Machine = Machine {
    abi: &convention::WIN64,
    int_params: &WIN64_INTS,
    private_ints: &WIN64_INTS,
    gprs: 16,
    xmms: 16,
    register_arguments: true,
    high_result: None,
};
const X86_MACHINE: Machine = Machine {
    abi: &convention::X86,
    int_params: &X86_INTS,
    private_ints: &X86_PRIVATE_INTS,
    gprs: 16,
    xmms: 16,
    register_arguments: false,
    high_result: None,
};
const ARM64_MACHINE: Machine = Machine {
    abi: &convention::AAPCS64,
    int_params: &AAPCS64_INTS,
    private_ints: &AAPCS64_INTS,
    gprs: 32,
    xmms: 32,
    register_arguments: true,
    high_result: None,
};
const ARM32_MACHINE: Machine = Machine {
    abi: &convention::AAPCS32,
    int_params: &AAPCS32_INTS,
    private_ints: &AAPCS32_INTS,
    gprs: 16,
    xmms: 16,
    register_arguments: false,
    high_result: None,
};
const O32_MACHINE: Machine = Machine {
    abi: &convention::O32,
    int_params: &O32_INTS,
    private_ints: &O32_INTS,
    gprs: 34,
    xmms: 32,
    register_arguments: true,
    high_result: Some(MIPS_V1),
};
const N32_MACHINE: Machine = Machine {
    abi: &convention::N32,
    int_params: &N32_INTS,
    private_ints: &N32_INTS,
    gprs: 34,
    xmms: 32,
    register_arguments: true,
    high_result: Some(MIPS_V1),
};

/// The one place a target's instruction set and width pick its convention: Win64 for 64-bit
/// x86, the x86 conventions for 32-bit, AAPCS64 and AAPCS32 for the ARMs, o32 and n32 for MIPS.
fn machine(target: Target) -> &'static Machine {
    match (target.isa, target.bits) {
        (Isa::Arm64, _) => &ARM64_MACHINE,
        (Isa::Arm32, _) => &ARM32_MACHINE,
        (Isa::Mips, _) => &O32_MACHINE,
        (Isa::MipsN32, _) => &N32_MACHINE,
        (Isa::X86, 64) => &WIN64_MACHINE,
        (Isa::X86, _) => &X86_MACHINE,
    }
}

impl Target {
    pub const WIN64: Target = Target { bits: 64, isa: Isa::X86 };
    pub const X86: Target = Target { bits: 32, isa: Isa::X86 };
    pub const ARM64: Target = Target { bits: 64, isa: Isa::Arm64 };
    pub const ARM32: Target = Target { bits: 32, isa: Isa::Arm32 };
    pub const MIPS_O32: Target = Target { bits: 32, isa: Isa::Mips };
    pub const MIPS_N32: Target = Target { bits: 32, isa: Isa::MipsN32 };

    /// o32 or n32: `a0` to `a3` (n32 to `a7`) and the stack, results in `v0` and `v1` or `$f0`,
    /// the return address in `ra`.
    pub fn is_mips(self) -> bool {
        self.isa.is_mips()
    }

    /// Whether an unknown callee takes its first integer arguments in registers: everywhere
    /// but 32-bit x86, whose conventions put them on the stack.
    pub fn register_arguments(self) -> bool {
        machine(self).register_arguments
    }

    /// The slot of an integer result: `rax`, `x0`, `r0` or `v0`.
    pub fn int_return(self) -> Slot {
        slot(self.abi().int_results[0])
    }

    /// The slot of the high half of a 64-bit result of a 32-bit target: `edx`, or `v1`.
    pub fn high_return(self) -> Slot {
        machine(self).high_result.unwrap_or(liveness::RDX)
    }

    pub fn is_64(self) -> bool {
        self.bits == 64
    }

    pub fn is_arm64(self) -> bool {
        self.isa == Isa::Arm64
    }

    /// AAPCS32, as 32-bit ARM Linux and Windows RT keep it: `r0` to `r3` and the stack, the
    /// return address in `lr`.
    pub fn is_arm32(self) -> bool {
        self.isa == Isa::Arm32
    }

    /// The calling convention the raise applies.
    pub fn abi(self) -> &'static Abi {
        machine(self).abi
    }

    /// Whether a parameter's position is one register of either class, as on Win64; elsewhere
    /// the integers and the floats each count their own registers.
    pub fn shares_positions(self) -> bool {
        self.abi().shared_positions
    }

    /// The size of a pointer and of a stack slot, in bytes.
    pub fn ptr(self) -> i64 {
        i64::from(self.bits / 8)
    }

    /// The integer type of the address width.
    pub fn int_ty(self) -> Type {
        Type::Int(self.bits as u16)
    }

    /// The slot of the stack pointer: `rsp`, `sp` on ARM64, `r13` on 32-bit ARM, `$29` on MIPS.
    pub fn sp(self) -> Slot {
        slot(self.abi().stack_pointer)
    }

    /// The bytes a call pushes for its return address and the callee's return pops: a pointer on
    /// x86, nothing on ARM and MIPS, where `bl` and `jal` leave it in a register.
    pub fn pushed_return(self) -> i64 {
        i64::from(self.abi().pushed_return())
    }

    /// The general and the vector registers of the target's state; MIPS's vector slots are the
    /// FPU's registers.
    pub fn gprs(self) -> u8 {
        machine(self).gprs
    }

    pub fn xmms(self) -> u8 {
        machine(self).xmms
    }

    /// The slots the target's state has: its general registers, the flags and its vector
    /// registers. The slot space is the widest state's; a narrower one leaves the rest unused.
    pub fn slots(self) -> impl Iterator<Item = Slot> {
        let (gprs, xmms) = (self.gprs(), self.xmms());
        Slot::all().filter(move |s| match *s {
            Slot::Gpr(n) => n < gprs,
            Slot::Flag(_) => true,
            Slot::Xmm(n) => n < xmms,
        })
    }

    /// The offset of the first stack argument above the entry stack pointer: past the return
    /// address, and on Win64 past the four home slots; on ARM64 the entry stack pointer itself;
    /// under o32 past the four home slots of `a0` to `a3` the caller reserves at its stack
    /// pointer, and under n32, which reserves none, the entry stack pointer.
    /// Also where a caller's outgoing arguments start above its stack pointer once the return
    /// address is pushed.
    pub fn first_stack_arg(self) -> i64 {
        i64::from(self.abi().first_stack_arg())
    }

    /// Where a caller's outgoing arguments start above its stack pointer at the call instruction,
    /// before any push: what an import called in place of the native call sees.
    pub fn outgoing_at_call(self) -> i64 {
        i64::from(self.abi().home_space)
    }

    /// The registers that carry integer arguments, by position: `rcx`, `rdx`, `r8`, `r9` on Win64;
    /// `ecx` and `edx` on x86, which `fastcall` and `thiscall` use; `x0` to `x7` on ARM64; `r0` to
    /// `r3` on 32-bit ARM; `a0` to `a3` (`$4` to `$7`) under o32 and on to `a7` (`$11`) under n32.
    pub fn int_param_slots(self) -> &'static [Slot] {
        machine(self).int_params
    }

    /// The registers a function whose every caller the lift sees (no pointer to it, not an
    /// export or the entry) may take integer arguments in, by position: the convention's, then
    /// on x86 `eax`, `ebx`, `esi` and `edi`, which MSVC passes a static function's arguments in.
    pub fn private_int_param_slots(self) -> &'static [Slot] {
        machine(self).private_ints
    }

    /// How many floats arrive in vector registers: four on Win64, where a position is a register
    /// of either class; three on x86, where clang passes the floats of a local function in `xmm0`
    /// to `xmm2` beside the integers in `ecx` and `edx`, each class counted on its own; eight on
    /// ARM64, `v0` to `v7`, counted on their own too; none on 32-bit ARM, whose VFP registers the
    /// lift does not model yet; two under o32, `$f12` and `$f14`, and eight under n32, `$f12` to
    /// `$f19`. o32 passes a float in them only while every parameter before it is a float too,
    /// in the integer registers after that; the count of each class on its own reads the first
    /// shape.
    pub fn float_params(self) -> usize {
        self.abi().float_params.len()
    }

    /// The vector slot of float parameter `place`: `xmm<place>` or `v<place>`; `$f12` and `$f14`
    /// under o32, where a double takes an even register and the odd one after it; `$f12` on under
    /// n32. A place past the convention's registers is its own vector slot, which no parameter
    /// reaches.
    pub fn float_param_slot(self, place: usize) -> Slot {
        self.abi().float_params.get(place).map_or(Slot::Xmm(place as u8), |reg| slot(*reg))
    }

    /// The register of parameter `index` of `params`: by position on Win64, by its place among
    /// the parameters of its class on x86, ARM64 and MIPS.
    pub fn param_slot(self, params: &[abi::Kind], index: usize) -> Option<Slot> {
        let kind = params[index];
        let float = matches!(kind, abi::Kind::Float | abi::Kind::Double);
        let place = if self.shares_positions() {
            index
        } else {
            params[..index].iter().filter(|k| matches!(k, abi::Kind::Float | abi::Kind::Double) == float).count()
        };
        if float { self.abi().float_params.get(place).copied().map(slot) } else { self.private_int_param_slots().get(place).copied() }
    }

    /// Whether the callee keeps the register: `rbx`, `rbp`, `rsp`, `rsi`, `rdi`, `r12` to `r15`
    /// and `xmm6` to `xmm15` on Win64; `ebx`, `ebp`, `esp`, `esi` and `edi` on x86; on ARM64
    /// `x19` to `x28`, the frame pointer `x29`, `sp`, the low halves of `v8` to `v15`, and `x18`,
    /// the thread's block, which no Windows function changes. The link register is the caller's
    /// to lose: `bl` writes it. On 32-bit ARM `r4` to `r11` and `sp`. On MIPS `s0` to `s7`, `sp`
    /// and `fp`, and `$0`, which nothing writes; `gp` under n32, where a callee restores it,
    /// while under o32 the caller reloads it after a call; `$f20` to `$f31` under o32 (the six
    /// doubles from `$f20`) and the even ones of them under n32. `ra` is the caller's: `jal`
    /// writes it. A vector slot past the state's vector registers, which no function of the
    /// target touches, answers as the last of them does.
    pub fn callee_saved(self, slot: Slot) -> bool {
        let reg = match slot {
            Slot::Gpr(n) => Reg::Gpr(n),
            Slot::Xmm(n) => Reg::Vector(n.min(self.xmms() - 1)),
            Slot::Flag(_) => return false,
        };
        self.abi().is_callee_saved(reg)
    }

    /// The entry stack pointer's offset past a 16-byte boundary: a native call pushes the return
    /// address onto an aligned stack, so a callee starts at 8 (Win64) or 12 (x86, where the
    /// alignment is 4 and 12 is what a caller aligned to 16 leaves); on ARM64 the stack pointer
    /// is 16-byte aligned at every call and nothing is pushed, so 0, as on MIPS, where o32 keeps
    /// 8 bytes and n32 16 at a call.
    pub fn entry_misalignment(self) -> u64 {
        u64::from(self.abi().entry_misalignment())
    }
}

/// How far to raise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    /// The machine IR as lifted: `F_x(ptr %state)`, every register in memory.
    Machine,
    /// Registers and flags as SSA values; the `%State` interface stays.
    Promoted,
    /// The signatures applied: parameters, frames, results; no `%State`.
    #[default]
    Abi,
}

impl Level {
    /// The name the command line and the workbench use for the level.
    pub fn name(self) -> &'static str {
        match self {
            Self::Machine => "machine",
            Self::Promoted => "promoted",
            Self::Abi => "abi",
        }
    }
}

impl std::str::FromStr for Level {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text {
            "machine" => Ok(Self::Machine),
            "promoted" => Ok(Self::Promoted),
            "abi" => Ok(Self::Abi),
            other => Err(format!("no raise level `{other}`: the levels are machine, promoted, abi")),
        }
    }
}

/// The options of a raise.
#[derive(Debug, Clone)]
pub struct Options {
    pub level: Level,
    /// The address width of the image: 64 or 32.
    pub bits: u32,
    /// The x87 unit the lift modelled: the promotion pushes an import's float result as the
    /// unit keeps it.
    pub x87: crate::x87::X87Unit,
    /// The instruction set of the state, which with `bits` makes the target.
    pub isa: Isa,
    /// The vtables of the program's classes: a call through a slot of one resolves to the
    /// functions that fill the slot (see [`abi::virtual_calls`]).
    pub vtables: Vec<Vtable>,
    /// Whether an object may come from the host's C++ runtime (the program imports `msvcp140`),
    /// whose vtable holds host functions: an x64 call through a slot then takes the indirect call
    /// for an address outside the slot's functions rather than trapping.
    pub host_objects: bool,
    /// The indirect calls a traced run resolved, by address, with the lifted functions each
    /// reached: a call to them directly, with the indirect call kept for any other target (see
    /// [`abi::traced_calls`]).
    pub traced_calls: BTreeMap<u64, Vec<u64>>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            level: Level::Abi,
            bits: 64,
            x87: crate::x87::X87Unit::Double,
            isa: Isa::X86,
            vtables: Vec::new(),
            host_objects: false,
            traced_calls: BTreeMap::new(),
        }
    }
}

impl Options {
    pub fn machine() -> Self {
        Self { level: Level::Machine, ..Self::default() }
    }

    pub fn target(&self) -> Target {
        Target { bits: self.bits, isa: self.isa }
    }
}

/// Runs the raising passes over every function of `module`: the steps of the level, in the order
/// [`steps::steps`] gives. `roots` are the functions whose callers the lift does not see (see
/// [`abi::infer`]).
pub fn raise(module: &mut Module, roots: &BTreeSet<String>, options: Options) -> HashMap<String, abi::Signature> {
    raise_with(module, roots, options, None, false)
}

/// [`raise`] that leaves the raised functions frozen in the module's `frozen`, in the order the
/// raise leaves them thawed, and none among its `functions`: for a reader that thaws one function
/// at a time (the C emit), so the raised module is never whole. [`thaw`] thaws them all.
pub fn raise_frozen(module: &mut Module, roots: &BTreeSet<String>, options: Options) -> HashMap<String, abi::Signature> {
    raise_with(module, roots, options, None, true)
}

/// Every function of `module` [`raise_frozen`] left frozen, thawed after its own.
pub fn thaw(module: &mut Module) {
    held::thaw_all(module);
}

/// [`raise`], stopped ahead of the step called `before`: neither it nor any step after it runs,
/// so the module is what that step is handed. `before` is a step of the level's order past the
/// inference, which every raise makes; a name the order does not hold stops nothing.
pub fn raise_before(
    module: &mut Module,
    roots: &BTreeSet<String>,
    options: Options,
    before: Option<&str>,
) -> HashMap<String, abi::Signature> {
    raise_with(module, roots, options, before, false)
}

/// [`raise_before`], which with `keep_frozen` leaves the raised functions frozen ([`raise_frozen`]).
// The options are the raise's own, as its callers across the crates hand them: the steps borrow
// them through the context.
#[allow(clippy::needless_pass_by_value)]
fn raise_with(
    module: &mut Module,
    roots: &BTreeSet<String>,
    options: Options,
    before: Option<&str>,
    keep_frozen: bool,
) -> HashMap<String, abi::Signature> {
    use steps::{Context, Pass};
    // The order, in the three runs the raise makes of it: the function steps ahead of the
    // inference, the ones each function goes through after it, and the module's.
    let mut order = steps::steps(options.level);
    if let Some(at) = before.and_then(|name| order.iter().position(|s| s.name == name)) {
        order.truncate(at);
    }
    let inference = order.iter().position(|s| matches!(s.pass, Pass::Signatures(_))).expect("every level infers the signatures");
    let (ahead, rest) = order.split_at(inference);
    let Pass::Signatures(infer) = rest[0].pass else { unreachable!() };
    let inference_name = rest[0].name;
    let rest = &rest[1..];
    let (each, whole) = rest.split_at(rest.iter().position(|s| !matches!(s.pass, Pass::Function(_))).unwrap_or(rest.len()));
    let abi_level = options.level == Level::Abi;
    // Below the ABI level the functions the lift froze come back at once, ahead of the module's
    // own (the import wrappers and stubs the lift added after them), in their order.
    if !abi_level {
        timed("raise: thaw", || {
            let frozen = std::mem::take(&mut module.frozen);
            let mut functions = abi::par_map(&frozen, |_, f| ra_asm_ir::freeze::thaw(f));
            drop(frozen);
            functions.append(&mut module.functions);
            module.functions = functions;
        });
    }
    let signatures = {
        let none = HashMap::new();
        let cx = Context::new(roots, &options, &none);
        // The steps ahead, in place (the constants at the ABI level): the inference's stack
        // analysis reads them, and so does the rewrite, which recognizes a pushed return address
        // by its value. The inference then reads the functions as they are, without a copy of
        // the module.
        if !ahead.is_empty() {
            timed("raise: functions ahead of the signatures", || {
                let globals = &module.globals;
                abi::par_each_mut(&mut module.frozen, |_, z| {
                    let mut f = ra_asm_ir::freeze::thaw(z);
                    run_each(ahead, &mut f, &cx, globals);
                    *z = ra_asm_ir::freeze::freeze(&f);
                });
                abi::par_each_mut(&mut module.functions, |_, f| {
                    if f.name.starts_with("F_") {
                        run_each(ahead, f, &cx, globals);
                    }
                });
            });
        }
        check_deadline(THE_MODULE, inference_name);
        timed(&format!("raise: {inference_name}"), || infer(module, &cx))
    };
    {
        let cx = Context::new(roots, &options, &signatures);
        if abi_level {
            // The universal call of x86 carries six stack dwords: where a function a pointer may
            // reach takes more, the calls through a pointer hand the rest on in a global.
            if cx.extra_dwords > 0 && !module.globals.iter().any(|g| g.name == crate::state::ARGS) {
                module.globals.push(crate::state::args_global(cx.extra_dwords));
            }
            // The frozen functions come back as the raise reaches them, ahead of the module's own.
            let frozen = std::mem::take(&mut module.frozen);
            let globals = &module.globals;
            // A function's raise reads the signatures, the vtables and the data, and writes the
            // function alone: the functions raise side by side, each compacted as it finishes. A
            // function with no signature (a wrapper, a stub) stays as the lift wrote it.
            let raise_one = |f: &mut ra_asm_ir::Function| {
                if signatures.contains_key(&f.name) {
                    run_each(each, f, &cx, globals);
                }
                f.compact();
            };
            if whole.is_empty() && !keep_frozen {
                // Each frozen function gives way to its raise as the raise reaches it: the frozen
                // bytes and the raised module are never both whole.
                timed("raise: functions", || {
                    let mut slots: Vec<Result<ra_asm_ir::Function, ra_asm_ir::freeze::Frozen>> = frozen.into_iter().map(Err).collect();
                    abi::par_each_mut(&mut slots, |_, slot| {
                        if let Err(z) = slot {
                            let mut f = ra_asm_ir::freeze::thaw(z);
                            raise_one(&mut f);
                            *slot = Ok(f);
                        }
                    });
                    let mut functions: Vec<ra_asm_ir::Function> = slots.into_iter().map(|s| s.expect("every slot is raised")).collect();
                    let mut own = std::mem::take(&mut module.functions);
                    abi::par_each_mut(&mut own, |_, f| raise_one(f));
                    functions.append(&mut own);
                    module.functions = functions;
                });
            } else {
                // The module steps read the functions frozen (see [`held`]): each function is
                // frozen again as its raise ends, so the raised module is never whole before the
                // steps have run, and its row of the steps' tables is written as it is frozen.
                timed("raise: functions", || {
                    let mut frozen = frozen;
                    let mut rows = Vec::new();
                    held::rewrite(&mut frozen, &mut rows, |_| true, |_, f| raise_one(f));
                    let mut own = std::mem::take(&mut module.functions);
                    abi::par_each_mut(&mut own, |_, f| raise_one(f));
                    rows.extend(abi::par_map(&own, |_, f| held::Row::of(f)));
                    frozen.extend(abi::par_map(&own, |_, f| ra_asm_ir::freeze::freeze(f)));
                    drop(own);
                    module.frozen = frozen;
                    *cx.rows() = rows;
                });
            }
        } else {
            timed("raise: functions", || {
                let globals = &module.globals;
                for f in &mut module.functions {
                    run_each(each, f, &cx, globals);
                }
            });
        }
        for step in whole {
            let Pass::Module(run) = step.pass else { unreachable!("`{}` follows the module steps", step.name) };
            check_deadline(THE_MODULE, step.name);
            timed(&format!("raise: {}", step.name), || run(module, &cx));
        }
        if abi_level && !whole.is_empty() && !keep_frozen {
            timed("raise: thaw", || held::thaw_all(module));
        }
    }
    // The functions still thawed go ahead of the frozen ones, where a thaw would put them.
    if keep_frozen && !module.functions.is_empty() {
        timed("raise: freeze", || {
            let functions = std::mem::take(&mut module.functions);
            let mut frozen = abi::par_map(&functions, |_, f| ra_asm_ir::freeze::freeze(f));
            drop(functions);
            frozen.append(&mut module.frozen);
            module.frozen = frozen;
        });
    }
    // A pass that breaks the shape of its level shows in the builds the tests run, where it
    // happens; a function may still break its level (`testdata/levels/PENDING.txt`), so the
    // raise goes on.
    if cfg!(debug_assertions) {
        let broken = violations(module, options.level);
        if let Some((name, violation)) = broken.first() {
            let (count, of, level) = (broken.len(), module.functions.len(), options.level.name());
            eprintln!("ra: raise: {count} of {of} functions break the {level} level; the first is `{name}`, {violation}");
        }
    }
    signatures
}

/// The functions of `module` that break the invariants of `level` (see [`check`]), each with its
/// first violation, in the module's order.
pub fn violations(module: &Module, level: Level) -> Vec<(&str, check::Violation)> {
    module.functions.iter().filter_map(|f| check::check_in(module, f, level).err().map(|violation| (f.name.as_str(), violation))).collect()
}

/// The function steps of `order` over `f`, in order, each once the deadline is seen not passed.
fn run_each(order: &[steps::Step], f: &mut ra_asm_ir::Function, cx: &steps::Context<'_>, globals: &[ra_asm_ir::Global]) {
    for step in order {
        if let steps::Pass::Function(run) = step.pass {
            check_deadline(&f.name, step.name);
            run(f, cx, globals);
        }
    }
}

/// The cleanup that follows a rewriting step: the simplification, the common subexpressions and
/// the dead code, to a fixpoint.
fn clean(f: &mut ra_asm_ir::Function) {
    loop {
        let changed = simplify::run(f) | simplify::cse(f);
        dce::run(f);
        if !changed {
            break;
        }
    }
}
