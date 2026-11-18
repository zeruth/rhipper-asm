//! MIPS III's doubleword instructions as rows with their semantics, over a machine of 64-bit
//! registers: the adds and subtracts, the shifts by a constant and by a register, and the
//! doubleword and unsigned-word loads and stores with the unaligned merges, little-endian as the
//! vectors of `testdata/vectors/mips64el` were taken.
//!
//! The rows are the decoder's own, in the places [`Mips::r5900_machine`] gives them: the MIPS rows,
//! then the 22 doubleword forms its table and the VR4300's share. Only those 22 carry semantics
//! here; every other row lifts to a stop that names its word.

use std::sync::LazyLock;

use ra_asm_arch::{RegClass, Register};
use ra_asm_machine::{Builder, Cond, Fields, Lift, Machine, Row, Sem, StateSpec, Table, Val};

use crate::Mips;

/// `hi` and `lo`, after the general registers.
pub const HI: u32 = 32;
pub const LO: u32 = 33;
/// The registers of the state.
pub const REGISTERS: u32 = LO + 1;

/// The state: [`REGISTERS`] registers of 64 bits, `r0` reading as zero, and a 64-bit program
/// counter.
pub const SPEC: StateSpec = StateSpec { registers: REGISTERS, bits: 64, pc_bits: 64, zero: Some(0) };

/// The exception code of an integer overflow (`Ov`), which `dadd`, `daddi` and `dsub` trap with.
pub const OVERFLOW: u64 = 12;

/// The registers by number, as the vector files name them: a name there is an identifier, which
/// objdump's `$n` is not.
const NAMES: [&str; REGISTERS as usize] = [
    "r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15", "r16", "r17", "r18", "r19",
    "r20", "r21", "r22", "r23", "r24", "r25", "r26", "r27", "r28", "r29", "r30", "r31", "hi", "lo",
];

const SP: u32 = 29;

const fn file() -> [Register; REGISTERS as usize] {
    let mut registers = [Register { id: 0, name: "", class: RegClass::General, full: 0, bits: 64, shift: 0 }; REGISTERS as usize];
    let mut n = 0;
    while n < REGISTERS {
        let class = if n == SP {
            RegClass::StackPointer
        } else if n < HI {
            RegClass::General
        } else {
            RegClass::Other
        };
        registers[n as usize] = Register { id: n as u16, name: NAMES[n as usize], class, full: n as u16, bits: 64, shift: 0 };
        n += 1;
    }
    registers
}

static FILE: [Register; REGISTERS as usize] = file();

/// `rs + rt` (or `rs - rt`) into `rd`.
fn wrapping(b: &mut dyn Builder, fields: &Fields<'_>, sub: bool) {
    let (rs, rt) = (b.r(fields.reg("rs")), b.r(fields.reg("rt")));
    let result = if sub { b.sub(64, rs, rt) } else { b.add(64, rs, rt) };
    b.set(fields.reg("rd"), result);
}

/// The signed `a + b` (or `a - b`) into `to`, trapping when it overflows. The trap stops the
/// program, so the register written before it is never seen.
fn checked(b: &mut dyn Builder, fields: &Fields<'_>, to: u32, a: Val, operand: Val, sub: bool) {
    let result = if sub { b.sub(64, a, operand) } else { b.add(64, a, operand) };
    b.set(to, result);
    // An overflow gives a result whose sign differs from the first operand's, where the second's
    // sign agreed with it for a subtraction and differed for an add.
    let changed = b.xor(64, a, result);
    let other = if sub { b.xor(64, a, operand) } else { b.xor(64, operand, result) };
    let both = b.and(64, changed, other);
    let zero = b.imm(64, 0);
    let fine = b.cmp(Cond::Sge, 64, both, zero);
    let next = b.addr(fields.next);
    b.branch_if(fine, next);
    b.trap(OVERFLOW);
}

fn daddu(b: &mut dyn Builder, fields: &Fields<'_>) {
    wrapping(b, fields, false);
}

fn dsubu(b: &mut dyn Builder, fields: &Fields<'_>) {
    wrapping(b, fields, true);
}

fn dadd(b: &mut dyn Builder, fields: &Fields<'_>) {
    let (rs, rt) = (b.r(fields.reg("rs")), b.r(fields.reg("rt")));
    checked(b, fields, fields.reg("rd"), rs, rt, false);
}

fn dsub(b: &mut dyn Builder, fields: &Fields<'_>) {
    let (rs, rt) = (b.r(fields.reg("rs")), b.r(fields.reg("rt")));
    checked(b, fields, fields.reg("rd"), rs, rt, true);
}

fn daddiu(b: &mut dyn Builder, fields: &Fields<'_>) {
    let rs = b.r(fields.reg("rs"));
    let imm = b.imm(64, fields.value("imm"));
    let sum = b.add(64, rs, imm);
    b.set(fields.reg("rt"), sum);
}

fn daddi(b: &mut dyn Builder, fields: &Fields<'_>) {
    let rs = b.r(fields.reg("rs"));
    let imm = b.imm(64, fields.value("imm"));
    checked(b, fields, fields.reg("rt"), rs, imm, false);
}

/// Which way a doubleword shift goes, and what fills in from the top of a right shift.
#[derive(Clone, Copy)]
enum Shift {
    Left,
    Logical,
    Arithmetic,
}

/// `rt` shifted by `sa` plus `base` into `rd`.
fn by_constant(b: &mut dyn Builder, fields: &Fields<'_>, shift: Shift, base: i64) {
    let rt = b.r(fields.reg("rt"));
    let amount = b.imm(64, fields.value("sa") + base);
    shifted(b, fields, shift, rt, amount);
}

/// `rt` shifted by the low six bits of `rs` into `rd`.
fn by_register(b: &mut dyn Builder, fields: &Fields<'_>, shift: Shift) {
    let rt = b.r(fields.reg("rt"));
    let rs = b.r(fields.reg("rs"));
    let six = b.imm(64, 63);
    let amount = b.and(64, rs, six);
    shifted(b, fields, shift, rt, amount);
}

fn shifted(b: &mut dyn Builder, fields: &Fields<'_>, shift: Shift, value: Val, amount: Val) {
    let result = match shift {
        Shift::Left => b.shl(64, value, amount),
        Shift::Logical => b.lshr(64, value, amount),
        Shift::Arithmetic => b.ashr(64, value, amount),
    };
    b.set(fields.reg("rd"), result);
}

fn dsll(b: &mut dyn Builder, fields: &Fields<'_>) {
    by_constant(b, fields, Shift::Left, 0);
}

fn dsrl(b: &mut dyn Builder, fields: &Fields<'_>) {
    by_constant(b, fields, Shift::Logical, 0);
}

fn dsra(b: &mut dyn Builder, fields: &Fields<'_>) {
    by_constant(b, fields, Shift::Arithmetic, 0);
}

fn dsll32(b: &mut dyn Builder, fields: &Fields<'_>) {
    by_constant(b, fields, Shift::Left, 32);
}

fn dsrl32(b: &mut dyn Builder, fields: &Fields<'_>) {
    by_constant(b, fields, Shift::Logical, 32);
}

fn dsra32(b: &mut dyn Builder, fields: &Fields<'_>) {
    by_constant(b, fields, Shift::Arithmetic, 32);
}

fn dsllv(b: &mut dyn Builder, fields: &Fields<'_>) {
    by_register(b, fields, Shift::Left);
}

fn dsrlv(b: &mut dyn Builder, fields: &Fields<'_>) {
    by_register(b, fields, Shift::Logical);
}

fn dsrav(b: &mut dyn Builder, fields: &Fields<'_>) {
    by_register(b, fields, Shift::Arithmetic);
}

/// The address a load or a store names: `rs` plus the signed offset.
fn address(b: &mut dyn Builder, fields: &Fields<'_>) -> Val {
    let base = b.r(fields.reg("rs"));
    let offset = b.imm(64, fields.value("offset"));
    b.add(64, base, offset)
}

fn ld(b: &mut dyn Builder, fields: &Fields<'_>) {
    let at = address(b, fields);
    let value = b.load(64, at);
    b.set(fields.reg("rt"), value);
}

fn lwu(b: &mut dyn Builder, fields: &Fields<'_>) {
    let at = address(b, fields);
    let value = b.load(32, at);
    let value = b.zext(64, value);
    b.set(fields.reg("rt"), value);
}

fn sd(b: &mut dyn Builder, fields: &Fields<'_>) {
    let at = address(b, fields);
    let value = b.r(fields.reg("rt"));
    b.store(64, at, value);
}

/// The doubleword the address lies in and the address's offset in it as a shift in bits.
fn unaligned(b: &mut dyn Builder, at: Val) -> (Val, Val) {
    let seven = b.imm(64, 7);
    let offset = b.and(64, at, seven);
    let three = b.imm(64, 3);
    let bits = b.shl(64, offset, three);
    let down = b.imm(64, !7);
    (b.and(64, at, down), bits)
}

/// `(1 << amount) - 1`: the bits below a shift.
fn below(b: &mut dyn Builder, amount: Val) -> Val {
    let one = b.imm(64, 1);
    let bit = b.shl(64, one, amount);
    b.sub(64, bit, one)
}

/// `!(-1 >> amount)`: the bits a logical right shift clears.
fn above(b: &mut dyn Builder, amount: Val) -> Val {
    let ones = b.imm(64, -1);
    let kept = b.lshr(64, ones, amount);
    b.xor(64, kept, ones)
}

/// `56 - bits`: how far the merges of the left forms shift.
fn from_top(b: &mut dyn Builder, bits: Val) -> Val {
    let top = b.imm(64, 56);
    b.sub(64, top, bits)
}

/// `ldl` (`left`) and `ldr`: the doubleword's bytes from its start to the address become the
/// register's high bytes, or from the address to its end its low bytes; the rest of it stays.
fn load_merge(b: &mut dyn Builder, fields: &Fields<'_>, left: bool) {
    let at = address(b, fields);
    let (aligned, bits) = unaligned(b, at);
    let word = b.load(64, aligned);
    let old = b.r(fields.reg("rt"));
    let (part, keep) = if left {
        let s = from_top(b, bits);
        (b.shl(64, word, s), below(b, s))
    } else {
        (b.lshr(64, word, bits), above(b, bits))
    };
    let kept = b.and(64, old, keep);
    let merged = b.or(64, part, kept);
    b.set(fields.reg("rt"), merged);
}

/// `sdl` (`left`) and `sdr`: the register's high bytes go to the doubleword from its start to the
/// address, or its low bytes from the address to its end; the rest of it stays.
fn store_merge(b: &mut dyn Builder, fields: &Fields<'_>, left: bool) {
    let at = address(b, fields);
    let (aligned, bits) = unaligned(b, at);
    let word = b.load(64, aligned);
    let value = b.r(fields.reg("rt"));
    let (part, keep) = if left {
        let s = from_top(b, bits);
        (b.lshr(64, value, s), above(b, s))
    } else {
        (b.shl(64, value, bits), below(b, bits))
    };
    let kept = b.and(64, word, keep);
    let merged = b.or(64, part, kept);
    b.store(64, aligned, merged);
}

fn ldl(b: &mut dyn Builder, fields: &Fields<'_>) {
    load_merge(b, fields, true);
}

fn ldr(b: &mut dyn Builder, fields: &Fields<'_>) {
    load_merge(b, fields, false);
}

fn sdl(b: &mut dyn Builder, fields: &Fields<'_>) {
    store_merge(b, fields, true);
}

fn sdr(b: &mut dyn Builder, fields: &Fields<'_>) {
    store_merge(b, fields, false);
}

/// The semantics of the doubleword rows by name.
pub const ROWS: &[(&str, Lift)] = &[
    ("dsllv", dsllv),
    ("dsrlv", dsrlv),
    ("dsrav", dsrav),
    ("dadd", dadd),
    ("daddu", daddu),
    ("dsub", dsub),
    ("dsubu", dsubu),
    ("dsll", dsll),
    ("dsrl", dsrl),
    ("dsra", dsra),
    ("dsll32", dsll32),
    ("dsrl32", dsrl32),
    ("dsra32", dsra32),
    ("daddi", daddi),
    ("daddiu", daddiu),
    ("ldl", ldl),
    ("ldr", ldr),
    ("lwu", lwu),
    ("sdl", sdl),
    ("sdr", sdr),
    ("ld", ld),
    ("sd", sd),
];

static MACHINE: LazyLock<Machine> = LazyLock::new(|| {
    let core = Mips::machine().table.rows().len();
    let decoder = &Mips::r5900_machine().table.rows()[..core + ROWS.len()];
    let rows: Vec<Row> = decoder
        .iter()
        .enumerate()
        .map(|(at, row)| {
            let lift = ROWS.iter().find(|(name, _)| at >= core && *name == row.name).map(|&(_, lift)| Sem(lift));
            Row { sem: lift.or(row.sem), ..*row }
        })
        .collect();
    Machine {
        name: "mips64el",
        table: Table::new(&rows).expect("the MIPS III rows make a table"),
        registers: &FILE,
        address_bits: 64,
        syntax: Mips::machine().syntax,
    }
});

/// The machine of MIPS III over [`SPEC`]: the MIPS rows and the doubleword forms, these with the
/// semantics of [`ROWS`].
pub fn machine() -> &'static Machine {
    &MACHINE
}
