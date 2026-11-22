//! The ARM instruction set of 32-bit ARM (A32): the code a handheld of the ARM7TDMI's time
//! starts in, and what its Thumb code changes to for what Thumb lacks.
//!
//! An instruction is one word, its condition in the top four bits. [`decode`] reads it as the
//! operation the Thumb decoder names ([`Op`]): the two sets say the same things of the same
//! machine, so one lift can serve both. The condition is the decoded instruction's `cond`, as an
//! IT block's is in Thumb; a `b` carries it itself. The PC an ARM instruction reads is its
//! address and eight.
//!
//! What decodes is the integer machine of the processors through ARMv7: the data processing
//! with every shift of its second source, the multiplies, the loads and stores of a word, a
//! byte, a halfword and a pair in every addressing form, the loads and stores of many in all
//! four directions, the branches, `bx`, `svc`, and the later bit fields, extensions, divides and
//! wide moves. The unconditional space, the coprocessors, the status-register moves and the
//! exclusive loads are [`Op::Unsupported`].

use ra_asm_arch::{Arch, Flow, Instr, Operand, Register, Target};

use crate::thumb::{Alu, Bits, Decoded, Offset, Op, PC, Shift, Source, record, register};

fn field(w: u32, hi: u32, lo: u32) -> u32 {
    (w >> lo) & ((1u32 << (hi - lo + 1)) - 1)
}

fn reg(w: u32, lo: u32) -> u8 {
    ((w >> lo) & 15) as u8
}

fn bit(w: u32, n: u32) -> bool {
    w & (1 << n) != 0
}

/// The shift a constant makes of a register: a right shift by none is by thirty-two, a rotate
/// by none is the one through the carry.
fn imm_shift(kind: u32, amount: u32) -> (Shift, u8) {
    let amount = amount as u8;
    match kind {
        0 => (Shift::Lsl, amount),
        1 => (Shift::Lsr, if amount == 0 { 32 } else { amount }),
        2 => (Shift::Asr, if amount == 0 { 32 } else { amount }),
        _ if amount == 0 => (Shift::Rrx, 1),
        _ => (Shift::Ror, amount),
    }
}

/// The instruction `word` at `address`. Every word decodes: one the decoder has no operation
/// for is [`Op::Unsupported`].
#[must_use]
pub fn decode(word: u32, address: u64) -> Decoded {
    let cond = (word >> 28) as u8;
    let op = if cond != 15 {
        operation(word, address, (cond < 14).then_some(cond))
    } else if field(word, 27, 25) == 0b101 {
        // `blx` to Thumb code: the offset in words, and a halfword more by bit 24.
        let offset = i64::from((word << 8).cast_signed() >> 6) + i64::from(field(word, 24, 24) << 1);
        Op::Call { target: address.wrapping_add(8).wrapping_add_signed(offset) & 0xffff_ffff, arm: true }
    } else {
        Op::Unsupported
    };
    Decoded { op, len: 4, raw: word, cond: (cond < 14).then_some(cond) }
}

fn operation(w: u32, address: u64, cond: Option<u8>) -> Op {
    let pc = address.wrapping_add(8);
    match field(w, 27, 25) {
        0b000 => {
            if w & 0x0fff_ffd0 == 0x012f_ff10 {
                return Op::BranchReg { rm: reg(w, 0), link: bit(w, 5) };
            }
            if w & 0x0ff0_00f0 == 0x0120_0070 {
                return Op::Trap; // bkpt
            }
            if w & 0x0f00_00f0 == 0x0000_0090 {
                return multiply(w);
            }
            if w & 0x0f00_00f0 == 0x0100_0090 {
                return Op::Unsupported; // swp and the exclusive loads and stores
            }
            if w & 0x90 == 0x90 {
                return halfword(w);
            }
            let (opcode, s) = (field(w, 24, 21), bit(w, 20));
            if w & 0x0fff_0ff0 == 0x016f_0f10 {
                return Op::Bits { kind: Bits::Clz, rd: reg(w, 12), rm: reg(w, 0) };
            }
            if (8..=11).contains(&opcode) && !s {
                return Op::Unsupported; // the status register's moves, the saturating sums
            }
            let src = if bit(w, 4) {
                let shift = [Shift::Lsl, Shift::Lsr, Shift::Asr, Shift::Ror][field(w, 6, 5) as usize];
                Source::RegReg { rm: reg(w, 0), shift, rs: reg(w, 8) }
            } else {
                let (shift, amount) = imm_shift(field(w, 6, 5), field(w, 11, 7));
                Source::Reg { rm: reg(w, 0), shift, amount }
            };
            data(opcode, s, reg(w, 12), reg(w, 16), src)
        }
        0b001 => {
            let (opcode, s, rd, rn) = (field(w, 24, 21), bit(w, 20), reg(w, 12), reg(w, 16));
            if (8..=11).contains(&opcode) && !s {
                let imm = (field(w, 19, 16) << 12 | field(w, 11, 0)) as u16;
                return match opcode {
                    8 => Op::MovWide { rd, imm, top: false },
                    10 => Op::MovWide { rd, imm, top: true },
                    9 if w & 0x0fff_ff00 == 0x0320_f000 => Op::Nop,
                    _ => Op::Unsupported, // a move to the status register
                };
            }
            // Eight bits rotated right by twice the four above them; a rotated one sets the carry.
            let rotation = 2 * field(w, 11, 8);
            let value = (w & 0xff).rotate_right(rotation);
            if !s && rn == PC && matches!(opcode, 2 | 4) {
                let target = if opcode == 4 { pc.wrapping_add(u64::from(value)) } else { pc.wrapping_sub(u64::from(value)) };
                return Op::Adr { rd, target: target & 0xffff_ffff };
            }
            data(opcode, s, rd, rn, Source::Imm { value, carry: (rotation != 0).then_some(value >> 31 != 0) })
        }
        0b010 => single(w, Offset::Imm(signed(w, field(w, 11, 0)))),
        0b011 if bit(w, 4) => media(w),
        0b011 => {
            let (shift, amount) = imm_shift(field(w, 6, 5), field(w, 11, 7));
            let offset = if bit(w, 23) && shift == Shift::Lsl {
                Offset::Reg { rm: reg(w, 0), shift: amount }
            } else {
                Offset::Shifted { rm: reg(w, 0), shift, amount, subtract: !bit(w, 23) }
            };
            single(w, offset)
        }
        0b100 => {
            if bit(w, 22) {
                return Op::Unsupported; // the other mode's registers, or the return of an exception
            }
            // The P and U bits: before or after, up or down.
            let (before, swapped) = match (bit(w, 24), bit(w, 23)) {
                (false, true) => (false, false),
                (true, false) => (true, false),
                (true, true) => (true, true),
                (false, false) => (false, true),
            };
            Op::Multiple { load: bit(w, 20), rn: reg(w, 16), list: w as u16, wback: bit(w, 21), before, swapped }
        }
        0b101 => {
            let offset = i64::from((w << 8).cast_signed() >> 6);
            let target = pc.wrapping_add_signed(offset) & 0xffff_ffff;
            if bit(w, 24) { Op::Call { target, arm: false } } else { Op::Branch { cond, target } }
        }
        // The number is the low byte of the comment, or the byte above it where a console's
        // BIOS reads its call's number (`swi 0x060000`).
        0b111 if bit(w, 24) => Op::Svc(if w & 0x00ff_0000 == 0 { w as u8 } else { (w >> 16) as u8 }),
        _ => Op::Unsupported, // the coprocessors
    }
}

/// An offset of the encoding's U bit: added, or taken away.
fn signed(w: u32, magnitude: u32) -> i32 {
    if bit(w, 23) { magnitude.cast_signed() } else { -magnitude.cast_signed() }
}

fn data(opcode: u32, s: bool, rd: u8, rn: u8, src: Source) -> Op {
    let op = [
        Alu::And,
        Alu::Eor,
        Alu::Sub,
        Alu::Rsb,
        Alu::Add,
        Alu::Adc,
        Alu::Sbc,
        Alu::Rsc,
        Alu::Tst,
        Alu::Teq,
        Alu::Cmp,
        Alu::Cmn,
        Alu::Orr,
        Alu::Mov,
        Alu::Bic,
        Alu::Mvn,
    ][opcode as usize];
    Op::Alu { op, s, rd: if op.compares() { 0 } else { rd }, rn: if op.unary() { 0 } else { rn }, src }
}

fn multiply(w: u32) -> Op {
    let (s, high, low, rs, rm) = (bit(w, 20), reg(w, 16), reg(w, 12), reg(w, 8), reg(w, 0));
    match field(w, 23, 21) {
        0 => Op::Mul { s, rd: high, rn: rm, rm: rs, acc: None, subtract: false },
        1 => Op::Mul { s, rd: high, rn: rm, rm: rs, acc: Some(low), subtract: false },
        3 if !s => Op::Mul { s, rd: high, rn: rm, rm: rs, acc: Some(low), subtract: true },
        // The long ones that set the flags have no operation here.
        kind @ 4..=7 if !s => Op::LongMul { signed: kind >= 6, accumulate: kind & 1 != 0, lo: low, hi: high, rn: rm, rm: rs },
        _ => Op::Unsupported,
    }
}

/// The loads and stores of a halfword, a signed byte and a pair.
fn halfword(w: u32) -> Op {
    let (index, immediate, wback, load) = (bit(w, 24), bit(w, 22), bit(w, 21), bit(w, 20));
    if !index && wback {
        return Op::Unsupported; // the unprivileged ones
    }
    let (rt, rn, rm) = (reg(w, 12), reg(w, 16), reg(w, 0));
    let wback = wback || !index;
    let magnitude = field(w, 11, 8) << 4 | field(w, 3, 0);
    let pair = !load && field(w, 6, 5) != 1;
    if pair {
        if !immediate || rt & 1 != 0 {
            return Op::Unsupported;
        }
        return Op::Dual { load: field(w, 6, 5) == 2, rt, rt2: rt + 1, rn, offset: signed(w, magnitude), index, wback };
    }
    let offset = match (immediate, bit(w, 23)) {
        (true, _) => Offset::Imm(signed(w, magnitude)),
        (false, true) => Offset::Reg { rm, shift: 0 },
        (false, false) => Offset::Shifted { rm, shift: Shift::Lsl, amount: 0, subtract: true },
    };
    let (bits, signed) = match field(w, 6, 5) {
        1 => (16, false),
        2 => (8, true),
        _ => (16, true),
    };
    Op::Mem { load, bits, signed, rt, rn, offset, index, wback }
}

/// The load or the store of a word or a byte.
fn single(w: u32, offset: Offset) -> Op {
    let (index, wback) = (bit(w, 24), bit(w, 21));
    if !index && wback {
        return Op::Unsupported; // the unprivileged ones
    }
    Op::Mem {
        load: bit(w, 20),
        bits: if bit(w, 22) { 8 } else { 32 },
        signed: false,
        rt: reg(w, 12),
        rn: reg(w, 16),
        offset,
        index,
        wback: wback || !index,
    }
}

/// The later instructions in the space of a register offset with bit four set.
fn media(w: u32) -> Op {
    let (rd, rm) = (reg(w, 12), reg(w, 0));
    if w & 0x0fd0_00f0 == 0x0710_0010 {
        return Op::Div { signed: !bit(w, 21), rd: reg(w, 16), rn: rm, rm: reg(w, 8) };
    }
    if w & 0x0fbf_03f0 == 0x06af_0070 || w & 0x0fbf_03f0 == 0x06bf_0070 {
        // sxtb, sxth, and with bit 22 uxtb, uxth; the source rotated right by bytes first.
        return Op::Extend { signed: !bit(w, 22), bits: if bit(w, 20) { 16 } else { 8 }, rd, rm, rotate: 8 * field(w, 11, 10) as u8 };
    }
    if w & 0x0fa0_0070 == 0x07a0_0050 {
        return Op::Field { signed: !bit(w, 22), rd, rn: rm, lsb: field(w, 11, 7) as u8, width: field(w, 20, 16) as u8 + 1 };
    }
    if w & 0x0fe0_0070 == 0x07c0_0010 {
        let (msb, lsb) = (field(w, 20, 16) as u8, field(w, 11, 7) as u8);
        if msb < lsb {
            return Op::Unsupported;
        }
        return Op::Insert { rd, rn: (rm != PC).then_some(rm), lsb, width: msb - lsb + 1 };
    }
    if w & 0x0fbf_0f70 == 0x06bf_0f30 {
        // rev and rev16, and with bit 22 rbit and revsh.
        let kind = match (bit(w, 22), bit(w, 7)) {
            (false, false) => Bits::Rev,
            (false, true) => Bits::Rev16,
            (true, false) => Bits::Rbit,
            (true, true) => Bits::Revsh,
        };
        return Op::Bits { kind, rd, rm };
    }
    if w & 0x0ff0_00f0 == 0x07f0_00f0 {
        return Op::Trap; // udf
    }
    Op::Unsupported
}

/// The ARM decoder, for the analysis: an instruction as the neutral record the Thumb one makes
/// of the same operation, with what differs between the two sets put right.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Arm;

impl Arch for Arm {
    fn name(&self) -> &'static str {
        "arm"
    }

    fn address_bits(&self) -> u16 {
        32
    }

    fn stack_pointer(&self) -> Register {
        register(crate::thumb::SP)
    }

    fn instruction_pointer(&self) -> Register {
        register(PC)
    }

    fn decode(&self, bytes: &[u8], address: u64) -> Option<Instr> {
        let word = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?);
        let d = decode(word, address);
        let mut instr = record(&d, address);
        // The PC a load reads is the address and eight, not Thumb's.
        if let Op::Mem { rn: PC, offset: Offset::Imm(offset), index: true, .. } = d.op {
            for operand in &mut instr.operands {
                if let Operand::Mem(m) = operand {
                    m.absolute = Some(address.wrapping_add(8).wrapping_add_signed(i64::from(offset)) & 0xffff_ffff);
                }
            }
        }
        // A load into the PC is a jump through what it reads.
        if matches!(d.op, Op::Mem { load: true, rt: PC, .. }) {
            instr.flow = Flow::Jump(Target::Indirect);
        }
        // A return or a jump through a register under a condition goes on when the condition
        // fails: the flow goes on, and the lift leaves under the condition.
        if d.cond.is_some() && !matches!(d.op, Op::Branch { .. }) {
            instr.prefixes.conditional = true;
            if matches!(instr.flow, Flow::Return | Flow::Jump(Target::Indirect)) {
                instr.flow = Flow::Next;
                instr.prefixes.conditional_leave = true;
            }
        }
        Some(instr)
    }
}

/// The decoder of a program of both instruction sets: ARM code where the image's mapping
/// symbols say so (`ranges`: the address a run starts at, and whether it is ARM code, in address
/// order), Thumb elsewhere.
#[derive(Debug, Clone, Default)]
pub struct ArmThumb {
    /// Where each set is known to start; the decoder adds where the branches it reads go.
    pub map: std::sync::Arc<ra_asm_arch::ArmMap>,
    /// The set of the code before the first place of the map: ARM code when set.
    pub arm: bool,
}

impl ArmThumb {
    fn arm_at(&self, address: u64) -> bool {
        self.map.at(address).unwrap_or(self.arm)
    }
}

impl Arch for ArmThumb {
    fn name(&self) -> &'static str {
        "arm+thumb"
    }

    fn address_bits(&self) -> u16 {
        32
    }

    fn stack_pointer(&self) -> Register {
        register(crate::thumb::SP)
    }

    fn instruction_pointer(&self) -> Register {
        register(PC)
    }

    fn decode(&self, bytes: &[u8], address: u64) -> Option<Instr> {
        let arm = self.arm_at(address);
        let instr = if arm { Arm.decode(bytes, address) } else { crate::thumb::Thumb.decode(bytes, address) }?;
        // Where a branch goes is code of the set the branch is in, unless the branch changes the
        // set: `blx` either way, and Thumb's `bx pc` into the ARM code after it.
        if let Flow::Call(Target::Direct(target)) | Flow::Jump(Target::Direct(target)) | Flow::CondJump(Target::Direct(target)) = instr.flow
        {
            let op =
                if arm { Some(decode(instr.code, address).op) } else { crate::thumb::decode_raw(instr.code, address, None).map(|d| d.op) };
            let changes = matches!(op, Some(Op::Call { arm: true, .. } | Op::BranchReg { rm: PC, .. }));
            self.map.mark(target, arm != changes);
        }
        Some(instr)
    }

    fn link(&self, before: &[Instr], instr: &mut Instr) {
        if !self.arm_at(instr.address) {
            crate::thumb::Thumb.link(before, instr);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::thumb::{LR, SP};

    #[test]
    fn the_record_is_the_arm_instruction_s() {
        let instr = |word: u32| Arm.decode(&word.to_le_bytes(), 0x8000).unwrap();
        // ldr r0, [pc, #4] reads the address and eight and four
        assert_eq!(instr(0xe59f_0004).absolute_memory_operand(), Some(0x800c));
        // bx lr returns; bxeq lr goes on; bne is a conditional jump; ldr pc, [r1] jumps
        assert_eq!(instr(0xe12f_ff1e).flow, Flow::Return);
        assert_eq!(instr(0x012f_ff1e).flow, Flow::Next);
        assert_eq!(instr(0x1aff_fffc).flow, Flow::CondJump(Target::Direct(0x7ff8)));
        assert_eq!(instr(0xe591_f000).flow, Flow::Jump(Target::Indirect));
        assert_eq!(instr(0xe59f_0004).len, 4);
    }

    fn op(word: u32) -> Op {
        decode(word, 0x8000).op
    }

    #[test]
    fn the_data_processing_decodes() {
        // addeq r0, r1, r2
        let d = decode(0x0081_0002, 0);
        assert_eq!(d.cond, Some(0));
        assert_eq!(d.op, Op::Alu { op: Alu::Add, s: false, rd: 0, rn: 1, src: Source::Reg { rm: 2, shift: Shift::Lsl, amount: 0 } });
        // movs r0, r1, rrx
        assert_eq!(
            op(0xe1b0_0061),
            Op::Alu { op: Alu::Mov, s: true, rd: 0, rn: 0, src: Source::Reg { rm: 1, shift: Shift::Rrx, amount: 1 } }
        );
        // cmp r1, r2, asr #32
        assert_eq!(
            op(0xe151_0042),
            Op::Alu { op: Alu::Cmp, s: true, rd: 0, rn: 1, src: Source::Reg { rm: 2, shift: Shift::Asr, amount: 32 } }
        );
        // orr r0, r1, r2, lsl r3
        assert_eq!(
            op(0xe181_0312),
            Op::Alu { op: Alu::Orr, s: false, rd: 0, rn: 1, src: Source::RegReg { rm: 2, shift: Shift::Lsl, rs: 3 } }
        );
        // mov r0, #0xff000000: the constant rotated, the carry its top bit
        assert_eq!(
            op(0xe3a0_04ff),
            Op::Alu { op: Alu::Mov, s: false, rd: 0, rn: 0, src: Source::Imm { value: 0xff00_0000, carry: Some(true) } }
        );
        // rsc r0, r1, r2
        assert!(matches!(op(0xe0e1_0002), Op::Alu { op: Alu::Rsc, .. }));
        // mul r0, r1, r2; umull r0, r1, r2, r3
        assert_eq!(op(0xe000_0291), Op::Mul { s: false, rd: 0, rn: 1, rm: 2, acc: None, subtract: false });
        assert_eq!(op(0xe081_0392), Op::LongMul { signed: false, accumulate: false, lo: 0, hi: 1, rn: 2, rm: 3 });
    }

    #[test]
    fn the_loads_and_stores_decode() {
        // ldr r0, [r1, #4]; ldr r0, [r1], #-4; ldrb r0, [r1, r2, lsl #2]!
        assert_eq!(
            op(0xe591_0004),
            Op::Mem { load: true, bits: 32, signed: false, rt: 0, rn: 1, offset: Offset::Imm(4), index: true, wback: false }
        );
        assert_eq!(
            op(0xe411_0004),
            Op::Mem { load: true, bits: 32, signed: false, rt: 0, rn: 1, offset: Offset::Imm(-4), index: false, wback: true }
        );
        assert_eq!(
            op(0xe7f1_0102),
            Op::Mem { load: true, bits: 8, signed: false, rt: 0, rn: 1, offset: Offset::Reg { rm: 2, shift: 2 }, index: true, wback: true }
        );
        // ldr r0, [r1, -r2]
        assert!(matches!(op(0xe711_0002), Op::Mem { offset: Offset::Shifted { rm: 2, subtract: true, .. }, .. }));
        // ldrsh r0, [r1, #2]
        assert_eq!(
            op(0xe1d1_00f2),
            Op::Mem { load: true, bits: 16, signed: true, rt: 0, rn: 1, offset: Offset::Imm(2), index: true, wback: false }
        );
        // push {r4, lr}; pop {r4, pc}; ldmib r1, {r2, r3}; stmda r1!, {r2}
        assert_eq!(
            op(0xe92d_4010),
            Op::Multiple { load: false, rn: SP, list: 1 << 4 | 1 << LR, wback: true, before: true, swapped: false }
        );
        assert_eq!(
            op(0xe8bd_8010),
            Op::Multiple { load: true, rn: SP, list: 1 << 4 | 1 << PC, wback: true, before: false, swapped: false }
        );
        assert_eq!(op(0xe991_000c), Op::Multiple { load: true, rn: 1, list: 0xc, wback: false, before: true, swapped: true });
        assert_eq!(op(0xe821_0004), Op::Multiple { load: false, rn: 1, list: 4, wback: true, before: false, swapped: true });
    }

    #[test]
    fn the_transfers_decode() {
        // b +8 from 0x8000 goes to the PC, the address and eight, and eight
        assert_eq!(op(0xea00_0002), Op::Branch { cond: None, target: 0x8010 });
        // bne -16
        assert_eq!(decode(0x1aff_fffc, 0x8000).op, Op::Branch { cond: Some(1), target: 0x7ff8 });
        // bl, bx lr, blx r2, svc 0
        assert_eq!(op(0xeb00_0200), Op::Call { target: 0x8808, arm: false });
        assert_eq!(op(0xe12f_ff1e), Op::BranchReg { rm: LR, link: false });
        assert_eq!(op(0xe12f_ff32), Op::BranchReg { rm: 2, link: true });
        assert_eq!(op(0xef00_0000), Op::Svc(0));
        // adr r0, +16; nop; what has no condition is another space
        assert_eq!(op(0xe28f_0010), Op::Adr { rd: 0, target: 0x8018 });
        assert_eq!(op(0xe320_f000), Op::Nop);
        assert_eq!(op(0xf57f_f000), Op::Unsupported);
    }
}
