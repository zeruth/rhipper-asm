//! The raising passes: from machine IR over `%State` to source-shaped IR.
//!
//! Promotion turns the registers and the flags into SSA values inside each function, guided by
//! liveness, so a flag no branch reads is never computed and a register travels in a value, not
//! through memory. In ABI mode the inferred signatures apply too: parameters, a frame, calls with
//! arguments, a returned value, and no `%State` at all. Simplification and dead-code elimination
//! clean up after either.

pub mod abi;
pub mod conditions;
pub mod dce;
pub mod frame;
pub mod known;
pub mod liveness;
pub mod promote;
pub mod prototypes;
pub mod simplify;
pub mod stack;
pub mod stubs;
pub mod widths;

use std::collections::{BTreeSet, HashMap};

use liveness::{R8, R9, RCX, RDX, Slot};
use ra_asm_ir::{Module, Type};

/// The machine the IR came from, by its pointer width, which sets the calling convention the
/// raise applies: Win64 for a 64-bit image, the x86 conventions (`cdecl`, `stdcall`, `fastcall`,
/// `thiscall`, told apart by the registers a function reads and the bytes it pops) for a 32-bit
/// one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub bits: u32,
}

impl Target {
    pub const WIN64: Target = Target { bits: 64 };
    pub const X86: Target = Target { bits: 32 };

    pub fn is_64(self) -> bool {
        self.bits == 64
    }

    /// The size of a pointer and of a stack slot, in bytes.
    pub fn ptr(self) -> i64 {
        i64::from(self.bits / 8)
    }

    /// The integer type of the address width.
    pub fn int_ty(self) -> Type {
        Type::Int(self.bits as u16)
    }

    /// The offset of the first stack argument above the entry stack pointer: past the return
    /// address, and on Win64 past the four home slots. Also where a caller's outgoing arguments
    /// start above its stack pointer once the return address is pushed.
    pub fn first_stack_arg(self) -> i64 {
        if self.is_64() { 0x28 } else { 4 }
    }

    /// Where a caller's outgoing arguments start above its stack pointer at the call instruction,
    /// before any push: what an import called in place of the native call sees.
    pub fn outgoing_at_call(self) -> i64 {
        if self.is_64() { 0x20 } else { 0 }
    }

    /// The registers that carry integer arguments, by position: `rcx`, `rdx`, `r8`, `r9` on Win64;
    /// `ecx` and `edx` on x86, which `fastcall` and `thiscall` use.
    pub fn int_param_slots(self) -> &'static [Slot] {
        if self.is_64() { &[RCX, RDX, R8, R9] } else { &[RCX, RDX] }
    }

    /// How many floats arrive in vector registers: four on Win64, where a position is a register
    /// of either class; three on x86, where clang passes the floats of a local function in `xmm0`
    /// to `xmm2` beside the integers in `ecx` and `edx`, each class counted on its own.
    pub fn float_params(self) -> usize {
        if self.is_64() { 4 } else { 3 }
    }

    /// The register of parameter `index` of `params`: by position on Win64, by its place among
    /// the parameters of its class on x86.
    pub fn param_slot(self, params: &[abi::Kind], index: usize) -> Option<Slot> {
        let kind = params[index];
        let float = matches!(kind, abi::Kind::Float | abi::Kind::Double);
        let place = if self.is_64() {
            index
        } else {
            params[..index].iter().filter(|k| matches!(k, abi::Kind::Float | abi::Kind::Double) == float).count()
        };
        if float { (place < self.float_params()).then_some(Slot::Xmm(place as u8)) } else { self.int_param_slots().get(place).copied() }
    }

    /// Whether the callee keeps the register: `rbx`, `rbp`, `rsp`, `rsi`, `rdi`, `r12` to `r15`
    /// and `xmm6` to `xmm15` on Win64; `ebx`, `ebp`, `esp`, `esi` and `edi` on x86.
    pub fn callee_saved(self, slot: Slot) -> bool {
        match slot {
            Slot::Gpr(n) if self.is_64() => matches!(n, 3 | 4 | 5 | 6 | 7 | 12 | 13 | 14 | 15),
            Slot::Gpr(n) => (3..=7).contains(&n),
            Slot::Xmm(n) => self.is_64() && n >= 6,
            Slot::Flag(_) => false,
        }
    }

    /// The entry stack pointer's offset past a 16-byte boundary: a native call pushes the return
    /// address onto an aligned stack, so a callee starts at 8 (Win64) or 12 (x86, where the
    /// alignment is 4 and 12 is what a caller aligned to 16 leaves).
    pub fn entry_misalignment(self) -> u64 {
        if self.is_64() { 8 } else { 12 }
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

/// The options of a raise.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub level: Level,
    /// The address width of the image: 64 or 32.
    pub bits: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self { level: Level::Abi, bits: 64 }
    }
}

impl Options {
    pub fn machine() -> Self {
        Self { level: Level::Machine, bits: 64 }
    }

    pub fn target(&self) -> Target {
        Target { bits: self.bits }
    }
}

/// Runs the raising passes over every function of `module`. `roots` are the functions whose
/// callers the lift does not see (see [`abi::infer`]).
pub fn raise(module: &mut Module, roots: &BTreeSet<String>, options: Options) -> HashMap<String, abi::Signature> {
    let target = options.target();
    let signatures = abi::infer(module, roots, target);
    match options.level {
        Level::Machine => return signatures,
        Level::Promoted => {
            for f in &mut module.functions {
                promote::promote(f);
                clean(f);
            }
        }
        Level::Abi => {
            for f in &mut module.functions {
                let Some(own) = signatures.get(&f.name) else { continue };
                // The constants first: the rewrite recognizes a pushed return address by its value.
                simplify::run(f);
                promote::promote_abi(f, &promote::Abi { own, all: &signatures, target });
                if own.noreturn && !f.attrs.iter().any(|a| a == "noreturn") {
                    f.attrs.push("noreturn".into());
                }
                clean(f);
                conditions::run(f);
                clean(f);
                frame::run(f);
                clean(f);
                // Known values and branches, to a fixpoint with the cleanup.
                for _ in 0..4 {
                    if !known::run(f) {
                        break;
                    }
                    clean(f);
                }
            }
            // The import wrappers served the machine model; the calls now go to the imports,
            // each declared with the arity the prototype table gives it.
            module.functions.retain(|f| crate::import_of_wrapper(&f.name).is_none());
            // An x86 function a pointer reaches gets its universal entry point.
            if !target.is_64() {
                stubs::add_universal_stubs(module, &signatures, target);
            }
            // The widths, to a fixpoint: each narrowing exposes the next through the cleanup.
            for _ in 0..4 {
                if !widths::run(module, roots) {
                    break;
                }
                for f in &mut module.functions {
                    clean(f);
                }
            }
            for d in &mut module.declarations {
                if let Some(arity) = prototypes::arity(&d.name)
                    && d.ty.params.len() == 8
                {
                    d.ty = ra_asm_ir::FnType::new(ra_asm_ir::Type::I64, vec![ra_asm_ir::Type::I64; arity.min(8)]);
                }
            }
        }
    }
    signatures
}

fn clean(f: &mut ra_asm_ir::Function) {
    loop {
        let changed = simplify::run(f) | simplify::cse(f);
        dce::run(f);
        if !changed {
            break;
        }
    }
}
