//! A vector read through a signature: what the ABI level needs of a record that was made over a
//! state.
//!
//! A vector holds registers and memory; a function of the ABI level takes arguments and returns
//! a value. [`Convention`] maps the one to the other by an [`abi::Signature`](Signature), as the
//! raise placed them: a register parameter is the register of its position at the entry, a stack
//! parameter the bytes the function read above its return address, a result the register the
//! convention returns it in, at the width its type keeps. It also says what else of a vector a
//! function without a state still answers for: the stack pointer it leaves (the return address
//! popped, and what the function pops beyond it), the registers it keeps for its caller, and the
//! memory outside the frame that died with the activation.

use anyhow::{Result, anyhow};
use ra_asm_ir::Type;
use ra_asm_ir::interp::{Value, mask};
use ra_asm_lift::Isa;
use ra_asm_lift::raise::Target;
use ra_asm_lift::raise::abi::{Kind, Signature};
use ra_asm_lift::raise::liveness::Slot;

use crate::record::Shape;
use crate::vector::{Register, Run, Vector};

/// The bytes below the frame the signature names that are the function's still: the room the
/// raise gives the frame it allocates, where the return address of a call lands.
const MARGIN: u64 = 16;

/// The bytes below that a function has when it moves its stack pointer by an amount known at run
/// time, as the raise's frame has them.
const DYNAMIC: u64 = 4096;

/// A result as a record holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct Returned {
    /// The register it lies in, as a line names it: `rax`, `xmm0`, `edx:eax` for a wide one.
    pub register: String,
    /// The value, at the type the function returns it at.
    pub value: Value,
    /// The low bits of the register that are the result.
    pub bits: u32,
}

impl Returned {
    /// Whether `replayed`, the value a function returned, is this result.
    pub fn is(&self, replayed: &Value) -> bool {
        replayed.as_u128() & mask(self.bits) == self.value.as_u128() & mask(self.bits)
    }
}

/// The registers of one machine as the signatures of its convention name them.
#[derive(Debug, Clone)]
pub struct Convention {
    target: Target,
    shape: Shape,
}

impl Convention {
    /// The convention of `isa` for an image of `bits` address bits.
    pub fn new(isa: Isa, bits: u32) -> Self {
        Self { target: Target { bits, isa }, shape: Shape::of(isa, bits) }
    }

    pub fn target(&self) -> Target {
        self.target
    }

    /// The name of the stack pointer.
    pub fn stack_pointer(&self) -> Option<&str> {
        self.shape.name(self.target.sp())
    }

    /// The value of the register `slot` among `registers`.
    fn register(&self, function: &str, registers: &[Register], slot: Slot) -> Result<u128> {
        let name = self.shape.name(slot).ok_or_else(|| anyhow!("{function}: the state has no register for {slot:?}"))?;
        read(registers, name).ok_or_else(|| anyhow!("{function}: the vector does not hold {name}"))
    }

    /// The stack pointer `vector` was entered with.
    pub fn entry_stack_pointer(&self, vector: &Vector) -> Result<u64> {
        Ok(self.register(&vector.function, &vector.entry, self.target.sp())? as u64)
    }

    /// The arguments `vector` entered a function of `signature` with, in the order the raise
    /// gives the parameters: the registers, the stack, and for a variadic function the address
    /// of its caller's argument block. A stack argument the function never read is zero: the
    /// vector does not hold it, and the function did not depend on it.
    pub fn arguments(&self, signature: &Signature, vector: &Vector) -> Result<Vec<Value>> {
        let target = self.target;
        let mut arguments = Vec::with_capacity(signature.param_count() + 1);
        for (index, kind) in signature.params.iter().enumerate() {
            let slot = target
                .param_slot(&signature.params, index)
                .ok_or_else(|| anyhow!("{}: parameter {} of the signature has no register", vector.function, index + 1))?;
            let bits = self.register(&vector.function, &vector.entry, slot)?;
            arguments.push(match kind {
                Kind::Int | Kind::Wide => Value::Int(bits & mask(target.bits)),
                Kind::Float => Value::F32(f32::from_bits(bits as u32)),
                Kind::Double => Value::F64(f64::from_bits(bits as u64)),
            });
        }
        let first = self.entry_stack_pointer(vector)?.wrapping_add(target.first_stack_arg() as u64);
        for (kind, offset) in signature.stack_params.iter().zip(signature.stack_offsets(target)) {
            let bits = word(&vector.memory_in, first.wrapping_add(offset as u64), kind.stack_width(target) as u64);
            arguments.push(match kind {
                Kind::Int | Kind::Wide => Value::Int(u128::from(bits)),
                Kind::Float => Value::F32(f32::from_bits(bits as u32)),
                Kind::Double => Value::F64(f64::from_bits(bits)),
            });
        }
        if signature.variadic {
            arguments.push(Value::Int(u128::from(first)));
        }
        Ok(arguments)
    }

    /// The result a function that returns `returns` at the type `ty` left in `registers`: the
    /// exit of a vector, or the results of a call. Nothing for a function that returns nothing;
    /// an error with the register's name when `registers` do not hold it.
    pub fn result(&self, returns: Option<Kind>, ty: &Type, registers: &[Register]) -> Result<Option<Returned>, String> {
        let name = |slot: Slot| self.shape.name(slot).ok_or_else(|| format!("{slot:?}"));
        let held = |name: &str| read(registers, name).ok_or_else(|| name.to_owned());
        let width = match ty {
            Type::Int(bits) => u32::from(*bits).min(64),
            _ => 64,
        };
        match (ty, returns) {
            (Type::Void, _) => Ok(None),
            (Type::Float, _) => {
                let register = name(Slot::Xmm(0))?;
                let value = Value::F32(f32::from_bits(held(register)? as u32));
                Ok(Some(Returned { register: register.to_owned(), value, bits: 32 }))
            }
            (Type::Double, _) => {
                let register = name(Slot::Xmm(0))?;
                let value = Value::F64(f64::from_bits(held(register)? as u64));
                Ok(Some(Returned { register: register.to_owned(), value, bits: 64 }))
            }
            (_, Some(Kind::Int)) => {
                let register = name(Slot::Gpr(0))?;
                let bits = width.min(self.target.bits);
                Ok(Some(Returned { register: register.to_owned(), value: Value::int(held(register)?, bits), bits }))
            }
            // The 64-bit result of an x86 function: the high half in `edx`, the low in `eax`.
            (_, Some(Kind::Wide)) => {
                let (low, high) = (name(Slot::Gpr(0))?, name(Slot::Gpr(2))?);
                let value = ((held(high)? & mask(32)) << 32) | (held(low)? & mask(32));
                Ok(Some(Returned { register: format!("{high}:{low}"), value: Value::int(value, width), bits: width }))
            }
            // A float kind at an integer type has no register a universal call returns it in.
            (_, Some(Kind::Float | Kind::Double) | None) => Ok(None),
        }
    }

    /// The stack pointer a function of `signature` leaves when it was entered with `entry`: the
    /// return address popped, and what the function pops beyond it.
    pub fn exit_stack_pointer(&self, signature: &Signature, entry: u64) -> u64 {
        let left = entry.wrapping_add(self.target.pushed_return() as u64).wrapping_add(signature.pops);
        (u128::from(left) & mask(self.target.bits)) as u64
    }

    /// The registers a function of `signature` hands back as it found them: the ones the
    /// convention has the callee save, but for the stack pointer, and the caller-saved ones the
    /// signature says it leaves alone.
    pub fn kept(&self, signature: &Signature) -> Vec<&str> {
        let sp = self.target.sp();
        (0..self.shape.general())
            .map(|n| Slot::Gpr(n as u8))
            .filter(|slot| *slot != sp && (self.target.callee_saved(*slot) || signature.preserves[slot.index()]))
            .filter_map(|slot| self.shape.name(slot))
            .collect()
    }

    /// The frame of a function of `signature` entered with the stack pointer `entry`, as a range
    /// of addresses: its own space below, the return address, the home space and the stack
    /// arguments above. The activation's bytes there died with it, and the ABI level keeps them
    /// in a frame of its own, somewhere else.
    pub fn frame(&self, signature: &Signature, entry: u64) -> (u64, u64) {
        let slack = if signature.dynamic_stack { DYNAMIC } else { 0 };
        let below = signature.frame_below.max(MARGIN) + MARGIN + slack;
        let above = self.target.first_stack_arg() as u64 + signature.stack_bytes(self.target) as u64;
        (entry.saturating_sub(below), entry.saturating_add(above))
    }
}

/// The value of the register `name` among `registers`.
fn read(registers: &[Register], name: &str) -> Option<u128> {
    registers.iter().find(|(register, _)| register == name).map(|(_, value)| *value)
}

/// The little-endian word of `width` bytes at `address` in `runs`; a byte no run holds is zero.
fn word(runs: &[Run], address: u64, width: u64) -> u64 {
    let byte = |at: u64| {
        runs.iter()
            .find_map(|(start, bytes)| at.checked_sub(*start).and_then(|offset| bytes.get(usize::try_from(offset).ok()?)))
            .copied()
            .unwrap_or(0)
    };
    (0..width.min(8)).fold(0, |word, n| word | (u64::from(byte(address.wrapping_add(n))) << (8 * n)))
}
