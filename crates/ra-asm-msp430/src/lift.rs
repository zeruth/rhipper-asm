//! The lift of an MSP430 program at the machine's own level: its code as one function of the
//! IR over the machine's state.
//!
//! [`RUN`] takes the state and the address to start at. The state is sixteen slots of eight
//! bytes, a register in the low two: `r0` to `r15`, the status register `r2` among them with
//! its flags as the bits the machine keeps them in (carry, zero, negative, and overflow at bit
//! eight), since a program reads and writes that register as any other. Every instruction is a
//! block that reads the state, computes, and writes it back; an instruction is one to three
//! words, so the code is read from its start, one instruction after another. A jump goes to
//! the block of its target; a move into the program counter (`br`, `ret`) and a call through a
//! register go to the dispatcher, which finds the block of the address.
//!
//! Memory is the machine's sixty-four kilobytes, from where [`MEMORY`] says its address zero
//! is; a word is read and written as the host has it, which is the machine's order on a
//! little-endian host. A microcontroller's program reaches the world through registers in its
//! low memory: a store to an address the instruction gives outright, below [`PERIPHERALS`], is
//! a call of [`OUT`] with the state, the address and the value. After an instruction that
//! writes the status register [`STATUS`] is called with the state: a program stops its
//! processor by a bit of that register, and the runtime ends the run there.
//!
//! What is lifted is the whole of the original processor but `dadd`, the decimal sum, and
//! `reti`: a block of those, and of words that are no instruction, calls [`TRAP`]. A
//! peripheral reached through a pointer is memory, and one read is memory too.

use std::collections::BTreeMap;

use ra_asm_ir::{
    BinOp, BlockId, CallConv, Callee, Cast, Const, Declaration, FnType, FunctionBuilder, Global, ICond, Linkage, Module, Operand as Ir,
    Terminator, Type, ValueId,
};

use crate::decode::{Cond, Decoded, One, Op, Operand, PC, SP, SR, Two, decode};

/// The function the lift writes: `void (ptr state, i64 pc)`.
pub const RUN: &str = "msp430_run";
/// A store to a peripheral's register: `void (ptr state, i64 address, i64 value)`.
pub const OUT: &str = "msp430_out";
/// The status register was written: `void (ptr state)`.
pub const STATUS: &str = "msp430_status";
/// Control came to what is no instruction the lift has: `void (i64 address)`.
pub const TRAP: &str = "msp430_trap";
/// The program's own eight bytes that say where its address zero is.
pub const MEMORY: &str = "msp430_memory";
/// The function of a whole program ([`crate::run::Program::lift`]): `void (ptr state)`.
pub const MAIN: &str = "msp430_main";
/// The runtime's taking of a segment: `void (i64 address, ptr bytes, i64 in_file, i64 size)`.
pub const SEGMENT: &str = "msp430_segment";
/// The slots of the state.
pub const SLOTS: u64 = 16;
/// Below this address lie the registers of the peripherals.
pub const PERIPHERALS: u16 = 0x1000;

/// The flags' bits of the status register.
const CARRY: u32 = 1;
const ZERO: u32 = 2;
const NEGATIVE: u32 = 4;
const OVERFLOW: u32 = 0x100;

const I32: Type = Type::I32;

/// The code of a program: its bytes, little-endian words, and the address of the first.
#[derive(Debug, Clone, Copy)]
pub struct Code<'a> {
    pub address: u16,
    pub bytes: &'a [u8],
}

struct Lift {
    b: FunctionBuilder,
    state: ValueId,
    block: BlockId,
    /// Where a move into the program counter goes, for the dispatcher.
    pc: ValueId,
}

/// Where a destination is: a register, or memory at an address already worked out.
enum Place {
    Reg(u8),
    Memory(Ir),
    /// A peripheral's register, by its address.
    Peripheral(u16),
}

impl Lift {
    fn slot(&mut self, n: u8) -> ValueId {
        self.b.gep(self.block, Type::I8, self.state, vec![(Type::I64, Ir::Int(8 * i128::from(n)))], "at")
    }

    /// A register as a word in thirty-two bits, the high half zero.
    fn get(&mut self, n: u8) -> Ir {
        let at = self.slot(n);
        let value = self.b.load(self.block, Type::I16, at, "r");
        self.b.cast(self.block, Cast::ZExt, Type::I16, value, I32, "r").into()
    }

    fn set(&mut self, n: u8, value: impl Into<Ir>) {
        let at = self.slot(n);
        let narrow = self.b.cast(self.block, Cast::Trunc, I32, value, Type::I16, "r");
        self.b.store(self.block, Type::I16, narrow, at);
    }

    fn bin(&mut self, op: BinOp, a: impl Into<Ir>, b: impl Into<Ir>) -> Ir {
        self.b.bin(self.block, op, I32, a, b, "v").into()
    }

    fn cmp(&mut self, cond: ICond, a: impl Into<Ir>, b: impl Into<Ir>) -> ValueId {
        self.b.icmp(self.block, cond, I32, a, b, "is")
    }

    /// One bit of a value, as zero or one.
    fn bit(&mut self, value: Ir, place: u32) -> Ir {
        let shifted = self.bin(BinOp::LShr, value, place);
        self.bin(BinOp::And, shifted, 1)
    }

    fn pointer(&mut self, address: Ir) -> ValueId {
        let address = self.bin(BinOp::And, address, 0xffff);
        let wide = self.b.cast(self.block, Cast::ZExt, I32, address, Type::I64, "address");
        let memory = self.b.load(self.block, Type::I64, Ir::Global(MEMORY.to_owned()), "memory");
        let wide = self.b.bin(self.block, BinOp::Add, Type::I64, wide, memory, "address");
        self.b.cast(self.block, Cast::IntToPtr, Type::I64, wide, Type::Ptr, "pointer")
    }

    /// The word or the byte at `address`, in thirty-two bits.
    fn load(&mut self, byte: bool, address: Ir) -> Ir {
        let at = self.pointer(address);
        let ty = if byte { Type::I8 } else { Type::I16 };
        let value = self.b.load(self.block, ty.clone(), at, "loaded");
        self.b.cast(self.block, Cast::ZExt, ty, value, I32, "loaded").into()
    }

    fn store(&mut self, byte: bool, value: Ir, address: Ir) {
        let at = self.pointer(address);
        let ty = if byte { Type::I8 } else { Type::I16 };
        let narrow = self.b.cast(self.block, Cast::Trunc, I32, value, ty.clone(), "narrow");
        self.b.store(self.block, ty, narrow, at);
    }

    /// The address of an operand in memory, a register stepped where the mode steps it.
    fn address(&mut self, operand: Operand, byte: bool) -> Option<Ir> {
        Some(match operand {
            Operand::Indexed { reg, offset } => {
                let base = self.get(reg);
                self.bin(BinOp::Add, base, u32::from(offset))
            }
            Operand::Absolute(address) | Operand::Symbolic(address) => Ir::Int(i128::from(address)),
            Operand::Indirect(reg) => self.get(reg),
            Operand::IndirectInc(reg) => {
                let at = self.get(reg);
                // The stack pointer stays even: a byte popped steps it a word.
                let step = if byte && reg != SP { 1 } else { 2 };
                let after = self.bin(BinOp::Add, at.clone(), step);
                self.set(reg, after);
                at
            }
            Operand::Reg(_) | Operand::Imm { .. } => return None,
        })
    }

    /// A source's value, of the operation's width; `after` is the address past the instruction,
    /// which is what the program counter reads as.
    fn source(&mut self, operand: Operand, byte: bool, after: u16) -> Ir {
        let mask = if byte { 0xff } else { 0xffff };
        match operand {
            Operand::Reg(PC) => Ir::Int(i128::from(after)),
            Operand::Reg(reg) => {
                let value = self.get(reg);
                if byte { self.bin(BinOp::And, value, mask) } else { value }
            }
            Operand::Imm { value, .. } => Ir::Int(i128::from(u32::from(value) & mask)),
            _ => {
                let address = self.address(operand, byte).expect("a memory operand");
                self.load(byte, address)
            }
        }
    }

    fn place(&mut self, operand: Operand, byte: bool) -> Place {
        match operand {
            Operand::Reg(reg) => Place::Reg(reg),
            Operand::Absolute(address) if address < PERIPHERALS => Place::Peripheral(address),
            _ => Place::Memory(self.address(operand, byte).expect("a memory operand")),
        }
    }

    fn read(&mut self, place: &Place, byte: bool, after: u16) -> Ir {
        match place {
            Place::Reg(reg) => self.source(Operand::Reg(*reg), byte, after),
            Place::Memory(address) => self.load(byte, address.clone()),
            Place::Peripheral(address) => self.load(byte, Ir::Int(i128::from(*address))),
        }
    }

    /// Writes a result: a register whole (a byte's high half zero), memory at the width, a
    /// peripheral through the runtime. Whether control leaves by the write.
    fn write(&mut self, place: &Place, byte: bool, value: Ir) -> bool {
        match place {
            Place::Reg(PC) => {
                let wide = self.b.cast(self.block, Cast::ZExt, I32, value, Type::I64, "target");
                self.b.store(self.block, Type::I64, wide, self.pc);
                return true;
            }
            Place::Reg(reg) => self.set(*reg, value),
            Place::Memory(address) => self.store(byte, value, address.clone()),
            Place::Peripheral(address) => {
                let wide = self.b.cast(self.block, Cast::ZExt, I32, value, Type::I64, "out");
                let args = vec![(Type::Ptr, self.state.into()), (Type::I64, Ir::Int(i128::from(*address))), (Type::I64, wide.into())];
                self.b.call(
                    self.block,
                    FnType::new(Type::Void, vec![Type::Ptr, Type::I64, Type::I64]),
                    Callee::Direct(OUT.to_owned()),
                    args,
                    "",
                );
            }
        }
        false
    }

    /// The flags after a result: zero and negative from it, the carry and the overflow given.
    fn flags(&mut self, result: Ir, byte: bool, carry: Ir, overflow: Ir) {
        let none = self.cmp(ICond::Eq, result.clone(), 0);
        let zero: Ir = self.b.select(self.block, none, I32, ZERO, 0, "z").into();
        let negative = self.bit(result, if byte { 7 } else { 15 });
        let negative = self.bin(BinOp::Mul, negative, NEGATIVE);
        let overflow = self.bin(BinOp::Mul, overflow, OVERFLOW);
        let status = self.get(SR);
        let mut status = self.bin(BinOp::And, status, !(CARRY | ZERO | NEGATIVE | OVERFLOW) & 0xffff);
        for flag in [carry, zero, negative, overflow] {
            status = self.bin(BinOp::Or, status, flag);
        }
        self.set(SR, status);
    }

    fn flag(&mut self, flag: u32) -> Ir {
        let status = self.get(SR);
        self.bit(status, flag.trailing_zeros())
    }

    /// `a + b + carry` at the width: the sum, its carry out and its signed overflow.
    fn sum(&mut self, a: Ir, b: Ir, carry: Ir, byte: bool) -> (Ir, Ir, Ir) {
        let (bits, mask) = if byte { (8, 0xff) } else { (16, 0xffff) };
        let wide = self.bin(BinOp::Add, a.clone(), b.clone());
        let wide = self.bin(BinOp::Add, wide, carry);
        let out = self.bit(wide.clone(), bits);
        let result = self.bin(BinOp::And, wide, mask);
        // Both operands differ from the sum in their sign.
        let (x, y) = (self.bin(BinOp::Xor, a, result.clone()), self.bin(BinOp::Xor, b, result.clone()));
        let both = self.bin(BinOp::And, x, y);
        let overflow = self.bit(both, bits - 1);
        (result, out, overflow)
    }

    /// An operation of two operands. Whether control leaves by its write.
    fn two(&mut self, op: Two, src: Operand, dst: Operand, byte: bool, after: u16) -> Option<bool> {
        let (bits, mask) = if byte { (8u32, 0xff) } else { (16, 0xffff) };
        let s = self.source(src, byte, after);
        let place = self.place(dst, byte);
        if op == Two::Mov {
            return Some(self.write(&place, byte, s));
        }
        let d = self.read(&place, byte, after);
        let result = match op {
            Two::Add | Two::Addc => {
                let carry = if op == Two::Addc { self.flag(CARRY) } else { Ir::Int(0) };
                let (result, out, overflow) = self.sum(d, s, carry, byte);
                self.flags(result.clone(), byte, out, overflow);
                Some(result)
            }
            Two::Sub | Two::Subc | Two::Cmp => {
                // A difference is the sum with the source's bits turned and one more, or the carry.
                let turned = self.bin(BinOp::Xor, s, mask);
                let carry = if op == Two::Subc { self.flag(CARRY) } else { Ir::Int(1) };
                let (result, out, overflow) = self.sum(d, turned, carry, byte);
                self.flags(result.clone(), byte, out, overflow);
                (op != Two::Cmp).then_some(result)
            }
            Two::Bit | Two::And | Two::Xor => {
                let result = self.bin(if op == Two::Xor { BinOp::Xor } else { BinOp::And }, d.clone(), s.clone());
                // The carry is that the result is not zero; `xor` overflows when both were negative.
                let some = self.cmp(ICond::Ne, result.clone(), 0);
                let carry: Ir = self.b.select(self.block, some, I32, CARRY, 0, "c").into();
                let overflow = if op == Two::Xor {
                    let both = self.bin(BinOp::And, d, s);
                    self.bit(both, bits - 1)
                } else {
                    Ir::Int(0)
                };
                self.flags(result.clone(), byte, carry, overflow);
                (op != Two::Bit).then_some(result)
            }
            Two::Bic => {
                let kept = self.bin(BinOp::Xor, s, mask);
                Some(self.bin(BinOp::And, d, kept))
            }
            Two::Bis => Some(self.bin(BinOp::Or, d, s)),
            Two::Dadd | Two::Mov => return None,
        };
        Some(match result {
            Some(result) => self.write(&place, byte, result),
            None => false,
        })
    }

    /// An operation of one operand that is no call.
    fn one(&mut self, op: One, operand: Operand, byte: bool, after: u16) {
        let (bits, mask) = if byte { (8u32, 0xff) } else { (16, 0xffff) };
        if op == One::Push {
            let value = self.source(operand, byte, after);
            let sp = self.get(SP);
            let sp = self.bin(BinOp::Sub, sp, 2);
            self.set(SP, sp.clone());
            self.store(byte, value, sp);
            return;
        }
        let place = self.place(operand, byte);
        let value = self.read(&place, byte, after);
        let result = match op {
            One::Rrc | One::Rra => {
                // Right by one: the bit that leaves is the carry, the bit that comes in the old
                // carry or the sign.
                let out = self.bin(BinOp::And, value.clone(), 1);
                let top = if op == One::Rrc { self.flag(CARRY) } else { self.bit(value.clone(), bits - 1) };
                let top = self.bin(BinOp::Shl, top, bits - 1);
                let shifted = self.bin(BinOp::LShr, value, 1);
                let result = self.bin(BinOp::Or, shifted, top);
                self.flags(result.clone(), byte, out, Ir::Int(0));
                result
            }
            One::Swpb => {
                let (high, low) = (self.bin(BinOp::LShr, value.clone(), 8), self.bin(BinOp::Shl, value, 8));
                let both = self.bin(BinOp::Or, high, low);
                self.bin(BinOp::And, both, mask)
            }
            One::Sxt => {
                let low = self.b.cast(self.block, Cast::Trunc, I32, value, Type::I8, "low");
                let wide: Ir = self.b.cast(self.block, Cast::SExt, Type::I8, low, I32, "wide").into();
                let result = self.bin(BinOp::And, wide, 0xffff);
                let some = self.cmp(ICond::Ne, result.clone(), 0);
                let carry: Ir = self.b.select(self.block, some, I32, CARRY, 0, "c").into();
                self.flags(result.clone(), false, carry, Ir::Int(0));
                result
            }
            One::Push | One::Call => unreachable!("handled before"),
        };
        self.write(&place, byte && op != One::Sxt, result);
    }

    /// Whether a jump's condition holds.
    fn holds(&mut self, cond: Cond) -> Option<ValueId> {
        let (flag, set) = match cond {
            Cond::Ne => (ZERO, false),
            Cond::Eq => (ZERO, true),
            Cond::Nc => (CARRY, false),
            Cond::C => (CARRY, true),
            Cond::N => (NEGATIVE, true),
            Cond::Ge | Cond::L => {
                let (negative, overflow) = (self.flag(NEGATIVE), self.flag(OVERFLOW));
                let differ = self.bin(BinOp::Xor, negative, overflow);
                return Some(self.cmp(if cond == Cond::L { ICond::Ne } else { ICond::Eq }, differ, 0));
            }
            Cond::Always => return None,
        };
        let flag = self.flag(flag);
        Some(self.cmp(if set { ICond::Ne } else { ICond::Eq }, flag, 0))
    }
}

/// Lifts `code` into a module with the one function [`RUN`].
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn lift(code: Code<'_>) -> Module {
    let mut module = Module::x86_64_windows("msp430");
    module.globals.push(Global {
        name: MEMORY.to_owned(),
        ty: Type::array(8, Type::I8),
        init: Some(Const::Bytes(vec![0; 8])),
        constant: false,
        linkage: Linkage::External,
        align: 8,
        address: None,
    });
    let declare = |module: &mut Module, name: &str, params: Vec<Type>| {
        module.declare(Declaration { name: name.to_owned(), ty: FnType::new(Type::Void, params), conv: CallConv::C, attrs: Vec::new() });
    };
    declare(&mut module, OUT, vec![Type::Ptr, Type::I64, Type::I64]);
    declare(&mut module, STATUS, vec![Type::Ptr]);
    declare(&mut module, TRAP, vec![Type::I64]);
    let mut b = FunctionBuilder::new(RUN, FnType::new(Type::Void, vec![Type::Ptr, Type::I64]), &["state", "start"]);
    let (entry, dispatch, nowhere) = (b.block("entry"), b.block("dispatch"), b.block("nowhere"));
    let state = b.param(0);
    let pc = b.alloca(entry, Type::I64, "pc");
    let start = b.param(1);
    b.store(entry, Type::I64, start, pc);
    b.terminate(entry, Terminator::Br(dispatch));
    // The code from its start, one instruction after another; a word that is none is stepped over.
    let words: Vec<u16> = code.bytes.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
    let mut instructions: Vec<(u16, Option<Decoded>)> = Vec::new();
    let mut index = 0;
    while index < words.len() {
        let address = code.address.wrapping_add(2 * index as u16);
        let decoded = decode(&words[index..], address);
        index += decoded.map_or(1, |d| usize::from(d.len / 2));
        instructions.push((address, decoded));
    }
    let mut blocks: BTreeMap<u16, BlockId> = BTreeMap::new();
    for &(address, _) in &instructions {
        b.address = Some(u64::from(address));
        blocks.insert(address, b.block(&format!("at.{address:x}")));
    }
    let mut lift = Lift { b, state, block: entry, pc };
    let trap = |lift: &mut Lift, address: Ir| {
        lift.b.call(lift.block, FnType::new(Type::Void, vec![Type::I64]), Callee::Direct(TRAP.to_owned()), vec![(Type::I64, address)], "");
        lift.b.terminate(lift.block, Terminator::Ret(None));
    };
    // The block of an address, or the dispatcher's way to none with the address said.
    let goes = |lift: &mut Lift, target: u16| match blocks.get(&target) {
        Some(&block) => block,
        None => {
            lift.b.store(lift.block, Type::I64, Ir::Int(i128::from(target)), pc);
            nowhere
        }
    };
    for &(address, decoded) in &instructions {
        lift.block = blocks[&address];
        lift.b.address = Some(u64::from(address));
        let Some(d) = decoded else {
            trap(&mut lift, Ir::Int(i128::from(address)));
            continue;
        };
        let after = address.wrapping_add(d.len);
        match d.op {
            Op::Jump { cond, target } => {
                let terminator = match lift.holds(cond) {
                    Some(taken) => {
                        // Both ways are blocks of the code: a jump out of it stops at the dispatcher.
                        let (then, els) = (goes(&mut lift, target), blocks.get(&after).copied().unwrap_or(nowhere));
                        Terminator::CondBr { cond: taken.into(), then, els }
                    }
                    None => Terminator::Br(goes(&mut lift, target)),
                };
                lift.b.terminate(lift.block, terminator);
            }
            Op::One { op: One::Call, operand } => {
                // The target is read before the return address goes on the stack.
                let target = lift.source(operand, false, after);
                let sp = lift.get(SP);
                let sp = lift.bin(BinOp::Sub, sp, 2);
                lift.set(SP, sp.clone());
                lift.store(false, Ir::Int(i128::from(after)), sp);
                let to = match operand {
                    Operand::Imm { value, .. } => goes(&mut lift, value),
                    _ => {
                        let wide = lift.b.cast(lift.block, Cast::ZExt, I32, target, Type::I64, "target");
                        lift.b.store(lift.block, Type::I64, wide, pc);
                        dispatch
                    }
                };
                lift.b.terminate(lift.block, Terminator::Br(to));
            }
            Op::One { op, operand } => {
                lift.one(op, operand, d.byte, after);
                let next = goes(&mut lift, after);
                lift.b.terminate(lift.block, Terminator::Br(next));
            }
            Op::Two { op, src, dst } => match lift.two(op, src, dst, d.byte, after) {
                Some(true) => lift.b.terminate(lift.block, Terminator::Br(dispatch)),
                Some(false) => {
                    // A program stops its processor by a bit of the status register.
                    if dst == Operand::Reg(SR) && !matches!(op, Two::Cmp | Two::Bit | Two::Bic) {
                        lift.b.call(
                            lift.block,
                            FnType::new(Type::Void, vec![Type::Ptr]),
                            Callee::Direct(STATUS.to_owned()),
                            vec![(Type::Ptr, state.into())],
                            "",
                        );
                    }
                    let next = goes(&mut lift, after);
                    lift.b.terminate(lift.block, Terminator::Br(next));
                }
                None => trap(&mut lift, Ir::Int(i128::from(address))),
            },
            Op::Reti => trap(&mut lift, Ir::Int(i128::from(address))),
        }
    }
    let target = lift.b.load(dispatch, Type::I64, pc, "target");
    let cases = blocks.iter().map(|(&address, &block)| (i128::from(address), block)).collect();
    lift.b.terminate(dispatch, Terminator::Switch { ty: Type::I64, value: target.into(), default: nowhere, cases });
    lift.block = nowhere;
    let target = lift.b.load(nowhere, Type::I64, pc, "target");
    trap(&mut lift, target.into());
    module.functions.push(lift.b.finish());
    module
}
