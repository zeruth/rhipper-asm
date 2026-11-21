//! The lift of an SH-4 program at the machine's own level: its code as one function of the IR
//! over the machine's state.
//!
//! [`RUN`] takes the state and the address to start at. The state is slots of eight bytes, a
//! register's value in the low four: `r0` to `r15`, then `pr`, `mach`, `macl`, `gbr`, and the
//! bits `T`, `Q` and `M` of the status register, a slot each. Every instruction word of the
//! code is a block that reads the state, computes, and writes it back. A branch goes to the
//! block of its target; a jump through a register (`jmp`, `jsr`, `rts`, `braf`, `bsrf`) goes to
//! the dispatcher, which finds the block of the address. Memory is the program's own addresses,
//! from where [`MEMORY`] says its address zero is. A `trapa` is a call of [`TRAPA`] with the
//! state and the trap's number; a word that is no instruction the lift has, where the code runs
//! into it, is a call of [`TRAP`].
//!
//! A delayed branch runs the instruction after it before it goes: the branch's block decides
//! (the condition, the target, the return address) on the state as the branch found it, then
//! holds the delay slot's instruction, then goes. The code keeps its constants between its
//! functions, and the lift reads those words as it reads any other: they are never run.
//!
//! What is lifted is the integer machine: the moves in all their addressing, the arithmetic
//! with its carries and overflows, the step division, the multiplies into `mach` and `macl`, the
//! shifts and rotates through `T`, the comparisons. The FPU's instructions and the privileged
//! ones are not.

use std::collections::BTreeMap;

use ra_asm_ir::{
    BinOp, BlockId, CallConv, Callee, Cast, Const, Declaration, FnType, FunctionBuilder, Global, ICond, Linkage, Module, Operand,
    Terminator, Type, ValueId,
};

/// The function the lift writes: `void (ptr state, i64 pc)`.
pub const RUN: &str = "sh_run";
/// The runtime's answer to a `trapa`: `void (ptr state, i64 number)`.
pub const TRAPA: &str = "sh_trapa";
/// What a word that is no instruction calls, with its address: `void (i64 pc)`.
pub const TRAP: &str = "sh_trap";
/// Where the program's memory is: eight bytes that hold the address the program's address zero
/// is at; zero where the program is mapped at its own addresses.
pub const MEMORY: &str = "sh_memory";

/// The slots of the state after the sixteen general registers.
pub const PR: u64 = 16;
pub const MACH: u64 = 17;
pub const MACL: u64 = 18;
pub const GBR: u64 = 19;
pub const T: u64 = 20;
pub const Q: u64 = 21;
pub const M: u64 = 22;
/// The slots of a state.
pub const SLOTS: u64 = 23;

/// The code of a program: its bytes, little-endian words, and the address of the first.
#[derive(Debug, Clone, Copy)]
pub struct Code<'a> {
    pub address: u64,
    pub bytes: &'a [u8],
}

struct Lift {
    b: FunctionBuilder,
    state: ValueId,
    block: BlockId,
}

const I32: Type = Type::I32;

impl Lift {
    fn slot(&mut self, n: u64) -> ValueId {
        self.b.gep(self.block, Type::I8, self.state, vec![(Type::I64, Operand::Int(8 * i128::from(n)))], "at")
    }

    /// The value of slot `n`: a general register, or one of the slots after them.
    fn get(&mut self, n: u64) -> Operand {
        let at = self.slot(n);
        self.b.load(self.block, I32, at, "r").into()
    }

    fn set(&mut self, n: u64, value: impl Into<Operand>) {
        let at = self.slot(n);
        self.b.store(self.block, I32, value, at);
    }

    fn bin(&mut self, op: BinOp, a: impl Into<Operand>, b: impl Into<Operand>) -> Operand {
        self.b.bin(self.block, op, I32, a, b, "v").into()
    }

    fn cmp(&mut self, cond: ICond, a: impl Into<Operand>, b: impl Into<Operand>) -> ValueId {
        self.b.icmp(self.block, cond, I32, a, b, "is")
    }

    /// A bit of the status register from a comparison.
    fn set_bit(&mut self, slot: u64, is: ValueId) {
        let bit = self.b.cast(self.block, Cast::ZExt, Type::I1, is, I32, "bit");
        self.set(slot, bit);
    }

    fn select(&mut self, is: ValueId, then: impl Into<Operand>, els: impl Into<Operand>) -> Operand {
        self.b.select(self.block, is, I32, then, els, "v").into()
    }

    /// The pointer the program's address `address` is.
    fn pointer(&mut self, address: impl Into<Operand>) -> ValueId {
        let wide = self.b.cast(self.block, Cast::ZExt, I32, address, Type::I64, "address");
        let memory = self.b.load(self.block, Type::I64, Operand::Global(MEMORY.to_owned()), "memory");
        let wide = self.b.bin(self.block, BinOp::Add, Type::I64, wide, memory, "address");
        self.b.cast(self.block, Cast::IntToPtr, Type::I64, wide, Type::Ptr, "pointer")
    }

    /// The byte, word or longword at `address`, a narrow one with its sign extended.
    fn load(&mut self, size: u32, address: impl Into<Operand>) -> Operand {
        let at = self.pointer(address);
        let ty = Type::Int(8 * size as u16);
        let value = self.b.load(self.block, ty.clone(), at, "loaded");
        if size == 4 { value.into() } else { self.b.cast(self.block, Cast::SExt, ty, value, I32, "loaded").into() }
    }

    fn store(&mut self, size: u32, value: impl Into<Operand>, address: impl Into<Operand>) {
        let at = self.pointer(address);
        let ty = Type::Int(8 * size as u16);
        let value: Operand =
            if size == 4 { value.into() } else { self.b.cast(self.block, Cast::Trunc, I32, value, ty.clone(), "narrow").into() };
        self.b.store(self.block, ty, value, at);
    }

    /// A sum or a difference of two registers and `T`, at twice the width: the carry or the
    /// borrow is the bit above the result.
    fn wide(&mut self, op: BinOp, a: Operand, b: Operand, with_t: bool) -> (Operand, ValueId) {
        let extend = |lift: &mut Self, v: Operand| lift.b.cast(lift.block, Cast::ZExt, I32, v, Type::I64, "wide");
        let (a, b) = (extend(self, a), extend(self, b));
        let mut sum = self.b.bin(self.block, op, Type::I64, a, b, "sum");
        if with_t {
            let t = self.get(T);
            let t = extend(self, t);
            sum = self.b.bin(self.block, op, Type::I64, sum, t, "sum");
        }
        let low: Operand = self.b.cast(self.block, Cast::Trunc, Type::I64, sum, I32, "v").into();
        let above = self.b.bin(self.block, BinOp::LShr, Type::I64, sum, 32, "above");
        let carried = self.b.icmp(self.block, ICond::Ne, Type::I64, above, 0, "carried");
        (low, carried)
    }

    /// The product of two registers at sixty-four bits, into `mach` and `macl`.
    fn multiply(&mut self, a: Operand, b: Operand, signed: bool) {
        let cast = if signed { Cast::SExt } else { Cast::ZExt };
        let (a, b) = (self.b.cast(self.block, cast, I32, a, Type::I64, "wide"), self.b.cast(self.block, cast, I32, b, Type::I64, "wide"));
        let product = self.b.bin(self.block, BinOp::Mul, Type::I64, a, b, "product");
        let low = self.b.cast(self.block, Cast::Trunc, Type::I64, product, I32, "low");
        let high = self.b.bin(self.block, BinOp::LShr, Type::I64, product, 32, "high");
        let high = self.b.cast(self.block, Cast::Trunc, Type::I64, high, I32, "high");
        self.set(MACL, low);
        self.set(MACH, high);
    }

    /// `shad` and `shld`: left by a count that is not negative, right by one that is, and by
    /// thirty-two (all sign, or nothing) when the negative count's low bits are zero.
    fn shift(&mut self, value: Operand, count: Operand, arithmetic: bool) -> Operand {
        let low = self.bin(BinOp::And, count.clone(), 31);
        let left = self.bin(BinOp::Shl, value.clone(), low.clone());
        let back = self.bin(BinOp::Sub, 32, low.clone());
        let back = self.bin(BinOp::And, back, 31);
        let op = if arithmetic { BinOp::AShr } else { BinOp::LShr };
        let right = self.bin(op, value.clone(), back);
        let all = if arithmetic { self.bin(BinOp::AShr, value, 31) } else { Operand::Int(0) };
        let whole = self.cmp(ICond::Eq, low, 0);
        let right = self.select(whole, all, right);
        let leftward = self.cmp(ICond::Sge, count, 0);
        self.select(leftward, left, right)
    }

    /// An instruction that is no transfer of control: whether the lift has it.
    #[allow(clippy::too_many_lines)]
    fn simple(&mut self, word: u16, address: u64) -> bool {
        let w = u32::from(word);
        let (n, m) = (u64::from((w >> 8) & 15), u64::from((w >> 4) & 15));
        let (low, byte) = (w & 15, w & 0xff);
        let imm = i64::from(byte as u8 as i8);
        let disp4 = i64::from(w & 15);
        let pc = address as i64;
        match w >> 12 {
            0x0 => match (low, m) {
                (0x4..=0x6, _) => {
                    let (base, index, value) = (self.get(n), self.get(0), self.get(m));
                    let at = self.bin(BinOp::Add, base, index);
                    self.store(1 << (low - 4), value, at);
                }
                (0xc..=0xe, _) => {
                    let (base, index) = (self.get(m), self.get(0));
                    let at = self.bin(BinOp::Add, base, index);
                    let value = self.load(1 << (low - 0xc), at);
                    self.set(n, value);
                }
                (0x7, _) => {
                    let (a, b) = (self.get(n), self.get(m));
                    let product = self.bin(BinOp::Mul, a, b);
                    self.set(MACL, product);
                }
                (0x8, 0) => self.set(T, 0),
                (0x8, 1) => self.set(T, 1),
                (0x9, 0) => {}
                (0x9, 1) => {
                    self.set(Q, 0);
                    self.set(M, 0);
                    self.set(T, 0);
                }
                (0x9, 2) => {
                    let t = self.get(T);
                    self.set(n, t);
                }
                (0xa, 0..=2) => {
                    let value = self.get([MACH, MACL, PR][m as usize]);
                    self.set(n, value);
                }
                (0x2, 1) => {
                    let value = self.get(GBR);
                    self.set(n, value);
                }
                _ => return false,
            },
            0x1 => {
                let (base, value) = (self.get(n), self.get(m));
                let at = self.bin(BinOp::Add, base, 4 * disp4);
                self.store(4, value, at);
            }
            0x2 => match low {
                0..=2 => {
                    let (at, value) = (self.get(n), self.get(m));
                    self.store(1 << low, value, at);
                }
                4..=6 => {
                    // The value is read before the register is moved down.
                    let size = 1 << (low - 4);
                    let (base, value) = (self.get(n), self.get(m));
                    let at = self.bin(BinOp::Sub, base, i64::from(size));
                    self.set(n, at.clone());
                    self.store(size, value, at);
                }
                7 => {
                    let (a, b) = (self.get(n), self.get(m));
                    let (q, m_bit) = (self.cmp(ICond::Slt, a.clone(), 0), self.cmp(ICond::Slt, b.clone(), 0));
                    self.set_bit(Q, q);
                    self.set_bit(M, m_bit);
                    let differ = self.bin(BinOp::Xor, a, b);
                    let t = self.cmp(ICond::Slt, differ, 0);
                    self.set_bit(T, t);
                }
                8 => {
                    let (a, b) = (self.get(n), self.get(m));
                    let both = self.bin(BinOp::And, a, b);
                    let none = self.cmp(ICond::Eq, both, 0);
                    self.set_bit(T, none);
                }
                9..=0xb => {
                    let (a, b) = (self.get(n), self.get(m));
                    let value = self.bin([BinOp::And, BinOp::Xor, BinOp::Or][(low - 9) as usize], a, b);
                    self.set(n, value);
                }
                0xc => {
                    // Whether any byte of the two is the same.
                    let (a, b) = (self.get(n), self.get(m));
                    let differ = self.bin(BinOp::Xor, a, b);
                    let mut any: Operand = Operand::Int(0);
                    for byte in 0..4 {
                        let shifted = self.bin(BinOp::LShr, differ.clone(), 8 * byte);
                        let one = self.bin(BinOp::And, shifted, 0xff);
                        let same = self.cmp(ICond::Eq, one, 0);
                        any = self.select(same, 1, any);
                    }
                    self.set(T, any);
                }
                0xd => {
                    let (a, b) = (self.get(n), self.get(m));
                    let (high, low) = (self.bin(BinOp::Shl, b, 16), self.bin(BinOp::LShr, a, 16));
                    let value = self.bin(BinOp::Or, high, low);
                    self.set(n, value);
                }
                _ => {
                    // The product of the low words, taken without their signs or with.
                    let (a, b) = (self.get(n), self.get(m));
                    let narrow = |lift: &mut Self, v: Operand| {
                        let word = lift.b.cast(lift.block, Cast::Trunc, I32, v, Type::I16, "word");
                        lift.b.cast(lift.block, if low == 0xe { Cast::ZExt } else { Cast::SExt }, Type::I16, word, I32, "word")
                    };
                    let (a, b) = (narrow(self, a), narrow(self, b));
                    let product = self.bin(BinOp::Mul, a, b);
                    self.set(MACL, product);
                }
            },
            0x3 => {
                let (a, b) = (self.get(n), self.get(m));
                match low {
                    0 | 2 | 3 | 6 | 7 => {
                        let cond = match low {
                            0 => ICond::Eq,
                            2 => ICond::Uge,
                            3 => ICond::Sge,
                            6 => ICond::Ugt,
                            _ => ICond::Sgt,
                        };
                        let is = self.cmp(cond, a, b);
                        self.set_bit(T, is);
                    }
                    4 => self.div1(n, a, b),
                    5 => self.multiply(a, b, false),
                    0xd => self.multiply(a, b, true),
                    8 => {
                        let value = self.bin(BinOp::Sub, a, b);
                        self.set(n, value);
                    }
                    0xc => {
                        let value = self.bin(BinOp::Add, a, b);
                        self.set(n, value);
                    }
                    0xa | 0xe => {
                        let (value, carried) = self.wide(if low == 0xa { BinOp::Sub } else { BinOp::Add }, a, b, true);
                        self.set(n, value);
                        self.set_bit(T, carried);
                    }
                    0xb | 0xf => {
                        // The result's sign is wrong for its operands' when it overflowed.
                        let op = if low == 0xb { BinOp::Sub } else { BinOp::Add };
                        let value = self.bin(op, a.clone(), b.clone());
                        let (x, y) = (self.bin(BinOp::Xor, a.clone(), value.clone()), self.bin(BinOp::Xor, a, b));
                        let y = if low == 0xb { y } else { self.bin(BinOp::Xor, y, -1) };
                        let both = self.bin(BinOp::And, x, y);
                        let overflowed = self.cmp(ICond::Slt, both, 0);
                        self.set(n, value);
                        self.set_bit(T, overflowed);
                    }
                    _ => return false,
                }
            }
            0x4 => {
                let value = self.get(n);
                match byte {
                    0x00 | 0x20 | 0x01 | 0x21 => {
                        let left = byte & 1 == 0;
                        let out = if left { self.cmp(ICond::Slt, value.clone(), 0) } else { self.odd(value.clone()) };
                        let op = if left {
                            BinOp::Shl
                        } else if byte == 0x21 {
                            BinOp::AShr
                        } else {
                            BinOp::LShr
                        };
                        let shifted = self.bin(op, value, 1);
                        self.set(n, shifted);
                        self.set_bit(T, out);
                    }
                    0x04 | 0x05 | 0x24 | 0x25 => {
                        // A rotate by one: through `T` or round the register itself.
                        let left = byte & 1 == 0;
                        let out = if left { self.cmp(ICond::Slt, value.clone(), 0) } else { self.odd(value.clone()) };
                        let carried = if byte & 0x20 != 0 {
                            self.get(T)
                        } else {
                            self.b.cast(self.block, Cast::ZExt, Type::I1, out, I32, "bit").into()
                        };
                        let rotated = if left {
                            let shifted = self.bin(BinOp::Shl, value, 1);
                            self.bin(BinOp::Or, shifted, carried)
                        } else {
                            let (shifted, top) = (self.bin(BinOp::LShr, value, 1), self.bin(BinOp::Shl, carried, 31));
                            self.bin(BinOp::Or, shifted, top)
                        };
                        self.set(n, rotated);
                        self.set_bit(T, out);
                    }
                    0x08 | 0x18 | 0x28 | 0x09 | 0x19 | 0x29 => {
                        let count = [2, 8, 16][(byte >> 4) as usize];
                        let shifted = self.bin(if byte & 1 == 0 { BinOp::Shl } else { BinOp::LShr }, value, count);
                        self.set(n, shifted);
                    }
                    0x10 => {
                        let less = self.bin(BinOp::Sub, value, 1);
                        self.set(n, less.clone());
                        let none = self.cmp(ICond::Eq, less, 0);
                        self.set_bit(T, none);
                    }
                    0x11 | 0x15 => {
                        let is = self.cmp(if byte == 0x11 { ICond::Sge } else { ICond::Sgt }, value, 0);
                        self.set_bit(T, is);
                    }
                    0x0a | 0x1a | 0x2a => self.set([MACH, MACL, PR][(byte >> 4) as usize], value),
                    0x1e => self.set(GBR, value),
                    0x02 | 0x12 | 0x22 => {
                        let at = self.bin(BinOp::Sub, value, 4);
                        self.set(n, at.clone());
                        let saved = self.get([MACH, MACL, PR][(byte >> 4) as usize]);
                        self.store(4, saved, at);
                    }
                    0x06 | 0x16 | 0x26 => {
                        let loaded = self.load(4, value.clone());
                        let after = self.bin(BinOp::Add, value, 4);
                        self.set(n, after);
                        self.set([MACH, MACL, PR][(byte >> 4) as usize], loaded);
                    }
                    _ if low == 0xc || low == 0xd => {
                        let count = self.get(m);
                        let shifted = self.shift(value, count, low == 0xc);
                        self.set(n, shifted);
                    }
                    _ => return false,
                }
            }
            0x5 => {
                let base = self.get(m);
                let at = self.bin(BinOp::Add, base, 4 * disp4);
                let value = self.load(4, at);
                self.set(n, value);
            }
            0x6 => {
                let source = self.get(m);
                let value = match low {
                    0..=2 => self.load(1 << low, source),
                    3 => source,
                    4..=6 => {
                        let size = 1 << (low - 4);
                        let value = self.load(size, source.clone());
                        // The address moves up unless the register is the one loaded.
                        if n != m {
                            let after = self.bin(BinOp::Add, source, i64::from(size));
                            self.set(m, after);
                        }
                        value
                    }
                    7 => self.bin(BinOp::Xor, source, -1),
                    8 => {
                        let (high, a, b) = (
                            self.bin(BinOp::And, source.clone(), 0xffff_0000u32),
                            self.bin(BinOp::Shl, source.clone(), 8),
                            self.bin(BinOp::LShr, source, 8),
                        );
                        let (a, b) = (self.bin(BinOp::And, a, 0xff00), self.bin(BinOp::And, b, 0xff));
                        let low = self.bin(BinOp::Or, a, b);
                        self.bin(BinOp::Or, high, low)
                    }
                    9 => {
                        let (a, b) = (self.bin(BinOp::Shl, source.clone(), 16), self.bin(BinOp::LShr, source, 16));
                        self.bin(BinOp::Or, a, b)
                    }
                    0xa => {
                        let (value, borrowed) = self.wide(BinOp::Sub, Operand::Int(0), source, true);
                        self.set_bit(T, borrowed);
                        value
                    }
                    0xb => self.bin(BinOp::Sub, 0, source),
                    0xc => self.bin(BinOp::And, source, 0xff),
                    0xd => self.bin(BinOp::And, source, 0xffff),
                    _ => {
                        let ty = if low == 0xe { Type::I8 } else { Type::I16 };
                        let narrow = self.b.cast(self.block, Cast::Trunc, I32, source, ty.clone(), "narrow");
                        self.b.cast(self.block, Cast::SExt, ty, narrow, I32, "v").into()
                    }
                };
                self.set(n, value);
            }
            0x7 => {
                let value = self.get(n);
                let sum = self.bin(BinOp::Add, value, imm);
                self.set(n, sum);
            }
            0x8 => match n {
                0 | 1 => {
                    let size = 1 << n;
                    let (base, value) = (self.get(m), self.get(0));
                    let at = self.bin(BinOp::Add, base, i64::from(size) * disp4);
                    self.store(size, value, at);
                }
                4 | 5 => {
                    let size = 1 << (n - 4);
                    let base = self.get(m);
                    let at = self.bin(BinOp::Add, base, i64::from(size) * disp4);
                    let value = self.load(size, at);
                    self.set(0, value);
                }
                8 => {
                    let value = self.get(0);
                    let is = self.cmp(ICond::Eq, value, imm);
                    self.set_bit(T, is);
                }
                _ => return false,
            },
            // A constant from the code: a word or a longword after the instruction.
            0x9 => {
                let value = self.load(2, pc + 4 + 2 * i64::from(byte));
                self.set(n, value);
            }
            0xd => {
                let value = self.load(4, (pc & !3) + 4 + 4 * i64::from(byte));
                self.set(n, value);
            }
            0xc => match n {
                0..=2 | 4..=6 => {
                    let size = 1 << (n & 3);
                    let base = self.get(GBR);
                    let at = self.bin(BinOp::Add, base, i64::from(size) * i64::from(byte));
                    if n < 4 {
                        let value = self.get(0);
                        self.store(size, value, at);
                    } else {
                        let value = self.load(size, at);
                        self.set(0, value);
                    }
                }
                7 => self.set(0, (pc & !3) + 4 + 4 * i64::from(byte)),
                8 => {
                    let value = self.get(0);
                    let both = self.bin(BinOp::And, value, i64::from(byte));
                    let none = self.cmp(ICond::Eq, both, 0);
                    self.set_bit(T, none);
                }
                9..=0xb => {
                    let value = self.get(0);
                    let value = self.bin([BinOp::And, BinOp::Xor, BinOp::Or][(n - 9) as usize], value, i64::from(byte));
                    self.set(0, value);
                }
                _ => return false,
            },
            0xe => self.set(n, imm),
            _ => return false,
        }
        true
    }

    /// Whether a value's lowest bit is set.
    fn odd(&mut self, value: Operand) -> ValueId {
        let bit = self.bin(BinOp::And, value, 1);
        self.cmp(ICond::Ne, bit, 0)
    }

    /// One step of a division: the dividend moves left through `T`, the divisor comes off or
    /// goes on by what `Q` and `M` were, and `Q` and `T` say which the next step does.
    fn div1(&mut self, n: u64, dividend: Operand, divisor: Operand) {
        let (old_q, m_bit, t) = (self.get(Q), self.get(M), self.get(T));
        let top = self.cmp(ICond::Slt, dividend.clone(), 0);
        let top: Operand = self.b.cast(self.block, Cast::ZExt, Type::I1, top, I32, "q").into();
        let shifted = self.bin(BinOp::Shl, dividend, 1);
        let before = self.bin(BinOp::Or, shifted, t);
        let subtract = self.cmp(ICond::Eq, old_q, m_bit.clone());
        let (less, more) = (self.bin(BinOp::Sub, before.clone(), divisor.clone()), self.bin(BinOp::Add, before.clone(), divisor));
        let after = self.select(subtract, less, more);
        // The borrow of the subtraction, or the carry of the addition.
        let (borrowed, carried) = (self.cmp(ICond::Ugt, after.clone(), before.clone()), self.cmp(ICond::Ult, after.clone(), before));
        let (borrowed, carried): (Operand, Operand) = (
            self.b.cast(self.block, Cast::ZExt, Type::I1, borrowed, I32, "bit").into(),
            self.b.cast(self.block, Cast::ZExt, Type::I1, carried, I32, "bit").into(),
        );
        let out = self.select(subtract, borrowed, carried);
        let q = self.bin(BinOp::Xor, top, out);
        let q = self.bin(BinOp::Xor, q, m_bit.clone());
        self.set(n, after);
        self.set(Q, q.clone());
        let same = self.cmp(ICond::Eq, q, m_bit);
        self.set_bit(T, same);
    }
}

/// Lifts `code` into a module with the one function [`RUN`].
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn lift(code: Code<'_>) -> Module {
    let mut module = Module::x86_64_windows("sh4");
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
    declare(&mut module, TRAPA, vec![Type::Ptr, Type::I64]);
    declare(&mut module, TRAP, vec![Type::I64]);
    let mut b = FunctionBuilder::new(RUN, FnType::new(Type::Void, vec![Type::Ptr, Type::I64]), &["state", "start"]);
    let (entry, dispatch, nowhere) = (b.block("entry"), b.block("dispatch"), b.block("nowhere"));
    let state = b.param(0);
    // Where a jump through a register goes, for the dispatcher to find the block of.
    let pc = b.alloca(entry, Type::I64, "pc");
    let start = b.param(1);
    b.store(entry, Type::I64, start, pc);
    b.terminate(entry, Terminator::Br(dispatch));
    let words: Vec<(u64, u16)> =
        code.bytes.chunks_exact(2).enumerate().map(|(n, w)| (code.address + 2 * n as u64, u16::from_le_bytes([w[0], w[1]]))).collect();
    let mut blocks: BTreeMap<u64, BlockId> = BTreeMap::new();
    for &(address, _) in &words {
        b.address = Some(address);
        blocks.insert(address, b.block(&format!("at.{address:x}")));
    }
    let word_at: BTreeMap<u64, u16> = words.iter().copied().collect();
    let mut lift = Lift { b, state, block: entry };
    let trap = |lift: &mut Lift, address: Operand| {
        lift.b.call(lift.block, FnType::new(Type::Void, vec![Type::I64]), Callee::Direct(TRAP.to_owned()), vec![(Type::I64, address)], "");
        lift.b.terminate(lift.block, Terminator::Ret(None));
    };
    for &(address, word) in &words {
        lift.block = blocks[&address];
        lift.b.address = Some(address);
        let at = |target: u64| blocks.get(&target).copied().unwrap_or(nowhere);
        let w = u32::from(word);
        let n = u64::from((w >> 8) & 15);
        let next = address + 2;
        // The transfers: which it is, where it goes when the code says, and whether the
        // instruction after it runs first.
        let near =
            |bits: u32, width: u32| address.wrapping_add(4).wrapping_add_signed((i64::from(bits) << (64 - width) >> (64 - width)) * 2);
        let transfer = match (w >> 12, w & 0xff, w & 0xf0ff) {
            (0x8, _, _) if matches!(n, 0x9 | 0xb | 0xd | 0xf) => Some((Some(near(w & 0xff, 8)), n >= 0xd)),
            (0xa | 0xb, _, _) => Some((Some(near(w & 0xfff, 12)), true)),
            (_, _, 0x0003 | 0x0023 | 0x400b | 0x402b) => Some((None, true)),
            (0x0, 0x0b, _) if word == 0x000b => Some((None, true)),
            _ => None,
        };
        let Some((target, delayed)) = transfer else {
            if word >> 8 == 0xc3 {
                let args = vec![(Type::Ptr, state.into()), (Type::I64, Operand::Int(i128::from(w & 0xff)))];
                lift.b.call(lift.block, FnType::new(Type::Void, vec![Type::Ptr, Type::I64]), Callee::Direct(TRAPA.to_owned()), args, "");
            } else if !lift.simple(word, address) {
                trap(&mut lift, Operand::Int(i128::from(address)));
                continue;
            }
            lift.b.terminate(lift.block, Terminator::Br(at(next)));
            continue;
        };
        // What the transfer decides, it decides on the state as it found it.
        let conditional = w >> 12 == 0x8;
        let taken = conditional.then(|| {
            let t = lift.get(T);
            lift.cmp(if matches!(n, 0x9 | 0xd) { ICond::Ne } else { ICond::Eq }, t, 0)
        });
        if target.is_none() {
            let through: Operand = match w & 0xf0ff {
                0x0003 | 0x0023 => {
                    let offset = lift.get(n);
                    lift.bin(BinOp::Add, offset, (address + 4) as i64)
                }
                0x400b | 0x402b => lift.get(n),
                _ => lift.get(PR),
            };
            let wide = lift.b.cast(lift.block, Cast::ZExt, I32, through, Type::I64, "target");
            lift.b.store(lift.block, Type::I64, wide, pc);
        }
        // `bsr`, `bsrf` and `jsr` leave where to come back to: past the delay slot.
        if w >> 12 == 0xb || matches!(w & 0xf0ff, 0x0003 | 0x400b) {
            lift.set(PR, (address + 4) as i64);
        }
        let after = if delayed {
            match word_at.get(&next) {
                Some(&slot) if lift.simple(slot, next) => next + 2,
                _ => {
                    trap(&mut lift, Operand::Int(i128::from(next)));
                    continue;
                }
            }
        } else {
            next
        };
        let goes = match (target, taken) {
            (Some(target), Some(taken)) => Terminator::CondBr { cond: taken.into(), then: at(target), els: at(after) },
            (Some(target), None) => Terminator::Br(at(target)),
            (None, _) => Terminator::Br(dispatch),
        };
        lift.b.terminate(lift.block, goes);
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
