//! The rows of the MIPS integer instructions and the [`Mips`] decoder over them: a word in the
//! file's byte order becomes the neutral [`Instr`] with the text llvm-objdump prints, and each
//! transfer carries the delay its form has.

use std::sync::LazyLock;

pub use ra_asm_arch::Delay;
use ra_asm_arch::{Arch, Flow as ArchFlow, Instr, Operand, RegClass, Register, Target};
use ra_asm_machine::{Field, Flow, Kind, Machine, Row, Syntax, Table};

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
/// The field `ext` and `ins` keep the last bit of their range in.
const MSB: Field = Field { name: "msb", lo: 11, width: 5, signed: false, kind: Kind::Imm };

const fn disp(bits: u16) -> Field {
    Field { name: "offset", lo: 0, width: 16, signed: true, kind: Kind::Mem { base: "rs", bits } }
}

/// The FPU's registers, which no field of [`Kind::Reg`] names: the machine's register file is the
/// general one.
const FS: Field = Field { name: "fs", lo: 11, width: 5, signed: false, kind: Kind::Imm };
const FT: Field = Field { name: "ft", lo: 16, width: 5, signed: false, kind: Kind::Imm };
const FD: Field = Field { name: "fd", lo: 6, width: 5, signed: false, kind: Kind::Imm };

const D8: Field = disp(8);
const D16: Field = disp(16);
const D32: Field = disp(32);
const D64: Field = disp(64);
const D128: Field = disp(128);

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

/// A multimedia instruction of the R5900: the `MMI` opcode, the group's function and the
/// operation's number in the shift-amount field.
const fn mmi(name: &'static str, funct: u32, sa: u32) -> Form {
    plain(name, R3, op(0x1c) | (sa << 6) | funct, &[RD, RS, RT])
}

/// An instruction of the R5900's FPU in the `S` format: `COP1`, the format and the function, with
/// `zero` the bits of the fields the form leaves at zero.
const fn single(name: &'static str, funct: u32, zero: u32, fields: &'static [Field]) -> Form {
    plain(name, 0xffe0_003f | zero, op(0x11) | (16 << 21) | funct, fields)
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

/// The forms the Allegrex of the PSP adds to MIPS II, which only its decoder reads: its
/// `SPECIAL` functions sit where MIPS III has its doubleword operations, and `SPECIAL3` where
/// the R5900 has `sq`. Its table is [`FORMS`] and these after them; an instruction's code
/// past [`FORMS`] is moved past [`R5900_FORMS`], so the code of a row of any table indexes
/// [`form`].
const ALLEGREX: &[Form] = &[
    // COP1: the moves, the single and double operations and the branches on a condition code.
    plain("lwc1", OP, op(0x31), &[FT, RS, D32]),
    plain("swc1", OP, op(0x39), &[FT, RS, D32]),
    plain("mfc1", 0xffe0_07ff, op(0x11), &[RT, FS]),
    plain("cfc1", 0xffe0_07ff, op(0x11) | (2 << 21), &[RT, FS]),
    plain("mtc1", 0xffe0_07ff, op(0x11) | (4 << 21), &[RT, FS]),
    plain("ctc1", 0xffe0_07ff, op(0x11) | (6 << 21), &[RT, FS]),
    plain("cop1.s", 0xffe0_0000, op(0x11) | (16 << 21), &[FD, FS, FT]),
    plain("cop1.d", 0xffe0_0000, op(0x11) | (17 << 21), &[FD, FS, FT]),
    plain("cop1.w", 0xffe0_0000, op(0x11) | (20 << 21), &[FD, FS]),
    transfer("bc1f", 0xffe3_0000, op(0x11) | (8 << 21), &[OFFSET], Flow::Branch, Delay::Slot),
    transfer("bc1t", 0xffe3_0000, op(0x11) | (8 << 21) | (1 << 16), &[OFFSET], Flow::Branch, Delay::Slot),
    transfer("bc1fl", 0xffe3_0000, op(0x11) | (8 << 21) | (2 << 16), &[OFFSET], Flow::Branch, Delay::Likely),
    transfer("bc1tl", 0xffe3_0000, op(0x11) | (8 << 21) | (3 << 16), &[OFFSET], Flow::Branch, Delay::Likely),
    plain("rotr", 0xffe0_003f, 0x0020_0002, &[RD, RT, SA]),
    plain("rotrv", R3, 0x46, &[RD, RT, RS]),
    plain("clz", 0xfc1f_07ff, 0x16, &[RD, RS]),
    plain("clo", 0xfc1f_07ff, 0x17, &[RD, RS]),
    plain("madd", 0xfc00_ffff, 0x1c, &[RS, RT]),
    plain("maddu", 0xfc00_ffff, 0x1d, &[RS, RT]),
    plain("max", R3, 0x2c, &[RD, RS, RT]),
    plain("min", R3, 0x2d, &[RD, RS, RT]),
    plain("msub", 0xfc00_ffff, 0x2e, &[RS, RT]),
    plain("msubu", 0xfc00_ffff, 0x2f, &[RS, RT]),
    plain("ext", 0xfc00_003f, op(0x1f), &[RT, RS, SA, MSB]),
    plain("ins", 0xfc00_003f, op(0x1f) | 0x04, &[RT, RS, SA, MSB]),
    plain("wsbh", 0xffe0_07ff, op(0x1f) | (0x02 << 6) | 0x20, &[RD, RT]),
    plain("wsbw", 0xffe0_07ff, op(0x1f) | (0x03 << 6) | 0x20, &[RD, RT]),
    plain("seb", 0xffe0_07ff, op(0x1f) | (0x10 << 6) | 0x20, &[RD, RT]),
    plain("bitrev", 0xffe0_07ff, op(0x1f) | (0x14 << 6) | 0x20, &[RD, RT]),
    plain("seh", 0xffe0_07ff, op(0x1f) | (0x18 << 6) | 0x20, &[RD, RT]),
    // The VFPU's opcodes, whose fields the lift reads from the word.
    plain("lv.s", OP, op(0x32), &[RS, D32]),
    plain("lv.q", OP, op(0x36), &[RS, D32]),
    plain("sv.s", OP, op(0x3a), &[RS, D32]),
    plain("sv.q", OP, op(0x3e), &[RS, D32]),
    plain("mfv", 0xffe0_ff80, op(0x12) | (3 << 21), &[RT]),
    plain("mtv", 0xffe0_ff80, op(0x12) | (7 << 21), &[RT]),
    plain("vfpu0", OP, op(0x18), &[]),
    plain("vfpu1", OP, op(0x19), &[]),
    plain("vfpu3", OP, op(0x1b), &[]),
    plain("vfpu4", OP, op(0x34), &[]),
    plain("vfpu5", OP, op(0x37), &[]),
    plain("vfpu6", OP, op(0x3c), &[]),
    plain("vfpu7", OP, op(0x3f), &[]),
];

/// The R5900's rows past [`FORMS`]: MIPS III's doubleword forms, which the 32-bit programs of the
/// other MIPS loaders do not have, and the Emotion Engine's own, which take encodings other MIPS
/// processors give other instructions (`SPECIAL2`, `SPECIAL3`) or leave reserved. Its table is
/// [`FORMS`] and these after them, so a row of both has the same index in either.
const R5900_FORMS: &[Form] = &[
    plain("dsllv", R3, 0x14, &[RD, RT, RS]),
    plain("dsrlv", R3, 0x16, &[RD, RT, RS]),
    plain("dsrav", R3, 0x17, &[RD, RT, RS]),
    plain("dadd", R3, 0x2c, &[RD, RS, RT]),
    plain("daddu", R3, 0x2d, &[RD, RS, RT]),
    plain("dsub", R3, 0x2e, &[RD, RS, RT]),
    plain("dsubu", R3, 0x2f, &[RD, RS, RT]),
    plain("dsll", 0xffe0_003f, 0x38, &[RD, RT, SA]),
    plain("dsrl", 0xffe0_003f, 0x3a, &[RD, RT, SA]),
    plain("dsra", 0xffe0_003f, 0x3b, &[RD, RT, SA]),
    plain("dsll32", 0xffe0_003f, 0x3c, &[RD, RT, SA]),
    plain("dsrl32", 0xffe0_003f, 0x3e, &[RD, RT, SA]),
    plain("dsra32", 0xffe0_003f, 0x3f, &[RD, RT, SA]),
    plain("daddi", OP, op(0x18), &[RT, RS, IMM]),
    plain("daddiu", OP, op(0x19), &[RT, RS, IMM]),
    plain("ldl", OP, op(0x1a), &[RT, RS, D64]),
    plain("ldr", OP, op(0x1b), &[RT, RS, D64]),
    plain("lwu", OP, op(0x27), &[RT, RS, D32]),
    plain("sdl", OP, op(0x2c), &[RT, RS, D64]),
    plain("sdr", OP, op(0x2d), &[RT, RS, D64]),
    plain("ld", OP, op(0x37), &[RT, RS, D64]),
    plain("sd", OP, op(0x3f), &[RT, RS, D64]),
    // The multiplies that also write `rd`, the second `hi` and `lo`, the quadword accesses and
    // the multimedia instructions.
    plain("mult", R3, 0x18, &[RD, RS, RT]),
    plain("multu", R3, 0x19, &[RD, RS, RT]),
    plain("lq", OP, op(0x1e), &[RT, RS, D128]),
    plain("sq", OP, op(0x1f), &[RT, RS, D128]),
    plain("madd", R3, op(0x1c), &[RD, RS, RT]),
    plain("maddu", R3, op(0x1c) | 0x01, &[RD, RS, RT]),
    plain("mfhi1", 0xffff_07ff, op(0x1c) | 0x10, &[RD]),
    plain("mthi1", 0xfc1f_ffff, op(0x1c) | 0x11, &[RS]),
    plain("mflo1", 0xffff_07ff, op(0x1c) | 0x12, &[RD]),
    plain("mtlo1", 0xfc1f_ffff, op(0x1c) | 0x13, &[RS]),
    plain("mult1", R3, op(0x1c) | 0x18, &[RD, RS, RT]),
    plain("multu1", R3, op(0x1c) | 0x19, &[RD, RS, RT]),
    plain("div1", 0xfc00_ffff, op(0x1c) | 0x1a, &[RS, RT]),
    plain("divu1", 0xfc00_ffff, op(0x1c) | 0x1b, &[RS, RT]),
    plain("madd1", R3, op(0x1c) | 0x20, &[RD, RS, RT]),
    plain("maddu1", R3, op(0x1c) | 0x21, &[RD, RS, RT]),
    plain("psllh", 0xffe0_003f, op(0x1c) | 0x34, &[RD, RT, SA]),
    plain("psrlh", 0xffe0_003f, op(0x1c) | 0x36, &[RD, RT, SA]),
    plain("psrah", 0xffe0_003f, op(0x1c) | 0x37, &[RD, RT, SA]),
    plain("psllw", 0xffe0_003f, op(0x1c) | 0x3c, &[RD, RT, SA]),
    plain("psrlw", 0xffe0_003f, op(0x1c) | 0x3e, &[RD, RT, SA]),
    plain("psraw", 0xffe0_003f, op(0x1c) | 0x3f, &[RD, RT, SA]),
    // MMI0.
    mmi("paddw", 0x08, 0),
    mmi("psubw", 0x08, 1),
    mmi("pcgtw", 0x08, 2),
    mmi("pmaxw", 0x08, 3),
    mmi("paddh", 0x08, 4),
    mmi("psubh", 0x08, 5),
    mmi("pcgth", 0x08, 6),
    mmi("pmaxh", 0x08, 7),
    mmi("paddb", 0x08, 8),
    mmi("psubb", 0x08, 9),
    mmi("pcgtb", 0x08, 10),
    mmi("paddsw", 0x08, 16),
    mmi("psubsw", 0x08, 17),
    mmi("pextlw", 0x08, 18),
    mmi("ppacw", 0x08, 19),
    mmi("paddsh", 0x08, 20),
    mmi("psubsh", 0x08, 21),
    mmi("pextlh", 0x08, 22),
    mmi("ppach", 0x08, 23),
    mmi("paddsb", 0x08, 24),
    mmi("psubsb", 0x08, 25),
    mmi("pextlb", 0x08, 26),
    mmi("ppacb", 0x08, 27),
    // MMI1.
    mmi("pabsw", 0x28, 1),
    mmi("pceqw", 0x28, 2),
    mmi("pminw", 0x28, 3),
    mmi("pabsh", 0x28, 5),
    mmi("pceqh", 0x28, 6),
    mmi("pminh", 0x28, 7),
    mmi("pceqb", 0x28, 10),
    mmi("padduw", 0x28, 16),
    mmi("psubuw", 0x28, 17),
    mmi("pextuw", 0x28, 18),
    mmi("padduh", 0x28, 20),
    mmi("psubuh", 0x28, 21),
    mmi("pextuh", 0x28, 22),
    mmi("paddub", 0x28, 24),
    mmi("psubub", 0x28, 25),
    mmi("pextub", 0x28, 26),
    // MMI2.
    mmi("psllvw", 0x09, 2),
    mmi("psrlvw", 0x09, 3),
    mmi("pmfhi", 0x09, 8),
    mmi("pmflo", 0x09, 9),
    mmi("pinth", 0x09, 10),
    mmi("pcpyld", 0x09, 14),
    mmi("pand", 0x09, 18),
    mmi("pxor", 0x09, 19),
    mmi("pexeh", 0x09, 26),
    mmi("prevh", 0x09, 27),
    mmi("pexew", 0x09, 30),
    mmi("prot3w", 0x09, 31),
    // MMI3.
    mmi("psravw", 0x29, 3),
    mmi("pmthi", 0x29, 8),
    mmi("pmtlo", 0x29, 9),
    mmi("pinteh", 0x29, 10),
    mmi("pcpyud", 0x29, 14),
    mmi("por", 0x29, 18),
    mmi("pnor", 0x29, 19),
    mmi("pexch", 0x29, 26),
    mmi("pcpyh", 0x29, 27),
    mmi("pexcw", 0x29, 30),
    // The FPU, singles only, with its accumulator and its one condition flag.
    plain("mfc1", 0xffe0_07ff, op(0x11), &[RT, FS]),
    plain("cfc1", 0xffe0_07ff, op(0x11) | (2 << 21), &[RT, FS]),
    plain("mtc1", 0xffe0_07ff, op(0x11) | (4 << 21), &[RT, FS]),
    plain("ctc1", 0xffe0_07ff, op(0x11) | (6 << 21), &[RT, FS]),
    transfer("bc1f", 0xffff_0000, op(0x11) | (8 << 21), &[OFFSET], Flow::Branch, Delay::Slot),
    transfer("bc1t", 0xffff_0000, op(0x11) | (8 << 21) | (1 << 16), &[OFFSET], Flow::Branch, Delay::Slot),
    transfer("bc1fl", 0xffff_0000, op(0x11) | (8 << 21) | (2 << 16), &[OFFSET], Flow::Branch, Delay::Likely),
    transfer("bc1tl", 0xffff_0000, op(0x11) | (8 << 21) | (3 << 16), &[OFFSET], Flow::Branch, Delay::Likely),
    single("add.s", 0x00, 0, &[FD, FS, FT]),
    single("sub.s", 0x01, 0, &[FD, FS, FT]),
    single("mul.s", 0x02, 0, &[FD, FS, FT]),
    single("div.s", 0x03, 0, &[FD, FS, FT]),
    single("sqrt.s", 0x04, 0xf800, &[FD, FT]),
    single("abs.s", 0x05, 0x1f_0000, &[FD, FS]),
    single("mov.s", 0x06, 0x1f_0000, &[FD, FS]),
    single("neg.s", 0x07, 0x1f_0000, &[FD, FS]),
    single("rsqrt.s", 0x16, 0, &[FD, FS, FT]),
    single("adda.s", 0x18, 0x7c0, &[FS, FT]),
    single("suba.s", 0x19, 0x7c0, &[FS, FT]),
    single("mula.s", 0x1a, 0x7c0, &[FS, FT]),
    single("madd.s", 0x1c, 0, &[FD, FS, FT]),
    single("msub.s", 0x1d, 0, &[FD, FS, FT]),
    single("madda.s", 0x1e, 0x7c0, &[FS, FT]),
    single("msuba.s", 0x1f, 0x7c0, &[FS, FT]),
    single("cvt.w.s", 0x24, 0x1f_0000, &[FD, FS]),
    single("max.s", 0x28, 0, &[FD, FS, FT]),
    single("min.s", 0x29, 0, &[FD, FS, FT]),
    single("c.f.s", 0x30, 0x7c0, &[FS, FT]),
    single("c.eq.s", 0x32, 0x7c0, &[FS, FT]),
    single("c.lt.s", 0x34, 0x7c0, &[FS, FT]),
    single("c.le.s", 0x36, 0x7c0, &[FS, FT]),
    plain("cvt.s.w", 0xffff_003f, op(0x11) | (20 << 21) | 0x20, &[FD, FS]),
    plain("lwc1", OP, op(0x31), &[FT, RS, D32]),
    plain("swc1", OP, op(0x39), &[FT, RS, D32]),
];

/// The rows of [`R5900_FORMS`] that are MIPS III's doubleword forms, which the VR4300 shares: the
/// R5900's own come after them.
const MIPS3_ROWS: usize = 22;

/// The VR4300's rows past [`FORMS`] and MIPS III's doubleword forms of [`R5900_FORMS`]: the
/// doubleword multiplies and divides, the linked doubleword accesses, coprocessor 0 (its moves, the
/// TLB's operations and `eret`), `cache`, and the FPU of the singles, doubles, words and longs
/// with its doubleword moves.
const VR4300: &[Form] = &[
    plain("dmult", 0xfc00_ffff, 0x1c, &[RS, RT]),
    plain("dmultu", 0xfc00_ffff, 0x1d, &[RS, RT]),
    plain("ddiv", 0xfc00_ffff, 0x1e, &[RS, RT]),
    plain("ddivu", 0xfc00_ffff, 0x1f, &[RS, RT]),
    plain("lld", OP, op(0x34), &[RT, RS, D64]),
    plain("scd", OP, op(0x3c), &[RT, RS, D64]),
    plain("mfc0", 0xffe0_07ff, op(0x10), &[RT, RD]),
    plain("dmfc0", 0xffe0_07ff, op(0x10) | (1 << 21), &[RT, RD]),
    plain("mtc0", 0xffe0_07ff, op(0x10) | (4 << 21), &[RT, RD]),
    plain("dmtc0", 0xffe0_07ff, op(0x10) | (5 << 21), &[RT, RD]),
    plain("tlbr", u32::MAX, op(0x10) | (16 << 21) | 0x01, &[]),
    plain("tlbwi", u32::MAX, op(0x10) | (16 << 21) | 0x02, &[]),
    plain("tlbwr", u32::MAX, op(0x10) | (16 << 21) | 0x06, &[]),
    plain("tlbp", u32::MAX, op(0x10) | (16 << 21) | 0x08, &[]),
    transfer("eret", u32::MAX, op(0x10) | (16 << 21) | 0x18, &[], Flow::Return, Delay::None),
    plain("cache", OP, op(0x2f), &[RS, D32]),
    plain("lwc1", OP, op(0x31), &[FT, RS, D32]),
    plain("ldc1", OP, op(0x35), &[FT, RS, D64]),
    plain("swc1", OP, op(0x39), &[FT, RS, D32]),
    plain("sdc1", OP, op(0x3d), &[FT, RS, D64]),
    plain("mfc1", 0xffe0_07ff, op(0x11), &[RT, FS]),
    plain("dmfc1", 0xffe0_07ff, op(0x11) | (1 << 21), &[RT, FS]),
    plain("cfc1", 0xffe0_07ff, op(0x11) | (2 << 21), &[RT, FS]),
    plain("mtc1", 0xffe0_07ff, op(0x11) | (4 << 21), &[RT, FS]),
    plain("dmtc1", 0xffe0_07ff, op(0x11) | (5 << 21), &[RT, FS]),
    plain("ctc1", 0xffe0_07ff, op(0x11) | (6 << 21), &[RT, FS]),
    plain("cop1.s", 0xffe0_0000, op(0x11) | (16 << 21), &[FD, FS, FT]),
    plain("cop1.d", 0xffe0_0000, op(0x11) | (17 << 21), &[FD, FS, FT]),
    plain("cop1.w", 0xffe0_0000, op(0x11) | (20 << 21), &[FD, FS]),
    plain("cop1.l", 0xffe0_0000, op(0x11) | (21 << 21), &[FD, FS]),
    transfer("bc1f", 0xffff_0000, op(0x11) | (8 << 21), &[OFFSET], Flow::Branch, Delay::Slot),
    transfer("bc1t", 0xffff_0000, op(0x11) | (8 << 21) | (1 << 16), &[OFFSET], Flow::Branch, Delay::Slot),
    transfer("bc1fl", 0xffff_0000, op(0x11) | (8 << 21) | (2 << 16), &[OFFSET], Flow::Branch, Delay::Likely),
    transfer("bc1tl", 0xffff_0000, op(0x11) | (8 << 21) | (3 << 16), &[OFFSET], Flow::Branch, Delay::Likely),
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

fn machine_of(name: &'static str, forms: &[&[Form]]) -> Machine {
    let rows: Vec<Row> = forms.iter().flat_map(|f| f.iter()).map(|form| form.row).collect();
    Machine {
        name,
        table: Table::new(&rows).expect("the MIPS rows make a table"),
        registers: &REGISTERS,
        address_bits: 32,
        syntax: Syntax { immediate_prefix: "", hex: true },
    }
}

static MACHINE: LazyLock<Machine> = LazyLock::new(|| machine_of("mips", &[FORMS]));
static R5900: LazyLock<Machine> = LazyLock::new(|| machine_of("r5900", &[FORMS, R5900_FORMS]));
static ALLEGREX_MACHINE: LazyLock<Machine> = LazyLock::new(|| machine_of("allegrex", &[FORMS, ALLEGREX]));
static VR4300_MACHINE: LazyLock<Machine> = LazyLock::new(|| machine_of("vr4300", &[FORMS, &R5900_FORMS[..MIPS3_ROWS], VR4300]));

/// The form of an instruction's code: a row of [`FORMS`], then of [`R5900_FORMS`], then of
/// [`ALLEGREX`], then of [`VR4300`].
fn form(at: usize) -> Option<&'static Form> {
    let past = |n: usize| at.checked_sub(n);
    FORMS
        .get(at)
        .or_else(|| R5900_FORMS.get(past(FORMS.len())?))
        .or_else(|| ALLEGREX.get(past(FORMS.len() + R5900_FORMS.len())?))
        .or_else(|| VR4300.get(past(FORMS.len() + R5900_FORMS.len() + ALLEGREX.len())?))
}

/// The 32-bit MIPS decoder, in the byte order of the file it reads; with `r5900`, the Emotion
/// Engine's, with `allegrex`, the PSP's, whose rows take encodings other MIPS processors give
/// other instructions, and with `vr4300`, the Nintendo 64's MIPS III.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mips {
    pub big_endian: bool,
    pub r5900: bool,
    pub allegrex: bool,
    pub vr4300: bool,
}

impl Mips {
    /// The machine the rows make, for the lift.
    pub fn machine() -> &'static Machine {
        &MACHINE
    }

    /// The machine of the R5900's rows.
    pub fn r5900_machine() -> &'static Machine {
        &R5900
    }

    /// The machine of the Allegrex's rows, the MIPS ones first.
    pub fn allegrex_machine() -> &'static Machine {
        &ALLEGREX_MACHINE
    }

    /// When the flow of `instr`, decoded by this crate, takes effect.
    pub fn delay(instr: &Instr) -> Delay {
        usize::try_from(instr.code).ok().and_then(form).map_or(Delay::None, |form| form.delay)
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

    fn delay(&self, instr: &Instr) -> Delay {
        Mips::delay(instr)
    }

    fn decode(&self, bytes: &[u8], address: u64) -> Option<Instr> {
        let word: [u8; 4] = bytes.get(..4)?.try_into().ok()?;
        let word = if self.big_endian { u32::from_be_bytes(word) } else { u32::from_le_bytes(word) };
        let machine: &Machine = if self.allegrex {
            &ALLEGREX_MACHINE
        } else if self.r5900 {
            &R5900
        } else if self.vr4300 {
            &VR4300_MACHINE
        } else {
            &MACHINE
        };
        let mut instr = machine.instr(word, address)?;
        if self.allegrex && usize::try_from(instr.code).ok()? >= FORMS.len() {
            instr.code += u32::try_from(R5900_FORMS.len()).ok()?;
        }
        if self.vr4300 && usize::try_from(instr.code).ok()? >= FORMS.len() + MIPS3_ROWS {
            instr.code += u32::try_from(R5900_FORMS.len() - MIPS3_ROWS + ALLEGREX.len()).ok()?;
        }
        let form = form(usize::try_from(instr.code).ok()?)?;
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
