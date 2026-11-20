//! The machine state and the helpers that read and write it: registers, flags and memory.
//!
//! `%State = type { [16 x i64], i64, [8 x i8], [32 x i128] }` on x86: the general registers in
//! iced's order, `rip`, the flags as bytes (cf, pf, af, zf, sf, df, of), and the vector registers:
//! `xmm0..xmm15` in the first sixteen slots, the upper halves of `ymm0..ymm15` in the sixteen
//! above them. The driver's C `struct State` has the same layout. On ARM64 the same four fields hold `x0..x30`
//! and `sp` (32 slots), `pc`, the flags as bytes (n, z, c, v) and `v0..v31`; on 32-bit ARM the
//! x86 shape holds `r0..r15`, the flags as ARM64 keeps them and `q0..q15`. On MIPS the four
//! fields hold `$0..$31` at 64 bits (MIPS III's width, every 32-bit result sign-extended into
//! it) with `hi` and `lo` after them as `$32` and `$33`, `pc`, the FPU's eight condition codes as
//! bytes and `$f0..$f31`, and six follow: the FPU's control and status register, the 64
//! registers of the `PlayStation`'s GTE, the upper halves of the R5900's 128-bit registers, the
//! R5900 FPU's accumulator, the 128 single registers of the PSP's VFPU and its condition bits.

use ra_asm_arch::state::{self as layout, StateSpec};
use ra_asm_ir::{AtomicOp, BinOp, BlockId, CallConv, Callee, Cast, Declaration, FnType, FunctionBuilder, ICond, Operand, Type, ValueId};
use ra_asm_x86::iced_x86::Register as R;

pub const STATE_TYPE: &str = "State";

/// The instruction set a state belongs to: which registers its slots hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Isa {
    #[default]
    X86,
    Arm64,
    /// 32-bit ARM in Thumb-2: `r0..r15` in the first 16 general slots, N, Z, C and V in the
    /// first four flag bytes, as on ARM64.
    Arm32,
    /// MIPS under o32: `$0..$31` in the 32 general slots, the FPU with the 32-bit registers of
    /// `Status.FR = 0`, where a double is an even register and the odd one after it.
    Mips,
    /// MIPS under n32: the state of [`Isa::Mips`] with the 64-bit FPU registers of `Status.FR = 1`.
    MipsN32,
}

impl Isa {
    pub fn is_mips(self) -> bool {
        matches!(self, Self::Mips | Self::MipsN32)
    }
}

/// `hi` and `lo` as the general slots past `$31`: a promoted or raised function keeps them in
/// values as it does a register.
pub const MIPS_HI: u8 = 32;
pub const MIPS_LO: u8 = 33;
/// The scalar field of the MIPS `%State` after the four every state has, by index.
pub const MIPS_FCSR: i64 = 4;
/// The field of the R5900's upper halves: bits 64 to 127 of general register `n` in slot `n`,
/// `hi1` and `lo1` (the upper halves of `hi` and `lo`) in [`MIPS_HI`] and [`MIPS_LO`].
pub const MIPS_UPPER: i64 = 5;
/// The field of the R5900 FPU's accumulator, a single the `adda`, `mula`, `madd` and `msub`
/// families read and write.
pub const MIPS_ACC: i64 = 6;
/// The field of the PSP's VFPU: its 128 single registers, register `n` of an instruction's
/// seven-bit field in slot `n`, so a matrix, a column and a row are views of the same singles.
pub const MIPS_VFPU: i64 = 7;
/// The field of the VFPU's condition bits: a lane's result of `vcmp` in bits 0 to 3, any lane's in
/// bit 4 and every lane's in bit 5.
pub const MIPS_VFPU_CC: i64 = 8;

/// The MIPS general registers by their number, in the o32 names (n32 calls `$8..$11` `a4..a7`),
/// then `hi` and `lo`.
pub const MIPS_GPR_NAMES: [&str; 34] = [
    "zero", "at", "v0", "v1", "a0", "a1", "a2", "a3", "t0", "t1", "t2", "t3", "t4", "t5", "t6", "t7", "s0", "s1", "s2", "s3", "s4", "s5",
    "s6", "s7", "t8", "t9", "k0", "k1", "gp", "sp", "fp", "ra", "hi", "lo",
];

/// The `svc` of a Thumb-2 program: `void @ra_svc(ptr %state, i64 imm)`. The runtime reads the
/// call's number and arguments from the registers of the state, as the program's system's
/// convention puts them, and writes the result back.
/// A MIPS `syscall` goes to `i64 @ra_syscall(i64 number, i64 a0, i64 a1, i64 a2, i64 a3)` instead:
/// the number and the four argument registers by value, the kernel's result back, negative with
/// the error number on a failure; the lift writes `v0` and `a3` from it, so a raised function
/// keeps its registers in values across the call.
pub fn declare_svc(module: &mut ra_asm_ir::Module) {
    module.declare(Declaration {
        name: "ra_svc".into(),
        ty: FnType::new(Type::Void, vec![Type::Ptr, Type::I64]),
        conv: CallConv::C,
        attrs: vec![],
    });
    module.declare(Declaration { name: "ra_syscall".into(), ty: syscall_type(), conv: CallConv::C, attrs: vec![] });
}

/// The type of `@ra_syscall`.
pub fn syscall_type() -> FnType {
    FnType::new(Type::I64, vec![Type::I64; 5])
}

/// The commands of the `PlayStation`'s GTE by the low six bits of a `cop2` word, as the runtime
/// names them: `ra_gte_<name>`.
const GTE_COMMANDS: [(u32, &str); 22] = [
    (0x01, "rtps"),
    (0x06, "nclip"),
    (0x0c, "op"),
    (0x10, "dpcs"),
    (0x11, "intpl"),
    (0x12, "mvmva"),
    (0x13, "ncds"),
    (0x14, "cdp"),
    (0x16, "ncdt"),
    (0x1b, "nccs"),
    (0x1c, "cc"),
    (0x1e, "ncs"),
    (0x20, "nct"),
    (0x28, "sqr"),
    (0x29, "dcpl"),
    (0x2a, "dpct"),
    (0x2d, "avsz3"),
    (0x2e, "avsz4"),
    (0x30, "rtpt"),
    (0x3d, "gpf"),
    (0x3e, "gpl"),
    (0x3f, "ncct"),
];

/// The runtime function of the GTE command `funct`, when it is one.
pub fn gte_command(funct: u32) -> Option<String> {
    GTE_COMMANDS.iter().find(|(f, _)| *f == funct).map(|(_, name)| format!("ra_gte_{name}"))
}

/// The type of a GTE command: `void (ptr %gte, i64 command)`, the [`PSX_GTE`] registers and the
/// whole command word, whose `sf`, `lm` and matrix fields the command reads.
pub fn gte_type() -> FnType {
    FnType::new(Type::Void, vec![Type::Ptr, Type::I64])
}

/// The GTE's registers, 32 bits each: data register `n` in slot `n`, control register `n` in
/// slot `32 + n`. A global of the module, as the one coprocessor of the machine is: a function at
/// the ABI level takes no coprocessor from its caller.
pub const PSX_GTE: &str = "ra_gte";

/// Where slot `slot` of the [`PSX_GTE`] registers is.
pub fn gte_slot(slot: u32) -> ra_asm_ir::Operand {
    ra_asm_ir::Operand::Address { global: PSX_GTE.to_owned(), offset: i64::from(slot) * 4 }
}

/// The `gp` of a `PlayStation` program. The start routine sets it and every function addresses the
/// small data through it, but a function at the ABI level takes no `gp` from its caller: a global
/// holds the register for all of them, as the one register of the machine does.
pub const PSX_GP: &str = "ra_gp";

/// The runtime of a `PlayStation` program: a BIOS function by its table's name, `ra_bios_<name>`
/// with the prototype [`crate::raise::prototypes::psx_bios`] gives it, each GTE command, and the
/// [`PSX_GP`] and [`PSX_GTE`] globals.
pub fn declare_psx(module: &mut ra_asm_ir::Module) {
    module.globals.push(ra_asm_ir::Global {
        name: PSX_GTE.to_owned(),
        ty: Type::array(64, Type::I32),
        init: Some(ra_asm_ir::Const::Zero),
        constant: false,
        linkage: ra_asm_ir::Linkage::External,
        align: 4,
        address: None,
    });
    module.globals.push(ra_asm_ir::Global {
        name: PSX_GP.to_owned(),
        ty: Type::I64,
        init: Some(ra_asm_ir::Const::Int(0)),
        constant: false,
        linkage: ra_asm_ir::Linkage::External,
        align: 8,
        address: None,
    });
    for call in crate::raise::prototypes::psx_bios_calls() {
        module.declare(Declaration { name: call.symbol(), ty: call.fn_type(), conv: CallConv::C, attrs: vec![] });
    }
    for (_, name) in GTE_COMMANDS {
        module.declare(Declaration { name: format!("ra_gte_{name}"), ty: gte_type(), conv: CallConv::C, attrs: vec![] });
    }
    // The R3000's coprocessor 0, which the kernel's code moves: the registers as the VR4300's are.
    declare_n64(module);
    module.declare(Declaration { name: PSX_EXCEPTION.to_owned(), ty: psx_exception_type(), conv: CallConv::C, attrs: vec![] });
    module.declare(Declaration { name: PSX_STATUS.to_owned(), ty: psx_status_type(), conv: CallConv::C, attrs: vec![] });
    module.declare(Declaration { name: PSX_RETURN.to_owned(), ty: psx_return_type(), conv: CallConv::C, attrs: vec![] });
}

/// `ra_psx_exception(state, code, address)`: the exception `code` (the processor's `ExcCode`) at
/// the instruction at `address` of a program that runs over the console's own kernel, which
/// takes it at its vector as the processor enters it and comes back where its handler returns.
/// With the ninth bit of `code` set the instruction is in the delay slot of the branch at
/// `address`.
pub const PSX_EXCEPTION: &str = "ra_psx_exception";

pub fn psx_exception_type() -> FnType {
    FnType::new(Type::Void, vec![Type::Ptr, Type::I64, Type::I64])
}

/// `ra_psx_return(state, target)`: a kernel's jump to `target` with `rfe` in its delay slot, the
/// return of an exception's handler. Where the exception was taken, the code there goes on; any
/// other address (another thread's) is where the processor goes on, and the frames of the code
/// the exception was taken in are left as they stand. The call does not return.
pub const PSX_RETURN: &str = "ra_psx_return";

pub fn psx_return_type() -> FnType {
    FnType::new(Type::Void, vec![Type::Ptr, Type::I64])
}

/// `ra_psx_status(state)`: the status register of coprocessor 0 was written. The run reads what
/// the write changes for the machine: the data cache cut off from the memory, in which the
/// stores that flush it reach no memory, and an interrupt the write lets through.
pub const PSX_STATUS: &str = "ra_psx_status";

pub fn psx_status_type() -> FnType {
    FnType::new(Type::Void, vec![Type::Ptr])
}

/// The VR4300's coprocessor 0 registers, register `n` in the `n`th 64-bit slot: a global of the
/// module, as the one coprocessor of the machine is, which `mfc0` and `mtc0` move and nothing else
/// reads, so a port needs no runtime for it.
pub const N64_COP0: &str = "ra_cop0";

/// The globals of a Nintendo 64 program: the [`N64_COP0`] registers.
pub fn declare_n64(module: &mut ra_asm_ir::Module) {
    module.globals.push(ra_asm_ir::Global {
        name: N64_COP0.to_owned(),
        ty: Type::array(32, Type::I64),
        init: Some(ra_asm_ir::Const::Zero),
        constant: false,
        linkage: ra_asm_ir::Linkage::External,
        align: 8,
        address: None,
    });
}

/// The arithmetic of the R5900's FPU, `ra_f32_ftz_<name>`, by its operand count: on the bits of
/// its single registers, denormals as zeros, exponent 255 as a number, each result rounded toward
/// zero, clamped to the largest magnitude and flushed below the smallest normal, as IEEE's is not.
const EE_FPU: [(&str, usize); 8] =
    [("add", 2), ("sub", 2), ("mul", 2), ("div", 2), ("rsqrt", 2), ("sqrt", 1), ("cvt_s_w", 1), ("cvt_w_s", 1)];

/// The type of an R5900 FPU function of `operands` singles: `i32 (i32, ...)`.
pub fn ee_fpu_type(operands: usize) -> FnType {
    FnType::new(Type::I32, vec![Type::I32; operands])
}

/// The R5900 FPU's functions, `ra_f32_ftz_*`, which the vector units' FMACs call as well.
pub fn declare_ee_fpu(module: &mut ra_asm_ir::Module) {
    for (name, operands) in EE_FPU {
        module.declare(Declaration { name: format!("ra_f32_ftz_{name}"), ty: ee_fpu_type(operands), conv: CallConv::C, attrs: vec![] });
    }
}

/// The runtime of a `PlayStation 2` program: the R5900 FPU's functions, and each EE kernel
/// function by its name in the kernel's table, `ra_ee_<name>` with the prototype
/// [`crate::raise::prototypes::ee_syscall`] gives it.
pub fn declare_ps2(module: &mut ra_asm_ir::Module) {
    declare_ee_fpu(module);
    for call in crate::raise::prototypes::ee_syscalls() {
        module.declare(Declaration { name: call.symbol(), ty: call.fn_type(), conv: CallConv::C, attrs: vec![] });
    }
}

/// The fields of the x86 `%State`.
pub fn state_fields() -> Vec<Type> {
    state_fields_for(Isa::X86)
}

impl Isa {
    /// The layout of the state of the instruction set. The driver's `struct State` and the host's
    /// `STATE_SIZE` follow the x86 one.
    pub fn spec(self) -> &'static StateSpec {
        match self {
            Self::X86 => &layout::X86,
            Self::Arm64 => &layout::ARM64,
            Self::Arm32 => &layout::ARM32,
            Self::Mips | Self::MipsN32 => &layout::MIPS,
        }
    }
}

/// The fields of `%State` for `isa`.
pub fn state_fields_for(isa: Isa) -> Vec<Type> {
    state_type_of(isa.spec())
}

/// The IR fields of the state `spec` lays out.
pub fn state_type_of(spec: &StateSpec) -> Vec<Type> {
    spec.fields
        .iter()
        .map(|f| match f.count {
            Some(n) => Type::array(u64::from(n), Type::Int(f.bits)),
            None => Type::Int(f.bits),
        })
        .collect()
}

/// The flags, by their byte index in the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    Cf = 0,
    Pf = 1,
    Af = 2,
    Zf = 3,
    Sf = 4,
    Df = 5,
    Of = 6,
}

/// The signature of an import adapter: four register arguments and four stack slots, as integers.
pub fn import_adapter_type() -> FnType {
    FnType::new(Type::I64, vec![Type::I64; 8])
}

/// Declares the runtime functions the lifted code calls.
pub fn declare_runtime(module: &mut ra_asm_ir::Module) {
    let void = |params: Vec<Type>| FnType::new(Type::Void, params);
    module.declare(Declaration {
        name: "ra_trap".into(),
        ty: void(vec![Type::I64, Type::I64]),
        conv: CallConv::C,
        attrs: vec!["noreturn".into()],
    });
    module.declare(Declaration { name: "ra_call".into(), ty: void(vec![Type::Ptr, Type::I64]), conv: CallConv::C, attrs: vec![] });
    // What the x86 function a pointer named popped: an import's pops from the binding table, else
    // what the universal entry point recorded in `ra_pops`.
    module.declare(Declaration {
        name: "ra_pops_of".into(),
        ty: FnType::new(Type::I64, vec![Type::I64]),
        conv: CallConv::C,
        attrs: vec![],
    });
    // The processor's own answers, from the host: `cpuid` into four dwords, `xgetbv`, `rdtsc`.
    module.declare(Declaration {
        name: "ra_cpuid".into(),
        ty: void(vec![Type::I64, Type::I64, Type::Ptr]),
        conv: CallConv::C,
        attrs: vec![],
    });
    module.declare(Declaration { name: "ra_xgetbv".into(), ty: FnType::new(Type::I64, vec![Type::I64]), conv: CallConv::C, attrs: vec![] });
    module.declare(Declaration { name: "ra_rdtsc".into(), ty: FnType::new(Type::I64, vec![]), conv: CallConv::C, attrs: vec![] });
    // An `alloca` of a size the run knows: the stack pointer to go on with, given the one the
    // code has, the bytes asked for and the alignment. The runtime of a raised port makes the
    // room on the host's stack; where the stack is the machine's own it is `(sp - n) & -align`.
    module.declare(Declaration {
        name: "ra_stack_probe".into(),
        ty: FnType::new(Type::I64, vec![Type::I64, Type::I64, Type::I64]),
        conv: CallConv::C,
        attrs: vec![],
    });
    // `pause`, the spin-wait hint: the host's own, so a loop of them takes the time it took.
    module.declare(Declaration { name: "ra_pause".into(), ty: FnType::new(Type::I64, vec![]), conv: CallConv::C, attrs: vec![] });
    // The thread environment block of the calling thread, as an address: the runtime keeps one
    // per thread and attaches a thread on its first access.
    module.declare(Declaration { name: "ra_teb".into(), ty: FnType::new(Type::I64, vec![]), conv: CallConv::C, attrs: vec![] });
    // The software f80 of the exact x87 unit, over pointers to 16-byte slots; the control word
    // goes along where the rounding or the precision matters.
    let (p, i, d) = (Type::Ptr, Type::I64, Type::Double);
    let f80 = [
        ("ra_f80_load", Type::Void, vec![p.clone(), p.clone()]),
        ("ra_f80_store", Type::Void, vec![p.clone(), p.clone()]),
        ("ra_f80_pack", Type::Void, vec![p.clone(), i.clone(), i.clone()]),
        ("ra_f80_from_f64", Type::Void, vec![p.clone(), d.clone()]),
        // The double unit's `fstp tbyte`: the ten bytes of the 80-bit form of a double.
        ("ra_f80_store_f64", Type::Void, vec![p.clone(), d.clone()]),
        ("ra_f80_from_i64", Type::Void, vec![p.clone(), i.clone()]),
        ("ra_f80_to_f64", d, vec![p.clone(), i.clone()]),
        ("ra_f80_to_f32", Type::Float, vec![p.clone(), i.clone()]),
        ("ra_f80_to_int", i.clone(), vec![p.clone(), i.clone(), i.clone()]),
        ("ra_f80_add", Type::Void, vec![p.clone(), p.clone(), p.clone(), i.clone()]),
        ("ra_f80_sub", Type::Void, vec![p.clone(), p.clone(), p.clone(), i.clone()]),
        ("ra_f80_mul", Type::Void, vec![p.clone(), p.clone(), p.clone(), i.clone()]),
        ("ra_f80_div", Type::Void, vec![p.clone(), p.clone(), p.clone(), i.clone()]),
        ("ra_f80_sqrt", Type::Void, vec![p.clone(), p.clone(), i.clone()]),
        ("ra_f80_rndint", Type::Void, vec![p.clone(), p.clone(), i.clone()]),
        ("ra_f80_chs", Type::Void, vec![p.clone(), p.clone()]),
        ("ra_f80_abs", Type::Void, vec![p.clone(), p.clone()]),
        ("ra_f80_cmp", i, vec![p.clone(), p]),
    ];
    for (name, ret, params) in f80 {
        module.declare(Declaration { name: name.into(), ty: FnType::new(ret, params), conv: CallConv::C, attrs: vec![] });
    }
    module.declare(Declaration {
        name: "ra_unsupported".into(),
        ty: void(vec![Type::I64, Type::I64]),
        conv: CallConv::C,
        attrs: vec!["noreturn".into()],
    });
    module.declare(Declaration {
        name: "llvm.ctpop.i8".into(),
        ty: FnType::new(Type::I8, vec![Type::I8]),
        conv: CallConv::C,
        attrs: vec![],
    });
    for bits in [8u16, 16, 32, 64] {
        let t = Type::Int(bits);
        module.declare(Declaration {
            name: format!("llvm.fshl.i{bits}"),
            ty: FnType::new(t.clone(), vec![t.clone(), t.clone(), t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.fshr.i{bits}"),
            ty: FnType::new(t.clone(), vec![t.clone(), t.clone(), t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.bswap.i{bits}"),
            ty: FnType::new(t.clone(), vec![t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.ctpop.i{bits}"),
            ty: FnType::new(t.clone(), vec![t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.cttz.i{bits}"),
            ty: FnType::new(t.clone(), vec![t.clone(), Type::I1]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.ctlz.i{bits}"),
            ty: FnType::new(t.clone(), vec![t.clone(), Type::I1]),
            conv: CallConv::C,
            attrs: vec![],
        });
    }
    for (name, t) in [("f32", Type::Float), ("f64", Type::Double)] {
        module.declare(Declaration {
            name: format!("llvm.sqrt.{name}"),
            ty: FnType::new(t.clone(), vec![t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.fabs.{name}"),
            ty: FnType::new(t.clone(), vec![t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.minnum.{name}"),
            ty: FnType::new(t.clone(), vec![t.clone(), t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.maxnum.{name}"),
            ty: FnType::new(t.clone(), vec![t.clone(), t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.trunc.{name}"),
            ty: FnType::new(t.clone(), vec![t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.floor.{name}"),
            ty: FnType::new(t.clone(), vec![t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.ceil.{name}"),
            ty: FnType::new(t.clone(), vec![t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        module.declare(Declaration {
            name: format!("llvm.rint.{name}"),
            ty: FnType::new(t.clone(), vec![t.clone()]),
            conv: CallConv::C,
            attrs: vec![],
        });
        // The x87's `fsincos`.
        for function in ["sin", "cos"] {
            module.declare(Declaration {
                name: format!("llvm.{function}.{name}"),
                ty: FnType::new(t.clone(), vec![t.clone()]),
                conv: CallConv::C,
                attrs: vec![],
            });
        }
    }
}

/// The globals the lifted code keeps beside the state: the x87 stack with its top, control word
/// and status word (the x87 state crosses calls, since a 32-bit function returns a double in
/// `st(0)`, so it lives in globals rather than in `%State`, and survives every raising pass as
/// plain memory). The thread environment block of an `fs:` or `gs:` access is the runtime's, one
/// per thread, reached through `ra_teb()`.
/// Declares the C++ exception protocol of the runtime, for a program that throws: see [`crate::eh`].
pub fn declare_eh_runtime(module: &mut ra_asm_ir::Module) {
    let i = Type::I64;
    let functions = [
        ("ra_eh_throw", Type::Void, vec![i.clone(), i.clone()]),
        ("ra_eh_throw_type", Type::Void, vec![i.clone(), i.clone(), i.clone()]),
        ("ra_eh_rethrow", Type::Void, vec![]),
        ("ra_eh_pending", i.clone(), vec![]),
        ("ra_eh_catches", i.clone(), vec![i.clone(), i.clone(), i]),
        ("ra_eh_enter_catch", Type::Void, vec![]),
        ("ra_eh_leave_catch", Type::Void, vec![]),
        ("ra_eh_suspend", Type::Void, vec![]),
        ("ra_eh_resume", Type::Void, vec![]),
    ];
    for (name, ret, params) in functions {
        module.declare(Declaration { name: name.into(), ty: FnType::new(ret, params), conv: CallConv::C, attrs: vec![] });
    }
}

/// Declares what the x86 frame handler's protocol adds: the thrown object's destructor and the
/// copy constructor of each of its catchable types, and at the end of a catch the object and the
/// destructor to run on it when the end of the catch ends the exception; and the `esp` the handler
/// saved when a catch began, which an unwind out of the catch puts back under the registration.
pub fn declare_eh32_runtime(module: &mut ra_asm_ir::Module) {
    let i = Type::I64;
    let functions = [
        ("ra_eh_throw_destructor", Type::Void, vec![i.clone()]),
        ("ra_eh_throw_copy", Type::Void, vec![i.clone()]),
        ("ra_eh_ending_object", i.clone(), vec![]),
        ("ra_eh_ending_destructor", i.clone(), vec![]),
        ("ra_eh_catch_sp", Type::Void, vec![i.clone()]),
        ("ra_eh_ending_sp", i, vec![]),
    ];
    for (name, ret, params) in functions {
        module.declare(Declaration { name: name.into(), ty: FnType::new(ret, params), conv: CallConv::C, attrs: vec![] });
    }
}

/// Declares the structured exception protocol of the runtime, for a 32-bit program with a
/// `__try`: see [`crate::eh`].
pub fn declare_seh_runtime(module: &mut ra_asm_ir::Module) {
    let i = Type::I64;
    let functions = [
        ("ra_eh_probe", i.clone(), vec![i.clone(), i.clone(), i.clone(), i.clone()]),
        ("ra_eh_raise", Type::Void, vec![i.clone(), i.clone(), i.clone(), i.clone(), i.clone()]),
        ("ra_eh_pointers", i.clone(), vec![]),
        ("ra_eh_handled", Type::Void, vec![]),
        ("ra_eh_unhandled", Type::Void, vec![i]),
    ];
    for (name, ret, params) in functions {
        module.declare(Declaration { name: name.into(), ty: FnType::new(ret, params), conv: CallConv::C, attrs: vec![] });
    }
}

pub const X87_STACK: &str = "ra_x87_stack";
pub const X87_TOP: &str = "ra_x87_top";
pub const X87_CW: &str = "ra_x87_cw";
pub const X87_SW: &str = "ra_x87_sw";
/// The scratch slots of the exact unit: a memory operand or a constant, converted for one
/// instruction.
pub const X87_TMP: &str = "ra_x87_tmp";
/// The image's thread-local storage, for the runtime's per-thread block: the template's start and
/// end, `__tls_index`, the zero fill and the callbacks array, five words that are zero for an image
/// without a TLS directory. Standalone, an address is a byte of a data global; otherwise it is the
/// address in the mapped image.
pub const TLS: &str = "ra_tls";
/// What the x86 function last called through a pointer popped beyond its return address: its
/// universal entry point writes it, the caller reads it to find its stack pointer again.
pub const POPS: &str = "ra_pops";
/// The stack dwords of an x86 call through a pointer past the six the universal call carries, a
/// word each: the caller writes them before the call, and the callee's universal entry point reads
/// the ones its function takes. Defined by the raise, for a program where a function a pointer may
/// reach takes more than six.
pub const ARGS: &str = "ra_args";
/// The binding table of a 32-bit program's imports, for the pops of a native function reached
/// through a pointer the lift could not resolve: three words per import, the linked symbol's
/// address, the address of its DLL and name in [`IMPORT_NAMES`] (zero for a function of the host's
/// runtime) and the bytes a call to it pops, then a zero triple. The runtime matches a callee by
/// the linked address or by the DLL's own address of the name, which a bound slot or
/// `GetProcAddress` hands the program.
pub const IMPORT_POPS: &str = "ra_import_pops";
/// The names of [`IMPORT_POPS`]: `dll\0name\0` runs, a name by ordinal as `#n`.
pub const IMPORT_NAMES: &str = "ra_import_names";
/// The pops of the functions a 32-bit program may ask `GetProcAddress` for by name: for each name
/// of a `stdcall` function that the bytes of the image spell, `name\0` and the bytes a call pops as
/// a little-endian `u16`, then an empty name. The runtime's `GetProcAddress` writes them into the
/// trampoline it hands out; a name the table lacks leaves the pops to the binding table.
pub const PROC_POPS: &str = "ra_proc_pops";
/// The kinds of the OpenGL functions a 32-bit program may ask its context for by name, for the
/// runtime's `ra_x86_sdl_gl_get_proc_address` and `ra_x86_wgl_get_proc_address`: `name\0`, then
/// the letters of its result and parameters and a zero, for each; then an empty name. Empty for
/// a program that imports neither.
pub const PROC_KINDS: &str = "ra_proc_kinds";
/// The SSE control and status register as the program sets it with `ldmxcsr` and reads it with
/// `stmxcsr`; the host's own arithmetic keeps its default rounding.
pub const MXCSR: &str = "ra_mxcsr";
/// The image-relative fields of a 64-bit program's RTTI in a standalone port, whose sections the
/// linker places apart: for each field its address and the address it names, both bytes of data
/// globals, then a zero pair. The runtime's startup rewrites each field to the offset of what it
/// names from the port's own module, the base the host's `__RTtypeid`, `__RTDynamicCast` and
/// `__RTCastToVoid` take from the locator.
pub const RTTI: &str = "ra_rtti";
/// The host functions that read the RTTI by its image-relative fields.
pub const RTTI_READERS: [&str; 3] = ["__RTtypeid", "__RTDynamicCast", "__RTCastToVoid"];

pub fn define_runtime_globals(module: &mut ra_asm_ir::Module, x87: crate::x87::X87Unit) {
    use ra_asm_ir::{Const, Global, Linkage};
    let global = |name: &str, ty: Type, init: Const, align: u32| Global {
        name: name.to_owned(),
        ty,
        init: Some(init),
        constant: false,
        linkage: Linkage::External,
        align,
        address: None,
    };
    // The exact unit's stack is eight 16-byte slots, each an 80-bit value and its padding.
    if x87 == crate::x87::X87Unit::Exact {
        module.globals.push(global(X87_STACK, Type::array(8 * crate::x87::F80_SLOT, Type::I8), Const::Zero, 16));
        module.globals.push(global(X87_TMP, Type::array(2 * crate::x87::F80_SLOT, Type::I8), Const::Zero, 16));
    } else {
        module.globals.push(global(X87_STACK, Type::array(8, Type::Double), Const::Zero, 16));
    }
    module.globals.push(global(X87_TOP, Type::I64, Const::Int(0), 8));
    // Windows starts a thread with the precision control at 53 bits, not the unit's own 64.
    module.globals.push(global(X87_CW, Type::I16, Const::Int(0x027f), 2));
    module.globals.push(global(X87_SW, Type::I16, Const::Int(0), 2));
    module.globals.push(global(POPS, Type::I64, Const::Int(0), 8));
    module.globals.push(global(MXCSR, Type::I32, Const::Int(0x1f80), 4));
}

/// The [`ARGS`] global, with room for `dwords` stack dwords: bytes, eight a dword, the shape of
/// zeroed storage every backend declares (as [`X87_TMP`] is).
pub fn args_global(dwords: usize) -> ra_asm_ir::Global {
    ra_asm_ir::Global {
        name: ARGS.to_owned(),
        ty: Type::array(8 * dwords as u64, Type::I8),
        init: Some(ra_asm_ir::Const::Zero),
        constant: false,
        linkage: ra_asm_ir::Linkage::External,
        align: 16,
        address: None,
    }
}

/// The [`TLS`] descriptor of `image` for `lifted`.
pub fn tls_global(image: &ra_asm_pe::Image, lifted: &crate::Lifted) -> ra_asm_ir::Global {
    use ra_asm_ir::{Const, Global, Linkage};
    let word = |va: u64| match lifted.standalone.then(|| lifted.address_of(va)).flatten() {
        Some(Operand::Address { global, offset }) if va != 0 => Const::Address { global, offset },
        _ => Const::Int(i128::from(va)),
    };
    let items = match &image.tls {
        Some(tls) => {
            vec![
                word(tls.start_va),
                word(tls.end_va),
                word(tls.index_va),
                Const::Int(i128::from(tls.zero_fill_size)),
                word(tls.callbacks_va),
            ]
        }
        None => vec![Const::Int(0); 5],
    };
    Global {
        name: TLS.to_owned(),
        ty: Type::Packed(vec![Type::I64; 5]),
        init: Some(Const::Struct(items)),
        constant: true,
        linkage: Linkage::External,
        align: 16,
        address: None,
    }
}

/// The [`RTTI`] table of `images` for a standalone `lifted`: none when the lift is not standalone,
/// the images hold no x64 RTTI, or the program calls none of the [`RTTI_READERS`]. A field or a
/// target outside the data globals stays out of the table.
pub fn rtti_global(images: &[&ra_asm_pe::Image], lifted: &crate::Lifted) -> Option<ra_asm_ir::Global> {
    use ra_asm_ir::{Const, Global, Linkage};
    if !lifted.standalone || !RTTI_READERS.iter().any(|r| lifted.imports.contains(*r)) {
        return None;
    }
    let data = |va: u64| {
        let section = lifted.data.iter().find(|s| va >= s.start && va < s.end)?;
        Some(Const::Address { global: section.global.clone(), offset: (va - section.start) as i64 })
    };
    let mut items: Vec<Const> = Vec::new();
    for image in images {
        for (field, target) in ra_asm_analysis::rtti::relative_fields(image) {
            if let (Some(field), Some(target)) = (data(field), data(target)) {
                items.extend([field, target]);
            }
        }
    }
    if items.is_empty() {
        return None;
    }
    items.extend([Const::Int(0), Const::Int(0)]);
    Some(Global {
        name: RTTI.to_owned(),
        ty: Type::Packed(vec![Type::I64; items.len()]),
        init: Some(Const::Struct(items)),
        constant: true,
        linkage: Linkage::External,
        align: 16,
        address: None,
    })
}

/// The [`IMPORT_POPS`] table and its [`IMPORT_NAMES`] for the imports `lifted` calls, from the
/// import tables of `images` (the program's and its second images').
pub fn import_pops_globals(images: &[&ra_asm_pe::Image], lifted: &crate::Lifted) -> [ra_asm_ir::Global; 2] {
    use ra_asm_ir::{Const, Global, Linkage};
    let mut names: Vec<u8> = Vec::new();
    let mut items: Vec<Const> = Vec::new();
    for symbol in &lifted.imports {
        // A function of the host's runtime (a shim, a linked-in C runtime's) is no DLL's export
        // and pops nothing; an import of unknown arity is taken as cdecl, as its direct call is.
        let import = images.iter().flat_map(|i| &i.imports).find(|i| crate::x86::import_symbol(&i.display_name()) == *symbol);
        let (name, pops) = match import {
            Some(import) => {
                let offset = names.len() as i64;
                names.extend_from_slice(import.dll.as_bytes());
                names.push(0);
                match (&import.name, import.ordinal) {
                    (Some(n), _) => names.extend_from_slice(n.as_bytes()),
                    (None, Some(ordinal)) => names.extend_from_slice(format!("#{ordinal}").as_bytes()),
                    (None, None) => {}
                }
                names.push(0);
                let pops = crate::raise::prototypes::stdcall_pops(&import.dll, symbol).unwrap_or(0);
                (Const::Address { global: IMPORT_NAMES.to_owned(), offset }, pops)
            }
            None => (Const::Int(0), 0),
        };
        items.extend([Const::Address { global: symbol.clone(), offset: 0 }, name, Const::Int(i128::from(pops))]);
    }
    items.extend([Const::Int(0), Const::Int(0), Const::Int(0)]);
    if names.is_empty() {
        names.push(0);
    }
    let global = |name: &str, ty: Type, init: Const| Global {
        name: name.to_owned(),
        ty,
        init: Some(init),
        constant: true,
        linkage: Linkage::External,
        align: 16,
        address: None,
    };
    [
        global(IMPORT_POPS, Type::Packed(vec![Type::I64; items.len()]), Const::Struct(items)),
        global(IMPORT_NAMES, Type::array(names.len() as u64, Type::I8), Const::Bytes(names)),
    ]
}

/// The [`PROC_POPS`] table of `images`: empty unless the program calls `GetProcAddress`, whose
/// names are strings of the image (a name it builds at run time is none of them).
pub fn proc_pops_global(images: &[&ra_asm_pe::Image], lifted: &crate::Lifted) -> ra_asm_ir::Global {
    let mut found = std::collections::BTreeMap::new();
    if lifted.imports.contains("ra_x86_get_proc_address") {
        for image in images {
            let bytes = image.mapped();
            let mut start = 0;
            for (i, &b) in bytes.iter().enumerate() {
                if b.is_ascii_alphanumeric() || b == b'_' {
                    continue;
                }
                if b == 0
                    && i - start >= 2
                    && let Ok(name) = std::str::from_utf8(&bytes[start..i])
                    && let Some(pops) = crate::raise::prototypes::proc_pops(name).and_then(|p| u16::try_from(p).ok())
                {
                    found.insert(name.to_owned(), pops);
                }
                start = i + 1;
            }
        }
    }
    let mut table = Vec::new();
    for (name, pops) in found {
        table.extend_from_slice(name.as_bytes());
        table.push(0);
        table.extend_from_slice(&pops.to_le_bytes());
    }
    table.push(0);
    ra_asm_ir::Global {
        name: PROC_POPS.to_owned(),
        ty: Type::array(table.len() as u64, Type::I8),
        init: Some(ra_asm_ir::Const::Bytes(table)),
        constant: true,
        linkage: ra_asm_ir::Linkage::External,
        align: 16,
        address: None,
    }
}

/// The [`PROC_KINDS`] table of `images`, for a program that asks its OpenGL context for functions
/// by name: each name among the images' strings that the OpenGL table knows, with its kinds.
pub fn proc_kinds_global(images: &[&ra_asm_pe::Image]) -> ra_asm_ir::Global {
    let mut found = std::collections::BTreeMap::new();
    for image in images {
        let bytes = image.mapped();
        let mut start = 0;
        for (i, &b) in bytes.iter().enumerate() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                continue;
            }
            if b == 0
                && i - start >= 4
                && let Ok(name) = std::str::from_utf8(&bytes[start..i])
                && let Some(kinds) = crate::raise::prototypes::opengl(name)
            {
                found.insert(name.to_owned(), kinds);
            }
            start = i + 1;
        }
    }
    let mut table = Vec::new();
    for (name, kinds) in found {
        table.extend_from_slice(name.as_bytes());
        table.push(0);
        table.extend_from_slice(kinds.as_bytes());
        table.push(0);
    }
    table.push(0);
    ra_asm_ir::Global {
        name: PROC_KINDS.to_owned(),
        ty: Type::array(table.len() as u64, Type::I8),
        init: Some(ra_asm_ir::Const::Bytes(table)),
        constant: true,
        linkage: ra_asm_ir::Linkage::External,
        align: 16,
        address: None,
    }
}

/// Whether `name` is one of the globals of [`define_runtime_globals`], the [`TLS`] descriptor,
/// the import binding table, the [`RTTI`] table or the [`ARGS`] of a call through a pointer.
pub fn is_runtime_global(name: &str) -> bool {
    matches!(
        name,
        X87_STACK
            | X87_TOP
            | X87_CW
            | X87_SW
            | X87_TMP
            | TLS
            | POPS
            | ARGS
            | MXCSR
            | IMPORT_POPS
            | IMPORT_NAMES
            | PROC_POPS
            | PROC_KINDS
            | RTTI
            | PSX_GP
            | PSX_GTE
            | N64_COP0
    )
}

/// The state helpers over one function under construction.
pub struct Machine<'b> {
    pub b: &'b mut FunctionBuilder,
    pub state: ValueId,
    /// 32 or 64: the address width.
    pub bits: u32,
    /// The x87 unit the lift models.
    pub x87: crate::x87::X87Unit,
    /// Whose registers the slots hold.
    pub isa: Isa,
}

impl Machine<'_> {
    pub fn address_type(&self) -> Type {
        Type::Int(self.bits as u16)
    }

    fn state_ty() -> Type {
        Type::Named(STATE_TYPE.to_owned())
    }

    /// The name of general register slot `number`: iced's order on x86, `x0..x30` and `sp` on ARM64.
    pub fn gpr_name(&self, number: u32) -> &'static str {
        match self.isa {
            Isa::X86 => GPR_NAMES[number as usize],
            Isa::Arm64 => ra_asm_arm64::registers::full_of(number as u16).name,
            Isa::Arm32 => ra_asm_arm64::thumb::register(number as u8).name,
            Isa::Mips | Isa::MipsN32 => MIPS_GPR_NAMES[number as usize],
        }
    }

    /// The pointer to scalar field `index` of the state past the four every state has: [`MIPS_FCSR`].
    pub fn field_ptr(&mut self, block: BlockId, index: i64, hint: &str) -> ValueId {
        self.b.gep(block, Self::state_ty(), self.state, vec![(Type::I32, 0i64.into()), (Type::I32, index.into())], hint)
    }

    // region Pointers into the state

    /// The pointer to the slot of a general register, by its number in the state (rax or x0 is 0).
    pub fn gpr_ptr(&mut self, block: BlockId, number: u32) -> ValueId {
        let hint = format!("p.{}", self.gpr_name(number));
        self.b.gep(
            block,
            Self::state_ty(),
            self.state,
            vec![(Type::I32, 0i64.into()), (Type::I32, 0i64.into()), (Type::I32, i64::from(number).into())],
            &hint,
        )
    }

    pub fn flag_ptr(&mut self, block: BlockId, flag: Flag) -> ValueId {
        self.flag_ptr_at(block, flag as i64, flag_name(flag))
    }

    /// The pointer to flag byte `index` of the state, named `name`.
    pub fn flag_ptr_at(&mut self, block: BlockId, index: i64, name: &str) -> ValueId {
        let hint = format!("p.{name}");
        self.b.gep(
            block,
            Self::state_ty(),
            self.state,
            vec![(Type::I32, 0i64.into()), (Type::I32, 2i64.into()), (Type::I32, index.into())],
            &hint,
        )
    }

    /// The pointer to the upper half of the R5900's general register `number` (`hi1` and `lo1`
    /// past `$31`).
    pub fn upper_ptr(&mut self, block: BlockId, number: u32) -> ValueId {
        let hint = format!("p.{}.u", self.gpr_name(number));
        self.b.gep(
            block,
            Self::state_ty(),
            self.state,
            vec![(Type::I32, 0i64.into()), (Type::I32, MIPS_UPPER.into()), (Type::I32, i64::from(number).into())],
            &hint,
        )
    }

    /// The pointer to VFPU register `number`, a single.
    pub fn vfpu_ptr(&mut self, block: BlockId, number: u32) -> ValueId {
        let hint = format!("p.v{number}");
        self.b.gep(
            block,
            Self::state_ty(),
            self.state,
            vec![(Type::I32, 0i64.into()), (Type::I32, MIPS_VFPU.into()), (Type::I32, i64::from(number).into())],
            &hint,
        )
    }

    /// The pointer to the VFPU's condition bits, which `vcmp` sets and `vcmov` reads.
    pub fn vfpu_cc_ptr(&mut self, block: BlockId) -> ValueId {
        self.b.gep(block, Self::state_ty(), self.state, vec![(Type::I32, 0i64.into()), (Type::I32, MIPS_VFPU_CC.into())], "p.vfpucc")
    }

    pub fn xmm_ptr(&mut self, block: BlockId, number: u32) -> ValueId {
        let hint = match self.isa {
            Isa::X86 => format!("p.xmm{number}"),
            Isa::Arm64 => format!("p.v{number}"),
            Isa::Arm32 => format!("p.q{number}"),
            Isa::Mips | Isa::MipsN32 => format!("p.f{number}"),
        };
        self.b.gep(
            block,
            Self::state_ty(),
            self.state,
            vec![(Type::I32, 0i64.into()), (Type::I32, 3i64.into()), (Type::I32, i64::from(number).into())],
            &hint,
        )
    }

    // endregion

    // region Registers

    /// The value of a general register or a part of one, at its own width.
    pub fn read_gpr(&mut self, block: BlockId, reg: R) -> ValueId {
        let full = reg.full_register();
        let p = self.gpr_ptr(block, full.number() as u32);
        let whole = self.b.load(block, Type::I64, p, GPR_NAMES[full.number()]);
        let bits = (reg.size() * 8) as u16;
        if bits == 64 {
            return whole;
        }
        let shifted = if is_high_byte(reg) { self.b.bin(block, BinOp::LShr, Type::I64, whole, 8i64, "hi") } else { whole };
        self.b.cast(block, Cast::Trunc, Type::I64, shifted, Type::Int(bits), reg_name(reg))
    }

    /// Writes a general register or a part of one. A 32-bit write zero-extends: in 64-bit mode
    /// that is the machine's rule, and in 32-bit mode nothing reads the upper half.
    pub fn write_gpr(&mut self, block: BlockId, reg: R, value: impl Into<Operand>) {
        let full = reg.full_register();
        let p = self.gpr_ptr(block, full.number() as u32);
        let bits = (reg.size() * 8) as u16;
        let value = value.into();
        if bits == 64 {
            self.b.store(block, Type::I64, value, p);
            return;
        }
        let wide = self.b.cast(block, Cast::ZExt, Type::Int(bits), value, Type::I64, &format!("{}.z", reg_name(reg)));
        if bits == 32 {
            self.b.store(block, Type::I64, wide, p);
            return;
        }
        let whole = self.b.load(block, Type::I64, p, GPR_NAMES[full.number()]);
        let (mask, shift) = if is_high_byte(reg) { (!0xff00u64, 8) } else { (!((1u64 << bits) - 1), 0) };
        let kept = self.b.bin(block, BinOp::And, Type::I64, whole, mask as i64, "kept");
        let placed = if shift > 0 { self.b.bin(block, BinOp::Shl, Type::I64, wide, shift, "placed") } else { wide };
        let merged = self.b.bin(block, BinOp::Or, Type::I64, kept, placed, GPR_NAMES[full.number()]);
        self.b.store(block, Type::I64, merged, p);
    }

    /// The stack pointer, at the address width.
    pub fn read_sp(&mut self, block: BlockId) -> ValueId {
        self.read_gpr(block, if self.bits == 64 { R::RSP } else { R::ESP })
    }

    pub fn write_sp(&mut self, block: BlockId, value: impl Into<Operand>) {
        self.write_gpr(block, if self.bits == 64 { R::RSP } else { R::ESP }, value);
    }

    /// Reads a vector register as `ty`, from its low bytes.
    pub fn read_xmm(&mut self, block: BlockId, reg: R, ty: Type) -> ValueId {
        let p = self.xmm_ptr(block, reg.number() as u32);
        self.b.load(block, ty, p, reg_name(reg))
    }

    /// Writes the low bytes of a vector register as `ty`; with `zero_upper`, the rest becomes zero.
    pub fn write_xmm(&mut self, block: BlockId, reg: R, ty: Type, value: impl Into<Operand>, zero_upper: bool) {
        self.write_vreg(block, reg.number() as u32, ty, value, zero_upper);
    }

    /// Reads vector slot `number` as `ty`, from its low bytes: a scalar, a vector of lanes, or the
    /// whole register.
    pub fn read_vreg(&mut self, block: BlockId, number: u32, ty: Type) -> ValueId {
        let p = self.xmm_ptr(block, number);
        let hint = match self.isa {
            Isa::X86 => format!("xmm{number}"),
            Isa::Arm64 => format!("v{number}"),
            Isa::Arm32 => format!("q{number}"),
            Isa::Mips | Isa::MipsN32 => format!("f{number}"),
        };
        self.b.load(block, ty, p, &hint)
    }

    /// Writes the low bytes of vector slot `number` as `ty`; with `zero_upper`, the rest becomes zero.
    pub fn write_vreg(&mut self, block: BlockId, number: u32, ty: Type, value: impl Into<Operand>, zero_upper: bool) {
        let p = self.xmm_ptr(block, number);
        if zero_upper && ty.bits() != Some(128) {
            self.b.store(block, Type::I128, 0i64, p);
        }
        self.b.store(block, ty, value, p);
    }

    // endregion

    // region Flags

    pub fn get_flag(&mut self, block: BlockId, flag: Flag) -> ValueId {
        let p = self.flag_ptr(block, flag);
        let byte = self.b.load(block, Type::I8, p, &format!("{}.b", flag_name(flag)));
        self.b.cast(block, Cast::Trunc, Type::I8, byte, Type::I1, flag_name(flag))
    }

    pub fn set_flag(&mut self, block: BlockId, flag: Flag, value: impl Into<Operand>) {
        let p = self.flag_ptr(block, flag);
        let byte = self.b.cast(block, Cast::ZExt, Type::I1, value, Type::I8, &format!("{}.b", flag_name(flag)));
        self.b.store(block, Type::I8, byte, p);
    }

    pub fn set_flag_const(&mut self, block: BlockId, flag: Flag, value: bool) {
        let p = self.flag_ptr(block, flag);
        self.b.store(block, Type::I8, i64::from(value), p);
    }

    /// ZF, SF and PF from a result.
    pub fn set_result_flags(&mut self, block: BlockId, ty: Type, result: ValueId) {
        let zf = self.b.icmp(block, ICond::Eq, ty.clone(), result, 0i64, "zf");
        self.set_flag(block, Flag::Zf, zf);
        let sf = self.b.icmp(block, ICond::Slt, ty.clone(), result, 0i64, "sf");
        self.set_flag(block, Flag::Sf, sf);
        let low = if ty == Type::I8 { result } else { self.b.cast(block, Cast::Trunc, ty, result, Type::I8, "low") };
        let ones = self
            .b
            .call(
                block,
                FnType::new(Type::I8, vec![Type::I8]),
                Callee::Direct("llvm.ctpop.i8".into()),
                vec![(Type::I8, low.into())],
                "ones",
            )
            .unwrap();
        let odd = self.b.bin(block, BinOp::And, Type::I8, ones, 1i64, "odd");
        let pf = self.b.icmp(block, ICond::Eq, Type::I8, odd, 0i64, "pf");
        self.set_flag(block, Flag::Pf, pf);
    }

    /// AF from the operands and the result of an add or a sub: bit 4 of `lhs ^ rhs ^ result`.
    pub fn set_af(&mut self, block: BlockId, ty: Type, lhs: impl Into<Operand>, rhs: impl Into<Operand>, result: ValueId) {
        let x = self.b.bin(block, BinOp::Xor, ty.clone(), lhs, rhs, "af.x");
        let y = self.b.bin(block, BinOp::Xor, ty.clone(), x, result, "af.y");
        let bit = self.b.bin(block, BinOp::And, ty.clone(), y, 16i64, "af.bit");
        let af = self.b.icmp(block, ICond::Ne, ty, bit, 0i64, "af");
        self.set_flag(block, Flag::Af, af);
    }

    // endregion

    // region Memory

    /// A host pointer for a machine address.
    pub fn ptr(&mut self, block: BlockId, address: impl Into<Operand>, hint: &str) -> ValueId {
        let address = address.into();
        let wide = if self.bits == 64 { address } else { self.b.cast(block, Cast::ZExt, Type::I32, address, Type::I64, "addr.z").into() };
        self.b.cast(block, Cast::IntToPtr, Type::I64, wide, Type::Ptr, hint)
    }

    /// A load from machine memory. The address is what the program computed, aligned to nothing
    /// the lift can know: an unaligned access, so no compiler turns a `movups` into a `movaps`.
    pub fn load(&mut self, block: BlockId, ty: Type, address: impl Into<Operand>, hint: &str) -> ValueId {
        let p = self.ptr(block, address, &format!("{hint}.p"));
        self.b.value(block, ra_asm_ir::Inst::Load { ty, ptr: p.into(), align: 1, volatile: false }, hint)
    }

    /// A store to machine memory, unaligned like [`Self::load`].
    pub fn store(&mut self, block: BlockId, ty: Type, value: impl Into<Operand>, address: impl Into<Operand>) {
        let p = self.ptr(block, address, "st.p");
        self.b.push(block, ra_asm_ir::Inst::Store { ty, value: value.into(), ptr: p.into(), align: 1, volatile: false }, "");
    }

    /// An atomic read-modify-write of the integer at a machine address: a locked instruction, or
    /// an `xchg` with memory; the value is the integer it was.
    pub fn atomic_rmw(
        &mut self,
        block: BlockId,
        op: AtomicOp,
        ty: Type,
        value: impl Into<Operand>,
        address: impl Into<Operand>,
    ) -> ValueId {
        let p = self.ptr(block, address, "at.p");
        self.b.value(block, ra_asm_ir::Inst::AtomicRmw { op, ty, ptr: p.into(), value: value.into() }, "old")
    }

    /// An atomic compare-exchange of the integer at a machine address; the value is the integer
    /// it was, equal to `expected` exactly when `new` replaced it.
    pub fn cmpxchg(
        &mut self,
        block: BlockId,
        ty: Type,
        expected: impl Into<Operand>,
        new: impl Into<Operand>,
        address: impl Into<Operand>,
    ) -> ValueId {
        let p = self.ptr(block, address, "cx.p");
        self.b.value(block, ra_asm_ir::Inst::CmpXchg { ty, ptr: p.into(), expected: expected.into(), new: new.into() }, "old")
    }

    /// Pushes a value of the address width.
    pub fn push(&mut self, block: BlockId, value: impl Into<Operand>) {
        let ty = self.address_type();
        let sp = self.read_sp(block);
        let new_sp = self.b.bin(block, BinOp::Sub, ty.clone(), sp, i64::from(self.bits / 8), "sp");
        self.store(block, ty, value, new_sp);
        self.write_sp(block, new_sp);
    }

    /// Pops a value of the address width.
    pub fn pop(&mut self, block: BlockId, hint: &str) -> ValueId {
        let ty = self.address_type();
        let sp = self.read_sp(block);
        let value = self.load(block, ty.clone(), sp, hint);
        let new_sp = self.b.bin(block, BinOp::Add, ty, sp, i64::from(self.bits / 8), "sp");
        self.write_sp(block, new_sp);
        value
    }

    /// Pushes a value of `ty`, which moves the stack pointer by the type's own size: the 16-bit
    /// push of an operand-size prefix.
    pub fn push_sized(&mut self, block: BlockId, ty: Type, value: impl Into<Operand>) {
        let addr = self.address_type();
        let size = i64::from(ty.bits().unwrap_or(self.bits) / 8);
        let sp = self.read_sp(block);
        let new_sp = self.b.bin(block, BinOp::Sub, addr, sp, size, "sp");
        self.store(block, ty, value, new_sp);
        self.write_sp(block, new_sp);
    }

    /// Pops a value of `ty`, by the type's own size.
    pub fn pop_sized(&mut self, block: BlockId, ty: Type, hint: &str) -> ValueId {
        let addr = self.address_type();
        let size = i64::from(ty.bits().unwrap_or(self.bits) / 8);
        let sp = self.read_sp(block);
        let value = self.load(block, ty, sp, hint);
        let new_sp = self.b.bin(block, BinOp::Add, addr, sp, size, "sp");
        self.write_sp(block, new_sp);
        value
    }

    // endregion

    /// `call @ra_trap(address, code)`, then `unreachable`.
    pub fn trap(&mut self, block: BlockId, address: u64, code: u64) {
        self.b.call(
            block,
            FnType::new(Type::Void, vec![Type::I64, Type::I64]),
            Callee::Direct("ra_trap".into()),
            vec![(Type::I64, address.into()), (Type::I64, code.into())],
            "",
        );
        self.b.terminate(block, ra_asm_ir::Terminator::Unreachable);
    }

    /// `call @ra_unsupported(address, code)`, then `unreachable`: an instruction the lift lacks.
    pub fn unsupported(&mut self, block: BlockId, address: u64, code: u64) {
        self.b.call(
            block,
            FnType::new(Type::Void, vec![Type::I64, Type::I64]),
            Callee::Direct("ra_unsupported".into()),
            vec![(Type::I64, address.into()), (Type::I64, code.into())],
            "",
        );
        self.b.terminate(block, ra_asm_ir::Terminator::Unreachable);
    }

    /// `select cond, a, b` on an integer type.
    pub fn select(
        &mut self,
        block: BlockId,
        cond: impl Into<Operand>,
        ty: Type,
        a: impl Into<Operand>,
        b: impl Into<Operand>,
        hint: &str,
    ) -> ValueId {
        self.b.select(block, cond, ty, a, b, hint)
    }

    /// An intrinsic call with one result.
    pub fn intrinsic(&mut self, block: BlockId, name: &str, ret: Type, args: Vec<(Type, Operand)>, hint: &str) -> ValueId {
        let params = args.iter().map(|(t, _)| t.clone()).collect();
        self.b.call(block, FnType::new(ret, params), Callee::Direct(name.to_owned()), args, hint).unwrap()
    }

    pub fn is_true(&mut self, block: BlockId, ty: Type, value: impl Into<Operand>, hint: &str) -> ValueId {
        self.b.icmp(block, ICond::Ne, ty, value, 0i64, hint)
    }

    pub fn zext(&mut self, block: BlockId, from: Type, value: impl Into<Operand>, to: Type, hint: &str) -> ValueId {
        if from == to {
            return match value.into() {
                Operand::Local(v) => v,
                other => self.b.bin(block, BinOp::Or, to, other, 0i64, hint),
            };
        }
        self.b.cast(block, Cast::ZExt, from, value, to, hint)
    }

    pub fn sext(&mut self, block: BlockId, from: Type, value: impl Into<Operand>, to: Type, hint: &str) -> ValueId {
        if from == to {
            return match value.into() {
                Operand::Local(v) => v,
                other => self.b.bin(block, BinOp::Or, to, other, 0i64, hint),
            };
        }
        self.b.cast(block, Cast::SExt, from, value, to, hint)
    }

    pub fn trunc(&mut self, block: BlockId, from: Type, value: impl Into<Operand>, to: Type, hint: &str) -> ValueId {
        if from == to {
            return match value.into() {
                Operand::Local(v) => v,
                other => self.b.bin(block, BinOp::Or, to, other, 0i64, hint),
            };
        }
        self.b.cast(block, Cast::Trunc, from, value, to, hint)
    }

    /// Materializes an operand as a value: an integer constant by `or` with zero, a floating one by
    /// the bitcast of its bits, since `or` is no operation of a floating type.
    pub fn value_of(&mut self, block: BlockId, ty: Type, operand: impl Into<Operand>) -> ValueId {
        match (operand.into(), &ty) {
            (Operand::Local(v), _) => v,
            (Operand::Float(f), Type::Double) => self.b.cast(block, Cast::Bitcast, Type::I64, f.to_bits() as i64, ty, "c"),
            (Operand::Float(f), Type::Float) => self.b.cast(block, Cast::Bitcast, Type::I32, i64::from((f as f32).to_bits()), ty, "c"),
            (other, _) => self.b.bin(block, BinOp::Or, ty, other, 0i64, "c"),
        }
    }
}

/// iced's general register numbers, in order.
const GPR_NAMES: [&str; 16] =
    ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"];

fn flag_name(flag: Flag) -> &'static str {
    match flag {
        Flag::Cf => "cf",
        Flag::Pf => "pf",
        Flag::Af => "af",
        Flag::Zf => "zf",
        Flag::Sf => "sf",
        Flag::Df => "df",
        Flag::Of => "of",
    }
}

fn is_high_byte(reg: R) -> bool {
    matches!(reg, R::AH | R::BH | R::CH | R::DH)
}

pub fn reg_name(reg: R) -> &'static str {
    ra_asm_x86::registers::name(reg)
}

#[cfg(test)]
mod spec {
    use ra_asm_arch::state::{Field, Role, StateSpec};
    use ra_asm_ir::types::layout as ir_layout;

    use super::*;

    /// The spec's offsets and size, checked against the IR's own layout of the type it derives.
    fn laid_out(spec: &StateSpec) -> (Vec<u64>, u64) {
        let (offsets, size) = ir_layout(&state_type_of(spec), 8).expect("the state has a size");
        assert_eq!((spec.offsets(), spec.size()), (offsets.clone(), size), "{}", spec.machine);
        (offsets, size)
    }

    fn slot_names(isa: Isa, role: Role) -> Vec<&'static str> {
        let (_, field) = isa.spec().field(role).expect("the field");
        (0..field.slots()).filter_map(|i| field.slot_name(i)).collect()
    }

    #[test]
    fn x86_state_is_todays() {
        let fields = state_fields_for(Isa::X86);
        assert_eq!(fields, vec![Type::array(16, Type::I64), Type::I64, Type::array(8, Type::I8), Type::array(32, Type::I128)]);
        assert_eq!(laid_out(Isa::X86.spec()), (vec![0, 128, 136, 144], 656));
        assert_eq!(slot_names(Isa::X86, Role::General), GPR_NAMES);
        let flags = [Flag::Cf, Flag::Pf, Flag::Af, Flag::Zf, Flag::Sf, Flag::Df, Flag::Of];
        assert_eq!(slot_names(Isa::X86, Role::Flags), flags.map(flag_name));
        for flag in flags {
            assert_eq!(Isa::X86.spec().slot_offset(Role::Flags, flag as u32), Some(136 + flag as u64));
        }
        assert_eq!(Isa::X86.spec().slot_offset(Role::Vector, 16), Some(144 + 16 * 16));
    }

    #[test]
    fn arm64_state_is_todays() {
        let fields = state_fields_for(Isa::Arm64);
        assert_eq!(fields, vec![Type::array(32, Type::I64), Type::I64, Type::array(8, Type::I8), Type::array(32, Type::I128)]);
        assert_eq!(laid_out(Isa::Arm64.spec()), (vec![0, 256, 264, 272], 784));
        let decoder: Vec<_> = (0..32).map(|n| ra_asm_arm64::registers::full_of(n).name).collect();
        assert_eq!(slot_names(Isa::Arm64, Role::General), decoder);
        assert_eq!(Isa::Arm64.spec().slot_offset(Role::General, 31), Some(248));
    }

    #[test]
    fn arm32_state_is_todays() {
        let fields = state_fields_for(Isa::Arm32);
        assert_eq!(fields, vec![Type::array(16, Type::I64), Type::I64, Type::array(8, Type::I8), Type::array(16, Type::I128)]);
        assert_eq!(laid_out(Isa::Arm32.spec()), (vec![0, 128, 136, 144], 400));
        let decoder: Vec<_> = (0..16).map(|n| ra_asm_arm64::thumb::register(n).name).collect();
        assert_eq!(slot_names(Isa::Arm32, Role::General), decoder);
    }

    #[test]
    fn mips_state_holds_the_vfpu() {
        let spec = Isa::Mips.spec();
        assert_eq!(Isa::MipsN32.spec(), spec);
        let fields = state_fields_for(Isa::Mips);
        assert_eq!(fields.len(), 9);
        assert_eq!(fields[MIPS_VFPU as usize], Type::array(128, Type::I32));
        assert_eq!(fields[MIPS_VFPU_CC as usize], Type::I32);
        let named: Vec<_> = [MIPS_FCSR, MIPS_UPPER, MIPS_ACC, MIPS_VFPU, MIPS_VFPU_CC].map(|i| spec.fields[i as usize].name).into();
        assert_eq!(named, ["fcsr", "upper", "acc", "vfpu", "vfpucc"]);
        assert_eq!(laid_out(spec), (vec![0, 272, 280, 288, 800, 808, 1080, 1084, 1596], 1600));
        let (index, vfpu) = (MIPS_VFPU as usize, &spec.fields[MIPS_VFPU as usize]);
        assert_eq!((vfpu.role, vfpu.slots(), vfpu.slot_size()), (Role::Extra, 128, 4));
        assert_eq!(spec.offsets()[index] + vfpu.size(), 1596, "the condition bits follow the VFPU's singles");
    }

    #[test]
    fn wide_registers_lay_out_without_overlap() {
        const WIDE: StateSpec = StateSpec {
            machine: "wide",
            fields: &[
                Field::single("flag", Role::Flags, 8),
                Field::group("q", Role::General, 32, 128),
                Field::single("pc", Role::ProgramCounter, 32),
                Field::group("extra", Role::Extra, 3, 16),
            ],
        };
        let spec = WIDE;
        let (offsets, size) = laid_out(&spec);
        assert_eq!((offsets.clone(), size), (vec![0, 16, 528, 532], 544));
        let mut end = 0;
        for (field, at) in spec.fields.iter().zip(&offsets) {
            assert!(*at >= end, "{} at {at} overlaps the field before, which ends at {end}", field.name);
            assert_eq!(at % field.align(), 0, "{} is not aligned", field.name);
            end = at + field.size();
        }
        assert!(end <= size);
        assert_eq!(spec.slot_offset(Role::General, 31), Some(16 + 31 * 16));
        assert_eq!(spec.slot_offset(Role::General, 32), None);
    }
}
