//! The lift of a RISC-V program at the machine's own level: its code as one function of the IR
//! over the machine's state.
//!
//! [`RUN`] takes the state (the thirty-two registers, eight bytes each; `x0` reads as zero and
//! keeps nothing) and the address to start at. Every instruction of the code is a block that
//! reads its registers from the state, computes, and writes its result back; a branch and a
//! `jal` go to the block of their target, and a jump through a register (`jalr`: a return, a
//! call through a pointer, a switch) goes to the dispatcher, which finds the block of the
//! address. Memory is the program's own addresses. An `ecall` is a call of [`ECALL`] with the
//! state, which the runtime answers; a word that decodes to nothing, and an address that is no
//! instruction's, are a call of [`TRAP`].
//!
//! The machine's corners are written out: a shift counts by the low bits of its count, the
//! word instructions compute on thirty-two bits and extend the sign, the high halves of a product
//! come from the product at twice the width, and a division by zero or the one that overflows
//! has the result the machine gives it and traps nothing.
//!
//! This is the level a console's kernel is lifted at: it says what the machine does and nothing
//! of functions. The functions, their parameters and their variables are the raise's to find.

use std::collections::BTreeMap;

use ra_asm_ir::{
    BinOp, BlockId, CallConv, Callee, Cast, Declaration, FnType, FunctionBuilder, ICond, Module, Operand, Terminator, Type, ValueId,
};

use crate::{Form, Instruction, Width};

/// The function the lift writes: `void (ptr state, i64 pc)`. The names here are ones a C
/// compiler takes, so the module goes through the C backend as it is.
pub const RUN: &str = "rv_run";
/// The runtime's answer to an `ecall`: `void (ptr state)`.
pub const ECALL: &str = "rv_ecall";
/// What code that is no code calls, with its address: `void (i64 pc)`.
pub const TRAP: &str = "rv_trap";
/// Where the program's memory is: eight bytes that hold the address the program's address zero
/// is at. An interpreter that maps the program at its own addresses leaves it zero; a port on a
/// host that cannot gives the program a block of its own and says where.
pub const MEMORY: &str = "rv_memory";
/// The function of a whole program ([`crate::run::Program::lift`]): `void (ptr state)`, which
/// hands the runtime each segment and runs the code from the program's entry.
pub const MAIN: &str = "rv_main";
/// The runtime's taking of a segment: `void (i64 address, ptr bytes, i64 in_file, i64 size)`.
pub const SEGMENT: &str = "rv_segment";

/// The code of a program: its bytes and the address of the first.
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

impl Lift {
    /// The value of register `n`.
    fn get(&mut self, n: u8) -> Operand {
        if n == 0 {
            return Operand::Int(0);
        }
        let at = self.b.gep(self.block, Type::I8, self.state, vec![(Type::I64, Operand::Int(8 * i128::from(n)))], "at");
        self.b.load(self.block, Type::I64, at, crate::REGISTERS[usize::from(n)]).into()
    }

    fn set(&mut self, n: u8, value: impl Into<Operand>) {
        if n != 0 {
            let at = self.b.gep(self.block, Type::I8, self.state, vec![(Type::I64, Operand::Int(8 * i128::from(n)))], "at");
            self.b.store(self.block, Type::I64, value, at);
        }
    }

    fn bin(&mut self, op: BinOp, ty: Type, a: impl Into<Operand>, b: impl Into<Operand>) -> ValueId {
        self.b.bin(self.block, op, ty, a, b, "v")
    }

    fn cast(&mut self, cast: Cast, from: Type, value: impl Into<Operand>, to: Type) -> ValueId {
        self.b.cast(self.block, cast, from, value, to, "v")
    }

    /// The low word of a value with its sign extended: what a word instruction leaves.
    fn word(&mut self, value: impl Into<Operand>) -> ValueId {
        let low = self.cast(Cast::Trunc, Type::I64, value, Type::I32);
        self.cast(Cast::SExt, Type::I32, low, Type::I64)
    }

    /// `a op b` of the multiply and divide extension at the width of `ty`, with the results the
    /// machine gives a division by zero and the division that overflows.
    fn divide(&mut self, mnemonic: &str, ty: &Type, a: Operand, b: Operand) -> ValueId {
        let bits = if *ty == Type::I32 { 32 } else { 64 };
        let least = Operand::Int(-(1i128 << (bits - 1)));
        let zero = self.b.icmp(self.block, ICond::Eq, ty.clone(), b.clone(), 0, "zero");
        let signed = matches!(mnemonic.trim_end_matches('w'), "div" | "rem");
        // The divisor the division is made by where the machine has an answer of its own.
        let (overflows, by) = if signed {
            let least_is = self.b.icmp(self.block, ICond::Eq, ty.clone(), a.clone(), least, "least");
            let minus = self.b.icmp(self.block, ICond::Eq, ty.clone(), b.clone(), -1, "minus");
            let overflows = self.bin(BinOp::And, Type::I1, least_is, minus);
            let either = self.bin(BinOp::Or, Type::I1, zero, overflows);
            (Some(overflows), self.b.select(self.block, either, ty.clone(), 1, b, "by"))
        } else {
            (None, self.b.select(self.block, zero, ty.clone(), 1, b, "by"))
        };
        let quotient = mnemonic.starts_with("div");
        let op = match (quotient, signed) {
            (true, true) => BinOp::SDiv,
            (true, false) => BinOp::UDiv,
            (false, true) => BinOp::SRem,
            (false, false) => BinOp::URem,
        };
        let mut result = self.bin(op, ty.clone(), a.clone(), by);
        if let Some(overflows) = overflows {
            // The least number by minus one is itself, and leaves nothing.
            let then = if quotient { a.clone() } else { Operand::Int(0) };
            result = self.b.select(self.block, overflows, ty.clone(), then, result, "v");
        }
        // By zero: every bit set, and the dividend left whole.
        let then = if quotient { Operand::Int(-1) } else { a };
        self.b.select(self.block, zero, ty.clone(), then, result, "v")
    }

    /// The high half of the product of two registers, each taken with its sign or without.
    fn high(&mut self, a: Operand, b: Operand, a_signed: bool, b_signed: bool) -> ValueId {
        let extend = |signed| if signed { Cast::SExt } else { Cast::ZExt };
        let (a, b) = (self.cast(extend(a_signed), Type::I64, a, Type::I128), self.cast(extend(b_signed), Type::I64, b, Type::I128));
        let product = self.bin(BinOp::Mul, Type::I128, a, b);
        let high = self.bin(BinOp::LShr, Type::I128, product, 64);
        self.cast(Cast::Trunc, Type::I128, high, Type::I64)
    }

    /// The pointer a guest address is.
    fn pointer(&mut self, base: u8, offset: i64) -> ValueId {
        let base = self.get(base);
        let address = self.bin(BinOp::Add, Type::I64, base, offset);
        // The program's address, from where its memory is.
        let memory = self.b.load(self.block, Type::I64, Operand::Global(MEMORY.to_owned()), "memory");
        let address = self.bin(BinOp::Add, Type::I64, address, memory);
        self.cast(Cast::IntToPtr, Type::I64, address, Type::Ptr)
    }
}

/// Lifts `code` for a machine of `width` into a module with the one function [`RUN`].
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn lift(code: Code<'_>, width: Width) -> Module {
    let mut module = Module::x86_64_windows("riscv");
    module.globals.push(ra_asm_ir::Global {
        name: MEMORY.to_owned(),
        ty: Type::array(8, Type::I8),
        init: Some(ra_asm_ir::Const::Bytes(vec![0; 8])),
        constant: false,
        linkage: ra_asm_ir::Linkage::External,
        align: 8,
        address: None,
    });
    let declare = |module: &mut Module, name: &str, params: Vec<Type>| {
        module.declare(Declaration { name: name.to_owned(), ty: FnType::new(Type::Void, params), conv: CallConv::C, attrs: Vec::new() });
    };
    declare(&mut module, ECALL, vec![Type::Ptr]);
    declare(&mut module, TRAP, vec![Type::I64]);
    let mut b = FunctionBuilder::new(RUN, FnType::new(Type::Void, vec![Type::Ptr, Type::I64]), &["state", "start"]);
    let entry = b.block("entry");
    let dispatch = b.block("dispatch");
    let nowhere = b.block("nowhere");
    let state = b.param(0);
    // Where a jump through a register goes, for the dispatcher to find the block of.
    let pc = b.alloca(entry, Type::I64, "pc");
    let start = b.param(1);
    b.store(entry, Type::I64, start, pc);
    b.terminate(entry, Terminator::Br(dispatch));
    // The instructions in the order they lie: each with its address and its length, two bytes
    // for a compressed one and four for the rest; what decodes to nothing keeps its length.
    let mut words: Vec<(u64, Option<Instruction>, u64)> = Vec::new();
    let mut offset = 0;
    while offset + 2 <= code.bytes.len() {
        let address = code.address + offset as u64;
        let (instruction, length) = crate::decode_at(&code.bytes[offset..], address, width);
        words.push((address, instruction, u64::from(length)));
        offset += usize::from(length);
    }
    let mut blocks: BTreeMap<u64, BlockId> = BTreeMap::new();
    for &(address, ..) in &words {
        b.address = Some(address);
        blocks.insert(address, b.block(&format!("at.{address:x}")));
    }
    let mut lift = Lift { b, state, block: entry };
    // An address that is no instruction's: the runtime hears of it, and the function is left.
    let trap = |lift: &mut Lift, block: BlockId, address: Operand| {
        lift.b.call(block, FnType::new(Type::Void, vec![Type::I64]), Callee::Direct(TRAP.to_owned()), vec![(Type::I64, address)], "");
        lift.b.terminate(block, Terminator::Ret(None));
    };
    for &(address, instruction, length) in &words {
        lift.block = blocks[&address];
        lift.b.address = Some(address);
        let next = address + length;
        let at = |target: u64| blocks.get(&target).copied().unwrap_or(nowhere);
        let Some(Instruction { mnemonic, form, rd, rs1, rs2, imm }) = instruction else {
            trap(&mut lift, blocks[&address], Operand::Int(i128::from(address)));
            continue;
        };
        let mut goes = Terminator::Br(at(next));
        match form {
            Form::Upper => {
                let upper = i64::from((imm as i32) << 12);
                lift.set(rd, if mnemonic == "lui" { upper } else { address.wrapping_add_signed(upper).cast_signed() });
            }
            Form::Jump => {
                lift.set(rd, next);
                goes = Terminator::Br(at(imm.cast_unsigned()));
            }
            Form::JumpRegister => {
                let base = lift.get(rs1);
                let target = lift.bin(BinOp::Add, Type::I64, base, imm);
                let target = lift.bin(BinOp::And, Type::I64, target, -2);
                lift.set(rd, next);
                lift.b.store(lift.block, Type::I64, target, pc);
                goes = Terminator::Br(dispatch);
            }
            Form::Branch => {
                let cond = match mnemonic {
                    "beq" => ICond::Eq,
                    "bne" => ICond::Ne,
                    "blt" => ICond::Slt,
                    "bge" => ICond::Sge,
                    "bltu" => ICond::Ult,
                    _ => ICond::Uge,
                };
                let (a, b) = (lift.get(rs1), lift.get(rs2));
                let taken = lift.b.icmp(lift.block, cond, Type::I64, a, b, "taken");
                goes = Terminator::CondBr { cond: taken.into(), then: at(imm.cast_unsigned()), els: at(next) };
            }
            Form::Load => {
                let (ty, signed) = match mnemonic {
                    "lb" => (Type::I8, true),
                    "lbu" => (Type::I8, false),
                    "lh" => (Type::I16, true),
                    "lhu" => (Type::I16, false),
                    "lw" => (Type::I32, true),
                    "lwu" => (Type::I32, false),
                    _ => (Type::I64, false),
                };
                let from = lift.pointer(rs1, imm);
                let mut value = lift.b.load(lift.block, ty.clone(), from, "loaded");
                if ty != Type::I64 {
                    value = lift.cast(if signed { Cast::SExt } else { Cast::ZExt }, ty, value, Type::I64);
                }
                lift.set(rd, value);
            }
            Form::Store => {
                let ty = match mnemonic {
                    "sb" => Type::I8,
                    "sh" => Type::I16,
                    "sw" => Type::I32,
                    _ => Type::I64,
                };
                let value = lift.get(rs2);
                let to = lift.pointer(rs1, imm);
                let value: Operand = if ty == Type::I64 { value } else { lift.cast(Cast::Trunc, Type::I64, value, ty.clone()).into() };
                lift.b.store(lift.block, ty, value, to);
            }
            Form::Immediate | Form::Register => {
                let a = lift.get(rs1);
                let b: Operand = if form == Form::Immediate { Operand::Int(i128::from(imm)) } else { lift.get(rs2) };
                let is_word = mnemonic.ends_with('w');
                let base = mnemonic.trim_end_matches('w').trim_end_matches('i');
                // `slti`'s name ends in the `i` of its immediate; `sltiu`'s has it inside.
                let base = if mnemonic.starts_with("sltiu") { "sltu" } else { base };
                let value = match base {
                    "add" | "sub" | "xor" | "or" | "and" | "mul" if !is_word => {
                        let op = match base {
                            "add" => BinOp::Add,
                            "sub" => BinOp::Sub,
                            "xor" => BinOp::Xor,
                            "or" => BinOp::Or,
                            "and" => BinOp::And,
                            _ => BinOp::Mul,
                        };
                        lift.bin(op, Type::I64, a, b)
                    }
                    "add" | "sub" | "mul" => {
                        let op = match base {
                            "add" => BinOp::Add,
                            "sub" => BinOp::Sub,
                            _ => BinOp::Mul,
                        };
                        let sum = lift.bin(op, Type::I64, a, b);
                        lift.word(sum)
                    }
                    "sll" | "srl" | "sra" => {
                        let op = match base {
                            "sll" => BinOp::Shl,
                            "srl" => BinOp::LShr,
                            _ => BinOp::AShr,
                        };
                        if is_word {
                            // On thirty-two bits, by the low five bits of the count.
                            let (low, count) =
                                (lift.cast(Cast::Trunc, Type::I64, a, Type::I32), lift.cast(Cast::Trunc, Type::I64, b, Type::I32));
                            let count = lift.bin(BinOp::And, Type::I32, count, 31);
                            let shifted = lift.bin(op, Type::I32, low, count);
                            lift.cast(Cast::SExt, Type::I32, shifted, Type::I64)
                        } else {
                            let mask = if width == Width::Rv64 { 63 } else { 31 };
                            let count = lift.bin(BinOp::And, Type::I64, b, mask);
                            lift.bin(op, Type::I64, a, count)
                        }
                    }
                    "slt" | "sltu" => {
                        let less = lift.b.icmp(lift.block, if base == "slt" { ICond::Slt } else { ICond::Ult }, Type::I64, a, b, "less");
                        lift.cast(Cast::ZExt, Type::I1, less, Type::I64)
                    }
                    "mulh" => lift.high(a, b, true, true),
                    "mulhsu" => lift.high(a, b, true, false),
                    "mulhu" => lift.high(a, b, false, false),
                    "div" | "divu" | "rem" | "remu" if is_word => {
                        let (a, b) = (lift.cast(Cast::Trunc, Type::I64, a, Type::I32), lift.cast(Cast::Trunc, Type::I64, b, Type::I32));
                        let result = lift.divide(mnemonic, &Type::I32, a.into(), b.into());
                        lift.cast(Cast::SExt, Type::I32, result, Type::I64)
                    }
                    "div" | "divu" | "rem" | "remu" => lift.divide(mnemonic, &Type::I64, a, b),
                    _ => {
                        trap(&mut lift, blocks[&address], Operand::Int(i128::from(address)));
                        continue;
                    }
                };
                lift.set(rd, value);
            }
            Form::System if mnemonic == "ecall" => {
                let state = lift.state;
                lift.b.call(
                    lift.block,
                    FnType::new(Type::Void, vec![Type::Ptr]),
                    Callee::Direct(ECALL.to_owned()),
                    vec![(Type::Ptr, state.into())],
                    "",
                );
            }
            Form::System => {
                trap(&mut lift, blocks[&address], Operand::Int(i128::from(address)));
                continue;
            }
        }
        lift.b.terminate(lift.block, goes);
    }
    // The dispatcher: the block of the address a register held.
    let target = lift.b.load(dispatch, Type::I64, pc, "target");
    let cases = blocks.iter().map(|(&address, &block)| (i128::from(address), block)).collect();
    lift.b.terminate(dispatch, Terminator::Switch { ty: Type::I64, value: target.into(), default: nowhere, cases });
    let target = lift.b.load(nowhere, Type::I64, pc, "target");
    trap(&mut lift, nowhere, target.into());
    module.functions.push(lift.b.finish());
    module
}
