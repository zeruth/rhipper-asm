//! The `AArch64` semantics: one yaxpeax instruction becomes IR over the [`Machine`] state.
//!
//! The state holds `x0..x30` and `sp` in the general slots, N, Z, C and V in the first four flag
//! bytes, and `v0..v31` in the vector slots. The integer subset is covered: the moves, the
//! arithmetic with and without flags, the logic, the shifts and bitfields, the compare-and-branch
//! forms, the loads and stores in every addressing mode with the pairs and the extends, multiply
//! and divide, the conditional selects, and `adrp` with the `add` or the load that completes its
//! address. Beside it, the scalar floating point (the moves and `movi`, the arithmetic and
//! `fma` forms, the conversions, `fcmp` into the flags and `fcsel`), the arithmetic also over the
//! float arrangements as IR vectors. An instruction outside it becomes a trap that names its address. Each analysis block
//! becomes an IR block; the terminator comes from the analysis, with the condition of a
//! conditional branch or the target of an indirect one computed by the instruction and consumed
//! when the block ends.
#![allow(clippy::many_single_char_names)]

use std::collections::HashMap;

use ra_asm_analysis::{Analysis, Block, Function, Terminator as T};
use ra_asm_arm64::Arm64;
use ra_asm_arm64::yaxpeax_arm::armv8::a64::{Instruction, Opcode, Operand as Op, SIMDSizeCode, ShiftStyle, SizeCode};
use ra_asm_ir::{BinOp, BlockId, Callee, Cast, FCond, FnType, FunctionBuilder, ICond, Inst, Linkage, Operand, Terminator, Type, ValueId};
use ra_asm_pe::Image;

use crate::state::{Isa, Machine};
use crate::x86::trap;
use crate::{Lifted, function_name};

/// The flags, by their byte index in the state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nzcv {
    N = 0,
    Z = 1,
    C = 2,
    V = 3,
}

impl Nzcv {
    fn name(self) -> &'static str {
        match self {
            Self::N => "n",
            Self::Z => "z",
            Self::C => "c",
            Self::V => "v",
        }
    }
}

/// The number of the stack pointer's slot: the one after `x30`.
const SP: u16 = 31;

fn bits_of(size: SizeCode) -> u16 {
    match size {
        SizeCode::X => 64,
        SizeCode::W => 32,
    }
}

fn simd_bits(size: SIMDSizeCode) -> u16 {
    match size {
        SIMDSizeCode::B => 8,
        SIMDSizeCode::H => 16,
        SIMDSizeCode::S => 32,
        SIMDSizeCode::D => 64,
        SIMDSizeCode::Q => 128,
    }
}

/// A general register operand: its number, its width and whether 31 means `sp` rather than `xzr`.
fn general(op: &Op) -> Option<(u16, u16, bool)> {
    match *op {
        Op::Register(size, n) => Some((n, bits_of(size), false)),
        Op::RegisterOrSP(size, n) => Some((n, bits_of(size), true)),
        _ => None,
    }
}

/// The value of general register `n` at `bits`; 31 is zero unless `or_sp`.
fn read_general(m: &mut Machine<'_>, block: BlockId, n: u16, bits: u16, or_sp: bool) -> ValueId {
    if n == SP && !or_sp {
        return m.value_of(block, Type::Int(bits), 0i64);
    }
    let p = m.gpr_ptr(block, u32::from(n));
    let whole = m.b.load(block, Type::I64, p, m.gpr_name(u32::from(n)));
    if bits == 64 { whole } else { m.b.cast(block, Cast::Trunc, Type::I64, whole, Type::Int(bits), &format!("w{n}")) }
}

/// Writes general register `n` at `bits`: a 32-bit write zero-extends, as the machine does; a
/// write to 31 goes nowhere unless `or_sp`.
fn write_general(m: &mut Machine<'_>, block: BlockId, n: u16, bits: u16, or_sp: bool, value: impl Into<Operand>) {
    if n == SP && !or_sp {
        return;
    }
    let p = m.gpr_ptr(block, u32::from(n));
    let wide = if bits == 64 {
        m.value_of(block, Type::I64, value)
    } else {
        m.b.cast(block, Cast::ZExt, Type::Int(bits), value, Type::I64, &format!("x{n}"))
    };
    m.b.store(block, Type::I64, wide, p);
}

/// Lifts one function.
pub fn lift_function(image: &Image, _analysis: &Analysis<'_>, lifted: &mut Lifted, f: &Function) -> ra_asm_ir::Function {
    let name = function_name(f.entry);
    let mut b = FunctionBuilder::new(&name, FnType::new(Type::Void, vec![Type::Ptr]), &["state"]);
    b.set_address(f.entry);
    if f.noreturn {
        b.add_attr("noreturn");
    }
    let state = b.param(0);
    let mut blocks = HashMap::new();
    blocks.insert(f.entry, b.block(&format!("b_{:x}", f.entry)));
    for blk in &f.blocks {
        if blk.start != f.entry {
            blocks.insert(blk.start, b.block(&format!("b_{:x}", blk.start)));
        }
    }
    let x87 = lifted.x87;
    let mut lifter = Lifter {
        m: Machine { b: &mut b, state, bits: 64, x87, isa: Isa::Arm64 },
        image,
        lifted,
        blocks,
        register_import_calls: &f.register_import_calls,
        cond: None,
        jump_target: None,
        pages: HashMap::new(),
    };
    for blk in &f.blocks {
        lifter.lift_block(blk);
    }
    let mut built = b.finish();
    // A function that never returns has no `ret` to come back through.
    if f.noreturn {
        for block in &mut built.blocks {
            if let Terminator::Ret(_) = block.terminator {
                block.terminator = Terminator::Unreachable;
            }
        }
    }
    built
}

struct Lifter<'a, 'b> {
    m: Machine<'b>,
    image: &'a Image,
    lifted: &'a mut Lifted,
    blocks: HashMap<u64, BlockId>,
    /// The calls through a register that holds an import's slot value, by address, from the analysis.
    register_import_calls: &'a HashMap<u64, String>,
    /// The condition of the conditional branch that ends the current block.
    cond: Option<ValueId>,
    /// The target of the indirect branch that ends the current block.
    jump_target: Option<ValueId>,
    /// The address a register holds since an `adrp` or `adr` of this block, by register number:
    /// the page an `add` or a load completes, which in standalone mode must name the global.
    pages: HashMap<u16, u64>,
}

impl Lifter<'_, '_> {
    fn warn(&mut self, text: String) {
        self.lifted.warnings.push(text);
    }

    // region Blocks

    fn lift_block(&mut self, blk: &Block) {
        let mut block = self.blocks[&blk.start];
        self.m.b.set_block_address(block, blk.start);
        self.cond = None;
        self.jump_target = None;
        self.pages.clear();
        for i in &blk.instrs {
            if self.m.b.has_terminator(block) {
                break;
            }
            self.m.b.address = Some(i.address);
            let Ok(bytes) = self.image.read(i.address, 4) else { break };
            let Some(d) = Arm64.decode_full(bytes, i.address) else { break };
            block = self.lift_instr(block, &d.inst, i.address, d.instr.code, &d.instr.text);
        }
        if self.m.b.has_terminator(block) {
            return;
        }
        let last = blk.instrs.last().map_or(blk.start, |i| i.address);
        self.m.b.address = Some(last);
        match &blk.terminator {
            T::FallThrough(t) | T::Jump(t) => self.branch(block, *t),
            T::CondJump { taken, fallthrough } => {
                let Some(cond) = self.cond else {
                    self.warn(format!("{last:#x}: a conditional branch with no condition"));
                    self.m.trap(block, last, trap::INVALID);
                    return;
                };
                let then = self.target_block(*taken);
                let els = self.target_block(*fallthrough);
                self.m.b.terminate(block, Terminator::CondBr { cond: cond.into(), then, els });
            }
            T::Switch { targets, .. } => {
                let default = self.m.b.block("switch.bad");
                self.m.trap(default, last, trap::BAD_SWITCH);
                let Some(target) = self.jump_target else {
                    self.warn(format!("{last:#x}: a switch with no computed target"));
                    self.m.trap(block, last, trap::INVALID);
                    return;
                };
                if self.lifted.standalone {
                    self.warn(format!("{last:#x}: a switch by its computed target reads the table of the image"));
                }
                let cases = targets.iter().filter_map(|t| self.blocks.get(t).map(|b| (i128::from(*t), *b))).collect();
                self.m.b.terminate(block, Terminator::Switch { ty: Type::I64, value: target.into(), default, cases });
            }
            T::IndirectJump => {
                let Some(target) = self.jump_target else {
                    self.m.trap(block, last, trap::INVALID);
                    return;
                };
                // A branch the analysis did not resolve: most often a tail call through a pointer.
                self.runtime_call(block, target);
                self.m.b.terminate(block, Terminator::Ret(None));
            }
            T::TailCall(t) => {
                self.call_lifted(block, *t);
                self.m.b.terminate(block, Terminator::Ret(None));
            }
            T::ImportTailCall(import) => {
                self.import_call(block, import);
                self.m.b.terminate(block, Terminator::Ret(None));
            }
            T::NoReturnCall(_) | T::NoReturnImportCall(_) => self.m.b.terminate(block, Terminator::Unreachable),
            T::Return => self.m.b.terminate(block, Terminator::Ret(None)),
            T::Trap => self.m.trap(block, last, trap::INT3),
            T::Halt => self.m.trap(block, last, trap::HLT),
            T::Invalid => self.m.trap(block, last, trap::INVALID),
        }
    }

    fn branch(&mut self, block: BlockId, target: u64) {
        let to = self.target_block(target);
        self.m.b.terminate(block, Terminator::Br(to));
    }

    /// The block for a target inside the function, or a new block that tail-calls it.
    fn target_block(&mut self, target: u64) -> BlockId {
        if let Some(&b) = self.blocks.get(&target) {
            return b;
        }
        let b = self.m.b.block(&format!("tail_{target:x}"));
        if self.lifted.functions.contains_key(&target) {
            self.call_lifted(b, target);
            self.m.b.terminate(b, Terminator::Ret(None));
        } else {
            self.m.trap(b, target, trap::INVALID);
        }
        b
    }

    // endregion

    // region Calls

    /// `call F_target(state)`.
    fn call_lifted(&mut self, block: BlockId, target: u64) {
        let name = function_name(target);
        self.m.b.call(block, FnType::new(Type::Void, vec![Type::Ptr]), Callee::Direct(name), vec![(Type::Ptr, self.m.state.into())], "");
    }

    /// `call @ra_call(state, target)`: the runtime finds the lifted function or calls native code.
    fn runtime_call(&mut self, block: BlockId, target: ValueId) {
        self.m.b.call(
            block,
            FnType::new(Type::Void, vec![Type::Ptr, Type::I64]),
            Callee::Direct("ra_call".into()),
            vec![(Type::Ptr, self.m.state.into()), (Type::I64, target.into())],
            "",
        );
    }

    /// Calls an import through its wrapper, `imp_<name>(ptr %state)`, which reads `x0..x7` and
    /// the stack, calls the import and stores the result to `x0`: see [`import_wrapper`].
    fn import_call(&mut self, block: BlockId, import: &str) {
        let symbol = crate::x86::import_symbol(import);
        self.lifted.imports.insert(symbol.clone());
        self.lifted.wrappers.insert((symbol.clone(), 0, false));
        let wrapper = crate::import_wrapper_name(&symbol, false);
        self.m.b.call(block, FnType::new(Type::Void, vec![Type::Ptr]), Callee::Direct(wrapper), vec![(Type::Ptr, self.m.state.into())], "");
    }

    // endregion

    // region Registers, flags and operands

    fn read(&mut self, block: BlockId, n: u16, bits: u16, or_sp: bool) -> ValueId {
        read_general(&mut self.m, block, n, bits, or_sp)
    }

    fn write(&mut self, block: BlockId, n: u16, bits: u16, or_sp: bool, value: impl Into<Operand>) {
        self.pages.remove(&n);
        write_general(&mut self.m, block, n, bits, or_sp, value);
    }

    /// The low `bits` of vector register `n`.
    fn read_vector(&mut self, block: BlockId, n: u16, bits: u16) -> ValueId {
        let p = self.m.xmm_ptr(block, u32::from(n));
        self.m.b.load(block, Type::Int(bits), p, &format!("v{n}"))
    }

    /// Writes the low `bits` of vector register `n` and clears the rest, as a scalar load does.
    fn write_vector(&mut self, block: BlockId, n: u16, bits: u16, value: impl Into<Operand>) {
        let p = self.m.xmm_ptr(block, u32::from(n));
        if bits != 128 {
            self.m.b.store(block, Type::I128, 0i64, p);
        }
        self.m.b.store(block, Type::Int(bits), value, p);
    }

    fn flag(&mut self, block: BlockId, flag: Nzcv) -> ValueId {
        let p = self.m.flag_ptr_at(block, flag as i64, flag.name());
        let byte = self.m.b.load(block, Type::I8, p, &format!("{}.b", flag.name()));
        self.m.b.cast(block, Cast::Trunc, Type::I8, byte, Type::I1, flag.name())
    }

    fn set_flag(&mut self, block: BlockId, flag: Nzcv, value: impl Into<Operand>) {
        let p = self.m.flag_ptr_at(block, flag as i64, flag.name());
        let byte = self.m.b.cast(block, Cast::ZExt, Type::I1, value, Type::I8, &format!("{}.b", flag.name()));
        self.m.b.store(block, Type::I8, byte, p);
    }

    /// N and Z from a result.
    fn set_nz(&mut self, block: BlockId, bits: u16, result: ValueId) {
        let n = self.m.b.icmp(block, ICond::Slt, Type::Int(bits), result, 0i64, "n");
        self.set_flag(block, Nzcv::N, n);
        let z = self.m.b.icmp(block, ICond::Eq, Type::Int(bits), result, 0i64, "z");
        self.set_flag(block, Nzcv::Z, z);
    }

    /// Brings a value of `from` bits to `to` bits: zero-extended, truncated, or as it is.
    fn resize(&mut self, block: BlockId, value: ValueId, from: u16, to: u16, hint: &str) -> ValueId {
        match from.cmp(&to) {
            std::cmp::Ordering::Less => self.m.b.cast(block, Cast::ZExt, Type::Int(from), value, Type::Int(to), hint),
            std::cmp::Ordering::Greater => self.m.b.cast(block, Cast::Trunc, Type::Int(from), value, Type::Int(to), hint),
            std::cmp::Ordering::Equal => value,
        }
    }

    /// The value of a source operand at `bits`: a register, an immediate, or a register shifted
    /// or extended as the operand says. `None` for a form the integer subset lacks.
    fn value(&mut self, block: BlockId, op: &Op, bits: u16) -> Option<ValueId> {
        let ty = Type::Int(bits);
        let v = match *op {
            Op::Register(size, n) => {
                let v = self.read(block, n, bits_of(size), false);
                self.resize(block, v, bits_of(size), bits, "op")
            }
            Op::RegisterOrSP(size, n) => {
                let v = self.read(block, n, bits_of(size), true);
                self.resize(block, v, bits_of(size), bits, "op")
            }
            Op::Immediate(v) => self.m.value_of(block, ty, i64::from(v)),
            Op::Imm64(v) => self.m.value_of(block, ty, v as i64),
            Op::Imm16(v) => self.m.value_of(block, ty, i64::from(v)),
            Op::ImmShift(v, shift) => self.m.value_of(block, ty, i64::from(v) << shift),
            Op::RegShift(style, amount, size, n) => {
                let own = bits_of(size);
                let v = self.read(block, n, own, false);
                let amount = i64::from(amount);
                let extended = match style {
                    ShiftStyle::LSL | ShiftStyle::UXTX => self.resize(block, v, own, bits, "op"),
                    ShiftStyle::LSR => {
                        let v = self.resize(block, v, own, bits, "op");
                        return Some(self.m.b.bin(block, BinOp::LShr, ty, v, amount, "lsr"));
                    }
                    ShiftStyle::ASR => {
                        let v = self.resize(block, v, own, bits, "op");
                        return Some(self.m.b.bin(block, BinOp::AShr, ty, v, amount, "asr"));
                    }
                    ShiftStyle::ROR => {
                        let v = self.resize(block, v, own, bits, "op");
                        return Some(self.m.intrinsic(
                            block,
                            &format!("llvm.fshr.i{bits}"),
                            ty.clone(),
                            vec![(ty.clone(), v.into()), (ty.clone(), v.into()), (ty, amount.into())],
                            "ror",
                        ));
                    }
                    ShiftStyle::UXTB
                    | ShiftStyle::UXTH
                    | ShiftStyle::UXTW
                    | ShiftStyle::SXTB
                    | ShiftStyle::SXTH
                    | ShiftStyle::SXTW
                    | ShiftStyle::SXTX => {
                        let width = match style {
                            ShiftStyle::UXTB | ShiftStyle::SXTB => 8,
                            ShiftStyle::UXTH | ShiftStyle::SXTH => 16,
                            ShiftStyle::UXTW | ShiftStyle::SXTW => 32,
                            _ => 64,
                        };
                        let signed = matches!(style, ShiftStyle::SXTB | ShiftStyle::SXTH | ShiftStyle::SXTW | ShiftStyle::SXTX);
                        let narrow = self.resize(block, v, own, width.min(own), "ext.n");
                        let from = Type::Int(width.min(own));
                        if width.min(own) == bits {
                            narrow
                        } else if signed {
                            self.m.b.cast(block, Cast::SExt, from, narrow, ty.clone(), "sext")
                        } else {
                            self.m.b.cast(block, Cast::ZExt, from, narrow, ty.clone(), "zext")
                        }
                    }
                };
                if amount > 0 { self.m.b.bin(block, BinOp::Shl, ty, extended, amount, "shifted") } else { extended }
            }
            _ => return None,
        };
        Some(v)
    }

    /// The truth of condition `cc`, from the flags.
    fn condition(&mut self, block: BlockId, cc: u8) -> ValueId {
        let base = match cc >> 1 {
            0 => self.flag(block, Nzcv::Z),
            1 => self.flag(block, Nzcv::C),
            2 => self.flag(block, Nzcv::N),
            3 => self.flag(block, Nzcv::V),
            4 => {
                let c = self.flag(block, Nzcv::C);
                let z = self.flag(block, Nzcv::Z);
                let nz = self.m.b.bin(block, BinOp::Xor, Type::I1, z, 1i64, "nz");
                self.m.b.bin(block, BinOp::And, Type::I1, c, nz, "hi")
            }
            5 => {
                let n = self.flag(block, Nzcv::N);
                let v = self.flag(block, Nzcv::V);
                self.m.b.icmp(block, ICond::Eq, Type::I1, n, v, "ge")
            }
            6 => {
                let n = self.flag(block, Nzcv::N);
                let v = self.flag(block, Nzcv::V);
                let ge = self.m.b.icmp(block, ICond::Eq, Type::I1, n, v, "ge");
                let z = self.flag(block, Nzcv::Z);
                let nz = self.m.b.bin(block, BinOp::Xor, Type::I1, z, 1i64, "nz");
                self.m.b.bin(block, BinOp::And, Type::I1, ge, nz, "gt")
            }
            _ => return self.m.value_of(block, Type::I1, 1i64),
        };
        if cc & 1 == 1 { self.m.b.bin(block, BinOp::Xor, Type::I1, base, 1i64, "not") } else { base }
    }

    /// `a + b + carry` at `bits`, with the carry out and the signed overflow: the one adder of
    /// `add`, `adc`, `sub` (with `b` inverted and a carry of one) and `sbc`.
    fn add_with_carry(
        &mut self,
        block: BlockId,
        bits: u16,
        a: ValueId,
        b: ValueId,
        carry: impl Into<Operand>,
    ) -> (ValueId, ValueId, ValueId) {
        let ty = Type::Int(bits);
        let wide = Type::Int(bits + 1);
        let wa = self.m.b.cast(block, Cast::ZExt, ty.clone(), a, wide.clone(), "a.w");
        let wb = self.m.b.cast(block, Cast::ZExt, ty.clone(), b, wide.clone(), "b.w");
        let wc = self.m.b.cast(block, Cast::ZExt, Type::I1, carry, wide.clone(), "c.w");
        let ab = self.m.b.bin(block, BinOp::Add, wide.clone(), wa, wb, "ab");
        let sum = self.m.b.bin(block, BinOp::Add, wide.clone(), ab, wc, "sum");
        let result = self.m.b.cast(block, Cast::Trunc, wide.clone(), sum, ty.clone(), "res");
        let high = self.m.b.bin(block, BinOp::LShr, wide.clone(), sum, i64::from(bits), "sum.hi");
        let c = self.m.b.cast(block, Cast::Trunc, wide, high, Type::I1, "c");
        let x = self.m.b.bin(block, BinOp::Xor, ty.clone(), a, result, "v.x");
        let y = self.m.b.bin(block, BinOp::Xor, ty.clone(), b, result, "v.y");
        let both = self.m.b.bin(block, BinOp::And, ty.clone(), x, y, "v.and");
        let v = self.m.b.icmp(block, ICond::Slt, ty, both, 0i64, "v");
        (result, c, v)
    }

    fn set_nzcv(&mut self, block: BlockId, bits: u16, result: ValueId, c: ValueId, v: ValueId) {
        self.set_nz(block, bits, result);
        self.set_flag(block, Nzcv::C, c);
        self.set_flag(block, Nzcv::V, v);
    }

    /// A constant address as an operand: in standalone mode, the address of the byte of the data
    /// global or the function it falls in; otherwise, the number.
    fn address_operand(&self, address: u64) -> Operand {
        if self.lifted.standalone
            && let Some(op) = self.lifted.address_of(address)
        {
            return op;
        }
        Operand::Int(i128::from(address))
    }

    /// The address a memory operand names and the base register to write back after the access,
    /// with its new value. `None` for a form the integer subset lacks.
    fn address(&mut self, block: BlockId, op: &Op, ip: u64) -> Option<(Operand, Option<(u16, ValueId)>)> {
        match *op {
            Op::RegPreIndex(base, offset, writeback) => {
                if !writeback && let Some(&page) = self.pages.get(&base) {
                    return Some((self.address_operand(page.wrapping_add(offset as i64 as u64)), None));
                }
                let b = self.read(block, base, 64, true);
                let addr = if offset == 0 { b } else { self.m.b.bin(block, BinOp::Add, Type::I64, b, i64::from(offset), "addr") };
                Some((addr.into(), writeback.then_some((base, addr))))
            }
            Op::RegPostIndex(base, offset) => {
                let b = self.read(block, base, 64, true);
                let after = self.m.b.bin(block, BinOp::Add, Type::I64, b, i64::from(offset), "addr.post");
                Some((b.into(), Some((base, after))))
            }
            Op::RegRegOffset(base, index, size, style, amount) => {
                let b = self.read(block, base, 64, true);
                let offset = self.value(block, &Op::RegShift(style, amount, size, index), 64)?;
                let addr = self.m.b.bin(block, BinOp::Add, Type::I64, b, offset, "addr");
                Some((addr.into(), None))
            }
            Op::PCOffset(offset) => Some((self.address_operand(ip.wrapping_add(offset as u64)), None)),
            _ => None,
        }
    }

    // endregion

    // region Instructions

    fn unsupported(&mut self, block: BlockId, ip: u64, code: u32, text: &str) -> BlockId {
        self.warn(format!("{ip:#x}: unsupported instruction `{text}`"));
        self.m.unsupported(block, ip, u64::from(code));
        block
    }

    fn lift_instr(&mut self, block: BlockId, i: &Instruction, ip: u64, code: u32, text: &str) -> BlockId {
        use Opcode as O;
        let ops = &i.operands;
        match i.opcode {
            // A direct branch and a return are the block's terminator, which the analysis gives.
            O::B
            | O::RET
            | O::HINT
            | O::DMB(_)
            | O::DSB(_)
            | O::ISB
            | O::SB
            | O::SSBB
            | O::PSSBB
            | O::CLREX
            | O::PRFM
            | O::PRFUM
            | O::PACIASP
            | O::PACIBSP
            | O::AUTIASP
            | O::AUTIBSP
            | O::PACIAZ
            | O::PACIBZ
            | O::AUTIAZ
            | O::AUTIBZ
            | O::XPACLRI => {}
            O::BRK => self.m.trap(block, ip, trap::INT3),
            O::UDF | O::Invalid => self.m.trap(block, ip, trap::UD2),
            O::HLT => self.m.trap(block, ip, trap::HLT),

            O::MOVZ | O::MOVN | O::MOVK => {
                let Some((rd, bits, _)) = general(&ops[0]) else { return self.unsupported(block, ip, code, text) };
                let Op::ImmShift(imm, shift) = ops[1] else { return self.unsupported(block, ip, code, text) };
                let placed = i64::from(imm) << shift;
                match i.opcode {
                    O::MOVZ => self.write(block, rd, bits, false, placed),
                    O::MOVN => self.write(block, rd, bits, false, !placed),
                    _ => {
                        let old = self.read(block, rd, bits, false);
                        let kept = self.m.b.bin(block, BinOp::And, Type::Int(bits), old, !(0xffffi64 << shift), "kept");
                        let merged = self.m.b.bin(block, BinOp::Or, Type::Int(bits), kept, placed, "movk");
                        self.write(block, rd, bits, false, merged);
                    }
                }
            }

            O::ADD | O::ADDS | O::SUB | O::SUBS | O::ADC | O::ADCS | O::SBC | O::SBCS => {
                let Some((rd, bits, rd_sp)) = general(&ops[0]) else { return self.unsupported(block, ip, code, text) };
                // The page an `adrp` formed, completed by its low bits: one address, which the
                // standalone output names by its global.
                if i.opcode == O::ADD
                    && let (Op::RegisterOrSP(_, rn), Op::Immediate(low)) = (ops[1], ops[2])
                    && let Some(&page) = self.pages.get(&rn)
                {
                    let whole = page.wrapping_add(u64::from(low));
                    let value = self.address_operand(whole);
                    self.write(block, rd, bits, rd_sp, value);
                    self.pages.insert(rd, whole);
                    return block;
                }
                let (Some(a), Some(b)) = (self.value(block, &ops[1], bits), self.value(block, &ops[2], bits)) else {
                    return self.unsupported(block, ip, code, text);
                };
                let subtract = matches!(i.opcode, O::SUB | O::SUBS | O::SBC | O::SBCS);
                let sets_flags = matches!(i.opcode, O::ADDS | O::SUBS | O::ADCS | O::SBCS);
                let with_carry = matches!(i.opcode, O::ADC | O::ADCS | O::SBC | O::SBCS);
                if !sets_flags && !with_carry {
                    let result = self.m.b.bin(block, if subtract { BinOp::Sub } else { BinOp::Add }, Type::Int(bits), a, b, "res");
                    self.write(block, rd, bits, rd_sp, result);
                    return block;
                }
                // A subtraction is `a + !b + 1`, the borrow being the inverted carry.
                let b = if subtract { self.m.b.bin(block, BinOp::Xor, Type::Int(bits), b, -1i64, "b.not") } else { b };
                let carry: Operand = if with_carry { self.flag(block, Nzcv::C).into() } else { i64::from(subtract).into() };
                let (result, c, v) = self.add_with_carry(block, bits, a, b, carry);
                if sets_flags {
                    self.set_nzcv(block, bits, result, c, v);
                }
                self.write(block, rd, bits, rd_sp, result);
            }

            O::AND | O::ANDS | O::ORR | O::ORN | O::EOR | O::EON | O::BIC | O::BICS => {
                let Some((rd, bits, rd_sp)) = general(&ops[0]) else { return self.unsupported(block, ip, code, text) };
                let (Some(a), Some(b)) = (self.value(block, &ops[1], bits), self.value(block, &ops[2], bits)) else {
                    return self.unsupported(block, ip, code, text);
                };
                let ty = Type::Int(bits);
                let b = if matches!(i.opcode, O::ORN | O::EON | O::BIC | O::BICS) {
                    self.m.b.bin(block, BinOp::Xor, ty.clone(), b, -1i64, "b.not")
                } else {
                    b
                };
                let op = match i.opcode {
                    O::AND | O::ANDS | O::BIC | O::BICS => BinOp::And,
                    O::ORR | O::ORN => BinOp::Or,
                    _ => BinOp::Xor,
                };
                let result = self.m.b.bin(block, op, ty, a, b, "res");
                if matches!(i.opcode, O::ANDS | O::BICS) {
                    self.set_nz(block, bits, result);
                    self.set_flag(block, Nzcv::C, 0i64);
                    self.set_flag(block, Nzcv::V, 0i64);
                }
                self.write(block, rd, bits, rd_sp, result);
            }

            O::SBFM | O::UBFM | O::BFM => {
                let Some((rd, bits, _)) = general(&ops[0]) else { return self.unsupported(block, ip, code, text) };
                let (Some(rn), Op::Immediate(immr), Op::Immediate(imms)) = (self.value(block, &ops[1], bits), ops[2], ops[3]) else {
                    return self.unsupported(block, ip, code, text);
                };
                let ty = Type::Int(bits);
                let size = i64::from(bits);
                let (immr, imms) = (i64::from(immr), i64::from(imms));
                let mask = |width: i64| if width >= size { -1i64 } else { (1i64 << width) - 1 };
                let result = match i.opcode {
                    O::UBFM if imms >= immr => {
                        // ubfx: the field at immr, imms - immr + 1 wide, at the bottom.
                        let shifted = self.m.b.bin(block, BinOp::LShr, ty.clone(), rn, immr, "ubfx.s");
                        self.m.b.bin(block, BinOp::And, ty, shifted, mask(imms - immr + 1), "ubfx")
                    }
                    O::UBFM => {
                        // ubfiz / lsl: the low imms + 1 bits, placed at size - immr.
                        let low = self.m.b.bin(block, BinOp::And, ty.clone(), rn, mask(imms + 1), "ubfiz.m");
                        self.m.b.bin(block, BinOp::Shl, ty, low, size - immr, "ubfiz")
                    }
                    O::SBFM if imms >= immr => {
                        // sbfx / asr / sxt*: the field's top bit to the top, then back down signed.
                        let up = self.m.b.bin(block, BinOp::Shl, ty.clone(), rn, size - 1 - imms, "sbfx.up");
                        self.m.b.bin(block, BinOp::AShr, ty, up, size - 1 - imms + immr, "sbfx")
                    }
                    O::SBFM => {
                        let up = self.m.b.bin(block, BinOp::Shl, ty.clone(), rn, size - 1 - imms, "sbfiz.up");
                        let signed = self.m.b.bin(block, BinOp::AShr, ty.clone(), up, size - 1 - imms, "sbfiz.s");
                        self.m.b.bin(block, BinOp::Shl, ty, signed, size - immr, "sbfiz")
                    }
                    _ if imms >= immr => {
                        // bfxil: the field at immr goes to the bottom of rd.
                        let width = imms - immr + 1;
                        let old = self.read(block, rd, bits, false);
                        let kept = self.m.b.bin(block, BinOp::And, ty.clone(), old, !mask(width), "bfxil.kept");
                        let shifted = self.m.b.bin(block, BinOp::LShr, ty.clone(), rn, immr, "bfxil.s");
                        let field = self.m.b.bin(block, BinOp::And, ty.clone(), shifted, mask(width), "bfxil.f");
                        self.m.b.bin(block, BinOp::Or, ty, kept, field, "bfxil")
                    }
                    _ => {
                        // bfi: the low imms + 1 bits of rn go to size - immr of rd.
                        let width = imms + 1;
                        let lsb = size - immr;
                        let old = self.read(block, rd, bits, false);
                        let kept = self.m.b.bin(block, BinOp::And, ty.clone(), old, !(mask(width) << lsb), "bfi.kept");
                        let low = self.m.b.bin(block, BinOp::And, ty.clone(), rn, mask(width), "bfi.m");
                        let placed = self.m.b.bin(block, BinOp::Shl, ty.clone(), low, lsb, "bfi.p");
                        self.m.b.bin(block, BinOp::Or, ty, kept, placed, "bfi")
                    }
                };
                self.write(block, rd, bits, false, result);
            }

            O::EXTR => {
                let Some((rd, bits, _)) = general(&ops[0]) else { return self.unsupported(block, ip, code, text) };
                let (Some(rn), Some(rm), Op::Immediate(lsb)) = (self.value(block, &ops[1], bits), self.value(block, &ops[2], bits), ops[3])
                else {
                    return self.unsupported(block, ip, code, text);
                };
                let ty = Type::Int(bits);
                let result = self.m.intrinsic(
                    block,
                    &format!("llvm.fshr.i{bits}"),
                    ty.clone(),
                    vec![(ty.clone(), rn.into()), (ty.clone(), rm.into()), (ty, i64::from(lsb).into())],
                    "extr",
                );
                self.write(block, rd, bits, false, result);
            }

            O::LSLV | O::LSRV | O::ASRV | O::RORV => {
                let Some((rd, bits, _)) = general(&ops[0]) else { return self.unsupported(block, ip, code, text) };
                let (Some(rn), Some(rm)) = (self.value(block, &ops[1], bits), self.value(block, &ops[2], bits)) else {
                    return self.unsupported(block, ip, code, text);
                };
                let ty = Type::Int(bits);
                let amount = self.m.b.bin(block, BinOp::And, ty.clone(), rm, i64::from(bits) - 1, "amount");
                let result = match i.opcode {
                    O::LSLV => self.m.b.bin(block, BinOp::Shl, ty, rn, amount, "lsl"),
                    O::LSRV => self.m.b.bin(block, BinOp::LShr, ty, rn, amount, "lsr"),
                    O::ASRV => self.m.b.bin(block, BinOp::AShr, ty, rn, amount, "asr"),
                    _ => self.m.intrinsic(
                        block,
                        &format!("llvm.fshr.i{bits}"),
                        ty.clone(),
                        vec![(ty.clone(), rn.into()), (ty.clone(), rn.into()), (ty, amount.into())],
                        "ror",
                    ),
                };
                self.write(block, rd, bits, false, result);
            }

            O::CSEL | O::CSINC | O::CSINV | O::CSNEG => {
                let Some((rd, bits, _)) = general(&ops[0]) else { return self.unsupported(block, ip, code, text) };
                let (Some(rn), Some(rm), Op::ConditionCode(cc)) =
                    (self.value(block, &ops[1], bits), self.value(block, &ops[2], bits), ops[3])
                else {
                    return self.unsupported(block, ip, code, text);
                };
                let ty = Type::Int(bits);
                let other = match i.opcode {
                    O::CSEL => rm,
                    O::CSINC => self.m.b.bin(block, BinOp::Add, ty.clone(), rm, 1i64, "inc"),
                    O::CSINV => self.m.b.bin(block, BinOp::Xor, ty.clone(), rm, -1i64, "inv"),
                    _ => self.m.b.bin(block, BinOp::Sub, ty.clone(), 0i64, rm, "neg"),
                };
                let cond = self.condition(block, cc);
                let result = self.m.select(block, cond, ty, rn, other, "csel");
                self.write(block, rd, bits, false, result);
            }

            O::CCMP | O::CCMN => {
                let Some((_, bits, _)) = general(&ops[0]) else { return self.unsupported(block, ip, code, text) };
                let (Some(a), Some(b), Op::Immediate(nzcv), Op::ConditionCode(cc)) =
                    (self.value(block, &ops[0], bits), self.value(block, &ops[1], bits), ops[2], ops[3])
                else {
                    return self.unsupported(block, ip, code, text);
                };
                let cond = self.condition(block, cc);
                let subtract = i.opcode == O::CCMP;
                let b = if subtract { self.m.b.bin(block, BinOp::Xor, Type::Int(bits), b, -1i64, "b.not") } else { b };
                let (result, c, v) = self.add_with_carry(block, bits, a, b, i64::from(subtract));
                let n = self.m.b.icmp(block, ICond::Slt, Type::Int(bits), result, 0i64, "n");
                let z = self.m.b.icmp(block, ICond::Eq, Type::Int(bits), result, 0i64, "z");
                for (flag, computed, bit) in [(Nzcv::N, n, 3), (Nzcv::Z, z, 2), (Nzcv::C, c, 1), (Nzcv::V, v, 0)] {
                    let chosen = self.m.select(block, cond, Type::I1, computed, i64::from((nzcv >> bit) & 1), flag.name());
                    self.set_flag(block, flag, chosen);
                }
            }

            O::MADD | O::MSUB => {
                let Some((rd, bits, _)) = general(&ops[0]) else { return self.unsupported(block, ip, code, text) };
                let (Some(rn), Some(rm), Some(ra)) =
                    (self.value(block, &ops[1], bits), self.value(block, &ops[2], bits), self.value(block, &ops[3], bits))
                else {
                    return self.unsupported(block, ip, code, text);
                };
                let ty = Type::Int(bits);
                let product = self.m.b.bin(block, BinOp::Mul, ty.clone(), rn, rm, "mul");
                let result = self.m.b.bin(block, if i.opcode == O::MADD { BinOp::Add } else { BinOp::Sub }, ty, ra, product, "res");
                self.write(block, rd, bits, false, result);
            }

            O::SMADDL | O::SMSUBL | O::UMADDL | O::UMSUBL => {
                let (Some((rd, _, _)), Some((n, _, _)), Some((m, _, _))) = (general(&ops[0]), general(&ops[1]), general(&ops[2])) else {
                    return self.unsupported(block, ip, code, text);
                };
                let Some(ra) = self.value(block, &ops[3], 64) else { return self.unsupported(block, ip, code, text) };
                let signed = matches!(i.opcode, O::SMADDL | O::SMSUBL);
                let cast = if signed { Cast::SExt } else { Cast::ZExt };
                let rn = self.read(block, n, 32, false);
                let rm = self.read(block, m, 32, false);
                let wn = self.m.b.cast(block, cast, Type::I32, rn, Type::I64, "n.w");
                let wm = self.m.b.cast(block, cast, Type::I32, rm, Type::I64, "m.w");
                let product = self.m.b.bin(block, BinOp::Mul, Type::I64, wn, wm, "mull");
                let add = matches!(i.opcode, O::SMADDL | O::UMADDL);
                let result = self.m.b.bin(block, if add { BinOp::Add } else { BinOp::Sub }, Type::I64, ra, product, "res");
                self.write(block, rd, 64, false, result);
            }

            O::SMULH | O::UMULH => {
                let Some((rd, _, _)) = general(&ops[0]) else { return self.unsupported(block, ip, code, text) };
                let (Some(rn), Some(rm)) = (self.value(block, &ops[1], 64), self.value(block, &ops[2], 64)) else {
                    return self.unsupported(block, ip, code, text);
                };
                let cast = if i.opcode == O::SMULH { Cast::SExt } else { Cast::ZExt };
                let wn = self.m.b.cast(block, cast, Type::I64, rn, Type::I128, "n.w");
                let wm = self.m.b.cast(block, cast, Type::I64, rm, Type::I128, "m.w");
                let product = self.m.b.bin(block, BinOp::Mul, Type::I128, wn, wm, "mulh.p");
                let high = self.m.b.bin(block, BinOp::LShr, Type::I128, product, 64i64, "mulh.h");
                let result = self.m.b.cast(block, Cast::Trunc, Type::I128, high, Type::I64, "mulh");
                self.write(block, rd, 64, false, result);
            }

            O::UDIV | O::SDIV => {
                let Some((rd, bits, _)) = general(&ops[0]) else { return self.unsupported(block, ip, code, text) };
                let (Some(rn), Some(rm)) = (self.value(block, &ops[1], bits), self.value(block, &ops[2], bits)) else {
                    return self.unsupported(block, ip, code, text);
                };
                let ty = Type::Int(bits);
                // A division by zero gives zero and the most negative value divided by minus one
                // gives itself: both are undefined in the IR, so the divisor is made safe first.
                let zero = self.m.b.icmp(block, ICond::Eq, ty.clone(), rm, 0i64, "div.zero");
                let mut unsafe_divisor = zero;
                if i.opcode == O::SDIV {
                    let min = self.m.b.icmp(block, ICond::Eq, ty.clone(), rn, i64::MIN >> (64 - bits), "div.min");
                    let minus_one = self.m.b.icmp(block, ICond::Eq, ty.clone(), rm, -1i64, "div.m1");
                    let overflow = self.m.b.bin(block, BinOp::And, Type::I1, min, minus_one, "div.ovf");
                    unsafe_divisor = self.m.b.bin(block, BinOp::Or, Type::I1, zero, overflow, "div.unsafe");
                }
                let divisor = self.m.select(block, unsafe_divisor, ty.clone(), 1i64, rm, "divisor");
                let quotient =
                    self.m.b.bin(block, if i.opcode == O::UDIV { BinOp::UDiv } else { BinOp::SDiv }, ty.clone(), rn, divisor, "quot");
                let result = self.m.select(block, zero, ty, 0i64, quotient, "div");
                self.write(block, rd, bits, false, result);
            }

            O::CLZ | O::CLS | O::RBIT | O::REV | O::REV16 | O::REV32 => {
                let Some((rd, bits, _)) = general(&ops[0]) else { return self.unsupported(block, ip, code, text) };
                let Some(rn) = self.value(block, &ops[1], bits) else { return self.unsupported(block, ip, code, text) };
                let ty = Type::Int(bits);
                let result = match i.opcode {
                    O::CLZ => self.m.intrinsic(
                        block,
                        &format!("llvm.ctlz.i{bits}"),
                        ty.clone(),
                        vec![(ty, rn.into()), (Type::I1, 0i64.into())],
                        "clz",
                    ),
                    O::CLS => {
                        let shifted = self.m.b.bin(block, BinOp::AShr, ty.clone(), rn, 1i64, "cls.s");
                        let changed = self.m.b.bin(block, BinOp::Xor, ty.clone(), rn, shifted, "cls.x");
                        let leading = self.m.intrinsic(
                            block,
                            &format!("llvm.ctlz.i{bits}"),
                            ty.clone(),
                            vec![(ty.clone(), changed.into()), (Type::I1, 0i64.into())],
                            "cls.z",
                        );
                        self.m.b.bin(block, BinOp::Sub, ty, leading, 1i64, "cls")
                    }
                    O::RBIT => self.m.intrinsic(block, &format!("llvm.bitreverse.i{bits}"), ty.clone(), vec![(ty, rn.into())], "rbit"),
                    O::REV => self.m.intrinsic(block, &format!("llvm.bswap.i{bits}"), ty.clone(), vec![(ty, rn.into())], "rev"),
                    O::REV16 => {
                        let lanes = if bits == 64 { 0x00ff_00ff_00ff_00ffu64 as i64 } else { 0x00ff_00ff };
                        let low = self.m.b.bin(block, BinOp::And, ty.clone(), rn, lanes, "rev16.l");
                        let up = self.m.b.bin(block, BinOp::Shl, ty.clone(), low, 8i64, "rev16.u");
                        let shifted = self.m.b.bin(block, BinOp::LShr, ty.clone(), rn, 8i64, "rev16.s");
                        let down = self.m.b.bin(block, BinOp::And, ty.clone(), shifted, lanes, "rev16.d");
                        self.m.b.bin(block, BinOp::Or, ty, up, down, "rev16")
                    }
                    _ => {
                        let swapped = self.m.intrinsic(block, "llvm.bswap.i64", Type::I64, vec![(Type::I64, rn.into())], "rev32.s");
                        self.m.intrinsic(
                            block,
                            "llvm.fshl.i64",
                            Type::I64,
                            vec![(Type::I64, swapped.into()), (Type::I64, swapped.into()), (Type::I64, 32i64.into())],
                            "rev32",
                        )
                    }
                };
                self.write(block, rd, bits, false, result);
            }

            O::ADR | O::ADRP => {
                let (Some((rd, _, _)), Op::PCOffset(offset)) = (general(&ops[0]), ops[1]) else {
                    return self.unsupported(block, ip, code, text);
                };
                let base = if i.opcode == O::ADRP { ip & !0xfff } else { ip };
                let target = base.wrapping_add(offset as u64);
                let value = self.address_operand(target);
                self.write(block, rd, 64, false, value);
                self.pages.insert(rd, target);
            }

            O::BL => {
                let Op::PCOffset(offset) = ops[0] else { return self.unsupported(block, ip, code, text) };
                let target = ip.wrapping_add(offset as u64);
                self.write(block, 30, 64, false, ip as i64 + 4);
                if self.lifted.functions.contains_key(&target) {
                    self.call_lifted(block, target);
                } else {
                    self.warn(format!("{ip:#x}: a call to {target:#x}, which is no function"));
                    self.m.trap(block, ip, trap::INVALID);
                }
                self.pages.clear();
            }
            O::BLR => {
                let Some((rn, _, _)) = general(&ops[0]) else { return self.unsupported(block, ip, code, text) };
                let target = self.read(block, rn, 64, false);
                self.write(block, 30, 64, false, ip as i64 + 4);
                if let Some(import) = self.register_import_calls.get(&ip).cloned() {
                    self.import_call(block, &import);
                } else {
                    self.runtime_call(block, target);
                }
                self.pages.clear();
            }
            O::BR => {
                let Some((rn, _, _)) = general(&ops[0]) else { return self.unsupported(block, ip, code, text) };
                self.jump_target = Some(self.read(block, rn, 64, false));
            }
            O::Bcc(cc) | O::BCcc(cc) => self.cond = Some(self.condition(block, cc)),
            O::CBZ | O::CBNZ => {
                let Some((rt, bits, _)) = general(&ops[0]) else { return self.unsupported(block, ip, code, text) };
                let v = self.read(block, rt, bits, false);
                let cond = if i.opcode == O::CBZ { ICond::Eq } else { ICond::Ne };
                self.cond = Some(self.m.b.icmp(block, cond, Type::Int(bits), v, 0i64, "cb"));
            }
            O::TBZ | O::TBNZ => {
                let (Some((rt, bits, _)), Op::Imm16(bit)) = (general(&ops[0]), ops[1]) else {
                    return self.unsupported(block, ip, code, text);
                };
                // The bit numbers the full register whatever the form prints.
                let v = self.read(block, rt, 64, false);
                let shifted = self.m.b.bin(block, BinOp::LShr, Type::I64, v, i64::from(bit), "tb.s");
                let tested = self.m.b.bin(block, BinOp::And, Type::I64, shifted, 1i64, "tb.bit");
                let _ = bits;
                let cond = if i.opcode == O::TBZ { ICond::Eq } else { ICond::Ne };
                self.cond = Some(self.m.b.icmp(block, cond, Type::I64, tested, 0i64, "tb"));
            }

            O::LDR
            | O::LDUR
            | O::LDAR
            | O::LDAPR
            | O::LDRB
            | O::LDURB
            | O::LDARB
            | O::LDAPRB
            | O::LDRH
            | O::LDURH
            | O::LDARH
            | O::LDAPRH
            | O::LDRSB
            | O::LDURSB
            | O::LDRSH
            | O::LDURSH
            | O::LDRSW
            | O::LDURSW => return self.load(block, i, ip, code, text),
            O::STR | O::STUR | O::STLR | O::STRB | O::STURB | O::STLRB | O::STRH | O::STURH | O::STLRH => {
                return self.store(block, i, ip, code, text);
            }
            O::LDP | O::LDNP | O::LDPSW | O::STP | O::STNP => return self.pair(block, i, ip, code, text),

            O::FMOV => {
                // A move of bits between registers of one width, or an encoded constant.
                let value = match (scalar_float(&ops[0]), general(&ops[0]), &ops[1]) {
                    (Some((_, ty, bits)), _, Op::ImmediateDouble(d)) => {
                        let pattern = if ty == Type::Float { i64::from((*d as f32).to_bits() as i32) } else { d.to_bits() as i64 };
                        self.m.value_of(block, Type::Int(bits), pattern)
                    }
                    (Some((_, _, bits)), _, Op::Register(size, n)) if bits_of(*size) == bits => self.read(block, *n, bits, false),
                    (Some((_, _, bits)), _, src) | (None, Some((_, bits, _)), src) => match scalar_float(src) {
                        Some((n, _, from)) if from == bits => self.read_vector(block, n, bits),
                        _ => return self.unsupported(block, ip, code, text),
                    },
                    _ => return self.unsupported(block, ip, code, text),
                };
                match (scalar_float(&ops[0]), general(&ops[0])) {
                    (Some((d, _, bits)), _) => self.write_vector(block, d, bits, value),
                    (None, Some((d, bits, _))) => self.write(block, d, bits, false, value),
                    _ => return self.unsupported(block, ip, code, text),
                }
            }
            O::MOVI => {
                let Some(imm) = vector_immediate(&ops[1]) else { return self.unsupported(block, ip, code, text) };
                match ops[0] {
                    Op::SIMDRegister(SIMDSizeCode::D, n) => self.write_vector(block, n, 64, imm as i64),
                    Op::SIMDRegisterElements(width @ (SIMDSizeCode::D | SIMDSizeCode::Q), n, lane)
                        if matches!(ops[2], Op::Nothing) && simd_bits(lane) <= 64 =>
                    {
                        // The lane's value repeated across 64 bits, then across the register.
                        let lane = u32::from(simd_bits(lane));
                        let lane_mask = if lane == 64 { u64::MAX } else { (1u64 << lane) - 1 };
                        let pattern = (0..64 / lane).fold(0u64, |acc, k| acc | ((imm & lane_mask) << (k * lane)));
                        if matches!(width, SIMDSizeCode::D) {
                            self.write_vector(block, n, 64, pattern as i64);
                        } else {
                            self.m.write_vreg(block, u32::from(n), Type::vector(2, Type::I64), pattern as i64, false);
                        }
                    }
                    _ => return self.unsupported(block, ip, code, text),
                }
            }

            O::FADD | O::FSUB | O::FMUL | O::FDIV => {
                let (Some((d, ty)), Some((a, aty)), Some((b, bty))) = (float_operand(&ops[0]), float_operand(&ops[1]), float_operand(&ops[2]))
                else {
                    return self.unsupported(block, ip, code, text);
                };
                if aty != ty || bty != ty {
                    return self.unsupported(block, ip, code, text);
                }
                let op = match i.opcode {
                    O::FADD => BinOp::FAdd,
                    O::FSUB => BinOp::FSub,
                    O::FMUL => BinOp::FMul,
                    _ => BinOp::FDiv,
                };
                let x = self.read_float(block, a, &ty);
                let y = self.read_float(block, b, &ty);
                let result = self.m.b.bin(block, op, ty.clone(), x, y, "f");
                self.write_float(block, d, &ty, result);
            }
            O::FNEG | O::FABS | O::FSQRT => {
                let (Some((d, ty, bits)), Some((n, nty, _))) = (scalar_float(&ops[0]), scalar_float(&ops[1])) else {
                    return self.unsupported(block, ip, code, text);
                };
                if nty != ty {
                    return self.unsupported(block, ip, code, text);
                }
                let result = if i.opcode == O::FSQRT {
                    let x = self.read_float(block, n, &ty);
                    self.m.intrinsic(block, &format!("llvm.sqrt.{}", float_suffix(&ty)), ty.clone(), vec![(ty.clone(), x.into())], "sqrt")
                } else {
                    // The sign bit flipped or cleared, NaNs included, as the machine does.
                    let x = self.read_vector(block, n, bits);
                    let sign = -1i64 << (bits - 1);
                    if i.opcode == O::FNEG {
                        self.m.b.bin(block, BinOp::Xor, Type::Int(bits), x, sign, "fneg")
                    } else {
                        self.m.b.bin(block, BinOp::And, Type::Int(bits), x, !sign, "fabs")
                    }
                };
                if i.opcode == O::FSQRT {
                    self.write_float(block, d, &ty, result);
                } else {
                    self.write_vector(block, d, bits, result);
                }
            }
            O::FMADD | O::FMSUB | O::FNMADD | O::FNMSUB => {
                let (Some((d, ty, _)), Some((n, nty, _)), Some((m, mty, _)), Some((a, aty, _))) =
                    (scalar_float(&ops[0]), scalar_float(&ops[1]), scalar_float(&ops[2]), scalar_float(&ops[3]))
                else {
                    return self.unsupported(block, ip, code, text);
                };
                if nty != ty || mty != ty || aty != ty {
                    return self.unsupported(block, ip, code, text);
                }
                // One rounding: `a + n * m` with the product, the addend or both negated.
                let x = self.read_float(block, n, &ty);
                let x = if matches!(i.opcode, O::FMSUB | O::FNMADD) { self.negate(block, &ty, x) } else { x };
                let y = self.read_float(block, m, &ty);
                let z = self.read_float(block, a, &ty);
                let z = if matches!(i.opcode, O::FNMADD | O::FNMSUB) { self.negate(block, &ty, z) } else { z };
                let args = vec![(ty.clone(), x.into()), (ty.clone(), y.into()), (ty.clone(), z.into())];
                let result = self.m.intrinsic(block, &format!("llvm.fma.{}", float_suffix(&ty)), ty.clone(), args, "fma");
                self.write_float(block, d, &ty, result);
            }
            O::FCVT => {
                let (Some((d, to, _)), Some((n, from, _))) = (scalar_float(&ops[0]), scalar_float(&ops[1])) else {
                    return self.unsupported(block, ip, code, text);
                };
                let cast = match (&from, &to) {
                    (Type::Float, Type::Double) => Cast::FPExt,
                    (Type::Double, Type::Float) => Cast::FPTrunc,
                    _ => return self.unsupported(block, ip, code, text),
                };
                let x = self.read_float(block, n, &from);
                let result = self.m.b.cast(block, cast, from, x, to.clone(), "fcvt");
                self.write_float(block, d, &to, result);
            }
            O::SCVTF | O::UCVTF => {
                // The fixed-point form carries a third operand, the fraction bits.
                let (Some((d, ty, _)), Op::Register(size, n), Op::Nothing) = (scalar_float(&ops[0]), ops[1], ops[2]) else {
                    return self.unsupported(block, ip, code, text);
                };
                let bits = bits_of(size);
                let x = self.read(block, n, bits, false);
                let cast = if i.opcode == O::SCVTF { Cast::SIToFP } else { Cast::UIToFP };
                let result = self.m.b.cast(block, cast, Type::Int(bits), x, ty.clone(), "cvtf");
                self.write_float(block, d, &ty, result);
            }
            O::FCVTZS | O::FCVTZU => {
                let (Op::Register(size, d), Some((n, ty, _)), Op::Nothing) = (ops[0], scalar_float(&ops[1]), ops[2]) else {
                    return self.unsupported(block, ip, code, text);
                };
                // Toward zero, saturating, and a NaN is zero: the `.sat` conversions exactly.
                let bits = bits_of(size);
                let x = self.read_float(block, n, &ty);
                let kind = if i.opcode == O::FCVTZS { "fptosi" } else { "fptoui" };
                let name = format!("llvm.{kind}.sat.i{bits}.{}", float_suffix(&ty));
                let result = self.m.intrinsic(block, &name, Type::Int(bits), vec![(ty.clone(), x.into())], "cvtz");
                self.write(block, d, bits, false, result);
            }
            O::FCMP | O::FCMPE => {
                let Some((n, ty, _)) = scalar_float(&ops[0]) else { return self.unsupported(block, ip, code, text) };
                let x = self.read_float(block, n, &ty);
                let y: Operand = match ops[1] {
                    // The one constant the encoding has.
                    Op::ImmediateDouble(_) | Op::Immediate(0) | Op::Imm64(0) => Operand::Float(0.0),
                    ref op => match scalar_float(op) {
                        Some((m, mty, _)) if mty == ty => self.read_float(block, m, &ty).into(),
                        _ => return self.unsupported(block, ip, code, text),
                    },
                };
                // Less is 1000, equal 0110, greater 0010 and unordered 0011.
                let less = self.fcmp(block, FCond::Olt, &ty, x, y.clone(), "lt");
                let equal = self.fcmp(block, FCond::Oeq, &ty, x, y.clone(), "eq");
                let not_less = self.fcmp(block, FCond::Uge, &ty, x, y.clone(), "uge");
                let unordered = self.fcmp(block, FCond::Uno, &ty, x, y, "uno");
                self.set_flag(block, Nzcv::N, less);
                self.set_flag(block, Nzcv::Z, equal);
                self.set_flag(block, Nzcv::C, not_less);
                self.set_flag(block, Nzcv::V, unordered);
            }
            O::FCSEL => {
                let (Some((d, ty, bits)), Some((a, aty, _)), Some((b, bty, _)), Op::ConditionCode(cc)) =
                    (scalar_float(&ops[0]), scalar_float(&ops[1]), scalar_float(&ops[2]), ops[3])
                else {
                    return self.unsupported(block, ip, code, text);
                };
                if aty != ty || bty != ty {
                    return self.unsupported(block, ip, code, text);
                }
                let cond = self.condition(block, cc);
                let x = self.read_vector(block, a, bits);
                let y = self.read_vector(block, b, bits);
                let result = self.m.b.select(block, cond, Type::Int(bits), x, y, "fcsel");
                self.write_vector(block, d, bits, result);
            }

            _ => return self.unsupported(block, ip, code, text),
        }
        block
    }

    /// The width of a sub-word access and whether the loaded value is sign-extended.
    fn narrow_access(opcode: Opcode) -> Option<(u16, bool)> {
        use Opcode as O;
        match opcode {
            O::LDRB | O::LDURB | O::LDARB | O::LDAPRB | O::STRB | O::STURB | O::STLRB => Some((8, false)),
            O::LDRSB | O::LDURSB => Some((8, true)),
            O::LDRH | O::LDURH | O::LDARH | O::LDAPRH | O::STRH | O::STURH | O::STLRH => Some((16, false)),
            O::LDRSH | O::LDURSH => Some((16, true)),
            O::LDRSW | O::LDURSW | O::LDPSW => Some((32, true)),
            _ => None,
        }
    }

    /// The register a load fills or a store reads: general or vector, with its width.
    fn data_register(op: &Op) -> Option<(u16, u16, bool)> {
        match *op {
            Op::Register(size, n) => Some((n, bits_of(size), false)),
            Op::SIMDRegister(size, n) => Some((n, simd_bits(size), true)),
            _ => None,
        }
    }

    fn writeback(&mut self, block: BlockId, writeback: Option<(u16, ValueId)>) {
        if let Some((base, value)) = writeback {
            self.write(block, base, 64, true, value);
        }
    }

    fn load(&mut self, block: BlockId, i: &Instruction, ip: u64, code: u32, text: &str) -> BlockId {
        let Some((rt, bits, vector)) = Self::data_register(&i.operands[0]) else { return self.unsupported(block, ip, code, text) };
        let Some((address, writeback)) = self.address(block, &i.operands[1], ip) else { return self.unsupported(block, ip, code, text) };
        let (width, signed) = Self::narrow_access(i.opcode).unwrap_or((bits, false));
        let value = self.m.load(block, Type::Int(width), address, "ld");
        if vector {
            self.write_vector(block, rt, width, value);
        } else {
            let value = if width == bits {
                value
            } else {
                self.m.b.cast(block, if signed { Cast::SExt } else { Cast::ZExt }, Type::Int(width), value, Type::Int(bits), "ld.x")
            };
            self.write(block, rt, bits, false, value);
        }
        self.writeback(block, writeback);
        block
    }

    fn store(&mut self, block: BlockId, i: &Instruction, ip: u64, code: u32, text: &str) -> BlockId {
        let Some((rt, bits, vector)) = Self::data_register(&i.operands[0]) else { return self.unsupported(block, ip, code, text) };
        let Some((address, writeback)) = self.address(block, &i.operands[1], ip) else { return self.unsupported(block, ip, code, text) };
        let (width, _) = Self::narrow_access(i.opcode).unwrap_or((bits, false));
        let value = if vector { self.read_vector(block, rt, width) } else { self.read(block, rt, bits, false) };
        let value = if vector || width == bits {
            value
        } else {
            self.m.b.cast(block, Cast::Trunc, Type::Int(bits), value, Type::Int(width), "st.n")
        };
        self.m.store(block, Type::Int(width), value, address);
        self.writeback(block, writeback);
        block
    }

    fn pair(&mut self, block: BlockId, i: &Instruction, ip: u64, code: u32, text: &str) -> BlockId {
        let (Some((r1, bits, vector)), Some((r2, _, _))) = (Self::data_register(&i.operands[0]), Self::data_register(&i.operands[1]))
        else {
            return self.unsupported(block, ip, code, text);
        };
        let Some((address, writeback)) = self.address(block, &i.operands[2], ip) else { return self.unsupported(block, ip, code, text) };
        let (width, signed) = Self::narrow_access(i.opcode).unwrap_or((bits, false));
        let first = self.m.value_of(block, Type::I64, address);
        let second = self.m.b.bin(block, BinOp::Add, Type::I64, first, i64::from(width / 8), "addr.2");
        let load = matches!(i.opcode, Opcode::LDP | Opcode::LDNP | Opcode::LDPSW);
        for (r, addr) in [(r1, first), (r2, second)] {
            if load {
                let value = self.m.load(block, Type::Int(width), addr, "ldp");
                if vector {
                    self.write_vector(block, r, width, value);
                } else {
                    let value = if width == bits {
                        value
                    } else {
                        self.m.b.cast(
                            block,
                            if signed { Cast::SExt } else { Cast::ZExt },
                            Type::Int(width),
                            value,
                            Type::Int(bits),
                            "ldp.x",
                        )
                    };
                    self.write(block, r, bits, false, value);
                }
            } else {
                let value = if vector { self.read_vector(block, r, width) } else { self.read(block, r, bits, false) };
                self.m.store(block, Type::Int(width), value, addr);
            }
        }
        self.writeback(block, writeback);
        block
    }

    // endregion

    // region Floating point

    /// Vector register `n` as `ty`, from its low bytes.
    fn read_float(&mut self, block: BlockId, n: u16, ty: &Type) -> ValueId {
        self.m.read_vreg(block, u32::from(n), ty.clone())
    }

    /// Writes vector register `n` as `ty` and clears the bytes past it, as every scalar and
    /// 64-bit vector write does.
    fn write_float(&mut self, block: BlockId, n: u16, ty: &Type, value: impl Into<Operand>) {
        self.m.write_vreg(block, u32::from(n), ty.clone(), value, true);
    }

    /// `x` with its sign bit flipped.
    fn negate(&mut self, block: BlockId, ty: &Type, x: ValueId) -> ValueId {
        let bits = if *ty == Type::Float { 32 } else { 64 };
        let int = self.m.b.cast(block, Cast::Bitcast, ty.clone(), x, Type::Int(bits), "neg.i");
        let flipped = self.m.b.bin(block, BinOp::Xor, Type::Int(bits), int, -1i64 << (bits - 1), "neg.x");
        self.m.b.cast(block, Cast::Bitcast, Type::Int(bits), flipped, ty.clone(), "neg")
    }

    fn fcmp(&mut self, block: BlockId, cond: FCond, ty: &Type, a: impl Into<Operand>, b: impl Into<Operand>, hint: &str) -> ValueId {
        self.m.b.value(block, Inst::FCmp { cond, ty: ty.clone(), lhs: a.into(), rhs: b.into() }, hint)
    }

    // endregion
}

/// A scalar floating-point register: its number, its type and its width. Half precision is not
/// covered and reads as `None`.
fn scalar_float(op: &Op) -> Option<(u16, Type, u16)> {
    match *op {
        Op::SIMDRegister(SIMDSizeCode::S, n) => Some((n, Type::Float, 32)),
        Op::SIMDRegister(SIMDSizeCode::D, n) => Some((n, Type::Double, 64)),
        _ => None,
    }
}

/// A floating-point operand of the arithmetic: a scalar register, or a vector register whose
/// arrangement gives the lanes.
fn float_operand(op: &Op) -> Option<(u16, Type)> {
    if let Some((n, ty, _)) = scalar_float(op) {
        return Some((n, ty));
    }
    let Op::SIMDRegisterElements(width @ (SIMDSizeCode::D | SIMDSizeCode::Q), n, lane @ (SIMDSizeCode::S | SIMDSizeCode::D)) = *op else {
        return None;
    };
    let element = if matches!(lane, SIMDSizeCode::S) { Type::Float } else { Type::Double };
    let lanes = u32::from(simd_bits(width) / simd_bits(lane));
    (lanes > 1).then(|| (n, Type::vector(lanes, element)))
}

/// The lane value of a vector immediate.
fn vector_immediate(op: &Op) -> Option<u64> {
    match *op {
        Op::Imm64(v) => Some(v),
        Op::Immediate(v) => Some(u64::from(v)),
        Op::ImmShift(v, shift) => Some(u64::from(v) << shift),
        Op::ImmShiftMSL(v, shift) => Some((u64::from(v) << shift) | ((1u64 << shift) - 1)),
        _ => None,
    }
}

/// The suffix an intrinsic of `ty` carries.
fn float_suffix(ty: &Type) -> &'static str {
    if *ty == Type::Float { "f32" } else { "f64" }
}

/// Builds the wrapper of an import for ARM64: it reads `x0..x7` (a floating-point parameter of a
/// typed prototype from `v0..v7`) and the stack slots past them out of the state, calls the import,
/// and stores the result to `x0` (or `v0`).
pub fn import_wrapper(symbol: &str, x87: crate::x87::X87Unit) -> ra_asm_ir::Function {
    use crate::raise::abi::Kind;
    let name = crate::import_wrapper_name(symbol, false);
    let mut b = FunctionBuilder::new(&name, FnType::new(Type::Void, vec![Type::Ptr]), &["state"]);
    b.set_linkage(Linkage::Internal);
    let state = b.param(0);
    let block = b.block("entry");
    let mut m = Machine { b: &mut b, state, bits: 64, x87, isa: Isa::Arm64 };
    let typed = crate::raise::prototypes::typed(symbol);
    let kinds: Vec<Kind> = typed.as_ref().map_or_else(|| vec![Kind::Int; 8], |t| t.params.clone());
    let fn_ty = typed.as_ref().map_or_else(crate::state::import_adapter_type, crate::raise::prototypes::Typed::fn_type);
    let sp = read_general(&mut m, block, SP, 64, true);
    let (mut ints, mut floats, mut stack) = (0u16, 0u16, 0i64);
    let mut args = Vec::with_capacity(kinds.len());
    for kind in &kinds {
        let v = match kind {
            Kind::Int | Kind::Wide if ints < 8 => {
                ints += 1;
                read_general(&mut m, block, ints - 1, 64, false)
            }
            Kind::Float | Kind::Double if floats < 8 => {
                floats += 1;
                let p = m.xmm_ptr(block, u32::from(floats - 1));
                m.b.load(block, kind.ty(), p, "arg")
            }
            _ => {
                let addr = m.b.bin(block, BinOp::Add, Type::I64, sp, stack, "arg.p");
                stack += 8;
                m.load(block, kind.ty(), addr, "arg")
            }
        };
        args.push((kind.ty(), v.into()));
    }
    let result = m.b.call(block, fn_ty, Callee::Direct(symbol.to_owned()), args, "ret");
    let returns = typed.as_ref().map_or(Some(Kind::Int), |t| t.returns);
    match (returns, result) {
        (Some(Kind::Int | Kind::Wide), Some(result)) => write_general(&mut m, block, 0, 64, false, result),
        (Some(kind @ (Kind::Float | Kind::Double)), Some(result)) => {
            let p = m.xmm_ptr(block, 0);
            m.b.store(block, Type::I128, 0i64, p);
            m.b.store(block, kind.ty(), result, p);
        }
        _ => {}
    }
    b.terminate(block, Terminator::Ret(None));
    b.finish()
}
