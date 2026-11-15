//! The rows of the SH-4 encoding and the [`Sh4`] decoder over them: a 16-bit word becomes the
//! neutral [`Instr`] with the text `sh-elf-objdump` prints, in the FPU mode the decoder is given.
//!
//! The same FPU word means a single or a double operation by `fpscr.pr`, and an `fmov` moves one
//! register or a pair by `fpscr.sz`. The word alone does not say which, so the decoder carries the
//! mode; the default, both bits clear, is the reading objdump prints. The SH-4A additions objdump
//! decodes in the same set (`movli.l`, `movua.l`, `fsrra`, `fsca` and their kin) are rows too, so
//! the decoder and objdump agree on every word.

use ra_asm_arch::{Access, Arch, Delay, Flow, Instr, MemOperand, Operand, Prefixes, Register, Target};

use crate::registers::{self, DBR, FPSCR, FPUL, FR0, GBR, M, MACH, MACL, PC, PR, Q, S, SGR, SP, SPC, SR, SSR, T, VBR, XMTRX};

/// The FPU mode a row needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Any,
    /// `fpscr.pr` clear: the single-precision reading.
    Single,
    /// `fpscr.pr` set: the double-precision reading, on even register numbers only.
    Double,
    /// `fpscr.sz` clear: an `fmov` of one register.
    Words,
    /// `fpscr.sz` set: an `fmov` of a pair, `dr` or `xd` by the low bit of the field.
    Pairs,
}

/// An operand of a row, by the field that holds it: `N` is bits 8 to 11, `M` bits 4 to 7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arg {
    N,
    M,
    R0,
    /// `r<k>_bank`, `k` in bits 4 to 6.
    Bank,
    /// A register the row names.
    Reg(u16),
    /// `@rn` of a jump or a call: the target is the register's value.
    Via,
    AtN,
    AtM,
    PostN,
    PostM,
    PreN,
    /// `@(r0,rn)`.
    IdxN,
    IdxM,
    /// `@(disp,rn)`, the four-bit displacement scaled by the width.
    DispN(u8),
    DispM(u8),
    /// `@(disp,gbr)`, the eight-bit displacement scaled by the width.
    Gbr(u8),
    IdxGbr,
    /// A word `@(disp,pc)`: four past the instruction and the displacement in halfwords.
    PcWord,
    /// A long `@(disp,pc)`: four past the instruction rounded down to a long, and the displacement
    /// in longs.
    PcLong,
    Simm,
    Uimm,
    /// The target of `bt`, `bf` and their delayed forms: eight bits of halfwords.
    Near,
    /// The target of `bra` and `bsr`: twelve bits of halfwords.
    Far,
    FrN,
    FrM,
    DrN,
    DrM,
    /// `fv<n>`, `n` in bits 10 and 11.
    FvN,
    /// `fv<m>`, `m` in bits 8 and 9.
    FvM,
    /// A pair of the double-width `fmov`: `dr<n>` or, with the low bit set, `xd<n-1>`.
    PairN,
    PairM,
}

/// How a row uses its operands: the last is the destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Use {
    /// Every operand is read.
    Read,
    /// The last is written.
    Write,
    /// The last is read and written.
    Modify,
    /// The memory operand is an address the cache or the prefetch acts on, not an access.
    Touch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Next,
    Jump,
    CondJump,
    Call,
    Return,
    Interrupt,
    Halt,
}

/// One form of the encoding.
#[derive(Debug, Clone, Copy)]
struct Row {
    mask: u16,
    bits: u16,
    mode: Mode,
    name: &'static str,
    args: &'static [Arg],
    uses: Use,
    /// The width of the memory access, in bits; zero when the form takes the address only.
    width: u16,
    implicit: &'static [(u16, Access)],
    kind: Kind,
    /// Whether the transfer takes effect after the delay slot.
    delayed: bool,
}

const fn row(mask: u16, bits: u16, name: &'static str, args: &'static [Arg], uses: Use) -> Row {
    Row { mask, bits, mode: Mode::Any, name, args, uses, width: 0, implicit: &[], kind: Kind::Next, delayed: false }
}

impl Row {
    const fn mem(self, width: u16) -> Self {
        Self { width, ..self }
    }

    const fn with(self, implicit: &'static [(u16, Access)]) -> Self {
        Self { implicit, ..self }
    }

    const fn mode(self, mode: Mode) -> Self {
        Self { mode, ..self }
    }

    const fn flow(self, kind: Kind) -> Self {
        Self { kind, ..self }
    }

    const fn delayed(self, kind: Kind) -> Self {
        Self { kind, delayed: true, ..self }
    }
}

/// A form with no operands.
const fn exact(bits: u16, name: &'static str) -> Row {
    row(0xffff, bits, name, &[], Use::Read)
}

/// `op rm,rn`.
const fn pair(bits: u16, name: &'static str, uses: Use) -> Row {
    row(0xf00f, bits, name, &[Arg::M, Arg::N], uses)
}

/// `op rn`, read and written.
const fn unary(bits: u16, name: &'static str) -> Row {
    row(0xf0ff, bits, name, &[Arg::N], Use::Modify)
}

/// A form of one register field and a register it names, in either order.
const fn one(bits: u16, name: &'static str, args: &'static [Arg]) -> Row {
    row(0xf0ff, bits, name, args, Use::Write)
}

/// An immediate form on `r0`, or a displacement form, whose low byte is the operand.
const fn byte(bits: u16, name: &'static str, args: &'static [Arg], uses: Use) -> Row {
    row(0xff00, bits, name, args, uses)
}

/// The FPU forms of two registers.
const fn float(bits: u16, name: &'static str, args: &'static [Arg], uses: Use, mode: Mode) -> Row {
    row(0xf00f, bits, name, args, uses).mode(mode).with(FP)
}

/// The FPU forms of one register field.
const fn float_one(bits: u16, name: &'static str, args: &'static [Arg], uses: Use, mode: Mode) -> Row {
    row(0xf0ff, bits, name, args, uses).mode(mode).with(FP)
}

const TW: &[(u16, Access)] = &[(T, Access::Write)];
const TR: &[(u16, Access)] = &[(T, Access::Read)];
const TRW: &[(u16, Access)] = &[(T, Access::ReadWrite)];
const SW: &[(u16, Access)] = &[(S, Access::Write)];
const MACL_W: &[(u16, Access)] = &[(MACL, Access::Write)];
const MAC_W: &[(u16, Access)] = &[(MACH, Access::Write), (MACL, Access::Write)];
const MAC_RW: &[(u16, Access)] = &[(MACH, Access::ReadWrite), (MACL, Access::ReadWrite), (S, Access::Read)];
const DIV0: &[(u16, Access)] = &[(Q, Access::Write), (M, Access::Write), (T, Access::Write)];
const DIV1: &[(u16, Access)] = &[(Q, Access::ReadWrite), (M, Access::Read), (T, Access::ReadWrite)];
const LINK: &[(u16, Access)] = &[(PR, Access::Write)];
const LINK_PC: &[(u16, Access)] = &[(PR, Access::Write), (PC, Access::Read)];
const AT_PC: &[(u16, Access)] = &[(PC, Access::Read)];
const RETURN: &[(u16, Access)] = &[(PR, Access::Read)];
const EXCEPTION_RETURN: &[(u16, Access)] = &[(SSR, Access::Read), (SPC, Access::Read), (SR, Access::Write)];
const TRAP: &[(u16, Access)] = &[(SR, Access::ReadWrite), (SSR, Access::Write), (SPC, Access::Write), (SGR, Access::Write)];
const FP: &[(u16, Access)] = &[(FPSCR, Access::Read)];
const FP_T: &[(u16, Access)] = &[(FPSCR, Access::Read), (T, Access::Write)];
const FP_MODE: &[(u16, Access)] = &[(FPSCR, Access::ReadWrite)];

use Arg::{
    AtM, AtN, Bank, DispM, DispN, DrM, DrN, Far, FrM, FrN, FvM, FvN, Gbr, IdxGbr, IdxM, IdxN, Near, PairM, PairN, PcLong, PcWord, PostM,
    PostN, PreN, R0, Reg, Simm, Uimm, Via,
};
use Mode::{Double, Pairs, Single, Words};
use Use::{Modify, Read, Touch, Write};

/// The forms, in the order of the first nibble.
const ROWS: &[Row] = &[
    // 0000.
    one(0x0002, "stc", &[Reg(SR), Arg::N]),
    one(0x0012, "stc", &[Reg(GBR), Arg::N]),
    one(0x0022, "stc", &[Reg(VBR), Arg::N]),
    one(0x0032, "stc", &[Reg(SSR), Arg::N]),
    one(0x0042, "stc", &[Reg(SPC), Arg::N]),
    one(0x003a, "stc", &[Reg(SGR), Arg::N]),
    one(0x00fa, "stc", &[Reg(DBR), Arg::N]),
    row(0xf08f, 0x0082, "stc", &[Bank, Arg::N], Write),
    row(0xf0ff, 0x0003, "bsrf", &[Arg::N], Read).with(LINK_PC).delayed(Kind::Call),
    row(0xf0ff, 0x0023, "braf", &[Arg::N], Read).with(AT_PC).delayed(Kind::Jump),
    one(0x0063, "movli.l", &[AtN, R0]).mem(32),
    one(0x0073, "movco.l", &[R0, AtN]).mem(32).with(TW),
    row(0xf0ff, 0x0083, "pref", &[AtN], Touch),
    row(0xf0ff, 0x0093, "ocbi", &[AtN], Touch),
    row(0xf0ff, 0x00a3, "ocbp", &[AtN], Touch),
    row(0xf0ff, 0x00b3, "ocbwb", &[AtN], Touch),
    one(0x00c3, "movca.l", &[R0, AtN]).mem(32),
    row(0xf0ff, 0x00d3, "prefi", &[AtN], Touch),
    row(0xf0ff, 0x00e3, "icbi", &[AtN], Touch),
    row(0xf00f, 0x0004, "mov.b", &[Arg::M, IdxN], Write).mem(8),
    row(0xf00f, 0x0005, "mov.w", &[Arg::M, IdxN], Write).mem(16),
    row(0xf00f, 0x0006, "mov.l", &[Arg::M, IdxN], Write).mem(32),
    pair(0x0007, "mul.l", Read).with(MACL_W),
    exact(0x0008, "clrt").with(TW),
    exact(0x0018, "sett").with(TW),
    exact(0x0028, "clrmac").with(MAC_W),
    exact(0x0038, "ldtlb"),
    exact(0x0048, "clrs").with(SW),
    exact(0x0058, "sets").with(SW),
    exact(0x0009, "nop"),
    exact(0x0019, "div0u").with(DIV0),
    one(0x0029, "movt", &[Arg::N]).with(TR),
    one(0x000a, "sts", &[Reg(MACH), Arg::N]),
    one(0x001a, "sts", &[Reg(MACL), Arg::N]),
    one(0x002a, "sts", &[Reg(PR), Arg::N]),
    one(0x005a, "sts", &[Reg(FPUL), Arg::N]),
    one(0x006a, "sts", &[Reg(FPSCR), Arg::N]),
    exact(0x000b, "rts").with(RETURN).delayed(Kind::Return),
    exact(0x001b, "sleep").flow(Kind::Halt),
    exact(0x002b, "rte").with(EXCEPTION_RETURN).delayed(Kind::Return),
    exact(0x00ab, "synco"),
    row(0xf00f, 0x000c, "mov.b", &[IdxM, Arg::N], Write).mem(8),
    row(0xf00f, 0x000d, "mov.w", &[IdxM, Arg::N], Write).mem(16),
    row(0xf00f, 0x000e, "mov.l", &[IdxM, Arg::N], Write).mem(32),
    row(0xf00f, 0x000f, "mac.l", &[PostM, PostN], Read).mem(32).with(MAC_RW),
    // 0001.
    row(0xf000, 0x1000, "mov.l", &[Arg::M, DispN(4)], Write).mem(32),
    // 0010.
    row(0xf00f, 0x2000, "mov.b", &[Arg::M, AtN], Write).mem(8),
    row(0xf00f, 0x2001, "mov.w", &[Arg::M, AtN], Write).mem(16),
    row(0xf00f, 0x2002, "mov.l", &[Arg::M, AtN], Write).mem(32),
    row(0xf00f, 0x2004, "mov.b", &[Arg::M, PreN], Write).mem(8),
    row(0xf00f, 0x2005, "mov.w", &[Arg::M, PreN], Write).mem(16),
    row(0xf00f, 0x2006, "mov.l", &[Arg::M, PreN], Write).mem(32),
    pair(0x2007, "div0s", Read).with(DIV0),
    pair(0x2008, "tst", Read).with(TW),
    pair(0x2009, "and", Modify),
    pair(0x200a, "xor", Modify),
    pair(0x200b, "or", Modify),
    pair(0x200c, "cmp/str", Read).with(TW),
    pair(0x200d, "xtrct", Modify),
    pair(0x200e, "mulu.w", Read).with(MACL_W),
    pair(0x200f, "muls.w", Read).with(MACL_W),
    // 0011.
    pair(0x3000, "cmp/eq", Read).with(TW),
    pair(0x3002, "cmp/hs", Read).with(TW),
    pair(0x3003, "cmp/ge", Read).with(TW),
    pair(0x3004, "div1", Modify).with(DIV1),
    pair(0x3005, "dmulu.l", Read).with(MAC_W),
    pair(0x3006, "cmp/hi", Read).with(TW),
    pair(0x3007, "cmp/gt", Read).with(TW),
    pair(0x3008, "sub", Modify),
    pair(0x300a, "subc", Modify).with(TRW),
    pair(0x300b, "subv", Modify).with(TW),
    pair(0x300c, "add", Modify),
    pair(0x300d, "dmuls.l", Read).with(MAC_W),
    pair(0x300e, "addc", Modify).with(TRW),
    pair(0x300f, "addv", Modify).with(TW),
    // 0100.
    unary(0x4000, "shll").with(TW),
    unary(0x4010, "dt").with(TW),
    unary(0x4020, "shal").with(TW),
    unary(0x4001, "shlr").with(TW),
    row(0xf0ff, 0x4011, "cmp/pz", &[Arg::N], Read).with(TW),
    unary(0x4021, "shar").with(TW),
    one(0x4002, "sts.l", &[Reg(MACH), PreN]).mem(32),
    one(0x4012, "sts.l", &[Reg(MACL), PreN]).mem(32),
    one(0x4022, "sts.l", &[Reg(PR), PreN]).mem(32),
    one(0x4032, "stc.l", &[Reg(SGR), PreN]).mem(32),
    one(0x4052, "sts.l", &[Reg(FPUL), PreN]).mem(32),
    one(0x4062, "sts.l", &[Reg(FPSCR), PreN]).mem(32),
    one(0x40f2, "stc.l", &[Reg(DBR), PreN]).mem(32),
    one(0x4003, "stc.l", &[Reg(SR), PreN]).mem(32),
    one(0x4013, "stc.l", &[Reg(GBR), PreN]).mem(32),
    one(0x4023, "stc.l", &[Reg(VBR), PreN]).mem(32),
    one(0x4033, "stc.l", &[Reg(SSR), PreN]).mem(32),
    one(0x4043, "stc.l", &[Reg(SPC), PreN]).mem(32),
    row(0xf08f, 0x4083, "stc.l", &[Bank, PreN], Write).mem(32),
    unary(0x4004, "rotl").with(TW),
    unary(0x4024, "rotcl").with(TRW),
    unary(0x4005, "rotr").with(TW),
    row(0xf0ff, 0x4015, "cmp/pl", &[Arg::N], Read).with(TW),
    unary(0x4025, "rotcr").with(TRW),
    one(0x4006, "lds.l", &[PostN, Reg(MACH)]).mem(32),
    one(0x4016, "lds.l", &[PostN, Reg(MACL)]).mem(32),
    one(0x4026, "lds.l", &[PostN, Reg(PR)]).mem(32),
    one(0x4036, "ldc.l", &[PostN, Reg(SGR)]).mem(32),
    one(0x4056, "lds.l", &[PostN, Reg(FPUL)]).mem(32),
    one(0x4066, "lds.l", &[PostN, Reg(FPSCR)]).mem(32),
    one(0x40f6, "ldc.l", &[PostN, Reg(DBR)]).mem(32),
    one(0x4007, "ldc.l", &[PostN, Reg(SR)]).mem(32),
    one(0x4017, "ldc.l", &[PostN, Reg(GBR)]).mem(32),
    one(0x4027, "ldc.l", &[PostN, Reg(VBR)]).mem(32),
    one(0x4037, "ldc.l", &[PostN, Reg(SSR)]).mem(32),
    one(0x4047, "ldc.l", &[PostN, Reg(SPC)]).mem(32),
    row(0xf08f, 0x4087, "ldc.l", &[PostN, Bank], Write).mem(32),
    unary(0x4008, "shll2"),
    unary(0x4018, "shll8"),
    unary(0x4028, "shll16"),
    unary(0x4009, "shlr2"),
    unary(0x4019, "shlr8"),
    unary(0x4029, "shlr16"),
    one(0x40a9, "movua.l", &[AtN, R0]).mem(32),
    one(0x40e9, "movua.l", &[PostN, R0]).mem(32),
    one(0x400a, "lds", &[Arg::N, Reg(MACH)]),
    one(0x401a, "lds", &[Arg::N, Reg(MACL)]),
    one(0x402a, "lds", &[Arg::N, Reg(PR)]),
    one(0x403a, "ldc", &[Arg::N, Reg(SGR)]),
    one(0x405a, "lds", &[Arg::N, Reg(FPUL)]),
    one(0x406a, "lds", &[Arg::N, Reg(FPSCR)]),
    one(0x40fa, "ldc", &[Arg::N, Reg(DBR)]),
    row(0xf0ff, 0x400b, "jsr", &[Via], Read).with(LINK).delayed(Kind::Call),
    row(0xf0ff, 0x401b, "tas.b", &[AtN], Modify).mem(8).with(TW),
    row(0xf0ff, 0x402b, "jmp", &[Via], Read).delayed(Kind::Jump),
    pair(0x400c, "shad", Modify),
    pair(0x400d, "shld", Modify),
    one(0x400e, "ldc", &[Arg::N, Reg(SR)]),
    one(0x401e, "ldc", &[Arg::N, Reg(GBR)]),
    one(0x402e, "ldc", &[Arg::N, Reg(VBR)]),
    one(0x403e, "ldc", &[Arg::N, Reg(SSR)]),
    one(0x404e, "ldc", &[Arg::N, Reg(SPC)]),
    row(0xf08f, 0x408e, "ldc", &[Arg::N, Bank], Write),
    row(0xf00f, 0x400f, "mac.w", &[PostM, PostN], Read).mem(16).with(MAC_RW),
    // 0101.
    row(0xf000, 0x5000, "mov.l", &[DispM(4), Arg::N], Write).mem(32),
    // 0110.
    row(0xf00f, 0x6000, "mov.b", &[AtM, Arg::N], Write).mem(8),
    row(0xf00f, 0x6001, "mov.w", &[AtM, Arg::N], Write).mem(16),
    row(0xf00f, 0x6002, "mov.l", &[AtM, Arg::N], Write).mem(32),
    pair(0x6003, "mov", Write),
    row(0xf00f, 0x6004, "mov.b", &[PostM, Arg::N], Write).mem(8),
    row(0xf00f, 0x6005, "mov.w", &[PostM, Arg::N], Write).mem(16),
    row(0xf00f, 0x6006, "mov.l", &[PostM, Arg::N], Write).mem(32),
    pair(0x6007, "not", Write),
    pair(0x6008, "swap.b", Write),
    pair(0x6009, "swap.w", Write),
    pair(0x600a, "negc", Write).with(TRW),
    pair(0x600b, "neg", Write),
    pair(0x600c, "extu.b", Write),
    pair(0x600d, "extu.w", Write),
    pair(0x600e, "exts.b", Write),
    pair(0x600f, "exts.w", Write),
    // 0111.
    row(0xf000, 0x7000, "add", &[Simm, Arg::N], Modify),
    // 1000.
    byte(0x8000, "mov.b", &[R0, DispM(1)], Write).mem(8),
    byte(0x8100, "mov.w", &[R0, DispM(2)], Write).mem(16),
    byte(0x8400, "mov.b", &[DispM(1), R0], Write).mem(8),
    byte(0x8500, "mov.w", &[DispM(2), R0], Write).mem(16),
    byte(0x8800, "cmp/eq", &[Simm, R0], Read).with(TW),
    byte(0x8900, "bt", &[Near], Read).with(TR).flow(Kind::CondJump),
    byte(0x8b00, "bf", &[Near], Read).with(TR).flow(Kind::CondJump),
    byte(0x8d00, "bt.s", &[Near], Read).with(TR).delayed(Kind::CondJump),
    byte(0x8f00, "bf.s", &[Near], Read).with(TR).delayed(Kind::CondJump),
    // 1001.
    row(0xf000, 0x9000, "mov.w", &[PcWord, Arg::N], Write).mem(16),
    // 1010 and 1011.
    row(0xf000, 0xa000, "bra", &[Far], Read).delayed(Kind::Jump),
    row(0xf000, 0xb000, "bsr", &[Far], Read).with(LINK).delayed(Kind::Call),
    // 1100.
    byte(0xc000, "mov.b", &[R0, Gbr(1)], Write).mem(8),
    byte(0xc100, "mov.w", &[R0, Gbr(2)], Write).mem(16),
    byte(0xc200, "mov.l", &[R0, Gbr(4)], Write).mem(32),
    byte(0xc300, "trapa", &[Uimm], Read).with(TRAP).flow(Kind::Interrupt),
    byte(0xc400, "mov.b", &[Gbr(1), R0], Write).mem(8),
    byte(0xc500, "mov.w", &[Gbr(2), R0], Write).mem(16),
    byte(0xc600, "mov.l", &[Gbr(4), R0], Write).mem(32),
    byte(0xc700, "mova", &[PcLong, R0], Write),
    byte(0xc800, "tst", &[Uimm, R0], Read).with(TW),
    byte(0xc900, "and", &[Uimm, R0], Modify),
    byte(0xca00, "xor", &[Uimm, R0], Modify),
    byte(0xcb00, "or", &[Uimm, R0], Modify),
    byte(0xcc00, "tst.b", &[Uimm, IdxGbr], Read).mem(8).with(TW),
    byte(0xcd00, "and.b", &[Uimm, IdxGbr], Modify).mem(8),
    byte(0xce00, "xor.b", &[Uimm, IdxGbr], Modify).mem(8),
    byte(0xcf00, "or.b", &[Uimm, IdxGbr], Modify).mem(8),
    // 1101 and 1110.
    row(0xf000, 0xd000, "mov.l", &[PcLong, Arg::N], Write).mem(32),
    row(0xf000, 0xe000, "mov", &[Simm, Arg::N], Write),
    // 1111: the FPU.
    float(0xf000, "fadd", &[FrM, FrN], Modify, Single),
    float(0xf001, "fsub", &[FrM, FrN], Modify, Single),
    float(0xf002, "fmul", &[FrM, FrN], Modify, Single),
    float(0xf003, "fdiv", &[FrM, FrN], Modify, Single),
    float(0xf004, "fcmp/eq", &[FrM, FrN], Read, Single).with(FP_T),
    float(0xf005, "fcmp/gt", &[FrM, FrN], Read, Single).with(FP_T),
    float(0xf000, "fadd", &[DrM, DrN], Modify, Double),
    float(0xf001, "fsub", &[DrM, DrN], Modify, Double),
    float(0xf002, "fmul", &[DrM, DrN], Modify, Double),
    float(0xf003, "fdiv", &[DrM, DrN], Modify, Double),
    float(0xf004, "fcmp/eq", &[DrM, DrN], Read, Double).with(FP_T),
    float(0xf005, "fcmp/gt", &[DrM, DrN], Read, Double).with(FP_T),
    float(0xf006, "fmov", &[IdxM, FrN], Write, Words).mem(32),
    float(0xf007, "fmov", &[FrM, IdxN], Write, Words).mem(32),
    float(0xf008, "fmov", &[AtM, FrN], Write, Words).mem(32),
    float(0xf009, "fmov", &[PostM, FrN], Write, Words).mem(32),
    float(0xf00a, "fmov", &[FrM, AtN], Write, Words).mem(32),
    float(0xf00b, "fmov", &[FrM, PreN], Write, Words).mem(32),
    float(0xf00c, "fmov", &[FrM, FrN], Write, Words),
    float(0xf006, "fmov", &[IdxM, PairN], Write, Pairs).mem(64),
    float(0xf007, "fmov", &[PairM, IdxN], Write, Pairs).mem(64),
    float(0xf008, "fmov", &[AtM, PairN], Write, Pairs).mem(64),
    float(0xf009, "fmov", &[PostM, PairN], Write, Pairs).mem(64),
    float(0xf00a, "fmov", &[PairM, AtN], Write, Pairs).mem(64),
    float(0xf00b, "fmov", &[PairM, PreN], Write, Pairs).mem(64),
    float(0xf00c, "fmov", &[PairM, PairN], Write, Pairs),
    float(0xf00e, "fmac", &[Reg(FR0), FrM, FrN], Modify, Single),
    float_one(0xf00d, "fsts", &[Reg(FPUL), FrN], Write, Mode::Any),
    float_one(0xf01d, "flds", &[FrN, Reg(FPUL)], Write, Mode::Any),
    float_one(0xf02d, "float", &[Reg(FPUL), FrN], Write, Single),
    float_one(0xf02d, "float", &[Reg(FPUL), DrN], Write, Double),
    float_one(0xf03d, "ftrc", &[FrN, Reg(FPUL)], Write, Single),
    float_one(0xf03d, "ftrc", &[DrN, Reg(FPUL)], Write, Double),
    float_one(0xf04d, "fneg", &[FrN], Modify, Single),
    float_one(0xf04d, "fneg", &[DrN], Modify, Double),
    float_one(0xf05d, "fabs", &[FrN], Modify, Single),
    float_one(0xf05d, "fabs", &[DrN], Modify, Double),
    float_one(0xf06d, "fsqrt", &[FrN], Modify, Single),
    float_one(0xf06d, "fsqrt", &[DrN], Modify, Double),
    float_one(0xf07d, "fsrra", &[FrN], Modify, Single),
    float_one(0xf08d, "fldi0", &[FrN], Write, Single),
    float_one(0xf09d, "fldi1", &[FrN], Write, Single),
    float_one(0xf0ad, "fcnvsd", &[Reg(FPUL), DrN], Write, Mode::Any),
    float_one(0xf0bd, "fcnvds", &[DrN, Reg(FPUL)], Write, Mode::Any),
    float_one(0xf0ed, "fipr", &[FvM, FvN], Modify, Single),
    row(0xf3ff, 0xf1fd, "ftrv", &[Reg(XMTRX), FvN], Modify).mode(Single).with(FP),
    row(0xf1ff, 0xf0fd, "fsca", &[Reg(FPUL), DrN], Write).mode(Single).with(FP),
    exact(0xf3fd, "fschg").with(FP_MODE),
    exact(0xf7fd, "fpchg").with(FP_MODE),
    exact(0xfbfd, "frchg").with(FP_MODE),
];

/// The SH-4 decoder in the FPU mode of `fpscr`, in the byte order of the file it reads: the
/// Dreamcast's is little-endian, the default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sh4 {
    pub big_endian: bool,
    /// `fpscr.pr`: the FPU operations are on the pairs `dr0` to `dr14`.
    pub double: bool,
    /// `fpscr.sz`: an `fmov` moves a pair.
    pub pairs: bool,
}

/// The bits of [`Instr::code`] above the word that say the mode it was decoded in.
const CODE_DOUBLE: u32 = 1 << 16;
const CODE_PAIRS: u32 = 1 << 17;

impl Sh4 {
    /// The row of `word` in this decoder's mode.
    fn find(self, word: u16) -> Option<&'static Row> {
        ROWS.iter().find(|row| {
            word & row.mask == row.bits
                && match row.mode {
                    Mode::Any => true,
                    Single => !self.double,
                    Double => self.double,
                    Words => !self.pairs,
                    Pairs => self.pairs,
                }
        })
    }

    /// The decoder and the word an instruction's code holds.
    fn from_code(code: u32) -> (Self, u16) {
        (Self { big_endian: false, double: code & CODE_DOUBLE != 0, pairs: code & CODE_PAIRS != 0 }, code as u16)
    }

    /// When the flow of `instr`, decoded by this crate, takes effect: the branches, the jumps, the
    /// calls and the returns of the SH have a delay slot, all but `bt` and `bf`.
    pub fn delay(instr: &Instr) -> Delay {
        let (sh4, word) = Self::from_code(instr.code);
        match sh4.find(word) {
            Some(row) if row.delayed => Delay::Slot,
            _ => Delay::None,
        }
    }

    /// Where control goes once the flow of `instr` has taken effect, the taken target first: a
    /// delayed transfer falls through, or returns from its call, past its delay slot.
    pub fn successors(instr: &Instr) -> Vec<u64> {
        let after = match Self::delay(instr) {
            Delay::None => instr.end(),
            Delay::Slot | Delay::Likely => instr.end().wrapping_add(2),
        } & u64::from(u32::MAX);
        let mut out = Vec::with_capacity(2);
        if let Flow::Jump(Target::Direct(target)) | Flow::CondJump(Target::Direct(target)) = instr.flow {
            out.push(target);
        }
        if instr.flow.falls_through() {
            out.push(after);
        }
        out
    }

    /// Decodes one word at `address`.
    pub fn decode_word(self, word: u16, address: u64) -> Option<Instr> {
        let row = self.find(word)?;
        let mut builder =
            Builder { row, word, address, operands: Vec::new(), texts: Vec::new(), registers: Vec::new(), reads: false, writes: false };
        let last = row.args.len().saturating_sub(1);
        for (i, &arg) in row.args.iter().enumerate() {
            let role = match row.uses {
                Write if i == last => Role::Written,
                Modify if i == last => Role::Modified,
                Read | Write | Modify => Role::Source,
                Touch => Role::Address,
            };
            builder.arg(arg, role)?;
        }
        builder.registers.extend(row.implicit.iter().map(|&(id, access)| (registers::register(id), access)));
        let target = builder.operands.iter().find_map(|o| match o {
            Operand::Addr(t) if matches!(row.args.first(), Some(Near | Far)) => Some(*t),
            _ => None,
        });
        let to = target.map_or(Target::Indirect, Target::Direct);
        let flow = match row.kind {
            Kind::Next => Flow::Next,
            Kind::Jump => Flow::Jump(to),
            Kind::CondJump => Flow::CondJump(to),
            Kind::Call => Flow::Call(to),
            Kind::Return => Flow::Return,
            Kind::Interrupt => Flow::Interrupt,
            Kind::Halt => Flow::Halt,
        };
        let text = if builder.texts.is_empty() { row.name.to_owned() } else { format!("{}\t{}", row.name, builder.texts.join(",")) };
        let mode = if self.double { CODE_DOUBLE } else { 0 };
        let code = u32::from(word) | mode | if self.pairs { CODE_PAIRS } else { 0 };
        Some(Instr {
            address,
            len: 2,
            mnemonic: row.name.to_owned(),
            text,
            operands: builder.operands,
            flow,
            prefixes: Prefixes::default(),
            registers: builder.registers,
            reads_memory: builder.reads,
            writes_memory: builder.writes,
            code,
        })
    }
}

/// How the row uses one operand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    Source,
    Written,
    Modified,
    Address,
}

impl Role {
    fn register(self) -> Access {
        match self {
            Role::Source | Role::Address => Access::Read,
            Role::Written => Access::Write,
            Role::Modified => Access::ReadWrite,
        }
    }
}

/// An address `disp` bytes past `from`, in the 32 bits of the SH's space.
fn relative(from: u64, disp: i64) -> u64 {
    from.wrapping_add(disp as u64) & u64::from(u32::MAX)
}

/// The record of one instruction as its operands fill it.
struct Builder {
    row: &'static Row,
    word: u16,
    address: u64,
    operands: Vec<Operand>,
    texts: Vec<String>,
    registers: Vec<(Register, Access)>,
    reads: bool,
    writes: bool,
}

impl Builder {
    fn n(&self) -> u16 {
        (self.word >> 8) & 0xf
    }

    fn m(&self) -> u16 {
        (self.word >> 4) & 0xf
    }

    fn reg(&mut self, register: Register, role: Role) {
        self.texts.push(register.name.to_owned());
        self.operands.push(Operand::Reg(register));
        self.registers.push((register, role.register()));
    }

    /// The memory operand at `base + index + disp`, as wide as the row's access.
    fn at(&self, base: Register, index: Option<Register>, disp: i64) -> MemOperand {
        MemOperand { segment: None, base: Some(base), index, scale: 1, disp, bits: self.row.width, absolute: None }
    }

    /// A memory operand: its base and index are read, and a base that steps is written too.
    fn mem(&mut self, text: String, operand: MemOperand, steps: bool, role: Role) {
        if operand.bits != 0 {
            match role {
                Role::Source => self.reads = true,
                Role::Written => self.writes = true,
                Role::Modified => {
                    self.reads = true;
                    self.writes = true;
                }
                Role::Address => {}
            }
        }
        if let Some(base) = operand.base {
            self.registers.push((base, if steps { Access::ReadWrite } else { Access::Read }));
        }
        if let Some(index) = operand.index {
            self.registers.push((index, Access::Read));
        }
        self.texts.push(text);
        self.operands.push(Operand::Mem(operand));
    }

    fn addr(&mut self, target: u64) {
        self.texts.push(format!("{target:#x}"));
        self.operands.push(Operand::Addr(target));
    }

    fn imm(&mut self, value: i64) {
        self.texts.push(format!("#{value}"));
        self.operands.push(Operand::Imm { value, bits: 8 });
    }

    /// Fills one operand; `None` when the field names no register in this mode.
    fn arg(&mut self, arg: Arg, role: Role) -> Option<()> {
        let (n, m) = (self.n(), self.m());
        let low = i64::from(self.word & 0xf);
        let byte = i64::from(self.word & 0xff);
        match arg {
            Arg::N => self.reg(registers::general(n), role),
            Arg::M => self.reg(registers::general(m), role),
            R0 => self.reg(registers::general(0), role),
            Bank => self.reg(registers::bank(m & 7), role),
            Reg(id) => self.reg(registers::register(id), role),
            Via => {
                let register = registers::general(n);
                self.texts.push(format!("@{}", register.name));
                self.operands.push(Operand::Reg(register));
                self.registers.push((register, Access::Read));
            }
            AtN | AtM | PostN | PostM | PreN => {
                let register = registers::general(if matches!(arg, AtM | PostM) { m } else { n });
                let (text, disp, steps) = match arg {
                    AtN | AtM => (format!("@{}", register.name), 0, false),
                    PostN | PostM => (format!("@{}+", register.name), 0, true),
                    // The access is below the register's value, which it leaves there.
                    _ => (format!("@-{}", register.name), -i64::from(self.row.width / 8), true),
                };
                let operand = self.at(register, None, disp);
                self.mem(text, operand, steps, role);
            }
            IdxN | IdxM => {
                let register = registers::general(if arg == IdxM { m } else { n });
                let operand = self.at(register, Some(registers::general(0)), 0);
                self.mem(format!("@(r0,{})", register.name), operand, false, role);
            }
            DispN(scale) | DispM(scale) => {
                let register = registers::general(if matches!(arg, DispM(_)) { m } else { n });
                let disp = low * i64::from(scale);
                let operand = self.at(register, None, disp);
                self.mem(format!("@({disp},{})", register.name), operand, false, role);
            }
            Gbr(scale) => {
                let disp = byte * i64::from(scale);
                let operand = self.at(registers::register(GBR), None, disp);
                self.mem(format!("@({disp},gbr)"), operand, false, role);
            }
            IdxGbr => {
                let operand = self.at(registers::register(GBR), Some(registers::general(0)), 0);
                self.mem("@(r0,gbr)".to_owned(), operand, false, role);
            }
            PcWord | PcLong => {
                let target = if arg == PcWord { relative(self.address, 4 + byte * 2) } else { relative(self.address & !3, 4 + byte * 4) };
                let disp = target.wrapping_sub(self.address) as i64;
                let operand = MemOperand { absolute: Some(target), ..self.at(registers::register(PC), None, disp) };
                self.mem(format!("{target:#x}"), operand, false, role);
            }
            Simm => self.imm(i64::from(self.word as u8 as i8)),
            Uimm => self.imm(byte),
            Near => self.addr(relative(self.address, 4 + i64::from(self.word as u8 as i8) * 2)),
            Far => {
                // The twelve bits, sign-extended.
                let disp = i64::from(((self.word << 4) as i16) >> 4);
                self.addr(relative(self.address, 4 + disp * 2));
            }
            FrN => self.reg(registers::single(n), role),
            FrM => self.reg(registers::single(m), role),
            DrN | DrM => {
                let field = if arg == DrN { n } else { m };
                if field % 2 != 0 {
                    return None;
                }
                self.reg(registers::double(field), role);
            }
            FvN => self.reg(registers::vector((n >> 2) * 4), role),
            FvM => self.reg(registers::vector((n & 3) * 4), role),
            PairN | PairM => {
                let field = if arg == PairN { n } else { m };
                let register = if field % 2 == 0 { registers::double(field) } else { registers::back_double(field - 1) };
                self.reg(register, role);
            }
        }
        Some(())
    }
}

impl Arch for Sh4 {
    fn name(&self) -> &'static str {
        "sh4"
    }

    fn address_bits(&self) -> u16 {
        32
    }

    fn stack_pointer(&self) -> Register {
        registers::register(SP)
    }

    fn instruction_pointer(&self) -> Register {
        registers::register(PC)
    }

    fn delay(&self, instr: &Instr) -> Delay {
        Sh4::delay(instr)
    }

    fn decode(&self, bytes: &[u8], address: u64) -> Option<Instr> {
        let word: [u8; 2] = bytes.get(..2)?.try_into().ok()?;
        let word = if self.big_endian { u16::from_be_bytes(word) } else { u16::from_le_bytes(word) };
        self.decode_word(word, address)
    }
}
