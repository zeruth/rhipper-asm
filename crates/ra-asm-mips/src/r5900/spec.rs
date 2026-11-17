//! The state of the Emotion Engine's processor and its machine of rows over it: 32 general
//! registers of 128 bits, `hi` and `lo`, the second `hi1` and `lo1` the pipeline-1 multiplies
//! write, the shift amount `sa`, and the FPU's 32 single registers with its accumulator, each a
//! slot of 128 bits of one register file.
//!
//! The MIPS rows are written for MIPS III's registers of 64 bits. The R5900 runs them unchanged
//! through [`Low`], which gives them the low 64 bits of every register and keeps the high 64
//! when they write one, as the processor does: only the multimedia instructions, `lq` and `sq`
//! touch the high halves.

use std::sync::LazyLock;

use ra_asm_arch::{RegClass, Register};
use ra_asm_machine::{Builder, Cond, Fields, Lift, Machine, Op, Row, Sem, StateSpec, Table, Val};

use crate::Mips;

/// `hi` and `lo`, after the general registers as MIPS numbers them for the lift.
pub const HI: u32 = 32;
pub const LO: u32 = 33;
/// The upper halves of the R5900's 128-bit `hi` and `lo`, which `mult1`, `div1` and their kin
/// write.
pub const HI1: u32 = 34;
pub const LO1: u32 = 35;
/// The shift amount `qfsrv` funnels by, which `mtsab` and `mtsah` set.
pub const SA: u32 = 36;
/// `$f0`; `$fn` is `F0 + n`.
pub const F0: u32 = 37;
/// The FPU's accumulator, which `adda.s`, `mula.s` and the multiply-adds keep.
pub const ACC: u32 = F0 + 32;
/// The slots of the register file.
pub const REGISTERS: u32 = ACC + 1;

/// The width of a slot: the general registers' and the multimedia instructions'.
pub const BITS: u16 = 128;
/// The width the MIPS rows see of a slot.
pub const LOW_BITS: u16 = 64;

/// The R5900's state: [`REGISTERS`] slots of 128 bits, `$zero` reading as zero, and a 32-bit
/// program counter.
pub const SPEC: StateSpec = StateSpec { registers: REGISTERS, bits: BITS, pc_bits: 32, zero: Some(0) };

/// The slots by number: the general registers as llvm-objdump names them, then the rest.
const NAMES: [&str; REGISTERS as usize] = [
    "$zero", "$1", "$2", "$3", "$4", "$5", "$6", "$7", "$8", "$9", "$10", "$11", "$12", "$13", "$14", "$15", "$16", "$17", "$18", "$19",
    "$20", "$21", "$22", "$23", "$24", "$25", "$26", "$27", "$gp", "$sp", "$fp", "$ra", "hi", "lo", "hi1", "lo1", "sa", "$f0", "$f1",
    "$f2", "$f3", "$f4", "$f5", "$f6", "$f7", "$f8", "$f9", "$f10", "$f11", "$f12", "$f13", "$f14", "$f15", "$f16", "$f17", "$f18", "$f19",
    "$f20", "$f21", "$f22", "$f23", "$f24", "$f25", "$f26", "$f27", "$f28", "$f29", "$f30", "$f31", "acc",
];

const SP: u32 = 29;

const fn file() -> [Register; REGISTERS as usize] {
    let mut registers = [Register { id: 0, name: "", class: RegClass::General, full: 0, bits: 0, shift: 0 }; REGISTERS as usize];
    let mut n = 0;
    while n < REGISTERS {
        let (class, bits) = if n == SP {
            (RegClass::StackPointer, BITS)
        } else if n < HI {
            (RegClass::General, BITS)
        } else if n < SA {
            (RegClass::Other, 64)
        } else if n == SA {
            (RegClass::Other, 32)
        } else {
            (RegClass::Float, 32)
        };
        registers[n as usize] = Register { id: n as u16, name: NAMES[n as usize], class, full: n as u16, bits, shift: 0 };
        n += 1;
    }
    registers
}

static FILE: [Register; REGISTERS as usize] = file();

/// The view of the R5900's registers a MIPS row has: each register is its slot's low 64 bits,
/// and a write leaves the high 64 as they were.
pub struct Low<'b>(pub &'b mut dyn Builder);

impl Builder for Low<'_> {
    fn imm(&mut self, bits: u16, value: i64) -> Val {
        self.0.imm(bits, value)
    }

    fn r(&mut self, register: u32) -> Val {
        let slot = self.0.r(register);
        self.0.trunc(LOW_BITS, slot)
    }

    fn set(&mut self, register: u32, value: Val) {
        let low = self.0.trunc(LOW_BITS, value);
        let low = self.0.zext(BITS, low);
        let slot = self.0.r(register);
        let half = self.0.imm(BITS, i64::from(LOW_BITS));
        let high = self.0.lshr(BITS, slot, half);
        let high = self.0.shl(BITS, high, half);
        let both = self.0.or(BITS, high, low);
        self.0.set(register, both);
    }

    fn op(&mut self, op: Op, bits: u16, a: Val, b: Val) -> Val {
        self.0.op(op, bits, a, b)
    }

    fn cmp(&mut self, cond: Cond, bits: u16, a: Val, b: Val) -> Val {
        self.0.cmp(cond, bits, a, b)
    }

    fn sext(&mut self, bits: u16, value: Val) -> Val {
        self.0.sext(bits, value)
    }

    fn zext(&mut self, bits: u16, value: Val) -> Val {
        self.0.zext(bits, value)
    }

    fn trunc(&mut self, bits: u16, value: Val) -> Val {
        self.0.trunc(bits, value)
    }

    fn load(&mut self, bits: u16, address: Val) -> Val {
        self.0.load(bits, address)
    }

    fn store(&mut self, bits: u16, address: Val, value: Val) {
        self.0.store(bits, address, value);
    }

    fn branch_if(&mut self, cond: Val, target: Val) {
        self.0.branch_if(cond, target);
    }

    fn jump(&mut self, target: Val) {
        self.0.jump(target);
    }

    fn call(&mut self, target: Val) {
        self.0.call(target);
    }

    fn trap(&mut self, code: u64) {
        self.0.trap(code);
    }
}

/// A MIPS row's semantics run on the R5900 through [`Low`].
macro_rules! low {
    ($lift:path) => {{
        fn on_r5900(b: &mut dyn Builder, fields: &Fields<'_>) {
            $lift(&mut Low(b), fields);
        }
        on_r5900 as Lift
    }};
}

/// `addu`: the 32-bit sum, sign-extended to the register.
fn addu(b: &mut dyn Builder, fields: &Fields<'_>) {
    let (rs, rt) = (b.r(fields.reg("rs")), b.r(fields.reg("rt")));
    let sum = b.add(32, rs, rt);
    let sum = b.sext(LOW_BITS, sum);
    b.set(fields.reg("rd"), sum);
}

/// The semantics of the MIPS rows by name, written for registers of 64 bits, as the R5900 runs
/// them.
const CORE: &[(&str, Lift)] = &[("addu", low!(addu))];

static MACHINE: LazyLock<Machine> = LazyLock::new(|| {
    let rows = Mips::r5900_machine().table.rows();
    let core = Mips::machine().table.rows().len();
    let rows: Vec<Row> = rows
        .iter()
        .enumerate()
        .map(|(at, row)| {
            let lift = CORE.iter().find(|(name, _)| at < core && *name == row.name).map(|&(_, lift)| Sem(lift));
            Row { sem: lift.or(row.sem), ..*row }
        })
        .collect();
    Machine {
        name: "r5900",
        table: Table::new(&rows).expect("the R5900 rows make a table"),
        registers: &FILE,
        address_bits: 32,
        syntax: Mips::r5900_machine().syntax,
    }
});

/// The R5900's machine over [`SPEC`]: the rows of [`Mips::r5900_machine`] in their places, each
/// MIPS row with its semantics run through [`Low`].
pub fn machine() -> &'static Machine {
    &MACHINE
}
