//! An MSP430 instruction taken apart.
//!
//! An instruction is one word and up to two more, one for each operand that needs a number: an
//! offset from a register, an address, or a constant. Its words are little-endian. Three shapes
//! cover the set: an operation of two operands (a source in one of seven modes, a destination
//! in one of four), an operation of one, and a jump on a condition by a signed count of words.
//!
//! Two registers are not registers as a source. `r2`, the status register, and `r3` stand for
//! the constants a program wants most (4, 8, 0, 1, 2 and -1) by the mode they are named in, so
//! those cost no second word; `r2` with an offset is an absolute address. `r0` is the program
//! counter: with an offset it names a place by its distance, and `@r0+` is the word after the
//! instruction, a constant.

/// The program counter, the stack pointer, the status register, and the second register that
/// stands for constants.
pub const PC: u8 = 0;
pub const SP: u8 = 1;
pub const SR: u8 = 2;
pub const CG: u8 = 3;

/// Where an operand is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    Reg(u8),
    /// The memory at a register's value and an offset.
    Indexed {
        reg: u8,
        offset: u16,
    },
    /// The memory at an address the instruction gives.
    Absolute(u16),
    /// The memory at an address the instruction gives by its distance from itself.
    Symbolic(u16),
    /// The memory a register points at.
    Indirect(u8),
    /// The same, the register stepped past it after.
    IndirectInc(u8),
    /// A constant: the word after the instruction, or one a register stands for (`generated`),
    /// which costs no word.
    Imm {
        value: u16,
        generated: bool,
    },
}

/// The operations of two operands, by the instruction's top four bits from four.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Two {
    Mov,
    Add,
    Addc,
    Subc,
    Sub,
    Cmp,
    Dadd,
    Bit,
    Bic,
    Bis,
    Xor,
    And,
}

/// The operations of one operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum One {
    Rrc,
    Swpb,
    Rra,
    Sxt,
    Push,
    Call,
}

/// The conditions of a jump, by the three bits that name them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cond {
    Ne,
    Eq,
    Nc,
    C,
    N,
    Ge,
    L,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Two { op: Two, src: Operand, dst: Operand },
    One { op: One, operand: Operand },
    Jump { cond: Cond, target: u16 },
    Reti,
}

/// One instruction: what it does, whether on bytes, and its length in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decoded {
    pub op: Op,
    pub byte: bool,
    pub len: u16,
}

const TWO: [Two; 12] =
    [Two::Mov, Two::Add, Two::Addc, Two::Subc, Two::Sub, Two::Cmp, Two::Dadd, Two::Bit, Two::Bic, Two::Bis, Two::Xor, Two::And];
const ONE: [One; 6] = [One::Rrc, One::Swpb, One::Rra, One::Sxt, One::Push, One::Call];
const COND: [Cond; 8] = [Cond::Ne, Cond::Eq, Cond::Nc, Cond::C, Cond::N, Cond::Ge, Cond::L, Cond::Always];

/// The words after an instruction's first, taken as its operands ask.
struct Words<'a> {
    words: &'a [u16],
    used: usize,
    address: u16,
}

impl Words<'_> {
    /// The next word, and the address it lies at.
    fn next(&mut self) -> Option<(u16, u16)> {
        let word = *self.words.get(1 + self.used)?;
        self.used += 1;
        Some((word, self.address.wrapping_add(2 * self.used as u16)))
    }

    /// A source, by its register and its two mode bits.
    fn source(&mut self, reg: u8, mode: u16) -> Option<Operand> {
        let constant = |value| Some(Operand::Imm { value, generated: true });
        match (reg, mode) {
            (CG, 0) => constant(0),
            (CG, 1) => constant(1),
            (CG, 2) => constant(2),
            (CG, _) => constant(0xffff),
            (SR, 2) => constant(4),
            (SR, 3) => constant(8),
            (_, 0) => Some(Operand::Reg(reg)),
            (_, 1) => self.indexed(reg),
            (_, 2) => Some(Operand::Indirect(reg)),
            (PC, _) => self.next().map(|(value, _)| Operand::Imm { value, generated: false }),
            _ => Some(Operand::IndirectInc(reg)),
        }
    }

    /// A register with an offset: an address of its own with the status register, a distance
    /// with the program counter.
    fn indexed(&mut self, reg: u8) -> Option<Operand> {
        let (offset, at) = self.next()?;
        Some(match reg {
            SR => Operand::Absolute(offset),
            PC => Operand::Symbolic(at.wrapping_add(offset)),
            _ => Operand::Indexed { reg, offset },
        })
    }
}

/// The instruction that starts with `words[0]` at `address`, or `None` where the words are
/// none the processor has, or end before the instruction does.
#[must_use]
pub fn decode(words: &[u16], address: u16) -> Option<Decoded> {
    let w = *words.first()?;
    let mut rest = Words { words, used: 0, address };
    let byte = w & 0x40 != 0;
    let (mode, reg) = ((w >> 4) & 3, (w & 15) as u8);
    let op = match w >> 12 {
        1 if w & 0x0c00 == 0 => {
            let which = ((w >> 7) & 7) as usize;
            if which == 6 {
                // `reti` is the one word.
                (w == 0x1300).then_some(Op::Reti)?
            } else {
                let op = *ONE.get(which)?;
                // Only the rotates and a push are of bytes.
                if byte && matches!(op, One::Swpb | One::Sxt | One::Call) {
                    return None;
                }
                Op::One { op, operand: rest.source(reg, mode)? }
            }
        }
        2 | 3 => {
            // Ten bits of words, signed, from the instruction after.
            let offset = ((w & 0x3ff) ^ 0x200).wrapping_sub(0x200);
            let target = address.wrapping_add(2).wrapping_add(offset.wrapping_mul(2));
            return Some(Decoded { op: Op::Jump { cond: COND[usize::from((w >> 10) & 7)], target }, byte: false, len: 2 });
        }
        top @ 4..=15 => {
            let src = rest.source(((w >> 8) & 15) as u8, mode)?;
            let dst = if w & 0x80 == 0 { Operand::Reg(reg) } else { rest.indexed(reg)? };
            Op::Two { op: TWO[usize::from(top) - 4], src, dst }
        }
        _ => return None,
    };
    Some(Decoded { op, byte, len: 2 + 2 * rest.used as u16 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shapes_decode() {
        // mov r8, r12
        assert_eq!(
            decode(&[0x480c], 0x50cc),
            Some(Decoded { op: Op::Two { op: Two::Mov, src: Operand::Reg(8), dst: Operand::Reg(12) }, byte: false, len: 2 })
        );
        // mov.b #10, 17(r1): a constant and an offset, a word each
        assert_eq!(
            decode(&[0x40f1, 0x000a, 0x0011], 0x5d4e),
            Some(Decoded {
                op: Op::Two {
                    op: Two::Mov,
                    src: Operand::Imm { value: 10, generated: false },
                    dst: Operand::Indexed { reg: 1, offset: 17 }
                },
                byte: true,
                len: 6
            })
        );
        // add #8, r1: the constant is the status register's in its fourth mode
        assert_eq!(
            decode(&[0x5231], 0x50d8),
            Some(Decoded {
                op: Op::Two { op: Two::Add, src: Operand::Imm { value: 8, generated: true }, dst: Operand::Reg(1) },
                byte: false,
                len: 2
            })
        );
        // mov.b r10, &514
        assert_eq!(
            decode(&[0x4ac2, 0x0202], 0x5d04),
            Some(Decoded { op: Op::Two { op: Two::Mov, src: Operand::Reg(10), dst: Operand::Absolute(0x202) }, byte: true, len: 4 })
        );
        // jmp $-50 and jeq $+46
        assert_eq!(
            decode(&[0x3fe6], 0x5120),
            Some(Decoded { op: Op::Jump { cond: Cond::Always, target: 0x5120 - 50 }, byte: false, len: 2 })
        );
        assert_eq!(decode(&[0x2416], 0x50f4), Some(Decoded { op: Op::Jump { cond: Cond::Eq, target: 0x50f4 + 46 }, byte: false, len: 2 }));
        // call #21458, and the pop that returns
        assert_eq!(
            decode(&[0x12b0, 0x53d2], 0x50d4),
            Some(Decoded { op: Op::One { op: One::Call, operand: Operand::Imm { value: 21458, generated: false } }, byte: false, len: 4 })
        );
        assert_eq!(
            decode(&[0x4130], 0x50da),
            Some(Decoded { op: Op::Two { op: Two::Mov, src: Operand::IndirectInc(1), dst: Operand::Reg(0) }, byte: false, len: 2 })
        );
        // a distance from the program counter, counted from the word that holds it
        assert_eq!(
            decode(&[0x4014, 0x0010], 0x4000).map(|d| d.op),
            Some(Op::Two { op: Two::Mov, src: Operand::Symbolic(0x4012), dst: Operand::Reg(4) })
        );
    }

    #[test]
    fn what_is_no_instruction_is_refused() {
        // Below the operations of one operand there is nothing, and their seventh is `reti` alone.
        assert_eq!(decode(&[0x0000], 0), None);
        assert_eq!(decode(&[0x1380], 0), None);
        assert_eq!(decode(&[0x1300], 0).map(|d| d.op), Some(Op::Reti));
        // A swap of a byte's halves is none; an instruction cut before its second word is none.
        assert_eq!(decode(&[0x10c5], 0), None);
        assert_eq!(decode(&[0x40f1, 0x000a], 0), None);
        assert_eq!(decode(&[], 0), None);
    }
}
