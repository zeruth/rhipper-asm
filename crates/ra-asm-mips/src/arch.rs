//! The rows of the MIPS integer instructions and the [`Mips`] decoder over them: a word in the
//! file's byte order becomes the neutral [`Instr`] with the text llvm-objdump prints, and each
//! transfer carries the delay its form has.

use std::sync::LazyLock;

use ra_asm_arch::{Arch, Flow as ArchFlow, Instr, Operand, RegClass, Register, Target};
use ra_asm_machine::{Field, Flow, Kind, Machine, Row, Syntax, Table};

/// When the flow of a transfer takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delay {
    /// At once: the instruction transfers nothing, or traps.
    None,
    /// After the next instruction, the delay slot, which runs whether or not a branch is taken.
    Slot,
    /// After the delay slot, which runs only when the branch is taken: a likely branch skips its
    /// slot when it falls through.
    Likely,
}

/// What the neutral record needs beyond what the row's fields say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    /// The row says it all.
    Row,
    /// `j` and `jal`: the target is the 26-bit word index inside the 256-megabyte region of the delay
    /// slot, which no relative field can say.
    Region,
    /// `syscall`: the system takes control and gives it back to the next instruction.
    System,
}

/// A row of the table with the delay of its transfer.
#[derive(Debug, Clone, Copy)]
struct Form {
    row: Row,
    delay: Delay,
    reach: Reach,
}

const RS: Field = Field { name: "rs", lo: 21, width: 5, signed: false, kind: Kind::Reg };
const RT: Field = Field { name: "rt", lo: 16, width: 5, signed: false, kind: Kind::Reg };
const RD: Field = Field { name: "rd", lo: 11, width: 5, signed: false, kind: Kind::Reg };
const SA: Field = Field { name: "sa", lo: 6, width: 5, signed: false, kind: Kind::Imm };
const IMM: Field = Field { name: "imm", lo: 0, width: 16, signed: true, kind: Kind::Imm };
/// The immediate of the logical forms and `lui`, which widen by zero.
const UIMM: Field = Field { name: "imm", lo: 0, width: 16, signed: false, kind: Kind::Imm };
/// A branch's offset, in words from the delay slot.
const OFFSET: Field = Field { name: "offset", lo: 0, width: 16, signed: true, kind: Kind::Rel { shift: 2, from: 4 } };
const INDEX: Field = Field { name: "index", lo: 0, width: 26, signed: false, kind: Kind::Imm };

const fn disp(bits: u16) -> Field {
    Field { name: "offset", lo: 0, width: 16, signed: true, kind: Kind::Mem { base: "rs", bits } }
}

const D8: Field = disp(8);
const D16: Field = disp(16);
const D32: Field = disp(32);

/// The opcode alone: the immediate forms.
const OP: u32 = 0xfc00_0000;
/// The opcode, the shift amount and the function: the three-register forms, whose shift is zero.
const R3: u32 = 0xfc00_07ff;
/// The opcode and the function, the rest free: the system and trap forms, whose code is ignored.
const FUNCT: u32 = 0xfc00_003f;
/// The opcode and `rt`: the branches on one register, and the `REGIMM` forms `rt` selects.
const OP_RT: u32 = 0xfc1f_0000;

const fn op(opcode: u32) -> u32 {
    opcode << 26
}

const fn regimm(rt: u32) -> u32 {
    op(0x01) | (rt << 16)
}

const fn plain(name: &'static str, mask: u32, bits: u32, fields: &'static [Field]) -> Form {
    Form { row: Row { name, mask, bits, fields, flow: Flow::Next, outlined: false, sem: None }, delay: Delay::None, reach: Reach::Row }
}

const fn transfer(name: &'static str, mask: u32, bits: u32, fields: &'static [Field], flow: Flow, delay: Delay) -> Form {
    Form { row: Row { name, mask, bits, fields, flow, outlined: false, sem: None }, delay, reach: Reach::Row }
}

/// The forms, each alias after the form it narrows: `nop`, `move`, `not`, `negu`, `b`, `beqz`,
/// `bnez`, `bal`, `jr $ra` and the one-register `jalr`, as llvm-objdump names them. No row carries
/// its semantics yet, so each lifts to a trap that names its word.
const FORMS: &[Form] = &[
    // SPECIAL.
    plain("nop", u32::MAX, 0, &[]),
    plain("sll", 0xffe0_003f, 0x00, &[RD, RT, SA]),
    plain("srl", 0xffe0_003f, 0x02, &[RD, RT, SA]),
    plain("sra", 0xffe0_003f, 0x03, &[RD, RT, SA]),
    plain("sllv", R3, 0x04, &[RD, RT, RS]),
    plain("srlv", R3, 0x06, &[RD, RT, RS]),
    plain("srav", R3, 0x07, &[RD, RT, RS]),
    transfer("jr", 0xfc1f_ffff, 0x08, &[RS], Flow::Jump, Delay::Slot),
    transfer("jr", u32::MAX, 0x03e0_0008, &[RS], Flow::Return, Delay::Slot),
    transfer("jalr", 0xfc1f_07ff, 0x09, &[RD, RS], Flow::Call, Delay::Slot),
    transfer("jalr", 0xfc1f_ffff, 0x0000_f809, &[RS], Flow::Call, Delay::Slot),
    plain("movz", R3, 0x0a, &[RD, RS, RT]),
    plain("movn", R3, 0x0b, &[RD, RS, RT]),
    Form {
        row: Row { name: "syscall", mask: FUNCT, bits: 0x0c, fields: &[], flow: Flow::Next, outlined: false, sem: None },
        delay: Delay::None,
        reach: Reach::System,
    },
    transfer("break", FUNCT, 0x0d, &[], Flow::Trap, Delay::None),
    plain("sync", 0xffff_f83f, 0x0f, &[]),
    plain("mfhi", 0xffff_07ff, 0x10, &[RD]),
    plain("mthi", 0xfc1f_ffff, 0x11, &[RS]),
    plain("mflo", 0xffff_07ff, 0x12, &[RD]),
    plain("mtlo", 0xfc1f_ffff, 0x13, &[RS]),
    plain("mult", 0xfc00_ffff, 0x18, &[RS, RT]),
    plain("multu", 0xfc00_ffff, 0x19, &[RS, RT]),
    plain("div", 0xfc00_ffff, 0x1a, &[RS, RT]),
    plain("divu", 0xfc00_ffff, 0x1b, &[RS, RT]),
    plain("add", R3, 0x20, &[RD, RS, RT]),
    plain("addu", R3, 0x21, &[RD, RS, RT]),
    plain("sub", R3, 0x22, &[RD, RS, RT]),
    plain("subu", R3, 0x23, &[RD, RS, RT]),
    plain("negu", 0xffe0_07ff, 0x23, &[RD, RT]),
    plain("and", R3, 0x24, &[RD, RS, RT]),
    plain("or", R3, 0x25, &[RD, RS, RT]),
    plain("move", 0xfc1f_07ff, 0x25, &[RD, RS]),
    plain("xor", R3, 0x26, &[RD, RS, RT]),
    plain("nor", R3, 0x27, &[RD, RS, RT]),
    plain("not", 0xfc1f_07ff, 0x27, &[RD, RS]),
    plain("slt", R3, 0x2a, &[RD, RS, RT]),
    plain("sltu", R3, 0x2b, &[RD, RS, RT]),
    plain("tge", FUNCT, 0x30, &[RS, RT]),
    plain("tgeu", FUNCT, 0x31, &[RS, RT]),
    plain("tlt", FUNCT, 0x32, &[RS, RT]),
    plain("tltu", FUNCT, 0x33, &[RS, RT]),
    plain("teq", FUNCT, 0x34, &[RS, RT]),
    plain("tne", FUNCT, 0x36, &[RS, RT]),
    // REGIMM.
    transfer("bltz", OP_RT, regimm(0x00), &[RS, OFFSET], Flow::Branch, Delay::Slot),
    transfer("bgez", OP_RT, regimm(0x01), &[RS, OFFSET], Flow::Branch, Delay::Slot),
    transfer("bltzl", OP_RT, regimm(0x02), &[RS, OFFSET], Flow::Branch, Delay::Likely),
    transfer("bgezl", OP_RT, regimm(0x03), &[RS, OFFSET], Flow::Branch, Delay::Likely),
    transfer("bltzal", OP_RT, regimm(0x10), &[RS, OFFSET], Flow::Call, Delay::Slot),
    transfer("bgezal", OP_RT, regimm(0x11), &[RS, OFFSET], Flow::Call, Delay::Slot),
    transfer("bal", 0xffff_0000, regimm(0x11), &[OFFSET], Flow::Call, Delay::Slot),
    transfer("bltzall", OP_RT, regimm(0x12), &[RS, OFFSET], Flow::Call, Delay::Likely),
    transfer("bgezall", OP_RT, regimm(0x13), &[RS, OFFSET], Flow::Call, Delay::Likely),
    // The jumps into the region.
    Form {
        row: Row { name: "j", mask: OP, bits: op(0x02), fields: &[INDEX], flow: Flow::Jump, outlined: false, sem: None },
        delay: Delay::Slot,
        reach: Reach::Region,
    },
    Form {
        row: Row { name: "jal", mask: OP, bits: op(0x03), fields: &[INDEX], flow: Flow::Call, outlined: false, sem: None },
        delay: Delay::Slot,
        reach: Reach::Region,
    },
    // The branches.
    transfer("beq", OP, op(0x04), &[RS, RT, OFFSET], Flow::Branch, Delay::Slot),
    transfer("beqz", OP_RT, op(0x04), &[RS, OFFSET], Flow::Branch, Delay::Slot),
    transfer("b", 0xffff_0000, op(0x04), &[OFFSET], Flow::Jump, Delay::Slot),
    transfer("bne", OP, op(0x05), &[RS, RT, OFFSET], Flow::Branch, Delay::Slot),
    transfer("bnez", OP_RT, op(0x05), &[RS, OFFSET], Flow::Branch, Delay::Slot),
    transfer("blez", OP_RT, op(0x06), &[RS, OFFSET], Flow::Branch, Delay::Slot),
    transfer("bgtz", OP_RT, op(0x07), &[RS, OFFSET], Flow::Branch, Delay::Slot),
    transfer("beql", OP, op(0x14), &[RS, RT, OFFSET], Flow::Branch, Delay::Likely),
    transfer("bnel", OP, op(0x15), &[RS, RT, OFFSET], Flow::Branch, Delay::Likely),
    transfer("blezl", OP_RT, op(0x16), &[RS, OFFSET], Flow::Branch, Delay::Likely),
    transfer("bgtzl", OP_RT, op(0x17), &[RS, OFFSET], Flow::Branch, Delay::Likely),
    // The immediates.
    plain("addi", OP, op(0x08), &[RT, RS, IMM]),
    plain("addiu", OP, op(0x09), &[RT, RS, IMM]),
    plain("slti", OP, op(0x0a), &[RT, RS, IMM]),
    plain("sltiu", OP, op(0x0b), &[RT, RS, IMM]),
    plain("andi", OP, op(0x0c), &[RT, RS, UIMM]),
    plain("ori", OP, op(0x0d), &[RT, RS, UIMM]),
    plain("xori", OP, op(0x0e), &[RT, RS, UIMM]),
    plain("lui", 0xffe0_0000, op(0x0f), &[RT, UIMM]),
    // SPECIAL2.
    plain("mul", R3, op(0x1c) | 0x02, &[RD, RS, RT]),
    // Memory.
    plain("lb", OP, op(0x20), &[RT, RS, D8]),
    plain("lh", OP, op(0x21), &[RT, RS, D16]),
    plain("lwl", OP, op(0x22), &[RT, RS, D32]),
    plain("lw", OP, op(0x23), &[RT, RS, D32]),
    plain("lbu", OP, op(0x24), &[RT, RS, D8]),
    plain("lhu", OP, op(0x25), &[RT, RS, D16]),
    plain("lwr", OP, op(0x26), &[RT, RS, D32]),
    plain("sb", OP, op(0x28), &[RT, RS, D8]),
    plain("sh", OP, op(0x29), &[RT, RS, D16]),
    plain("swl", OP, op(0x2a), &[RT, RS, D32]),
    plain("sw", OP, op(0x2b), &[RT, RS, D32]),
    plain("swr", OP, op(0x2e), &[RT, RS, D32]),
    plain("ll", OP, op(0x30), &[RT, RS, D32]),
    plain("sc", OP, op(0x38), &[RT, RS, D32]),
];

/// The general registers by number, as llvm-objdump names them.
const NAMES: [&str; 32] = [
    "$zero", "$1", "$2", "$3", "$4", "$5", "$6", "$7", "$8", "$9", "$10", "$11", "$12", "$13", "$14", "$15", "$16", "$17", "$18", "$19",
    "$20", "$21", "$22", "$23", "$24", "$25", "$26", "$27", "$gp", "$sp", "$fp", "$ra",
];

const SP: u16 = 29;

const fn file() -> [Register; 32] {
    let mut registers = [Register { id: 0, name: "", class: RegClass::General, full: 0, bits: 32, shift: 0 }; 32];
    let mut n: u16 = 0;
    while n < 32 {
        let class = if n == SP { RegClass::StackPointer } else { RegClass::General };
        registers[n as usize] = Register { id: n, name: NAMES[n as usize], class, full: n, bits: 32, shift: 0 };
        n += 1;
    }
    registers
}

static REGISTERS: [Register; 32] = file();

const PC: Register = Register { id: 32, name: "pc", class: RegClass::InstructionPointer, full: 32, bits: 32, shift: 0 };

static MACHINE: LazyLock<Machine> = LazyLock::new(|| {
    let rows: Vec<Row> = FORMS.iter().map(|form| form.row).collect();
    Machine {
        name: "mips",
        table: Table::new(&rows).expect("the MIPS rows make a table"),
        registers: &REGISTERS,
        address_bits: 32,
        syntax: Syntax { immediate_prefix: "", hex: true },
    }
});

/// The 32-bit MIPS decoder, in the byte order of the file it reads.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mips {
    pub big_endian: bool,
}

impl Mips {
    /// The machine the rows make, for the lift.
    pub fn machine() -> &'static Machine {
        &MACHINE
    }

    /// When the flow of `instr`, decoded by this crate, takes effect.
    pub fn delay(instr: &Instr) -> Delay {
        usize::try_from(instr.code).ok().and_then(|at| FORMS.get(at)).map_or(Delay::None, |form| form.delay)
    }

    /// Where control goes once the flow of `instr` has taken effect, the taken target first: a
    /// delayed transfer falls through, or returns from its call, past its delay slot.
    pub fn successors(instr: &Instr) -> Vec<u64> {
        let after = match Self::delay(instr) {
            Delay::None => instr.end(),
            Delay::Slot | Delay::Likely => instr.end().wrapping_add(4),
        } & u64::from(u32::MAX);
        let mut out = Vec::with_capacity(2);
        if let ArchFlow::Jump(Target::Direct(target)) | ArchFlow::CondJump(Target::Direct(target)) = instr.flow {
            out.push(target);
        }
        if instr.flow.falls_through() {
            out.push(after);
        }
        out
    }
}

impl Arch for Mips {
    fn name(&self) -> &'static str {
        if self.big_endian { "mips" } else { "mipsel" }
    }

    fn address_bits(&self) -> u16 {
        32
    }

    fn stack_pointer(&self) -> Register {
        REGISTERS[usize::from(SP)]
    }

    fn instruction_pointer(&self) -> Register {
        PC
    }

    fn decode(&self, bytes: &[u8], address: u64) -> Option<Instr> {
        let word: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
        let word = if self.big_endian { u32::from_be_bytes(word) } else { u32::from_le_bytes(word) };
        let mut instr = MACHINE.instr(word, address)?;
        let form = FORMS.get(usize::try_from(instr.code).ok()?)?;
        match form.reach {
            Reach::Row => {}
            Reach::Region => {
                let Some(&Operand::Imm { value, .. }) = instr.operands.first() else { return None };
                let slot = address.wrapping_add(4) & u64::from(u32::MAX);
                let target = (slot & 0xf000_0000) | ((value as u64) << 2);
                instr.operands = vec![Operand::Addr(target)];
                instr.flow = if form.row.flow == Flow::Call {
                    ArchFlow::Call(Target::Direct(target))
                } else {
                    ArchFlow::Jump(Target::Direct(target))
                };
                instr.text = format!("{}\t{target:#x}", instr.mnemonic);
            }
            Reach::System => instr.flow = ArchFlow::Interrupt,
        }
        Some(instr)
    }
}

#[cfg(test)]
mod tests;
