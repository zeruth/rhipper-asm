//! An MSP430 instruction as `llvm-objdump` writes it.
//!
//! The processor has few instructions, and its assemblers have names for the uses the constant
//! registers make cheap: a move of zero is `clr`, a pop is a move from the stack with a step,
//! `ret` is that pop into the program counter. The disassembler writes those names, and so
//! does this.

use crate::decode::{Cond, Decoded, One, Op, Operand, PC, SP, SR, Two, decode};

fn operand(operand: Operand) -> String {
    match operand {
        Operand::Reg(reg) => format!("r{reg}"),
        Operand::Indexed { reg, offset } => format!("{}(r{reg})", offset.cast_signed()),
        Operand::Absolute(address) => format!("&{address}"),
        Operand::Symbolic(address) => format!("{address}"),
        Operand::Indirect(reg) => format!("@r{reg}"),
        Operand::IndirectInc(reg) => format!("@r{reg}+"),
        // A constant a register stands for is written with its sign; one of a word, without.
        Operand::Imm { value, generated: true } => format!("#{}", value.cast_signed()),
        Operand::Imm { value, generated: false } => format!("#{value}"),
    }
}

/// The text of a decoded instruction at `address`.
#[must_use]
pub fn text(d: &Decoded, address: u16) -> String {
    let b = if d.byte { ".b" } else { "" };
    let constant = |o: Operand, n: u16| matches!(o, Operand::Imm { value, generated: true } if value == n);
    match d.op {
        Op::Reti => "reti".to_owned(),
        Op::Jump { cond, target } => {
            let name = match cond {
                Cond::Ne => "jne",
                Cond::Eq => "jeq",
                Cond::Nc => "jlo",
                Cond::C => "jhs",
                Cond::N => "jn",
                Cond::Ge => "jge",
                Cond::L => "jl",
                Cond::Always => "jmp",
            };
            format!("{name}\t${:+}", target.wrapping_sub(address).cast_signed())
        }
        Op::One { op, operand: o } => {
            let name = match op {
                One::Rrc => "rrc",
                One::Swpb => "swpb",
                One::Rra => "rra",
                One::Sxt => "sxt",
                One::Push => "push",
                One::Call => "call",
            };
            format!("{name}{b}\t{}", operand(o))
        }
        Op::Two { op, src, dst } => {
            // The names of the cheap uses.
            let alias = match (op, src, dst) {
                (Two::Mov, Operand::IndirectInc(SP), Operand::Reg(PC)) if !d.byte => Some("ret".to_owned()),
                (Two::Mov, Operand::IndirectInc(SP), _) => Some(format!("pop{b}\t{}", operand(dst))),
                (Two::Mov, _, Operand::Reg(PC)) if !d.byte => Some(format!("br\t{}", operand(src))),
                (Two::Mov, _, _) if constant(src, 0) => Some(format!("clr{b}\t{}", operand(dst))),
                (Two::Cmp, _, _) if constant(src, 0) => Some(format!("tst{b}\t{}", operand(dst))),
                (Two::Add, _, _) if constant(src, 1) => Some(format!("inc{b}\t{}", operand(dst))),
                (Two::Add, _, _) if constant(src, 2) => Some(format!("incd{b}\t{}", operand(dst))),
                (Two::Sub, _, _) if constant(src, 1) => Some(format!("dec{b}\t{}", operand(dst))),
                (Two::Sub, _, _) if constant(src, 2) => Some(format!("decd{b}\t{}", operand(dst))),
                (Two::Xor, _, _) if constant(src, 0xffff) => Some(format!("inv{b}\t{}", operand(dst))),
                (Two::Bic, _, Operand::Reg(SR)) if constant(src, 1) && !d.byte => Some("clrc".to_owned()),
                (Two::Bis, _, Operand::Reg(SR)) if constant(src, 1) && !d.byte => Some("setc".to_owned()),
                _ => None,
            };
            if let Some(alias) = alias {
                return alias;
            }
            let name = match op {
                Two::Mov => "mov",
                Two::Add => "add",
                Two::Addc => "addc",
                Two::Subc => "subc",
                Two::Sub => "sub",
                Two::Cmp => "cmp",
                Two::Dadd => "dadd",
                Two::Bit => "bit",
                Two::Bic => "bic",
                Two::Bis => "bis",
                Two::Xor => "xor",
                Two::And => "and",
            };
            format!("{name}{b}\t{}, {}", operand(src), operand(dst))
        }
    }
}

/// The instruction at `address` as text, a tab between its name and its operands, and its
/// length in bytes; `None` where the words are no instruction.
#[must_use]
pub fn disassemble(words: &[u16], address: u16) -> Option<(String, u16)> {
    let d = decode(words, address)?;
    Some((text(&d, address), d.len))
}
