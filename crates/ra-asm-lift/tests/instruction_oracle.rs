//! The instruction oracle: every integer and SSE form the x86 lift covers, checked against the
//! processor. The generator walks iced's code table, keeps the forms whose mnemonic has an arm in
//! `lift_instr` and whose operands are registers, memory or immediates, and gives each random
//! operands over a random machine state. Each sample runs natively in a page of this process (the
//! faults caught by a vectored handler and the run resumed in the trampoline's epilogue) and
//! through the lift and the IR interpreter; a difference names the form, the input state and the
//! field that differs, the flags by name. Flags the manual leaves undefined are not compared.
//!
//! The suite runs at a fixed seed; `RA_ORACLE_SEED=random` (or a number) picks another and prints
//! it, `RA_ORACLE_SAMPLES` the states per form, for a nightly run.

#![cfg(all(windows, target_arch = "x86_64"))]

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::c_void;
use std::sync::Once;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use ra_asm_ir::FnType;
use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_x86::iced_x86::{
    Code, Encoder, EncodingKind, Formatter, Instruction, IntelFormatter, Mnemonic, OpCodeOperandKind as K, OpKind, Register as R,
    RflagsBits,
};

// region The arena

/// The memory both runs share: the state, the trampoline's scratch, a data buffer, a stack, and
/// the interpreter's statics and frames, one allocation at one address.
const ARENA_LEN: u64 = 0x10000;
const STATE: u64 = 0;
const SCRATCH: u64 = 0x1000;
const DATA: u64 = 0x2000;
const DATA_LEN: u64 = 0x2000;
const STACK: u64 = 0x4000;
const STACK_LEN: u64 = 0x4000;
const STATICS: u64 = 0x8000;
const FRAMES: u64 = 0xC000;
const PAGE_LEN: u64 = 0x1000;

/// The offsets of `%State`: sixteen general registers, `rip` at 128, the flag bytes, sixteen
/// vectors.
const FLAGS_AT: u64 = 136;
const XMM_AT: u64 = 144;
const STATE_LEN: usize = 400;

const FLAG_NAMES: [&str; 7] = ["cf", "pf", "af", "zf", "sf", "df", "of"];
const FLAG_BITS: [u32; 7] = [0, 2, 4, 6, 7, 10, 11];
const GPR_NAMES: [&str; 16] =
    ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"];

#[link(name = "kernel32")]
unsafe extern "system" {
    fn VirtualAlloc(address: *mut c_void, size: usize, kind: u32, protect: u32) -> *mut c_void;
    fn AddVectoredExceptionHandler(first: u32, handler: unsafe extern "system" fn(*mut c_void) -> i32) -> *mut c_void;
}

struct Arena {
    base: u64,
    code: u64,
}

impl Arena {
    fn new() -> Self {
        // Below 4 GB keeps every address a small constant in the trampoline and the IR alike.
        let base = (0x2000_0000u64..0x7000_0000)
            .step_by(0x0100_0000)
            .find(|&at| unsafe { VirtualAlloc(at as usize as *mut c_void, ARENA_LEN as usize, 0x3000, 0x04) } as usize as u64 == at)
            .expect("an address below 4 GB for the arena");
        let code = unsafe { VirtualAlloc(std::ptr::null_mut(), PAGE_LEN as usize, 0x3000, 0x40) } as usize as u64;
        assert!(code != 0, "an executable page");
        Self { base, code }
    }

    fn write(&self, at: u64, bytes: &[u8]) {
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), (self.base + at) as usize as *mut u8, bytes.len()) };
    }

    fn read(&self, at: u64, len: usize) -> Vec<u8> {
        let mut out = vec![0u8; len];
        unsafe { std::ptr::copy_nonoverlapping((self.base + at) as usize as *const u8, out.as_mut_ptr(), len) };
        out
    }
}

// endregion

// region The native run

static PAGE_START: AtomicU64 = AtomicU64::new(0);
static PAGE_END: AtomicU64 = AtomicU64::new(0);
static EPILOGUE: AtomicU64 = AtomicU64::new(0);
static FAULT: AtomicU32 = AtomicU32::new(0);
static HANDLER: Once = Once::new();

/// A fault in the trampoline's page: record it and resume at the epilogue, which restores the
/// host's stack and registers. Anything else is someone else's.
unsafe extern "system" fn on_exception(info: *mut c_void) -> i32 {
    let record = unsafe { *(info as *const *const u32) };
    let context = unsafe { *(info as *const *mut u8).add(1) };
    let rip = unsafe { context.add(0xF8).cast::<u64>() };
    let at = unsafe { *rip };
    if at >= PAGE_START.load(Ordering::SeqCst) && at < PAGE_END.load(Ordering::SeqCst) {
        FAULT.store(unsafe { *record }, Ordering::SeqCst);
        unsafe { *rip = EPILOGUE.load(Ordering::SeqCst) };
        return -1;
    }
    0
}

/// The machine code of the trampoline, hand-assembled.
struct Asm(Vec<u8>);

impl Asm {
    fn rex(&mut self, w: bool, reg: u32, rm: u32) {
        let v = 0x40 | u8::from(w) << 3 | (((reg >> 3) & 1) as u8) << 2 | ((rm >> 3) & 1) as u8;
        if v != 0x40 {
            self.0.push(v);
        }
    }

    fn bytes(&mut self, b: &[u8]) {
        self.0.extend_from_slice(b);
    }

    fn disp(&mut self, d: u32) {
        self.0.extend_from_slice(&d.to_le_bytes());
    }

    /// `mov r64, [rax + disp]`
    fn load_from_rax(&mut self, reg: u32, d: u32) {
        self.rex(true, reg, 0);
        self.bytes(&[0x8B, 0x80 | ((reg & 7) as u8) << 3]);
        self.disp(d);
    }

    /// `mov [rax + disp], r64`
    fn store_to_rax(&mut self, reg: u32, d: u32) {
        self.rex(true, reg, 0);
        self.bytes(&[0x89, 0x80 | ((reg & 7) as u8) << 3]);
        self.disp(d);
    }

    /// `movdqu xmm, [base + disp]` or the store; `base` is rax or rsp.
    fn movdqu(&mut self, load: bool, xmm: u32, rsp: bool, d: u32) {
        self.0.push(0xF3);
        self.rex(false, xmm, 0);
        self.bytes(&[0x0F, if load { 0x6F } else { 0x7F }]);
        if rsp {
            self.bytes(&[0x84 | ((xmm & 7) as u8) << 3, 0x24]);
        } else {
            self.0.push(0x80 | ((xmm & 7) as u8) << 3);
        }
        self.disp(d);
    }

    fn push(&mut self, reg: u32) {
        self.rex(false, 0, reg);
        self.0.push(0x50 | (reg & 7) as u8);
    }

    fn pop(&mut self, reg: u32) {
        self.rex(false, 0, reg);
        self.0.push(0x58 | (reg & 7) as u8);
    }

    fn mov_rax_imm(&mut self, v: u64) {
        self.bytes(&[0x48, 0xB8]);
        self.bytes(&v.to_le_bytes());
    }

    fn mov_rcx_imm(&mut self, v: u64) {
        self.bytes(&[0x48, 0xB9]);
        self.bytes(&v.to_le_bytes());
    }
}

/// Writes the trampoline around `instr` into the code page: the callee-saved registers and
/// `xmm6..15` saved, the host stack pointer parked in the scratch, the state loaded (flags through
/// `popfq`, `rax` last), the instruction, the state stored (`rax` through a `moffs` store before
/// anything else moves, the flags after the host stack is back), and the epilogue the fault
/// handler also lands in.
fn build_trampoline(arena: &Arena, instr: &[u8]) -> extern "C" fn() {
    let state = arena.base + STATE;
    let scratch = arena.base + SCRATCH;
    let mut a = Asm(Vec::new());
    for r in [3, 5, 6, 7, 12, 13, 14, 15] {
        a.push(r);
    }
    a.bytes(&[0x48, 0x81, 0xEC, 0xA0, 0, 0, 0]);
    for k in 0..10 {
        a.movdqu(false, 6 + k, true, 16 * k);
    }
    a.mov_rax_imm(scratch);
    a.bytes(&[0x48, 0x89, 0x20]);
    a.mov_rax_imm(state);
    for x in 0..16 {
        a.movdqu(true, x, false, (XMM_AT + 16 * u64::from(x)) as u32);
    }
    a.bytes(&[0xFF, 0xB0]);
    a.disp(FLAGS_AT as u32);
    a.0.push(0x9D);
    for r in (1..16).filter(|&r| r != 4) {
        a.load_from_rax(r, 8 * r);
    }
    a.load_from_rax(4, 32);
    a.bytes(&[0x48, 0x8B, 0x00]);
    a.bytes(instr);
    a.bytes(&[0x48, 0xA3]);
    a.bytes(&(scratch + 8).to_le_bytes());
    a.mov_rax_imm(state);
    a.store_to_rax(1, 8);
    a.store_to_rax(4, 32);
    a.mov_rcx_imm(scratch);
    a.bytes(&[0x48, 0x8B, 0x21]);
    a.0.push(0x9C);
    a.bytes(&[0x8F, 0x80]);
    a.disp(FLAGS_AT as u32);
    for r in (2..16).filter(|&r| r != 4) {
        a.store_to_rax(r, 8 * r);
    }
    a.bytes(&[0x48, 0x8B, 0x49, 0x08, 0x48, 0x89, 0x08]);
    for x in 0..16 {
        a.movdqu(false, x, false, (XMM_AT + 16 * u64::from(x)) as u32);
    }
    let epilogue = a.0.len() as u64;
    a.0.push(0xFC);
    a.mov_rax_imm(scratch);
    a.bytes(&[0x48, 0x8B, 0x20]);
    for k in 0..10 {
        a.movdqu(true, 6 + k, true, 16 * k);
    }
    a.bytes(&[0x48, 0x81, 0xC4, 0xA0, 0, 0, 0]);
    for r in [15, 14, 13, 12, 7, 6, 5, 3] {
        a.pop(r);
    }
    a.0.push(0xC3);
    assert!(a.0.len() as u64 <= PAGE_LEN);
    unsafe { std::ptr::copy_nonoverlapping(a.0.as_ptr(), arena.code as usize as *mut u8, a.0.len()) };
    PAGE_START.store(arena.code, Ordering::SeqCst);
    PAGE_END.store(arena.code + PAGE_LEN, Ordering::SeqCst);
    EPILOGUE.store(arena.code + epilogue, Ordering::SeqCst);
    HANDLER.call_once(|| {
        let h = unsafe { AddVectoredExceptionHandler(1, on_exception) };
        assert!(!h.is_null(), "a vectored exception handler");
    });
    unsafe { std::mem::transmute::<u64, extern "C" fn()>(arena.code) }
}

/// Runs the trampoline once; the fault's code when the instruction faulted.
fn run_native(entry: extern "C" fn()) -> Option<u32> {
    FAULT.store(0, Ordering::SeqCst);
    entry();
    match FAULT.load(Ordering::SeqCst) {
        0 => None,
        code => Some(code),
    }
}

// endregion

// region The lifted run

/// A PE32+ with one `.text` section that holds `code` at RVA 0x1000, the entry point.
const IMAGE_BASE: u64 = 0x7_0000_0000;

fn pe_with_code(code: &[u8]) -> Vec<u8> {
    let mut f = vec![0u8; 0x400];
    let w16 = |f: &mut Vec<u8>, at: usize, v: u16| f[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let w32 = |f: &mut Vec<u8>, at: usize, v: u32| f[at..at + 4].copy_from_slice(&v.to_le_bytes());
    let w64 = |f: &mut Vec<u8>, at: usize, v: u64| f[at..at + 8].copy_from_slice(&v.to_le_bytes());
    f[0] = b'M';
    f[1] = b'Z';
    w32(&mut f, 0x3C, 0x40);
    f[0x40..0x44].copy_from_slice(b"PE\0\0");
    let coff = 0x44;
    w16(&mut f, coff, 0x8664);
    w16(&mut f, coff + 2, 1);
    w16(&mut f, coff + 16, 0xF0);
    w16(&mut f, coff + 18, 0x22);
    let opt = coff + 20;
    w16(&mut f, opt, 0x20B);
    w32(&mut f, opt + 4, 0x200);
    w32(&mut f, opt + 16, 0x1000);
    w32(&mut f, opt + 20, 0x1000);
    w64(&mut f, opt + 24, IMAGE_BASE);
    w32(&mut f, opt + 32, 0x1000);
    w32(&mut f, opt + 36, 0x200);
    w16(&mut f, opt + 40, 6);
    w16(&mut f, opt + 48, 6);
    w32(&mut f, opt + 56, 0x2000);
    w32(&mut f, opt + 60, 0x200);
    w16(&mut f, opt + 68, 3);
    w64(&mut f, opt + 72, 0x10_0000);
    w64(&mut f, opt + 80, 0x1000);
    w64(&mut f, opt + 88, 0x10_0000);
    w64(&mut f, opt + 96, 0x1000);
    w32(&mut f, opt + 108, 16);
    let sec = opt + 0xF0;
    f[sec..sec + 5].copy_from_slice(b".text");
    w32(&mut f, sec + 8, 0x1000);
    w32(&mut f, sec + 12, 0x1000);
    w32(&mut f, sec + 16, 0x200);
    w32(&mut f, sec + 20, 0x200);
    w32(&mut f, sec + 36, 0x6000_0020);
    f[0x200..0x400].fill(0xCC);
    f[0x200..0x200 + code.len()].copy_from_slice(code);
    f
}

/// The host of the interpreted run: a trap is a fault of the lifted code, anything else it asks
/// for is a thing a lone instruction has no business calling.
struct Lone;

impl Host for Lone {
    fn call(&mut self, _vm: &mut Vm<'_>, name: &str, _ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        match name {
            "ra_trap" => Err(Stop::fault(format!("trap {}", args.get(1).map_or(0, Value::as_u64)))),
            "ra_unsupported" => Err(Stop::fault("unsupported by the lift")),
            other => Err(Stop::fault(format!("a call to {other}"))),
        }
    }
}

/// The lift of one instruction followed by `ret`, ready to run.
struct LiftedForm {
    lifted: ra_asm_lift::Lifted,
    name: String,
    warnings: Vec<String>,
}

fn lift_instruction(bytes: &[u8]) -> LiftedForm {
    let mut code = bytes.to_vec();
    code.push(0xC3);
    let image = ra_asm_pe::Image::parse(pe_with_code(&code)).expect("the synthetic image parses");
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
    let lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let warnings = lifted.warnings.clone();
    LiftedForm { name: ra_asm_lift::function_name(IMAGE_BASE + 0x1000), lifted, warnings }
}

fn new_vm<'m>(arena: &Arena, module: &'m ra_asm_ir::Module) -> Vm<'m> {
    let mut memory = Memory::new();
    memory.add_region(Region::new(arena.base, ARENA_LEN, "arena"));
    let mut vm = Vm::new(module, memory);
    vm.set_static_region(arena.base + STATICS, FRAMES - STATICS);
    vm.set_frame_region(arena.base + FRAMES, ARENA_LEN - FRAMES);
    vm.limit = 2_000_000;
    vm.place().expect("the module places");
    vm
}

/// Runs the lifted instruction over the state in the arena; the fault when it stopped.
fn run_lifted(vm: &mut Vm<'_>, name: &str, arena: &Arena) -> Option<String> {
    match vm.call_named(&mut Lone, name, vec![Value::Int(u128::from(arena.base + STATE))]) {
        Ok(_) => None,
        Err(Stop::Fault(m)) => Some(m),
        Err(Stop::Limit(n)) => Some(format!("the limit of {n} instructions")),
        Err(Stop::Exit(c)) => Some(format!("exit {c}")),
    }
}

// endregion

// region The generator

/// xorshift64*, so a seed means the same run on every machine.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len() as u64) as usize]
    }

    fn chance(&mut self, out_of: u64) -> bool {
        self.below(out_of) == 0
    }

    /// A value with interesting shapes as often as a uniform one: small, all ones, a sign bit,
    /// a power of two, a byte pattern.
    fn value(&mut self) -> u64 {
        match self.below(8) {
            0 => self.below(16),
            1 => u64::MAX - self.below(16),
            2 => 1u64 << self.below(64),
            3 => (1u64 << self.below(64)).wrapping_sub(1),
            4 => 0x8000_0000_0000_0000 | self.below(16),
            5 => 0x7FFF_FFFF_FFFF_FFF0 | self.below(16),
            _ => self.next(),
        }
    }

    fn fill(&mut self, buf: &mut [u8]) {
        for chunk in buf.chunks_mut(8) {
            let v = self.next().to_le_bytes();
            chunk.copy_from_slice(&v[..chunk.len()]);
        }
    }
}

const GPR8: [R; 19] = [
    R::AL,
    R::CL,
    R::DL,
    R::BL,
    R::AH,
    R::CH,
    R::DH,
    R::BH,
    R::BPL,
    R::SIL,
    R::DIL,
    R::R8L,
    R::R9L,
    R::R10L,
    R::R11L,
    R::R12L,
    R::R13L,
    R::R14L,
    R::R15L,
];
const GPR16: [R; 15] =
    [R::AX, R::CX, R::DX, R::BX, R::BP, R::SI, R::DI, R::R8W, R::R9W, R::R10W, R::R11W, R::R12W, R::R13W, R::R14W, R::R15W];
const GPR32: [R; 15] =
    [R::EAX, R::ECX, R::EDX, R::EBX, R::EBP, R::ESI, R::EDI, R::R8D, R::R9D, R::R10D, R::R11D, R::R12D, R::R13D, R::R14D, R::R15D];
const GPR64: [R; 15] =
    [R::RAX, R::RCX, R::RDX, R::RBX, R::RBP, R::RSI, R::RDI, R::R8, R::R9, R::R10, R::R11, R::R12, R::R13, R::R14, R::R15];
const XMM: [R; 16] = [
    R::XMM0,
    R::XMM1,
    R::XMM2,
    R::XMM3,
    R::XMM4,
    R::XMM5,
    R::XMM6,
    R::XMM7,
    R::XMM8,
    R::XMM9,
    R::XMM10,
    R::XMM11,
    R::XMM12,
    R::XMM13,
    R::XMM14,
    R::XMM15,
];

/// The mnemonics of `lift_instr` the oracle does not run, each with its reason: control flow
/// (the harness runs one instruction and returns), the environment (another answer every run or
/// a host call), the 32-bit-only forms, and the one whose result the manual leaves undefined.
const NOT_RUN: &[(&str, &str)] = &[
    ("Int", "an interrupt leaves the process (int 0x29 ends it)"),
    ("Call", "control flow"),
    ("Ret", "control flow"),
    ("Retf", "control flow"),
    ("Jmp", "control flow"),
    ("Jo", "control flow"),
    ("Jno", "control flow"),
    ("Jb", "control flow"),
    ("Jae", "control flow"),
    ("Je", "control flow"),
    ("Jne", "control flow"),
    ("Jbe", "control flow"),
    ("Ja", "control flow"),
    ("Js", "control flow"),
    ("Jns", "control flow"),
    ("Jp", "control flow"),
    ("Jnp", "control flow"),
    ("Jl", "control flow"),
    ("Jge", "control flow"),
    ("Jle", "control flow"),
    ("Jg", "control flow"),
    ("Jrcxz", "control flow"),
    ("Jecxz", "control flow"),
    ("Jcxz", "control flow"),
    ("Loop", "control flow"),
    ("Loope", "control flow"),
    ("Loopne", "control flow"),
    ("Cpuid", "the host answers"),
    ("Xgetbv", "the host answers"),
    ("Rdtsc", "another value every run"),
    ("Stmxcsr", "the runtime's own control register"),
    ("Ldmxcsr", "the runtime's own control register"),
    ("Pushfd", "32-bit mode only"),
    ("Popfd", "32-bit mode only"),
];

/// The mnemonics with an arm in `lift_instr`, read off the source.
fn lifted_mnemonics() -> BTreeSet<String> {
    let source = include_str!("../src/x86.rs");
    let start = source.find("fn lift_instr(").expect("lift_instr");
    let end = source[start..].find("fn widen_to(").expect("the end of lift_instr") + start;
    let body = &source[start..end];
    let mut out = BTreeSet::new();
    let mut rest = body;
    while let Some(at) = rest.find("M::") {
        let ident: String = rest[at + 3..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if !ident.is_empty() {
            out.insert(ident);
        }
        rest = &rest[at + 3..];
    }
    out
}

fn mnemonic_name(m: Mnemonic) -> String {
    format!("{m:?}")
}

fn operand_kind_supported(k: K) -> bool {
    matches!(
        k,
        K::None
            | K::r8_reg
            | K::r8_or_mem
            | K::r8_opcode
            | K::r16_reg
            | K::r16_rm
            | K::r16_or_mem
            | K::r16_opcode
            | K::r32_reg
            | K::r32_rm
            | K::r32_or_mem
            | K::r32_opcode
            | K::r64_reg
            | K::r64_rm
            | K::r64_or_mem
            | K::r64_opcode
            | K::xmm_reg
            | K::xmm_rm
            | K::xmm_or_mem
            | K::mem
            | K::mem_offs
            | K::imm8
            | K::imm8_const_1
            | K::imm8sex16
            | K::imm8sex32
            | K::imm8sex64
            | K::imm16
            | K::imm32
            | K::imm32sex64
            | K::imm64
            | K::al
            | K::cl
            | K::ax
            | K::dx
            | K::eax
            | K::rax
            | K::seg_rSI
            | K::es_rDI
    )
}

/// The forms the oracle runs: every legacy-encoded code of 64-bit mode whose mnemonic the lift
/// has an arm for and whose operands the generator can produce.
fn forms(lifted: &BTreeSet<String>) -> Vec<Code> {
    let skipped: BTreeSet<&str> = NOT_RUN.iter().map(|(m, _)| *m).collect();
    Code::values()
        .filter(|&code| {
            let op = code.op_code();
            let name = mnemonic_name(code.mnemonic());
            op.is_instruction()
                && op.encoding() == EncodingKind::Legacy
                && op.is_available_in_mode(64)
                && lifted.contains(&name)
                && !skipped.contains(name.as_str())
                && op.op_kinds().iter().all(|&k| operand_kind_supported(k))
                // bswap of a 16-bit register leaves it undefined.
                && code != Code::Bswap_r16
        })
        .collect()
}

/// The registers a sample pins to a value before it runs, by full register.
type Pins = Vec<(R, u64)>;

/// A random instance of `code`: the instruction, and the registers it needs at certain values
/// (the bases of its memory operands, the string registers, a divisor's high half).
fn instance(code: Code, rng: &mut Rng, arena: &Arena) -> Option<(Instruction, Pins)> {
    let data_mid = arena.base + DATA + DATA_LEN / 2;
    let stack_mid = arena.base + STACK + STACK_LEN / 2;
    let mut ins = Instruction::default();
    ins.set_code(code);
    let mut pins = Pins::new();
    let kinds = code.op_code().op_kinds();
    for (n, &k) in kinds.iter().enumerate() {
        let n = n as u32;
        let reg = |ins: &mut Instruction, r: R| {
            ins.set_op_kind(n, OpKind::Register);
            ins.set_op_register(n, r);
        };
        let rm = |ins: &mut Instruction, rng: &mut Rng, pins: &mut Pins, regs: &[R]| {
            if rng.chance(2) {
                reg(ins, rng.pick(regs));
            } else {
                memory(ins, n, rng, pins, data_mid);
            }
        };
        match k {
            K::None => {}
            K::r8_reg | K::r8_opcode => reg(&mut ins, rng.pick(&GPR8)),
            K::r16_reg | K::r16_rm | K::r16_opcode => reg(&mut ins, rng.pick(&GPR16)),
            K::r32_reg | K::r32_rm | K::r32_opcode => reg(&mut ins, rng.pick(&GPR32)),
            K::r64_reg | K::r64_rm | K::r64_opcode => reg(&mut ins, rng.pick(&GPR64)),
            K::xmm_reg | K::xmm_rm => reg(&mut ins, rng.pick(&XMM)),
            K::r8_or_mem => rm(&mut ins, rng, &mut pins, &GPR8),
            K::r16_or_mem => rm(&mut ins, rng, &mut pins, &GPR16),
            K::r32_or_mem => rm(&mut ins, rng, &mut pins, &GPR32),
            K::r64_or_mem => rm(&mut ins, rng, &mut pins, &GPR64),
            K::xmm_or_mem => rm(&mut ins, rng, &mut pins, &XMM),
            K::mem => memory(&mut ins, n, rng, &mut pins, data_mid),
            K::mem_offs => {
                ins.set_op_kind(n, OpKind::Memory);
                ins.set_memory_base(R::None);
                ins.set_memory_displ_size(8);
                ins.set_memory_displacement64(data_mid.wrapping_add_signed(16 * (rng.below(128) as i64 - 64)));
            }
            K::imm8 => {
                ins.set_op_kind(n, OpKind::Immediate8);
                ins.set_immediate8(immediate8(code, rng));
            }
            K::imm8_const_1 => {
                ins.set_op_kind(n, OpKind::Immediate8);
                ins.set_immediate8(1);
            }
            K::imm8sex16 => {
                ins.set_op_kind(n, OpKind::Immediate8to16);
                ins.set_immediate8to16(rng.value() as i8 as i16);
            }
            K::imm8sex32 => {
                ins.set_op_kind(n, OpKind::Immediate8to32);
                ins.set_immediate8to32(rng.value() as i8 as i32);
            }
            K::imm8sex64 => {
                ins.set_op_kind(n, OpKind::Immediate8to64);
                ins.set_immediate8to64(rng.value() as i8 as i64);
            }
            K::imm16 => {
                ins.set_op_kind(n, OpKind::Immediate16);
                ins.set_immediate16(rng.value() as u16);
            }
            K::imm32 => {
                ins.set_op_kind(n, OpKind::Immediate32);
                ins.set_immediate32(rng.value() as u32);
            }
            K::imm32sex64 => {
                ins.set_op_kind(n, OpKind::Immediate32to64);
                ins.set_immediate32to64(rng.value() as i32 as i64);
            }
            K::imm64 => {
                ins.set_op_kind(n, OpKind::Immediate64);
                ins.set_immediate64(rng.value());
            }
            K::al => reg(&mut ins, R::AL),
            K::cl => reg(&mut ins, R::CL),
            K::ax => reg(&mut ins, R::AX),
            K::dx => reg(&mut ins, R::DX),
            K::eax => reg(&mut ins, R::EAX),
            K::rax => reg(&mut ins, R::RAX),
            K::seg_rSI => ins.set_op_kind(n, OpKind::MemorySegRSI),
            K::es_rDI => ins.set_op_kind(n, OpKind::MemoryESRDI),
            _ => return None,
        }
    }
    let m = code.mnemonic();
    if kinds.iter().any(|k| matches!(k, K::seg_rSI | K::es_rDI)) {
        // The string registers point into the data buffer, apart enough for a count of 32 either
        // way; a repeat half the time, by the mnemonic's own prefix.
        pins.push((R::RSI, arena.base + DATA + 0x800));
        pins.push((R::RDI, arena.base + DATA + 0x1800));
        pins.push((R::RCX, rng.below(33)));
        if rng.chance(2) {
            match m {
                Mnemonic::Cmpsb
                | Mnemonic::Cmpsw
                | Mnemonic::Cmpsd
                | Mnemonic::Cmpsq
                | Mnemonic::Scasb
                | Mnemonic::Scasw
                | Mnemonic::Scasd
                | Mnemonic::Scasq => {
                    if rng.chance(2) {
                        ins.set_has_repe_prefix(true);
                    } else {
                        ins.set_has_repne_prefix(true);
                    }
                }
                _ => ins.set_has_rep_prefix(true),
            }
        }
    }
    match m {
        Mnemonic::Leave => pins.push((R::RBP, stack_mid - 0x40)),
        // A random high half overflows nearly every quotient: most samples keep it small. The
        // 64-bit forms keep it the extension of the low half: the lift stops on a 128-bit
        // dividend by decision (the C port links no 128-bit division).
        Mnemonic::Div if kinds[0] == K::r64_or_mem => pins.push((R::RDX, 0)),
        Mnemonic::Idiv if kinds[0] == K::r64_or_mem => {
            let low = rng.value();
            pins.push((R::RAX, low));
            pins.push((R::RDX, if (low as i64) < 0 { u64::MAX } else { 0 }));
        }
        Mnemonic::Div if !rng.chance(4) => pins.push((R::RDX, rng.below(4))),
        Mnemonic::Idiv if !rng.chance(4) => pins.push((R::RDX, if rng.chance(2) { 0 } else { u64::MAX })),
        // A 16-bit double shift by more than 16 is undefined.
        Mnemonic::Shld | Mnemonic::Shrd if kinds[0] == K::r16_or_mem && kinds[2] == K::cl => {
            pins.push((R::RCX, (rng.next() & !0xFF) | rng.below(17)));
        }
        // A bit offset in a register reaches as far as the register says: keep it in the buffer.
        Mnemonic::Bt | Mnemonic::Bts | Mnemonic::Btr | Mnemonic::Btc
            if ins.op0_kind() == OpKind::Memory && ins.op1_kind() == OpKind::Register =>
        {
            let offset = (rng.below(0x4000) as i64 - 0x2000) as u64;
            pins.push((ins.op1_register().full_register(), offset));
        }
        _ => {}
    }
    Some((ins, pins))
}

/// A memory operand `[base + index * scale + disp]` sixteen-aligned inside the data buffer, or
/// around the stack pointer one time in eight.
fn memory(ins: &mut Instruction, n: u32, rng: &mut Rng, pins: &mut Pins, data_mid: u64) {
    ins.set_op_kind(n, OpKind::Memory);
    let base = if rng.chance(8) { R::RSP } else { rng.pick(&GPR64) };
    ins.set_memory_base(base);
    if base != R::RSP {
        pins.push((base, data_mid));
    }
    let mut disp = 16 * (rng.below(128) as i64 - 64);
    if rng.chance(4) {
        let index = rng.pick(&GPR64);
        if index != base {
            let scale = [1u32, 2, 4, 8][rng.below(4) as usize];
            ins.set_memory_index(index);
            ins.set_memory_index_scale(scale);
            pins.push((index, (16 / u64::from(scale)) * rng.below(4)));
            // The index reaches forward: keep the whole operand in the buffer.
            disp = disp.min(0x3C0);
        }
    }
    // iced sizes a 64-bit address's displacement as 1 or 8, the latter a sign-extended disp32.
    ins.set_memory_displ_size(if (-128..=127).contains(&disp) { 1 } else { 8 });
    ins.set_memory_displacement64(disp as u64);
}

/// A byte immediate that means what the instruction can use: a vector compare's predicate is
/// three bits, a shift count is whatever.
fn immediate8(code: Code, rng: &mut Rng) -> u8 {
    match code.mnemonic() {
        Mnemonic::Cmpps | Mnemonic::Cmppd | Mnemonic::Cmpss | Mnemonic::Cmpsd => rng.below(8) as u8,
        Mnemonic::Shld | Mnemonic::Shrd if code.op_code().op_kinds()[0] == K::r16_or_mem => rng.below(17) as u8,
        _ => rng.value() as u8,
    }
}

fn encode(ins: &Instruction) -> Result<Vec<u8>, String> {
    let mut encoder = Encoder::new(64);
    encoder.encode(ins, IMAGE_BASE + 0x1000).map_err(|e| e.to_string())?;
    Ok(encoder.take_buffer())
}

// endregion

// region The samples

/// A machine state: what the arena holds before and after a run.
#[derive(Clone)]
struct Picture {
    gprs: [u64; 16],
    flags: [u8; 7],
    xmm: [u128; 16],
    data: Vec<u8>,
    stack: Vec<u8>,
}

impl Picture {
    fn random(rng: &mut Rng, arena: &Arena, pins: &Pins) -> Self {
        let mut gprs = [0u64; 16];
        for g in &mut gprs {
            *g = rng.value();
        }
        gprs[4] = arena.base + STACK + STACK_LEN / 2;
        for &(r, v) in pins {
            gprs[r.number()] = v;
        }
        let mut flags = [0u8; 7];
        for f in &mut flags {
            *f = u8::from(rng.chance(2));
        }
        let mut xmm = [0u128; 16];
        for x in &mut xmm {
            *x = (u128::from(rng.value()) << 64) | u128::from(rng.value());
        }
        let mut data = vec![0u8; DATA_LEN as usize];
        rng.fill(&mut data);
        let mut stack = vec![0u8; STACK_LEN as usize];
        rng.fill(&mut stack);
        // What a pop of the flags would load: nothing that traps (no TF) or the harness cannot
        // restore.
        let safe = (rng.next() & 0xCD5) | 2;
        let sp = (STACK_LEN / 2) as usize;
        stack[sp..sp + 8].copy_from_slice(&safe.to_le_bytes());
        Self { gprs, flags, xmm, data, stack }
    }

    fn flags_word(&self) -> u64 {
        FLAG_BITS.iter().zip(self.flags).fold(2u64, |w, (&bit, f)| w | u64::from(f) << bit)
    }

    /// Writes the state as the side reads it: the flags as one word for the processor, as bytes
    /// for the lifted code.
    fn write(&self, arena: &Arena, native: bool) {
        let mut state = vec![0u8; STATE_LEN];
        for (i, g) in self.gprs.iter().enumerate() {
            state[8 * i..8 * i + 8].copy_from_slice(&g.to_le_bytes());
        }
        if native {
            state[FLAGS_AT as usize..FLAGS_AT as usize + 8].copy_from_slice(&self.flags_word().to_le_bytes());
        } else {
            state[FLAGS_AT as usize..FLAGS_AT as usize + 7].copy_from_slice(&self.flags);
        }
        for (i, x) in self.xmm.iter().enumerate() {
            let at = XMM_AT as usize + 16 * i;
            state[at..at + 16].copy_from_slice(&x.to_le_bytes());
        }
        arena.write(STATE, &state);
        arena.write(DATA, &self.data);
        arena.write(STACK, &self.stack);
    }

    fn read(arena: &Arena, native: bool) -> Self {
        let state = arena.read(STATE, STATE_LEN);
        let mut gprs = [0u64; 16];
        for (i, g) in gprs.iter_mut().enumerate() {
            *g = u64::from_le_bytes(state[8 * i..8 * i + 8].try_into().unwrap());
        }
        let mut flags = [0u8; 7];
        if native {
            let word = u64::from_le_bytes(state[FLAGS_AT as usize..FLAGS_AT as usize + 8].try_into().unwrap());
            for (f, &bit) in flags.iter_mut().zip(&FLAG_BITS) {
                *f = ((word >> bit) & 1) as u8;
            }
        } else {
            flags.copy_from_slice(&state[FLAGS_AT as usize..FLAGS_AT as usize + 7]);
        }
        let mut xmm = [0u128; 16];
        for (i, x) in xmm.iter_mut().enumerate() {
            let at = XMM_AT as usize + 16 * i;
            *x = u128::from_le_bytes(state[at..at + 16].try_into().unwrap());
        }
        Self { gprs, flags, xmm, data: arena.read(DATA, DATA_LEN as usize), stack: arena.read(STACK, STACK_LEN as usize) }
    }

    fn describe(&self, ins: &Instruction, arena: &Arena) -> String {
        let mut out = String::new();
        for (i, name) in GPR_NAMES.iter().enumerate() {
            out.push_str(&format!("{name}={:#x} ", self.gprs[i]));
        }
        out.push_str("flags=");
        for (name, f) in FLAG_NAMES.iter().zip(self.flags) {
            out.push_str(&format!("{name}{f} "));
        }
        for n in 0..ins.op_count() {
            if ins.op_kind(n) == OpKind::Register && ins.op_register(n).is_xmm() {
                out.push_str(&format!("xmm{}={:#034x} ", ins.op_register(n).number(), self.xmm[ins.op_register(n).number()]));
            }
        }
        if let Some(at) = effective_address(ins, self)
            && let Some(bytes) = self.bytes_at(arena, at)
        {
            out.push_str(&format!("[{at:#x}]="));
            for b in bytes {
                out.push_str(&format!("{b:02x}"));
            }
        }
        out
    }

    /// Sixteen bytes of the data or the stack buffer at an arena address, when they are in one.
    fn bytes_at(&self, arena: &Arena, at: u64) -> Option<&[u8]> {
        let off = at.checked_sub(arena.base)?;
        let (buf, start) = if (DATA..DATA + DATA_LEN).contains(&off) { (&self.data, DATA) } else { (&self.stack, STACK) };
        let i = (off - start) as usize;
        buf.get(i..(i + 16).min(buf.len()))
    }
}

/// The address a memory operand names in the input state.
fn effective_address(ins: &Instruction, p: &Picture) -> Option<u64> {
    (0..ins.op_count()).find(|&n| ins.op_kind(n) == OpKind::Memory)?;
    let base = ins.memory_base();
    let mut at = ins.memory_displacement64();
    if base != R::None {
        at = at.wrapping_add(p.gprs[base.number()]);
    }
    if ins.memory_index() != R::None {
        at = at.wrapping_add(p.gprs[ins.memory_index().number()].wrapping_mul(u64::from(ins.memory_index_scale())));
    }
    Some(at)
}

/// How a run ended: the picture, or the fault.
enum Ran {
    State(Picture),
    Fault(String),
}

/// The fields of two pictures that differ, the flags the manual leaves undefined ignored.
fn differences(native: &Picture, lifted: &Picture, undefined: u32) -> Vec<String> {
    let mut out = Vec::new();
    for i in 0..16 {
        // The lifted function returned: its `ret` popped eight bytes the trampoline did not.
        let l = if i == 4 { lifted.gprs[4].wrapping_sub(8) } else { lifted.gprs[i] };
        if native.gprs[i] != l {
            out.push(format!("{}: native {:#x}, lifted {l:#x}", GPR_NAMES[i], native.gprs[i]));
        }
    }
    let undefined_bits = [RflagsBits::CF, RflagsBits::PF, RflagsBits::AF, RflagsBits::ZF, RflagsBits::SF, RflagsBits::DF, RflagsBits::OF];
    for (i, name) in FLAG_NAMES.iter().enumerate() {
        if undefined & undefined_bits[i] == 0 && native.flags[i] != lifted.flags[i] {
            out.push(format!("{name}: native {}, lifted {}", native.flags[i], lifted.flags[i]));
        }
    }
    for i in 0..16 {
        if native.xmm[i] != lifted.xmm[i] {
            out.push(format!("xmm{i}: native {:#034x}, lifted {:#034x}", native.xmm[i], lifted.xmm[i]));
        }
    }
    for (what, a, b) in [("data", &native.data, &lifted.data), ("stack", &native.stack, &lifted.stack)] {
        if let Some(i) = a.iter().zip(b).position(|(x, y)| x != y) {
            let end = (i + 16).min(a.len());
            let hex = |s: &[u8]| s.iter().map(|b| format!("{b:02x}")).collect::<String>();
            out.push(format!("{what}+{i:#x}: native {}, lifted {}", hex(&a[i..end]), hex(&b[i..end])));
        }
    }
    out
}

// endregion

// region The run

struct Settings {
    seed: u64,
    samples: u32,
}

fn settings() -> Settings {
    let seed = match std::env::var("RA_ORACLE_SEED").ok().as_deref() {
        None | Some("") => 0x5EED_0108,
        Some("random") => std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64 | 1,
        Some(text) => text.parse().expect("RA_ORACLE_SEED is a number or `random`"),
    };
    let samples = std::env::var("RA_ORACLE_SAMPLES").ok().and_then(|s| s.parse().ok()).unwrap_or(12);
    Settings { seed, samples }
}

fn format(ins: &Instruction) -> String {
    let mut f = IntelFormatter::new();
    let mut out = String::new();
    f.format(ins, &mut out);
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
}

/// Runs every form: each sample natively and through the lift, and collects the differences.
#[test]
fn every_form_matches_the_processor() {
    let Settings { seed, samples } = settings();
    eprintln!("instruction oracle: seed {seed:#x}, {samples} samples per form");
    let lifted = lifted_mnemonics();
    let forms = forms(&lifted);
    let arena = Arena::new();
    let mut rng = Rng(seed);
    let mut failures: Vec<String> = Vec::new();
    let mut unencodable: Vec<(Code, String)> = Vec::new();
    let mut ran: BTreeMap<String, u32> = BTreeMap::new();
    let mut runs = 0u32;
    for &code in &forms {
        let mut form_failures = 0;
        for sample in 0..samples {
            // The operands are drawn with the state: a register that will not encode (ah with a
            // rex prefix) is drawn again.
            let mut last_error = String::new();
            let Some((ins, pins, bytes)) = (0..8).find_map(|_| {
                let (ins, pins) = instance(code, &mut rng, &arena)?;
                match encode(&ins) {
                    Ok(bytes) => Some((ins, pins, bytes)),
                    Err(e) => {
                        last_error = format!("{}: {e}", format(&ins));
                        None
                    }
                }
            }) else {
                unencodable.push((code, last_error));
                break;
            };
            let text = format(&ins);
            let lift = lift_instruction(&bytes);
            let site = format!("{code:?} `{text}` ({})", hex(&bytes));
            if !lift.warnings.is_empty() {
                failures.push(format!("{site}: the lift warned: {}", lift.warnings.join("; ")));
                break;
            }
            let entry = build_trampoline(&arena, &bytes);
            let mut vm = new_vm(&arena, &lift.lifted.module);
            let input = Picture::random(&mut rng, &arena, &pins);
            input.write(&arena, true);
            let native = match run_native(entry) {
                None => Ran::State(Picture::read(&arena, true)),
                Some(fault) => Ran::Fault(format!("exception {fault:#x}")),
            };
            input.write(&arena, false);
            let interpreted = match run_lifted(&mut vm, &lift.name, &arena) {
                None => Ran::State(Picture::read(&arena, false)),
                Some(fault) => Ran::Fault(fault),
            };
            runs += 1;
            *ran.entry(mnemonic_name(code.mnemonic())).or_default() += 1;
            let diffs = match (&native, &interpreted) {
                (Ran::State(n), Ran::State(l)) => differences(n, l, ins.rflags_undefined()),
                (Ran::Fault(_), Ran::Fault(l)) if l.contains("unsupported") => vec![format!("the lift does not cover the form: {l}")],
                (Ran::Fault(_), Ran::Fault(_)) => Vec::new(),
                (Ran::Fault(n), Ran::State(_)) => vec![format!("the processor faulted ({n}), the lifted code ran")],
                (Ran::State(_), Ran::Fault(l)) => vec![format!("the processor ran, the lifted code stopped: {l}")],
            };
            if !diffs.is_empty() {
                failures.push(format!("{site} sample {sample}:\n    {}\n    in: {}", diffs.join("\n    "), input.describe(&ins, &arena)));
                form_failures += 1;
                if form_failures == 2 {
                    break;
                }
            }
        }
    }
    let not_run: BTreeSet<&str> = NOT_RUN.iter().map(|(m, _)| *m).collect();
    let uncovered: Vec<&String> = lifted.iter().filter(|m| !not_run.contains(m.as_str()) && !ran.contains_key(*m)).collect();
    eprintln!(
        "instruction oracle: {} forms, {runs} runs, {} mnemonics; {} forms did not encode",
        forms.len(),
        ran.len(),
        unencodable.len()
    );
    for (code, error) in &unencodable {
        eprintln!("  not encoded: {code:?} ({error})");
    }
    assert!(
        failures.is_empty() && uncovered.is_empty(),
        "{} differences at seed {seed:#x}:\n{}\narms of lift_instr without a form in the generator: {uncovered:?}",
        failures.len(),
        failures.join("\n")
    );
}

/// Every arm of `lift_instr` has a form in the generator or a reason not to.
#[test]
fn every_arm_has_a_form() {
    let lifted = lifted_mnemonics();
    let forms = forms(&lifted);
    let generated: BTreeSet<String> = forms.iter().map(|c| mnemonic_name(c.mnemonic())).collect();
    let not_run: BTreeSet<&str> = NOT_RUN.iter().map(|(m, _)| *m).collect();
    let missing: Vec<&String> = lifted.iter().filter(|m| !generated.contains(*m) && !not_run.contains(m.as_str())).collect();
    assert!(missing.is_empty(), "arms of lift_instr without a form: {missing:?}");
    let stale: Vec<&&str> = not_run.iter().filter(|m| !lifted.contains(**m)).collect();
    assert!(stale.is_empty(), "NOT_RUN names mnemonics lift_instr has no arm for: {stale:?}");
}

// endregion
