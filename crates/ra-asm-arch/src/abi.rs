//! A calling convention as data: where the parameters and the results go, what a callee keeps,
//! how the stack is laid out at an entry and where the return address lives. The lift reads a
//! convention from an [`Abi`] value instead of asking which architecture it is, so a second
//! convention is a constant, not a branch in every method.

use Reg::{Gpr, Vector as V};

/// A register of a convention, in the lift's numbering: `Gpr` is the general register of that
/// number (`rax` 0 to `r15` 15 on x86, `x0` to `x30` and `sp` 31 on ARM64, `r0` to `r15` on 32-bit
/// ARM), `Vector` the vector register (`xmm`, `v`, `d`), `X87` a place on the x87 stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Reg {
    Gpr(u8),
    Vector(u8),
    X87(u8),
}

/// Where a call leaves the return address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReturnAddress {
    /// Pushed onto the stack, one pointer wide: the callee's stack pointer points at it.
    Stack,
    /// In a link register, which the callee saves itself when it calls in turn.
    Link(Reg),
}

/// A calling convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Abi {
    pub name: &'static str,
    /// The size of a pointer and of a stack slot, in bytes.
    pub ptr_bytes: u8,
    /// The registers that carry integer and pointer parameters, by position.
    pub int_params: &'static [Reg],
    /// The registers that carry floating-point parameters, by position.
    pub float_params: &'static [Reg],
    /// Whether a parameter's position picks its register across both classes (Win64: the second
    /// parameter is `rdx` or `xmm1`), rather than its place among the parameters of its class.
    pub shared_positions: bool,
    /// The registers of an integer result, the low part first.
    pub int_results: &'static [Reg],
    /// The registers of a floating-point result.
    pub float_results: &'static [Reg],
    /// The registers a callee gives back as it found them.
    pub callee_saved: &'static [Reg],
    /// The alignment of the stack pointer at a call instruction, in bytes.
    pub stack_align: u8,
    /// The bytes a caller reserves above the return address for the callee to spill its register
    /// parameters into (Win64's home space).
    pub home_space: u8,
    pub return_address: ReturnAddress,
    pub stack_pointer: Reg,
    /// The register that addresses the small-data area, on the conventions that have one.
    pub global_pointer: Option<Reg>,
}

impl Abi {
    pub fn is_callee_saved(&self, reg: Reg) -> bool {
        self.callee_saved.contains(&reg)
    }

    /// Whether `reg` carries a parameter of either class.
    pub fn is_param(&self, reg: Reg) -> bool {
        self.int_params.contains(&reg) || self.float_params.contains(&reg)
    }

    /// The bytes a call pushes: the return address on the stack, nothing with a link register.
    pub fn pushed_return(&self) -> u8 {
        match self.return_address {
            ReturnAddress::Stack => self.ptr_bytes,
            ReturnAddress::Link(_) => 0,
        }
    }

    /// Where the first stack parameter is above the callee's entry stack pointer: past the return
    /// address and the home space.
    pub fn first_stack_arg(&self) -> u8 {
        self.pushed_return() + self.home_space
    }

    /// The entry stack pointer's offset past a `stack_align` boundary, which the pushed return
    /// address leaves.
    pub fn entry_misalignment(&self) -> u8 {
        (self.stack_align - self.pushed_return() % self.stack_align) % self.stack_align
    }
}

/// The Microsoft x64 convention: four parameters by position in `rcx`, `rdx`, `r8`, `r9` or
/// `xmm0` to `xmm3`, a 32-byte home space, `rbx`, `rsp`, `rbp`, `rsi`, `rdi`, `r12` to `r15` and
/// `xmm6` to `xmm15` kept.
pub const WIN64: Abi = Abi {
    name: "win64",
    ptr_bytes: 8,
    int_params: &[Gpr(1), Gpr(2), Gpr(8), Gpr(9)],
    float_params: &[V(0), V(1), V(2), V(3)],
    shared_positions: true,
    int_results: &[Gpr(0)],
    float_results: &[V(0)],
    callee_saved: &[
        Gpr(3),
        Gpr(4),
        Gpr(5),
        Gpr(6),
        Gpr(7),
        Gpr(12),
        Gpr(13),
        Gpr(14),
        Gpr(15),
        V(6),
        V(7),
        V(8),
        V(9),
        V(10),
        V(11),
        V(12),
        V(13),
        V(14),
        V(15),
    ],
    stack_align: 16,
    home_space: 32,
    return_address: ReturnAddress::Stack,
    stack_pointer: Gpr(4),
    global_pointer: None,
};

/// The 32-bit x86 conventions as one: the stack carries the parameters (`cdecl`, `stdcall`),
/// `ecx` and `edx` the first integer ones under `fastcall` and `thiscall`, `xmm0` to `xmm2` the
/// first floats under `vectorcall`; `edx:eax` or `st(0)` the result; `ebx`, `esp`, `ebp`, `esi`,
/// `edi` kept.
pub const X86: Abi = Abi {
    name: "x86",
    ptr_bytes: 4,
    int_params: &[Gpr(1), Gpr(2)],
    float_params: &[V(0), V(1), V(2)],
    shared_positions: false,
    int_results: &[Gpr(0), Gpr(2)],
    float_results: &[Reg::X87(0)],
    callee_saved: &[Gpr(3), Gpr(4), Gpr(5), Gpr(6), Gpr(7)],
    stack_align: 16,
    home_space: 0,
    return_address: ReturnAddress::Stack,
    stack_pointer: Gpr(4),
    global_pointer: None,
};

/// The Arm 64-bit procedure call standard: `x0` to `x7` and `v0` to `v7` by their place in each
/// class, `x30` the link register, `x18` to `x29`, `sp` and `v8` to `v15` kept (`x18` is the
/// platform register, which Windows reserves for the TEB).
pub const AAPCS64: Abi = Abi {
    name: "aapcs64",
    ptr_bytes: 8,
    int_params: &[Gpr(0), Gpr(1), Gpr(2), Gpr(3), Gpr(4), Gpr(5), Gpr(6), Gpr(7)],
    float_params: &[V(0), V(1), V(2), V(3), V(4), V(5), V(6), V(7)],
    shared_positions: false,
    int_results: &[Gpr(0), Gpr(1)],
    float_results: &[V(0), V(1), V(2), V(3)],
    callee_saved: &[
        Gpr(18),
        Gpr(19),
        Gpr(20),
        Gpr(21),
        Gpr(22),
        Gpr(23),
        Gpr(24),
        Gpr(25),
        Gpr(26),
        Gpr(27),
        Gpr(28),
        Gpr(29),
        Gpr(31),
        V(8),
        V(9),
        V(10),
        V(11),
        V(12),
        V(13),
        V(14),
        V(15),
    ],
    stack_align: 16,
    home_space: 0,
    return_address: ReturnAddress::Link(Gpr(30)),
    stack_pointer: Gpr(31),
    global_pointer: None,
};

/// The Arm 32-bit procedure call standard, base variant: `r0` to `r3` carry every parameter,
/// floats included, and `r0:r1` the result; `r14` the link register; `r4` to `r11` and `sp` kept.
pub const AAPCS32: Abi = Abi {
    name: "aapcs32",
    ptr_bytes: 4,
    int_params: &[Gpr(0), Gpr(1), Gpr(2), Gpr(3)],
    float_params: &[],
    shared_positions: false,
    int_results: &[Gpr(0), Gpr(1)],
    float_results: &[],
    callee_saved: &[Gpr(4), Gpr(5), Gpr(6), Gpr(7), Gpr(8), Gpr(9), Gpr(10), Gpr(11), Gpr(13)],
    stack_align: 8,
    home_space: 0,
    return_address: ReturnAddress::Link(Gpr(14)),
    stack_pointer: Gpr(13),
    global_pointer: None,
};

/// Every convention the lift knows.
pub const ALL: [Abi; 4] = [WIN64, X86, AAPCS64, AAPCS32];
