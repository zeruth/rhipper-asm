//! What a row's instruction does, as a function over a [`Builder`]: registers read and written,
//! integer arithmetic and comparisons at a width, extensions, memory, and where control goes. Run
//! against the IR builder with a [`StateSpec`] it lifts the instruction; another builder derives
//! something else from the same function, so a machine's meaning is written once, in its rows.
//!
//! The widths are the row's to say: an operation at a width takes the low bits of a wider value,
//! and a narrower value is a fault of the row, since only the row knows whether it widens by sign
//! or by zero.

use std::fmt;

use ra_asm_arch::Register;
use ra_asm_ir::{
    BinOp, BlockId, CallConv, Callee, Cast, Declaration, FnType, Function, FunctionBuilder, ICond, Module, Operand, Terminator, Type,
    ValueId,
};

use crate::instr::Machine;
use crate::table::{Field, Kind, Row};

/// The name of the state's type in the module.
pub const STATE_TYPE: &str = "State";

/// The layout of `%State` for a machine of one register file: `{ [registers x iN], iP }`, the
/// registers of `bits` bits each, then the program counter of `pc_bits`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateSpec {
    pub registers: u32,
    pub bits: u16,
    pub pc_bits: u16,
    /// The register that reads as zero and drops what is written to it, when the machine has one.
    pub zero: Option<u32>,
}

impl StateSpec {
    /// The fields of `%State`.
    pub fn fields(&self) -> Vec<Type> {
        vec![Type::array(u64::from(self.registers), Type::Int(self.bits)), Type::Int(self.pc_bits)]
    }

    /// Where register `register` lies in the state, in bytes.
    pub fn register_offset(&self, register: u32) -> u64 {
        u64::from(register) * bytes(self.bits)
    }

    /// Where the program counter lies in the state, in bytes.
    pub fn pc_offset(&self) -> u64 {
        (u64::from(self.registers) * bytes(self.bits)).next_multiple_of(bytes(self.pc_bits))
    }

    /// The size of the state, in bytes.
    pub fn size(&self) -> u64 {
        (self.pc_offset() + bytes(self.pc_bits)).next_multiple_of(bytes(self.bits).max(bytes(self.pc_bits)))
    }

    /// Defines `%State` in `module` and declares the runtime functions a lifted row calls:
    /// `ra_trap` and `ra_unsupported`, which never return, and `ra_call`, which runs the code at an
    /// address over the state.
    pub fn declare(&self, module: &mut Module) {
        if !module.types.iter().any(|(name, _)| name == STATE_TYPE) {
            module.types.push((STATE_TYPE.to_owned(), self.fields()));
        }
        let runtime = [("ra_trap", Type::I64, true), ("ra_unsupported", Type::I64, true), ("ra_call", Type::Ptr, false)];
        for (name, first, noreturn) in runtime {
            module.declare(Declaration {
                name: name.into(),
                ty: FnType::new(Type::Void, vec![first, Type::I64]),
                conv: CallConv::C,
                attrs: if noreturn { vec!["noreturn".into()] } else { Vec::new() },
            });
        }
    }
}

/// The bytes a value of `bits` bits takes in memory.
fn bytes(bits: u16) -> u64 {
    u64::from(bits.div_ceil(8)).next_power_of_two()
}

/// A value of the instruction being built, as its builder numbers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Val(pub u32);

/// The integer operations; a shift's amount is below the width, as the row masks it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    And,
    Or,
    Xor,
    Shl,
    LShr,
    AShr,
}

/// The integer comparisons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cond {
    Eq,
    Ne,
    Ult,
    Ule,
    Ugt,
    Uge,
    Slt,
    Sle,
    Sgt,
    Sge,
}

/// The function a row carries.
pub type Lift = fn(&mut dyn Builder, &Fields<'_>);

/// A row's semantics: the function, compared by its address.
#[derive(Clone, Copy)]
pub struct Sem(pub Lift);

impl fmt::Debug for Sem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Sem")
    }
}

impl PartialEq for Sem {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::fn_addr_eq(self.0, other.0)
    }
}

impl Eq for Sem {}

/// The operations a row's semantics is written in. A value has the width it was made at; a
/// comparison is one bit; an address is unsigned and taken at the width it has.
pub trait Builder {
    /// The constant `value` at `bits`.
    fn imm(&mut self, bits: u16, value: i64) -> Val;
    /// The register `register`, at the width of the machine's registers.
    fn r(&mut self, register: u32) -> Val;
    /// Writes the low register-width bits of `value` to `register`.
    fn set(&mut self, register: u32, value: Val);
    fn op(&mut self, op: Op, bits: u16, a: Val, b: Val) -> Val;
    fn cmp(&mut self, cond: Cond, bits: u16, a: Val, b: Val) -> Val;
    fn sext(&mut self, bits: u16, value: Val) -> Val;
    fn zext(&mut self, bits: u16, value: Val) -> Val;
    fn trunc(&mut self, bits: u16, value: Val) -> Val;
    /// The `bits` bits of memory at `address`.
    fn load(&mut self, bits: u16, address: Val) -> Val;
    fn store(&mut self, bits: u16, address: Val, value: Val);
    /// Goes to `target` when `cond` holds, and on with the instruction when it does not.
    fn branch_if(&mut self, cond: Val, target: Val);
    /// Goes to `target`: the instruction says nothing after it.
    fn jump(&mut self, target: Val);
    /// Runs the code at `target` and goes on with the instruction when it returns.
    fn call(&mut self, target: Val);
    /// Stops the program with `code`: the instruction says nothing after it.
    fn trap(&mut self, code: u64);

    fn add(&mut self, bits: u16, a: Val, b: Val) -> Val {
        self.op(Op::Add, bits, a, b)
    }

    fn sub(&mut self, bits: u16, a: Val, b: Val) -> Val {
        self.op(Op::Sub, bits, a, b)
    }

    fn mul(&mut self, bits: u16, a: Val, b: Val) -> Val {
        self.op(Op::Mul, bits, a, b)
    }

    fn and(&mut self, bits: u16, a: Val, b: Val) -> Val {
        self.op(Op::And, bits, a, b)
    }

    fn or(&mut self, bits: u16, a: Val, b: Val) -> Val {
        self.op(Op::Or, bits, a, b)
    }

    fn xor(&mut self, bits: u16, a: Val, b: Val) -> Val {
        self.op(Op::Xor, bits, a, b)
    }

    fn shl(&mut self, bits: u16, a: Val, b: Val) -> Val {
        self.op(Op::Shl, bits, a, b)
    }

    fn lshr(&mut self, bits: u16, a: Val, b: Val) -> Val {
        self.op(Op::LShr, bits, a, b)
    }

    fn ashr(&mut self, bits: u16, a: Val, b: Val) -> Val {
        self.op(Op::AShr, bits, a, b)
    }

    fn eq(&mut self, bits: u16, a: Val, b: Val) -> Val {
        self.cmp(Cond::Eq, bits, a, b)
    }

    fn ne(&mut self, bits: u16, a: Val, b: Val) -> Val {
        self.cmp(Cond::Ne, bits, a, b)
    }

    fn ult(&mut self, bits: u16, a: Val, b: Val) -> Val {
        self.cmp(Cond::Ult, bits, a, b)
    }

    fn slt(&mut self, bits: u16, a: Val, b: Val) -> Val {
        self.cmp(Cond::Slt, bits, a, b)
    }

    /// The code address `address`, as a constant.
    fn addr(&mut self, address: u64) -> Val {
        self.imm(64, address as i64)
    }
}

/// The fields of the decoded word, by name, for the row's semantics; a name the row does not have
/// is a fault of the row.
pub struct Fields<'a> {
    machine: &'a Machine,
    row: &'a Row,
    values: &'a [i64],
    /// The address of the instruction.
    pub address: u64,
    /// The address of the instruction after it.
    pub next: u64,
}

impl Fields<'_> {
    fn at(&self, name: &str) -> (&Field, i64) {
        let at = self.row.fields.iter().position(|field| field.name == name);
        let at = at.unwrap_or_else(|| panic!("row `{}` has no field `{name}`", self.row.name));
        (&self.row.fields[at], self.values[at])
    }

    /// The value of a field: a constant or a displacement, sign-extended when the field is signed.
    pub fn value(&self, name: &str) -> i64 {
        self.at(name).1
    }

    /// The register a register field names.
    pub fn reg(&self, name: &str) -> u32 {
        self.at(name).1 as u32
    }

    /// The code address a relative field names.
    pub fn target(&self, name: &str) -> u64 {
        match self.at(name) {
            (Field { kind: Kind::Rel { shift, from }, .. }, value) => self.machine.resolve(self.address, *shift, *from, value),
            _ => panic!("field `{name}` of row `{}` is no relative target", self.row.name),
        }
    }
}

impl Machine {
    /// The instruction `word` at `address` lifted alone into `void @F_<address>(ptr %state)` over
    /// the state `spec` lays out: what its row's semantics says, then the program counter set to
    /// where control goes. A row without semantics lifts to `ra_unsupported`; `None` when no row
    /// accepts the word.
    pub fn lift(&self, spec: &StateSpec, word: u32, address: u64) -> Option<Function> {
        let decoded = self.table.decode(word)?;
        let mut b = FunctionBuilder::new(&format!("F_{address:08x}"), FnType::new(Type::Void, vec![Type::Ptr]), &["state"]);
        b.set_address(address);
        b.address = Some(address);
        let block = b.block("entry");
        let state = b.param(0);
        let mut ir = Ir { b, spec, registers: self.registers, state, block, address, values: Vec::new(), left: false };
        let next = self.wrap(address.wrapping_add(4));
        match decoded.row.sem {
            Some(Sem(lift)) => lift(&mut ir, &Fields { machine: self, row: decoded.row, values: &decoded.values, address, next }),
            None => ir.stop("ra_unsupported", u64::from(word)),
        }
        if !ir.left {
            let next = ir.addr(next);
            ir.jump(next);
        }
        Some(ir.b.finish())
    }
}

/// The IR builder of one instruction: the state's fields by the spec, memory at host addresses.
struct Ir<'a> {
    b: FunctionBuilder,
    spec: &'a StateSpec,
    registers: &'a [Register],
    state: ValueId,
    /// The block the instruction goes on in.
    block: BlockId,
    address: u64,
    /// Every value made, with its width.
    values: Vec<(Operand, u16)>,
    /// Whether control has left the instruction, so nothing more may be built.
    left: bool,
}

/// `value` sign-extended from its low `bits` bits: the one form a constant of that width has.
fn fit(value: i128, bits: u16) -> i128 {
    if bits >= 128 {
        return value;
    }
    let shift = 128 - u32::from(bits);
    (value << shift) >> shift
}

/// The ones of `bits` bits, below 128.
fn ones(bits: u16) -> i128 {
    ((1u128 << bits) - 1) as i128
}

impl Ir<'_> {
    fn val(&mut self, operand: Operand, bits: u16) -> Val {
        let at = Val(self.values.len() as u32);
        self.values.push((operand, bits));
        at
    }

    fn get(&self, value: Val) -> (Operand, u16) {
        self.values[value.0 as usize].clone()
    }

    fn open(&self) -> BlockId {
        assert!(!self.left, "the semantics of the instruction at {:#x} goes on after control left it", self.address);
        self.block
    }

    /// The low `bits` bits of `value`.
    fn at(&mut self, value: Val, bits: u16) -> Operand {
        let (operand, width) = self.get(value);
        assert!(width >= bits, "the semantics of the instruction at {:#x} takes a value of {width} bits at {bits}", self.address);
        if width == bits {
            return operand;
        }
        if let Operand::Int(constant) = operand {
            return Operand::Int(fit(constant, bits));
        }
        let block = self.open();
        Operand::Local(self.b.cast(block, Cast::Trunc, Type::Int(width), operand, Type::Int(bits), "lo"))
    }

    /// `value` as an unsigned number of `bits` bits: an address or a program counter.
    fn unsigned(&mut self, value: Val, bits: u16) -> Operand {
        let (operand, width) = self.get(value);
        if width >= bits {
            return self.at(value, bits);
        }
        if let Operand::Int(constant) = operand {
            return Operand::Int(constant & ones(width));
        }
        let block = self.open();
        Operand::Local(self.b.cast(block, Cast::ZExt, Type::Int(width), operand, Type::Int(bits), "wide"))
    }

    fn pointer(&mut self, address: Val) -> Operand {
        let address = self.unsigned(address, 64);
        let block = self.open();
        Operand::Local(self.b.cast(block, Cast::IntToPtr, Type::I64, address, Type::Ptr, "p"))
    }

    fn name(&self, register: u32) -> String {
        self.registers.get(register as usize).map_or_else(|| format!("r{register}"), |known| known.name.to_owned())
    }

    fn register_pointer(&mut self, register: u32) -> ValueId {
        assert!(
            register < self.spec.registers,
            "the instruction at {:#x} names register {register}, which the state has not",
            self.address
        );
        let hint = format!("p.{}", self.name(register));
        let block = self.open();
        let indices = vec![(Type::I32, Operand::Int(0)), (Type::I32, Operand::Int(0)), (Type::I32, Operand::Int(i128::from(register)))];
        self.b.gep(block, Type::Named(STATE_TYPE.to_owned()), self.state, indices, &hint)
    }

    /// Sets the program counter to `target` and returns: control has left the instruction.
    fn leave(&mut self, target: Val) {
        let pc = self.unsigned(target, self.spec.pc_bits);
        let block = self.open();
        let indices = vec![(Type::I32, Operand::Int(0)), (Type::I32, Operand::Int(1))];
        let pointer = self.b.gep(block, Type::Named(STATE_TYPE.to_owned()), self.state, indices, "p.pc");
        self.b.store(block, Type::Int(self.spec.pc_bits), pc, pointer);
        self.b.terminate(block, Terminator::Ret(None));
        self.left = true;
    }

    /// Calls the runtime's `name` with the instruction's address and `code`, which never returns.
    fn stop(&mut self, name: &str, code: u64) {
        let block = self.open();
        let args = vec![(Type::I64, Operand::from(self.address)), (Type::I64, Operand::from(code))];
        self.b.call(block, FnType::new(Type::Void, vec![Type::I64, Type::I64]), Callee::Direct(name.to_owned()), args, "");
        self.b.terminate(block, Terminator::Unreachable);
        self.left = true;
    }
}

impl Builder for Ir<'_> {
    fn imm(&mut self, bits: u16, value: i64) -> Val {
        self.val(Operand::Int(fit(i128::from(value), bits)), bits)
    }

    fn r(&mut self, register: u32) -> Val {
        let bits = self.spec.bits;
        if self.spec.zero == Some(register) {
            return self.val(Operand::Int(0), bits);
        }
        let pointer = self.register_pointer(register);
        let name = self.name(register);
        let block = self.open();
        let value = self.b.load(block, Type::Int(bits), pointer, &name);
        self.val(Operand::Local(value), bits)
    }

    fn set(&mut self, register: u32, value: Val) {
        let bits = self.spec.bits;
        let value = self.at(value, bits);
        if self.spec.zero == Some(register) {
            return;
        }
        let pointer = self.register_pointer(register);
        let block = self.open();
        self.b.store(block, Type::Int(bits), value, pointer);
    }

    fn op(&mut self, op: Op, bits: u16, a: Val, b: Val) -> Val {
        let (a, b) = (self.at(a, bits), self.at(b, bits));
        let op = match op {
            Op::Add => BinOp::Add,
            Op::Sub => BinOp::Sub,
            Op::Mul => BinOp::Mul,
            Op::And => BinOp::And,
            Op::Or => BinOp::Or,
            Op::Xor => BinOp::Xor,
            Op::Shl => BinOp::Shl,
            Op::LShr => BinOp::LShr,
            Op::AShr => BinOp::AShr,
        };
        let block = self.open();
        let value = self.b.bin(block, op, Type::Int(bits), a, b, op.name());
        self.val(Operand::Local(value), bits)
    }

    fn cmp(&mut self, cond: Cond, bits: u16, a: Val, b: Val) -> Val {
        let (a, b) = (self.at(a, bits), self.at(b, bits));
        let cond = match cond {
            Cond::Eq => ICond::Eq,
            Cond::Ne => ICond::Ne,
            Cond::Ult => ICond::Ult,
            Cond::Ule => ICond::Ule,
            Cond::Ugt => ICond::Ugt,
            Cond::Uge => ICond::Uge,
            Cond::Slt => ICond::Slt,
            Cond::Sle => ICond::Sle,
            Cond::Sgt => ICond::Sgt,
            Cond::Sge => ICond::Sge,
        };
        let block = self.open();
        let value = self.b.icmp(block, cond, Type::Int(bits), a, b, cond.name());
        self.val(Operand::Local(value), 1)
    }

    fn sext(&mut self, bits: u16, value: Val) -> Val {
        let (operand, width) = self.get(value);
        assert!(width <= bits, "the instruction at {:#x} sign-extends {width} bits to {bits}", self.address);
        if width == bits {
            return value;
        }
        if let Operand::Int(constant) = operand {
            return self.val(Operand::Int(constant), bits);
        }
        let block = self.open();
        let wide = self.b.cast(block, Cast::SExt, Type::Int(width), operand, Type::Int(bits), "sext");
        self.val(Operand::Local(wide), bits)
    }

    fn zext(&mut self, bits: u16, value: Val) -> Val {
        let width = self.get(value).1;
        assert!(width <= bits, "the instruction at {:#x} zero-extends {width} bits to {bits}", self.address);
        if width == bits {
            return value;
        }
        let wide = self.unsigned(value, bits);
        self.val(wide, bits)
    }

    fn trunc(&mut self, bits: u16, value: Val) -> Val {
        let low = self.at(value, bits);
        self.val(low, bits)
    }

    fn load(&mut self, bits: u16, address: Val) -> Val {
        let pointer = self.pointer(address);
        let block = self.open();
        let value = self.b.load(block, Type::Int(bits), pointer, "m");
        self.val(Operand::Local(value), bits)
    }

    fn store(&mut self, bits: u16, address: Val, value: Val) {
        let value = self.at(value, bits);
        let pointer = self.pointer(address);
        let block = self.open();
        self.b.store(block, Type::Int(bits), value, pointer);
    }

    fn branch_if(&mut self, cond: Val, target: Val) {
        let width = self.get(cond).1;
        assert_eq!(width, 1, "the instruction at {:#x} branches on a value of {width} bits", self.address);
        let cond = self.at(cond, 1);
        let block = self.open();
        let taken = self.b.block(&format!("taken_{:x}", self.address));
        let next = self.b.block(&format!("next_{:x}", self.address));
        self.b.terminate(block, Terminator::CondBr { cond, then: taken, els: next });
        self.block = taken;
        self.leave(target);
        self.block = next;
        self.left = false;
    }

    fn jump(&mut self, target: Val) {
        self.leave(target);
    }

    fn call(&mut self, target: Val) {
        let target = self.unsigned(target, 64);
        let block = self.open();
        let args = vec![(Type::Ptr, Operand::Local(self.state)), (Type::I64, target)];
        self.b.call(block, FnType::new(Type::Void, vec![Type::Ptr, Type::I64]), Callee::Direct("ra_call".into()), args, "");
    }

    fn trap(&mut self, code: u64) {
        self.stop("ra_trap", code);
    }
}
