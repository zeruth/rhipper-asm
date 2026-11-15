//! The neutral record of a decoded word: the row's name as the mnemonic, its fields as operands by
//! their kinds, its flow with the target resolved, and the text the machine's objdump prints.

use ra_asm_arch::{Flow as ArchFlow, Instr, MemOperand, Operand, Prefixes, Register, Target};

use crate::table::{Flow, Kind, Table};

/// How a machine's objdump writes the operands that are not registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Syntax {
    /// What comes before a constant: `#` on ARM and SH, nothing on MIPS and the Power ISA.
    pub immediate_prefix: &'static str,
    /// Whether constants and displacements are written in hex, as `0x10`, rather than in decimal.
    pub hex: bool,
}

/// A machine described by rows: its table, the registers its register fields name, and how its
/// text is written.
#[derive(Debug, Clone)]
pub struct Machine {
    pub name: &'static str,
    pub table: Table,
    /// The register a register field names, indexed by the field's value.
    pub registers: &'static [Register],
    /// The width of an address, in bits: a relative target wraps at it as the program counter does.
    pub address_bits: u16,
    pub syntax: Syntax,
}

impl Machine {
    /// The instruction `word` at `address`; `None` when no row accepts the word or a register field
    /// names a register the machine does not have.
    ///
    /// The text is the mnemonic, a tab and the operands joined by `, `, as objdump prints it; a
    /// memory operand is `disp(base)`. The registers the instruction touches and whether it reads or
    /// writes memory are what its row's semantics does ([`Machine::access`]); `None` too when the
    /// semantics names a register the machine does not have.
    pub fn instr(&self, word: u32, address: u64) -> Option<Instr> {
        let decoded = self.table.decode(word)?;
        let row = decoded.row;
        let bases: Vec<&str> = row
            .fields
            .iter()
            .filter_map(|field| match field.kind {
                Kind::Mem { base, .. } => Some(base),
                _ => None,
            })
            .collect();
        let mut operands = Vec::with_capacity(row.fields.len());
        let mut target = None;
        for (field, &value) in row.fields.iter().zip(&decoded.values) {
            let operand = match field.kind {
                Kind::Reg if bases.contains(&field.name) => continue,
                Kind::Reg => Operand::Reg(self.register(value)?),
                Kind::Imm => Operand::Imm { value, bits: u16::from(field.width) },
                Kind::Rel { shift, from } => {
                    let to = self.resolve(address, shift, from, value);
                    target.get_or_insert(to);
                    Operand::Addr(to)
                }
                Kind::Mem { base, bits } => Operand::Mem(MemOperand {
                    segment: None,
                    base: Some(self.register(decoded.field(base)?)?),
                    index: None,
                    scale: 1,
                    disp: value,
                    bits,
                    absolute: None,
                }),
            };
            operands.push(operand);
        }
        let to = target.map_or(Target::Indirect, Target::Direct);
        let flow = match row.flow {
            Flow::Next => ArchFlow::Next,
            Flow::Jump => ArchFlow::Jump(to),
            Flow::Branch => ArchFlow::CondJump(to),
            Flow::Call => ArchFlow::Call(to),
            Flow::Return => ArchFlow::Return,
            Flow::Trap => ArchFlow::Trap,
        };
        let mnemonic = row.name.to_owned();
        let text = if operands.is_empty() {
            mnemonic.clone()
        } else {
            format!("{mnemonic}\t{}", operands.iter().map(|operand| self.shown(operand)).collect::<Vec<_>>().join(", "))
        };
        // The row's place in the table is the exact form the lift dispatches on.
        let code = u32::try_from(self.table.rows().iter().position(|other| std::ptr::eq(other, row))?).ok()?;
        let access = self.access(word, address)?;
        let registers =
            access.registers.iter().map(|&(n, touch)| Some((self.register(i64::from(n))?, touch))).collect::<Option<Vec<_>>>()?;
        Some(Instr {
            address,
            len: 4,
            mnemonic,
            text,
            operands,
            flow,
            prefixes: Prefixes::default(),
            registers,
            reads_memory: access.reads_memory,
            writes_memory: access.writes_memory,
            code,
        })
    }

    fn register(&self, value: i64) -> Option<Register> {
        usize::try_from(value).ok().and_then(|at| self.registers.get(at)).copied()
    }

    /// The code address a relative field of `value` names from the instruction at `address`.
    pub(crate) fn resolve(&self, address: u64, shift: u8, from: i8, value: i64) -> u64 {
        self.wrap(address.wrapping_add_signed(i64::from(from)).wrapping_add_signed(value.wrapping_shl(u32::from(shift))))
    }

    pub(crate) fn wrap(&self, address: u64) -> u64 {
        if self.address_bits >= 64 { address } else { address & ((1u64 << self.address_bits) - 1) }
    }

    fn shown(&self, operand: &Operand) -> String {
        match operand {
            Operand::Reg(register) => register.name.to_owned(),
            Operand::Imm { value, .. } => format!("{}{}", self.syntax.immediate_prefix, self.number(*value)),
            Operand::Addr(address) => format!("{address:#x}"),
            Operand::Mem(memory) => format!("{}({})", self.number(memory.disp), memory.base.map_or("", |base| base.name)),
        }
    }

    fn number(&self, value: i64) -> String {
        if !self.syntax.hex {
            value.to_string()
        } else if value < 0 {
            format!("-{:#x}", value.unsigned_abs())
        } else {
            format!("{value:#x}")
        }
    }
}
