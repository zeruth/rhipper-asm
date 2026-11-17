//! The MIPS semantics: one instruction word of MIPS I to IV becomes IR over the [`Machine`] state.
//!
//! The state holds `$0..$31` at 64 bits, MIPS III's width: a 32-bit operation reads the low
//! half of its operands and writes its result sign-extended, as the machine does, so a 64-bit
//! operation of MIPS III sees the same registers. `$0` reads as the constant zero and a write to
//! it is dropped. `hi` and `lo` are scalar fields of the state, the FPU's eight condition codes
//! are its flag bytes and its registers the vector slots: under o32 (`Status.FR = 0`) a single is
//! the low word of its slot and a double the even register and the odd one after it, under n32
//! (`Status.FR = 1`) a double is the low half of its own slot.
//!
//! A branch's delay slot is the instruction after it in the block, as the builder places it: the
//! branch's condition, its target and its link are taken from the registers before the slot runs,
//! then the slot runs, then control leaves; a likely branch's slot that shares the block runs on
//! the taken path only. The words are read in the image's order; a big-endian program's memory
//! keeps its order and every load and store swaps its bytes, so the registers hold the values the
//! machine sees and `lwl`/`lwr`/`swl`/`swr` merge from the word's other end. The arithmetic and the
//! conversions to an integer that round by the FCSR take its default mode, to nearest. Every
//! encoding outside the subset (the coprocessor 0 of a processor other than the VR4300, `COP1X`, the doubleword unaligned accesses,
//! the conditional traps on an immediate, a branch in a delay slot) traps with its address.
//!
//! A `PlayStation` program runs on the R3000: no FPU, so a COP1 word traps with its address, and the
//! GTE as coprocessor 2, whose registers are a field of the state that `mfc2`, `mtc2`, `cfc2`,
//! `ctc2`, `lwc2` and `swc2` move, and whose commands are calls on `ra_gte_<op>`. A jump to the
//! BIOS's `A0`, `B0` or `C0` entry with the function's number in `t1`, both constants of the block,
//! is a call on the function the table names.
//!
//! A `PlayStation 2` program runs on the R5900, whose registers are 128 bits: every instruction
//! of MIPS III writes the low 64 as above and leaves the upper 64, a field of their own in the
//! state, as they are. `lq` and `sq` move the whole register; `mult` and `madd` also write `lo`
//! to `rd`, and their forms ending in `1` work on the second pair, `hi1` and `lo1`, the upper
//! halves of `hi` and `lo`; the multimedia instructions are vector IR on the register's words,
//! halfwords or bytes. Its FPU has singles only and is not IEEE's: no infinity, denormals as
//! zeros, every result rounded toward zero, so its arithmetic and its conversions are calls on
//! the runtime's `ra_f32_ftz_*` on the registers' bits, and its accumulator, which the
//! multiply-adds keep, is a field of the state. The VU0 macro instructions of coprocessor 2 and
//! the EE kernel's `syscall` trap with their address.
//!
//! A PSP program runs on the Allegrex, MIPS II with the `SPECIAL` and `SPECIAL3` forms of
//! MIPS32 release 2 (`ext`, `ins`, `seb`, `seh`, `wsbh`, `bitrev`, the rotates) and its own
//! (`min`, `max`, `clz`, `clo`, `madd`, `msub`) in the encodings MIPS III gives its doubleword
//! operations, which it lacks; the VFPU traps with its address. An import's stub, the
//! `jr $ra` over the `syscall` the kernel writes when it loads the module, is the call on the
//! import its NID names.
//!
//! A Nintendo 64 program runs on the VR4300, MIPS III with the FPU's 64-bit registers
//! (`Status.FR = 1`, as libdragon starts it): coprocessor 0's registers are a global the moves
//! read and write, the TLB maps every address as itself, and `cache`, like `sync`, has nothing to
//! do in a port's memory.
#![allow(clippy::many_single_char_names)]

use std::collections::HashMap;

use ra_asm_analysis::{Block, Function, Terminator as T};
use ra_asm_arch::Instr;
use ra_asm_ir::{BinOp, BlockId, Callee, Cast, FCond, FnType, FunctionBuilder, ICond, Inst, Operand, Terminator, Type, ValueId};
use ra_asm_pe::Image;

use crate::raise::prototypes::{BiosCall, psx_bios};
use crate::state::{Isa, MIPS_ACC, MIPS_FCSR, MIPS_GTE, MIPS_HI, MIPS_LO, Machine, N64_COP0, PSX_GP};
use crate::x86::trap;
use crate::{Lifted, function_name};

/// `ra`, the register a call links through.
const RA: u8 = 31;
/// `t1`, where a BIOS call takes the function's number.
const T1: u8 = 9;
/// `gp`, the small-data pointer.
const GP: u8 = 28;
/// The VR4300's `Count`, coprocessor 0's register 9.
const COUNT: u8 = 9;
/// The ticks of `Count` between two reads of it: a loop that polls it runs some eight
/// instructions, a cycle each, and `Count` ticks every other cycle.
const COUNT_STEP: i64 = 4;

/// Lifts one function.
pub fn lift_function(image: &Image, lifted: &mut Lifted, f: &Function) -> ra_asm_ir::Function {
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
    let (x87, isa) = (lifted.x87, lifted.isa);
    let r3000 = image.platform() == Some("psx");
    let allegrex = image.platform() == Some("psp");
    let r5900 = image.platform() == Some("ps2");
    let mut lifter = Lifter {
        m: Machine { b: &mut b, state, bits: 32, x87, isa },
        image,
        lifted,
        blocks,
        cond: None,
        jump_target: None,
        consts: HashMap::new(),
        r3000,
        allegrex,
        r5900,
        vr4300: image.platform() == Some("n64"),
        big: image.big_endian(),
    };
    // A PSP import's stub holds the `syscall` the kernel writes only once it loads the module, an
    // IOP one the jump the loader writes over its `jr $ra`.
    let modular = allegrex || image.platform() == Some("iop");
    let import = image.imports.iter().find(|i| modular && i.iat_va == f.entry).and_then(|i| i.name.clone());
    if let Some(import) = import {
        let block = lifter.blocks[&f.entry];
        lifter.import_call(block, &import);
        lifter.m.b.terminate(block, Terminator::Ret(None));
    } else {
        for blk in &f.blocks {
            lifter.lift_block(blk);
        }
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

/// The fields of an instruction word.
#[derive(Debug, Clone, Copy)]
struct Word(u32);

impl Word {
    fn op(self) -> u32 {
        self.0 >> 26
    }

    fn rs(self) -> u8 {
        ((self.0 >> 21) & 31) as u8
    }

    fn rt(self) -> u8 {
        ((self.0 >> 16) & 31) as u8
    }

    fn rd(self) -> u8 {
        ((self.0 >> 11) & 31) as u8
    }

    fn sa(self) -> u8 {
        ((self.0 >> 6) & 31) as u8
    }

    fn funct(self) -> u32 {
        self.0 & 63
    }

    /// The 16-bit immediate, sign-extended.
    fn simm(self) -> i64 {
        i64::from(self.0 as u16 as i16)
    }

    /// The 16-bit immediate, zero-extended.
    fn uimm(self) -> i64 {
        i64::from(self.0 & 0xffff)
    }

    /// The condition code a `bc1`, a `movf` or a `movt` reads.
    fn branch_cc(self) -> i64 {
        i64::from((self.0 >> 18) & 7)
    }

    /// Whether a `bc1`, a `movt` or a `movt.fmt` tests the condition code for true.
    fn tests_true(self) -> bool {
        (self.0 >> 16) & 1 == 1
    }

    /// Whether the word is a branch or a jump: an instruction with a delay slot.
    fn is_branch(self) -> bool {
        match self.op() {
            0 => matches!(self.funct(), 8 | 9),
            1 => matches!(self.rt(), 0..=3 | 16..=19),
            2..=7 | 20..=23 => true,
            17 => self.rs() == 8,
            _ => false,
        }
    }
}

/// Where a branch calls, once its slot has run.
#[derive(Clone, Copy)]
enum Call {
    Direct(u64),
    Register(ValueId),
}

struct Lifter<'a, 'b> {
    m: Machine<'b>,
    image: &'a Image,
    lifted: &'a mut Lifted,
    blocks: HashMap<u64, BlockId>,
    /// The condition of the conditional branch that ends the current block.
    cond: Option<ValueId>,
    /// The target of the jump through a register that ends the current block, as a 64-bit address.
    jump_target: Option<ValueId>,
    /// The registers the block has set to a constant so far, at 32 bits: where a BIOS call's
    /// entry and number come from.
    consts: HashMap<u8, u32>,
    /// A `PlayStation` program: the R3000, with the GTE and no FPU.
    r3000: bool,
    /// A PSP program: the Allegrex, whose own `SPECIAL` and `SPECIAL3` forms replace MIPS III's.
    allegrex: bool,
    /// A `PlayStation 2` program: the R5900, with its 128-bit registers and multimedia
    /// instructions.
    r5900: bool,
    /// A Nintendo 64 program: the VR4300, with its coprocessor 0 and `cache`.
    vr4300: bool,
    /// A big-endian program, whose memory keeps its order: every load and store swaps its bytes.
    big: bool,
}

impl Lifter<'_, '_> {
    fn warn(&mut self, text: String) {
        self.lifted.warnings.push(text);
    }

    fn word(&self, address: u64) -> Option<Word> {
        self.image.u32(address).ok().map(Word)
    }

    /// A load of `ty` from guest memory: a big-endian program's bytes are swapped into the host's
    /// order, so a register holds the value the machine sees.
    fn guest_load(&mut self, block: BlockId, ty: Type, addr: impl Into<Operand>, hint: &str) -> ValueId {
        let v = self.m.load(block, ty.clone(), addr, hint);
        self.swap(block, ty, v)
    }

    /// A store of `ty` to guest memory, in the program's order as [`Self::guest_load`] reads it.
    fn guest_store(&mut self, block: BlockId, ty: Type, v: ValueId, addr: impl Into<Operand>) {
        let v = self.swap(block, ty.clone(), v);
        self.m.store(block, ty, v, addr);
    }

    /// `v` with its bytes reversed in a big-endian program, as is in a little-endian one: the swap
    /// between the program's order and the host's goes both ways.
    fn swap(&mut self, block: BlockId, ty: Type, v: ValueId) -> ValueId {
        match ty {
            Type::Int(bits @ (16 | 32 | 64)) if self.big => {
                self.m.intrinsic(block, &format!("llvm.bswap.i{bits}"), ty.clone(), vec![(ty, v.into())], "be")
            }
            _ => v,
        }
    }

    fn fr64(&self) -> bool {
        self.m.isa == Isa::MipsN32
    }

    // region Blocks

    fn lift_block(&mut self, blk: &Block) {
        let mut block = self.blocks[&blk.start];
        self.m.b.set_block_address(block, blk.start);
        self.cond = None;
        self.jump_target = None;
        self.consts.clear();
        let mut n = 0;
        while let Some(i) = blk.instrs.get(n) {
            if self.m.b.has_terminator(block) {
                break;
            }
            self.m.b.address = Some(i.address);
            let Some(w) = self.word(i.address) else {
                self.m.trap(block, i.address, trap::INVALID);
                break;
            };
            n += 1;
            if w.is_branch() {
                let slot = blk.instrs.get(n).filter(|s| s.address == i.address.wrapping_add(4));
                if slot.is_some() {
                    n += 1;
                }
                block = self.lift_branch(block, w, i.address, slot);
                // A callee leaves no register as the block knew it.
                self.consts.clear();
            } else {
                block = self.lift_instr(block, w, i.address);
            }
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
                let cases = targets.iter().filter_map(|t| self.blocks.get(t).map(|b| (i128::from(*t), *b))).collect();
                self.m.b.terminate(block, Terminator::Switch { ty: Type::I64, value: target.into(), default, cases });
            }
            T::IndirectJump => {
                let Some(target) = self.jump_target else {
                    self.m.trap(block, last, trap::INVALID);
                    return;
                };
                // A jump the analysis did not resolve: most often a tail call through a pointer.
                self.runtime_call(block, target);
                self.m.b.terminate(block, Terminator::Ret(None));
            }
            T::TailCall(t) => {
                self.call_lifted(block, *t);
                self.m.b.terminate(block, Terminator::Ret(None));
            }
            T::ImportTailCall(import) => {
                self.warn(format!("{last:#x}: a tail call of the import {import}, which the MIPS lift does not bind"));
                self.m.trap(block, last, trap::INVALID);
            }
            T::NoReturnCall(_) | T::NoReturnImportCall(_) => self.m.b.terminate(block, Terminator::Unreachable),
            T::Return => self.m.b.terminate(block, Terminator::Ret(None)),
            T::Trap => self.m.trap(block, last, trap::UD2),
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

    /// A branch or a jump and its delay slot: the condition, the target and the link from the
    /// registers before the slot, the slot, then the call a linking one makes. Where control
    /// goes after is the block's terminator.
    fn lift_branch(&mut self, block: BlockId, w: Word, ip: u64, slot: Option<&Instr>) -> BlockId {
        let (rs, rt) = (w.rs(), w.rt());
        let relative = ip.wrapping_add(4).wrapping_add((w.simm() << 2) as u64) & 0xffff_ffff;
        let bios = self.bios_entry(w, ip);
        let (mut cond, mut likely, mut link, mut call, mut jump) = (None, false, None, None, None);
        match w.op() {
            0 => {
                let target = self.reg32(block, rs);
                let wide = self.m.b.cast(block, Cast::ZExt, Type::I32, target, Type::I64, "target");
                if w.funct() == 9 {
                    link = Some(w.rd());
                    call = Some(Call::Register(wide));
                } else {
                    jump = Some(wide);
                }
            }
            1 if rt & 16 == 0 => {
                let a = self.reg64(block, rs);
                let cc = if rt & 1 == 0 { ICond::Slt } else { ICond::Sge };
                cond = Some(self.m.b.icmp(block, cc, Type::I64, a, 0i64, "cond"));
                likely = rt & 2 != 0;
            }
            // `bal`: the unconditional `bgezal $0`.
            1 if rt == 17 && rs == 0 => {
                link = Some(RA);
                call = Some(Call::Direct(relative));
            }
            2 => {}
            3 => {
                link = Some(RA);
                call = Some(Call::Direct((ip.wrapping_add(4) & 0xf000_0000) | (u64::from(w.0 & 0x03ff_ffff) << 2)));
            }
            4..=7 | 20..=23 => {
                let a = self.reg64(block, rs);
                let c = match w.op() & 0xf {
                    4 | 5 => {
                        let b = self.reg64(block, rt);
                        let cc = if w.op() & 1 == 0 { ICond::Eq } else { ICond::Ne };
                        self.m.b.icmp(block, cc, Type::I64, a, b, "cond")
                    }
                    6 => self.m.b.icmp(block, ICond::Sle, Type::I64, a, 0i64, "cond"),
                    _ => self.m.b.icmp(block, ICond::Sgt, Type::I64, a, 0i64, "cond"),
                };
                cond = Some(c);
                likely = w.op() >= 20;
            }
            17 if !self.r3000 => {
                let f = self.fcc(block, w.branch_cc());
                cond = Some(if w.tests_true() { f } else { self.m.b.bin(block, BinOp::Xor, Type::I1, f, 1i64, "cond") });
                likely = (w.0 >> 17) & 1 == 1;
            }
            _ => return self.unsupported(block, w, ip),
        }
        if let Some(r) = link {
            self.set64(block, r, i64::from(ip.wrapping_add(8) as u32 as i32));
        }
        match slot {
            Some(s) => {
                let Some(sw) = self.word(s.address).filter(|sw| !sw.is_branch()) else {
                    self.warn(format!("{:#x}: a branch in the delay slot of the branch at {ip:#x}", s.address));
                    self.m.trap(block, s.address, trap::INVALID);
                    return block;
                };
                self.m.b.address = Some(s.address);
                if let (true, Some(c)) = (likely, cond) {
                    let then = self.m.b.block(&format!("slot_{:x}", s.address));
                    let next = self.m.b.block(&format!("taken_{ip:x}"));
                    self.m.b.terminate(block, Terminator::CondBr { cond: c.into(), then, els: next });
                    let end = self.lift_instr(then, sw, s.address);
                    if !self.m.b.has_terminator(end) {
                        self.m.b.terminate(end, Terminator::Br(next));
                    }
                    return self.after_slot(next, ip, cond, jump, call);
                }
                let end = self.lift_instr(block, sw, s.address);
                if let Some(entry) = bios {
                    // `j` and `jr` leave for the BIOS, which returns to the caller's `ra`.
                    let tail = w.op() == 2 || (w.op() == 0 && w.funct() == 8);
                    return self.bios_call(end, entry, tail, ip);
                }
                self.after_slot(end, ip, cond, jump, call)
            }
            // A likely branch's slot lies on its taken edge.
            None if likely => self.after_slot(block, ip, cond, jump, call),
            None => {
                self.warn(format!("{ip:#x}: a branch whose delay slot is not in its block"));
                self.m.trap(block, ip, trap::INVALID);
                block
            }
        }
    }

    fn after_slot(&mut self, block: BlockId, ip: u64, cond: Option<ValueId>, jump: Option<ValueId>, call: Option<Call>) -> BlockId {
        if self.m.b.has_terminator(block) {
            return block;
        }
        self.m.b.address = Some(ip);
        self.cond = cond;
        self.jump_target = jump;
        match call {
            Some(Call::Direct(t)) if self.lifted.functions.contains_key(&t) => self.call_lifted(block, t),
            Some(Call::Direct(t)) => self.runtime_call(block, t as i64),
            Some(Call::Register(target)) => self.runtime_call(block, target),
            None => {}
        }
        block
    }

    // endregion

    // region Calls

    /// The BIOS entry (`0xa0`, `0xb0`, `0xc0`, in any of the address space's mirrors) a jump or a
    /// call of a `PlayStation` program goes to: a `j` or `jal` that names it, or a `jr` or `jalr`
    /// through a register the block set to it.
    fn bios_entry(&self, w: Word, ip: u64) -> Option<u64> {
        if !self.r3000 {
            return None;
        }
        let target = match w.op() {
            0 => u64::from(*self.consts.get(&w.rs())?),
            2 | 3 => (ip.wrapping_add(4) & 0xf000_0000) | (u64::from(w.0 & 0x03ff_ffff) << 2),
            _ => return None,
        };
        let entry = target & 0x1fff_ffff;
        matches!(entry, 0xa0 | 0xb0 | 0xc0).then_some(entry)
    }

    /// A call on the BIOS, once the jump's slot has run: the function the number in `t1` names in
    /// the table of `entry`, on its arguments from `a0` and with its result in `v0`; after a jump
    /// to the entry (`tail`) the function returns to its caller. A number the block does not set,
    /// or one the table does not name, traps.
    fn bios_call(&mut self, block: BlockId, entry: u64, tail: bool, ip: u64) -> BlockId {
        if self.m.b.has_terminator(block) {
            return block;
        }
        self.m.b.address = Some(ip);
        let number = self.consts.get(&T1).copied();
        let Some(call) = number.and_then(|n| psx_bios(entry, u64::from(n))) else {
            let held = number.map_or_else(|| "no constant".to_owned(), |n| format!("{n:#x}"));
            self.warn(format!("{ip:#x}: a BIOS call through {entry:#x} with {held} in t1, which the table does not name"));
            self.m.trap(block, ip, trap::INVALID);
            return block;
        };
        self.bios_function(block, call);
        if tail {
            self.m.b.terminate(block, Terminator::Ret(None));
        }
        block
    }

    /// `v0 = ra_bios_<name>(a0, ...)`, its result a word as the BIOS leaves it.
    fn bios_function(&mut self, block: BlockId, call: BiosCall) {
        let args: Vec<(Type, Operand)> = (0..call.params).map(|i| (Type::I64, self.reg64(block, 4 + i).into())).collect();
        if let Some(r) = self.m.b.call(block, call.fn_type(), Callee::Direct(call.symbol()), args, call.name) {
            let word = self.m.b.cast(block, Cast::Trunc, Type::I64, r, Type::I32, "v0.w");
            self.set32(block, 2, word);
        }
    }

    /// Follows the constants of the block through `w`: `lui`, and `addiu` and `ori` on `$0` or on
    /// a register the block holds; a register any other word writes leaves the table.
    fn track(&mut self, w: Word) {
        let (rs, rt) = (w.rs(), w.rt());
        let known = |consts: &HashMap<u8, u32>, r: u8| if r == 0 { Some(0) } else { consts.get(&r).copied() };
        let constant = match w.op() {
            9 => known(&self.consts, rs).map(|v| v.wrapping_add(w.simm() as u32)),
            13 => known(&self.consts, rs).map(|v| v | w.uimm() as u32),
            15 => Some((w.0 & 0xffff) << 16),
            _ => None,
        };
        let written = match w.op() {
            0 => Some(w.rd()),
            1 if rt & 16 != 0 => Some(RA),
            3 => Some(RA),
            31 if w.funct() == 0x20 => Some(w.rd()),
            8..=15 | 24..=27 | 31 | 32..=39 | 48 | 52 | 55 | 56 | 60 => Some(rt),
            16..=18 if rs < 4 => Some(rt),
            _ => None,
        };
        if let Some(r) = written {
            self.consts.remove(&r);
        }
        if let (Some(v), true) = (constant, rt != 0) {
            self.consts.insert(rt, v);
        }
    }

    /// `call F_target(state)`.
    fn call_lifted(&mut self, block: BlockId, target: u64) {
        let name = function_name(target);
        self.m.b.call(block, FnType::new(Type::Void, vec![Type::Ptr]), Callee::Direct(name), vec![(Type::Ptr, self.m.state.into())], "");
    }

    /// Calls an import through its wrapper, `imp_<name>(ptr %state)`, which reads `a0..a3` and
    /// the stack past their home slots and leaves the result in `v0`: see [`import_wrapper`].
    fn import_call(&mut self, block: BlockId, import: &str) {
        let symbol = crate::x86::import_symbol(import);
        self.lifted.imports.insert(symbol.clone());
        self.lifted.wrappers.insert((symbol.clone(), 0, false));
        let wrapper = crate::import_wrapper_name(&symbol, false);
        self.m.b.call(block, FnType::new(Type::Void, vec![Type::Ptr]), Callee::Direct(wrapper), vec![(Type::Ptr, self.m.state.into())], "");
    }

    /// `call @ra_call(state, target)`: the runtime finds the lifted function at the address.
    fn runtime_call(&mut self, block: BlockId, target: impl Into<Operand>) {
        self.m.b.call(
            block,
            FnType::new(Type::Void, vec![Type::Ptr, Type::I64]),
            Callee::Direct("ra_call".into()),
            vec![(Type::Ptr, self.m.state.into()), (Type::I64, target.into())],
            "",
        );
    }

    // endregion

    // region Registers

    /// General register `n` at 64 bits; `$0` is zero.
    fn reg64(&mut self, block: BlockId, n: u8) -> ValueId {
        if n == 0 {
            return self.m.value_of(block, Type::I64, 0i64);
        }
        let p = self.gpr_ptr(block, n);
        self.m.b.load(block, Type::I64, p, self.m.gpr_name(u32::from(n)))
    }

    /// Where general register `n` lives: its field of the state, but for the R3000's `gp`, which
    /// is the [`PSX_GP`] global.
    fn gpr_ptr(&mut self, block: BlockId, n: u8) -> ValueId {
        if self.r3000 && n == GP {
            let gp = Operand::Address { global: PSX_GP.to_owned(), offset: 0 };
            return self.m.b.cast(block, Cast::IntToPtr, Type::I64, gp, Type::Ptr, "p.gp");
        }
        self.m.gpr_ptr(block, u32::from(n))
    }

    /// The low word of general register `n`, which a 32-bit operation reads.
    fn reg32(&mut self, block: BlockId, n: u8) -> ValueId {
        if n == 0 {
            return self.m.value_of(block, Type::I32, 0i64);
        }
        let whole = self.reg64(block, n);
        let name = format!("{}.w", self.m.gpr_name(u32::from(n)));
        self.m.b.cast(block, Cast::Trunc, Type::I64, whole, Type::I32, &name)
    }

    fn reg(&mut self, block: BlockId, n: u8, ty: &Type) -> ValueId {
        if *ty == Type::I32 { self.reg32(block, n) } else { self.reg64(block, n) }
    }

    /// Writes general register `n`; a write to `$0` is dropped.
    fn set64(&mut self, block: BlockId, n: u8, value: impl Into<Operand>) {
        if n == 0 {
            return;
        }
        let p = self.gpr_ptr(block, n);
        self.m.b.store(block, Type::I64, value, p);
    }

    /// Writes a 32-bit result, sign-extended into the register.
    fn set32(&mut self, block: BlockId, n: u8, value: impl Into<Operand>) {
        if n == 0 {
            return;
        }
        let name = format!("{}.x", self.m.gpr_name(u32::from(n)));
        let wide = self.m.b.cast(block, Cast::SExt, Type::I32, value, Type::I64, &name);
        self.set64(block, n, wide);
    }

    fn set(&mut self, block: BlockId, n: u8, ty: &Type, value: impl Into<Operand>) {
        if *ty == Type::I32 { self.set32(block, n, value) } else { self.set64(block, n, value) }
    }

    /// Writes `hi` or `lo`; a 32-bit value is sign-extended, as the 32-bit multiplies and divides
    /// leave it.
    fn set_field(&mut self, block: BlockId, index: u8, ty: &Type, value: impl Into<Operand>) {
        self.set(block, index, ty, value);
    }

    fn field(&mut self, block: BlockId, index: u8) -> ValueId {
        self.reg64(block, index)
    }

    /// The FPU's condition code `cc`.
    fn fcc(&mut self, block: BlockId, cc: i64) -> ValueId {
        let name = format!("fcc{cc}");
        let p = self.m.flag_ptr_at(block, cc, &name);
        let byte = self.m.b.load(block, Type::I8, p, &format!("{name}.b"));
        self.m.b.cast(block, Cast::Trunc, Type::I8, byte, Type::I1, &name)
    }

    fn set_fcc(&mut self, block: BlockId, cc: i64, value: impl Into<Operand>) {
        let name = format!("fcc{cc}");
        let p = self.m.flag_ptr_at(block, cc, &name);
        let byte = self.m.b.cast(block, Cast::ZExt, Type::I1, value, Type::I8, &format!("{name}.b"));
        self.m.b.store(block, Type::I8, byte, p);
    }

    /// The bits of FPU register `n` as a single (32) or a double (64).
    fn fpr(&mut self, block: BlockId, n: u8, double: bool) -> ValueId {
        if !double {
            return self.m.read_vreg(block, u32::from(n), Type::I32);
        }
        if self.fr64() {
            return self.m.read_vreg(block, u32::from(n), Type::I64);
        }
        let lo = self.m.read_vreg(block, u32::from(n & !1), Type::I32);
        let hi = self.m.read_vreg(block, u32::from(n | 1), Type::I32);
        let lo = self.m.b.cast(block, Cast::ZExt, Type::I32, lo, Type::I64, "lo");
        let hi = self.m.b.cast(block, Cast::ZExt, Type::I32, hi, Type::I64, "hi");
        let high = self.m.b.bin(block, BinOp::Shl, Type::I64, hi, 32i64, "high");
        self.m.b.bin(block, BinOp::Or, Type::I64, high, lo, &format!("f{n}.d"))
    }

    fn set_fpr(&mut self, block: BlockId, n: u8, double: bool, value: impl Into<Operand>) {
        if !double {
            self.m.write_vreg(block, u32::from(n), Type::I32, value, false);
        } else if self.fr64() {
            self.m.write_vreg(block, u32::from(n), Type::I64, value, false);
        } else {
            let value = self.m.value_of(block, Type::I64, value);
            let lo = self.m.b.cast(block, Cast::Trunc, Type::I64, value, Type::I32, "lo");
            let high = self.m.b.bin(block, BinOp::LShr, Type::I64, value, 32i64, "high");
            let hi = self.m.b.cast(block, Cast::Trunc, Type::I64, high, Type::I32, "hi");
            self.m.write_vreg(block, u32::from(n & !1), Type::I32, lo, false);
            self.m.write_vreg(block, u32::from(n | 1), Type::I32, hi, false);
        }
    }

    fn freg(&mut self, block: BlockId, n: u8, double: bool) -> ValueId {
        let (ity, fty) = float_types(double);
        let bits = self.fpr(block, n, double);
        self.m.b.cast(block, Cast::Bitcast, ity, bits, fty, &format!("f{n}"))
    }

    fn set_freg(&mut self, block: BlockId, n: u8, double: bool, value: impl Into<Operand>) {
        let (ity, fty) = float_types(double);
        let bits = self.m.b.cast(block, Cast::Bitcast, fty, value, ity, "bits");
        self.set_fpr(block, n, double, bits);
    }

    // endregion

    // region Instructions

    fn unsupported(&mut self, block: BlockId, w: Word, ip: u64) -> BlockId {
        self.warn(format!("{ip:#x}: the MIPS lift does not cover the word {:#010x}", w.0));
        self.m.unsupported(block, ip, u64::from(w.0));
        block
    }

    /// A trap where `cond` holds; the lift goes on in the block where it does not.
    fn trap_if(&mut self, block: BlockId, cond: ValueId, ip: u64, code: u64) -> BlockId {
        let bad = self.m.b.block(&format!("trap_{ip:x}"));
        let ok = self.m.b.block(&format!("ok_{ip:x}"));
        self.m.b.terminate(block, Terminator::CondBr { cond: cond.into(), then: bad, els: ok });
        self.m.trap(bad, ip, code);
        ok
    }

    /// `a + b` (or `a - b`), whose signed overflow traps: the block after the check and the result.
    fn checked_sum(&mut self, block: BlockId, ty: &Type, a: ValueId, b: impl Into<Operand>, sub: bool, ip: u64) -> (BlockId, ValueId) {
        let ty = ty.clone();
        let b: Operand = b.into();
        let result = self.m.b.bin(block, if sub { BinOp::Sub } else { BinOp::Add }, ty.clone(), a, b.clone(), "sum");
        let x = self.m.b.bin(block, BinOp::Xor, ty.clone(), a, result, "ov.a");
        let y = if sub {
            self.m.b.bin(block, BinOp::Xor, ty.clone(), a, b, "ov.b")
        } else {
            self.m.b.bin(block, BinOp::Xor, ty.clone(), b, result, "ov.b")
        };
        let both = self.m.b.bin(block, BinOp::And, ty.clone(), x, y, "ov");
        let over = self.m.b.icmp(block, ICond::Slt, ty, both, 0i64, "overflow");
        (self.trap_if(block, over, ip, trap::INT), result)
    }

    /// The address a load or a store names: base plus the signed offset, at 32 bits.
    fn address(&mut self, block: BlockId, w: Word) -> ValueId {
        let base = self.reg32(block, w.rs());
        self.m.b.bin(block, BinOp::Add, Type::I32, base, w.simm(), "addr")
    }

    fn lift_instr(&mut self, block: BlockId, w: Word, ip: u64) -> BlockId {
        let (rs, rt) = (w.rs(), w.rt());
        self.track(w);
        match w.op() {
            0 => return self.special(block, w, ip),
            // The R3000 has no FPU.
            17 | 49 | 53 | 57 | 61 if self.r3000 => return self.unsupported(block, w, ip),
            18 | 50 | 58 if self.r3000 => return self.cop2(block, w, ip),
            31 if self.allegrex => return self.special3(block, w, ip),
            // The Allegrex gives the doubleword opcodes of MIPS III to the VFPU, which the lift
            // does not read.
            24 | 25 | 52 | 55 | 60 | 63 if self.allegrex => return self.unsupported(block, w, ip),
            28 if self.r5900 => return self.mmi(block, w, ip),
            16 if self.vr4300 => return self.cop0(block, w, ip),
            // cache: the port's memory has no cache to write back or invalidate.
            47 if self.vr4300 => {}
            17 if self.r5900 => return self.ee_cop1(block, w, ip),
            // The R5900's coprocessor 2 is VU0 in macro mode (`cop2`, `lqc2`, `sqc2`), and its
            // FPU has no doubles (`ldc1`, `sdc1`).
            18 | 53 | 54 | 61 | 62 if self.r5900 => return self.unsupported(block, w, ip),
            // lq, sq: the low four bits of the address are not used.
            30 | 31 if self.r5900 => {
                let addr = self.address(block, w);
                let aligned = self.m.b.bin(block, BinOp::And, Type::I32, addr, -16i64, "aligned");
                if w.op() == 30 {
                    let v = self.m.load(block, Type::I128, aligned, "lq");
                    self.set128(block, rt, v);
                } else {
                    let v = self.reg128(block, rt);
                    self.m.store(block, Type::I128, v, aligned);
                }
            }
            // ldl, ldr, sdl, sdr: the doubleword merges of MIPS III, which the Allegrex lacks.
            26 | 27 if !self.r3000 && !self.allegrex => {
                let addr = self.address(block, w);
                self.unaligned_load(block, addr, rt, w.op() == 26, &Type::I64);
            }
            44 | 45 if !self.r3000 && !self.allegrex => {
                let addr = self.address(block, w);
                self.unaligned_store(block, addr, rt, w.op() == 44, &Type::I64);
            }
            // addi, daddi: the signed add that traps on overflow.
            8 | 24 => {
                let ty = if w.op() == 8 { Type::I32 } else { Type::I64 };
                let a = self.reg(block, rs, &ty);
                let b = self.m.value_of(block, ty.clone(), w.simm());
                let (block, r) = self.checked_sum(block, &ty, a, b, false, ip);
                self.set(block, rt, &ty, r);
                return block;
            }
            9 | 25 => {
                let ty = if w.op() == 9 { Type::I32 } else { Type::I64 };
                let a = self.reg(block, rs, &ty);
                let r = self.m.b.bin(block, BinOp::Add, ty.clone(), a, w.simm(), "sum");
                self.set(block, rt, &ty, r);
            }
            10 | 11 => {
                let a = self.reg64(block, rs);
                let cc = if w.op() == 10 { ICond::Slt } else { ICond::Ult };
                let c = self.m.b.icmp(block, cc, Type::I64, a, w.simm(), "lt");
                let r = self.m.b.cast(block, Cast::ZExt, Type::I1, c, Type::I64, "set");
                self.set64(block, rt, r);
            }
            12..=14 => {
                let op = [BinOp::And, BinOp::Or, BinOp::Xor][(w.op() - 12) as usize];
                let a = self.reg64(block, rs);
                let r = self.m.b.bin(block, op, Type::I64, a, w.uimm(), "logic");
                self.set64(block, rt, r);
            }
            15 => self.set64(block, rt, i64::from(((w.0 & 0xffff) << 16) as i32)),
            17 => return self.cop1(block, w, ip),
            32..=39 | 48 | 52 | 55 => self.load(block, w),
            40 | 41 | 43 | 56 | 60 | 63 => {
                let ty = match w.op() {
                    40 => Type::I8,
                    41 => Type::I16,
                    43 | 56 => Type::I32,
                    _ => Type::I64,
                };
                let addr = self.address(block, w);
                let v = self.reg64(block, rt);
                let v = self.m.trunc(block, Type::I64, v, ty.clone(), "st");
                self.guest_store(block, ty, v, addr);
                // sc, scd: one thread, so the conditional store always succeeds.
                if matches!(w.op(), 56 | 60) {
                    self.set64(block, rt, 1i64);
                }
            }
            42 | 46 => {
                let addr = self.address(block, w);
                self.unaligned_store(block, addr, rt, w.op() == 42, &Type::I32);
            }
            49 | 53 => {
                let addr = self.address(block, w);
                let double = w.op() == 53;
                let v = self.guest_load(block, float_types(double).0, addr, "ld");
                self.set_fpr(block, rt, double, v);
            }
            57 | 61 => {
                let addr = self.address(block, w);
                let double = w.op() == 61;
                let v = self.fpr(block, rt, double);
                self.guest_store(block, float_types(double).0, v, addr);
            }
            // pref: a hint.
            51 => {}
            _ => return self.unsupported(block, w, ip),
        }
        block
    }

    /// Coprocessor 0 of the VR4300, its registers the [`N64_COP0`] global: `mfc0` and `dmfc0` read
    /// one, `mtc0` and `dmtc0` write it. `Count` runs at half the processor's clock, so a read of it
    /// moves it on by [`COUNT_STEP`], the time a loop that polls it takes, and a wait on it ends. The
    /// TLB maps every address as itself, so its operations change nothing, and `eret` leaves as
    /// the return its block ends in.
    fn cop0(&mut self, block: BlockId, w: Word, ip: u64) -> BlockId {
        let (rt, rd) = (w.rt(), w.rd());
        match w.rs() {
            0 | 1 => {
                let p = self.cop0_ptr(block, rd);
                let mut v = self.m.b.load(block, Type::I64, p, "cop0");
                if rd == COUNT {
                    v = self.m.b.bin(block, BinOp::Add, Type::I64, v, COUNT_STEP, "count");
                    self.m.b.store(block, Type::I64, v, p);
                }
                if w.rs() == 0 {
                    let low = self.m.trunc(block, Type::I64, v, Type::I32, "cop0.w");
                    self.set32(block, rt, low);
                } else {
                    self.set64(block, rt, v);
                }
            }
            4 | 5 => {
                let v = if w.rs() == 4 {
                    let low = self.reg32(block, rt);
                    self.m.sext(block, Type::I32, low, Type::I64, "cop0.x")
                } else {
                    self.reg64(block, rt)
                };
                let p = self.cop0_ptr(block, rd);
                self.m.b.store(block, Type::I64, v, p);
            }
            // tlbr, tlbwi, tlbwr, tlbp, eret.
            16 if matches!(w.funct(), 1 | 2 | 6 | 8 | 0x18) => {}
            _ => return self.unsupported(block, w, ip),
        }
        block
    }

    fn cop0_ptr(&mut self, block: BlockId, n: u8) -> ValueId {
        let at = Operand::Address { global: N64_COP0.to_owned(), offset: i64::from(n) * 8 };
        self.m.b.cast(block, Cast::IntToPtr, Type::I64, at, Type::Ptr, "p.cop0")
    }

    fn load(&mut self, block: BlockId, w: Word) {
        let rt = w.rt();
        let addr = self.address(block, w);
        let (ty, signed) = match w.op() {
            32 => (Type::I8, true),
            36 => (Type::I8, false),
            33 => (Type::I16, true),
            37 => (Type::I16, false),
            35 | 48 => (Type::I32, true),
            39 => (Type::I32, false),
            52 | 55 => (Type::I64, true),
            op => return self.unaligned_load(block, addr, rt, op == 34, &Type::I32),
        };
        let v = self.guest_load(block, ty.clone(), addr, "ld");
        let r = if signed { self.m.sext(block, ty, v, Type::I64, "ld.x") } else { self.m.zext(block, ty, v, Type::I64, "ld.x") };
        self.set64(block, rt, r);
    }

    /// The offset of `addr` in its aligned word (`ty` wide) as a shift of `ty`, in bits, and the
    /// aligned address. A big-endian machine numbers the word's bytes from its high end, so its
    /// offset is counted from the other end: the merges below are then the little-endian ones over
    /// the word as [`Self::guest_load`] reads it.
    fn unaligned_parts(&mut self, block: BlockId, addr: ValueId, ty: &Type) -> (ValueId, ValueId) {
        let bytes: i64 = if *ty == Type::I32 { 4 } else { 8 };
        let k = self.m.b.bin(block, BinOp::And, Type::I32, addr, bytes - 1, "k");
        let k = if self.big { self.m.b.bin(block, BinOp::Xor, Type::I32, k, bytes - 1, "k.be") } else { k };
        let aligned = self.m.b.bin(block, BinOp::And, Type::I32, addr, -bytes, "aligned");
        let k = if bytes == 8 { self.m.b.cast(block, Cast::ZExt, Type::I32, k, Type::I64, "k.x") } else { k };
        (self.m.b.bin(block, BinOp::Shl, ty.clone(), k, 3i64, "bits"), aligned)
    }

    /// `lwl` (`left`) and `lwr` on a little-endian machine, or `ldl` and `ldr` at `ty` I64: the
    /// bytes of the aligned word from the address to its end become the register's high bytes,
    /// or from its start to the address its low bytes; the rest of the register stays.
    fn unaligned_load(&mut self, block: BlockId, addr: ValueId, rt: u8, left: bool, ty: &Type) {
        let top = if *ty == Type::I32 { 24i64 } else { 56 };
        let (bits, aligned) = self.unaligned_parts(block, addr, ty);
        let word = self.guest_load(block, ty.clone(), aligned, "word");
        let old = self.reg(block, rt, ty);
        let (part, keep) = if left {
            let s = self.m.b.bin(block, BinOp::Sub, ty.clone(), top, bits, "s");
            let part = self.m.b.bin(block, BinOp::Shl, ty.clone(), word, s, "part");
            let one = self.m.b.bin(block, BinOp::Shl, ty.clone(), 1i64, s, "one");
            (part, self.m.b.bin(block, BinOp::Sub, ty.clone(), one, 1i64, "keep"))
        } else {
            let part = self.m.b.bin(block, BinOp::LShr, ty.clone(), word, bits, "part");
            let mask = self.m.b.bin(block, BinOp::LShr, ty.clone(), -1i64, bits, "mask");
            (part, self.m.b.bin(block, BinOp::Xor, ty.clone(), mask, -1i64, "keep"))
        };
        let kept = self.m.b.bin(block, BinOp::And, ty.clone(), old, keep, "kept");
        let merged = self.m.b.bin(block, BinOp::Or, ty.clone(), part, kept, "merged");
        self.set(block, rt, ty, merged);
    }

    /// `swl` (`left`) and `swr` on a little-endian machine, or `sdl` and `sdr` at `ty` I64: the
    /// register's high bytes go to the aligned word from its start to the address, or its low
    /// bytes from the address to its end.
    fn unaligned_store(&mut self, block: BlockId, addr: ValueId, rt: u8, left: bool, ty: &Type) {
        let top = if *ty == Type::I32 { 24i64 } else { 56 };
        let (bits, aligned) = self.unaligned_parts(block, addr, ty);
        let word = self.guest_load(block, ty.clone(), aligned, "word");
        let value = self.reg(block, rt, ty);
        let (part, keep) = if left {
            let s = self.m.b.bin(block, BinOp::Sub, ty.clone(), top, bits, "s");
            let part = self.m.b.bin(block, BinOp::LShr, ty.clone(), value, s, "part");
            let mask = self.m.b.bin(block, BinOp::LShr, ty.clone(), -1i64, s, "mask");
            (part, self.m.b.bin(block, BinOp::Xor, ty.clone(), mask, -1i64, "keep"))
        } else {
            let part = self.m.b.bin(block, BinOp::Shl, ty.clone(), value, bits, "part");
            let one = self.m.b.bin(block, BinOp::Shl, ty.clone(), 1i64, bits, "one");
            (part, self.m.b.bin(block, BinOp::Sub, ty.clone(), one, 1i64, "keep"))
        };
        let kept = self.m.b.bin(block, BinOp::And, ty.clone(), word, keep, "kept");
        let merged = self.m.b.bin(block, BinOp::Or, ty.clone(), part, kept, "merged");
        self.guest_store(block, ty.clone(), merged, aligned);
    }

    /// The `SPECIAL` opcode: the register-to-register operations.
    fn special(&mut self, block: BlockId, w: Word, ip: u64) -> BlockId {
        let (rs, rt, rd, sa) = (w.rs(), w.rt(), w.rd(), w.sa());
        let funct = w.funct();
        let shift_op = || [BinOp::Shl, BinOp::Shl, BinOp::LShr, BinOp::AShr][(funct & 3) as usize];
        match funct {
            // rotr, rotrv, of the Allegrex and MIPS32 release 2: `srl` with `rs` (or `srlv` with
            // `sa`) at one.
            2 | 6 if self.allegrex && (if funct == 2 { rs } else { sa }) == 1 => {
                let v = self.reg32(block, rt);
                let amount: Operand = if funct == 2 { i64::from(sa).into() } else { self.reg_masked(block, rs, Type::I32, 31).into() };
                let r = self.m.intrinsic(
                    block,
                    "llvm.fshr.i32",
                    Type::I32,
                    vec![(Type::I32, v.into()), (Type::I32, v.into()), (Type::I32, amount)],
                    "rotr",
                );
                self.set32(block, rd, r);
            }
            // The other values of those fields are reserved.
            2 | 6 if self.allegrex && (if funct == 2 { rs } else { sa }) != 0 => return self.unsupported(block, w, ip),
            // clz, clo: the leading zeros, or ones, of the word; 32 for a word of none.
            22 | 23 if self.allegrex => {
                let mut v = self.reg32(block, rs);
                if funct == 23 {
                    v = self.m.b.bin(block, BinOp::Xor, Type::I32, v, -1i64, "ones");
                }
                let r = self.m.intrinsic(block, "llvm.ctlz.i32", Type::I32, vec![(Type::I32, v.into()), (Type::I1, 0i64.into())], "count");
                self.set32(block, rd, r);
            }
            // madd, maddu, msub, msubu: the 64-bit product of the words added to `hi:lo`, or
            // taken from it.
            28 | 29 | 46 | 47 if self.allegrex => self.multiply_accumulate(block, rs, rt, funct & 1 == 0, funct >= 46),
            // max, min: of the signed words.
            44 | 45 if self.allegrex => {
                let a = self.reg32(block, rs);
                let b = self.reg32(block, rt);
                let cc = if funct == 44 { ICond::Sgt } else { ICond::Slt };
                let c = self.m.b.icmp(block, cc, Type::I32, a, b, "pick");
                let r = self.m.select(block, c, Type::I32, a, b, if funct == 44 { "max" } else { "min" });
                self.set32(block, rd, r);
            }
            // The rest of MIPS III's doubleword functions are no Allegrex instruction.
            20 | 30 | 31 | 56..=63 if self.allegrex => return self.unsupported(block, w, ip),
            0 | 2 | 3 | 4 | 6 | 7 => {
                let v = self.reg32(block, rt);
                let amount: Operand = if funct < 4 { i64::from(sa).into() } else { self.reg_masked(block, rs, Type::I32, 31).into() };
                let r = self.m.b.bin(block, shift_op(), Type::I32, v, amount, "shift");
                self.set32(block, rd, r);
            }
            20 | 22 | 23 | 56 | 58 | 59 | 60 | 62 | 63 => {
                let v = self.reg64(block, rt);
                let amount: Operand = match funct {
                    20..=23 => self.reg_masked(block, rs, Type::I64, 63).into(),
                    56..=59 => i64::from(sa).into(),
                    _ => (i64::from(sa) + 32).into(),
                };
                let r = self.m.b.bin(block, shift_op(), Type::I64, v, amount, "shift");
                self.set64(block, rd, r);
            }
            // movf, movt: on an FPU condition code.
            1 => {
                let f = self.fcc(block, w.branch_cc());
                let c = if w.tests_true() { f } else { self.m.b.bin(block, BinOp::Xor, Type::I1, f, 1i64, "not") };
                self.move_if(block, c, rs, rd);
            }
            10 | 11 => {
                let t = self.reg64(block, rt);
                let cc = if funct == 10 { ICond::Eq } else { ICond::Ne };
                let c = self.m.b.icmp(block, cc, Type::I64, t, 0i64, "c");
                self.move_if(block, c, rs, rd);
            }
            // The EE kernel's calls take their number in `v1`: no o32 Linux call. The R5900 has
            // no doubleword multiply or divide.
            12 | 28..=31 if self.r5900 => return self.unsupported(block, w, ip),
            // syscall, by o32 Linux: the number in `v0`, the arguments from `a0`; the result in
            // `v0` and the error flag in `a3`, which a failure sets with the error number in `v0`.
            12 => {
                let args: Vec<(Type, Operand)> = [2, 4, 5, 6, 7].iter().map(|&n| (Type::I64, self.reg64(block, n).into())).collect();
                let raw = self
                    .m
                    .b
                    .call(block, crate::state::syscall_type(), Callee::Direct("ra_syscall".into()), args, "sys")
                    .expect("an i64 result");
                let failed = self.m.b.icmp(block, ICond::Uge, Type::I64, raw, -4095i64, "sys.failed");
                let magnitude = self.m.b.bin(block, BinOp::Sub, Type::I64, 0i64, raw, "sys.errno");
                let v0 = self.m.b.select(block, failed, Type::I64, magnitude, raw, "sys.v0");
                let a3 = self.m.b.cast(block, Cast::ZExt, Type::I1, failed, Type::I64, "sys.a3");
                self.set64(block, 2, v0);
                self.set64(block, 7, a3);
            }
            13 => {
                self.m.trap(block, ip, trap::INT3);
                return block;
            }
            // sync: one thread sees its memory in order.
            15 => {}
            16 | 18 => {
                let v = self.field(block, if funct == 16 { MIPS_HI } else { MIPS_LO });
                self.set64(block, rd, v);
            }
            17 | 19 => {
                let v = self.reg64(block, rs);
                self.set_field(block, if funct == 17 { MIPS_HI } else { MIPS_LO }, &Type::I64, v);
            }
            // The R5900's multiply also writes `lo` to `rd`.
            24 | 25 if self.r5900 => self.multiply(block, w, funct == 24, false, false),
            24 | 25 | 28 | 29 => {
                let (ty, wide, bits) = if funct < 28 { (Type::I32, Type::I64, 32) } else { (Type::I64, Type::I128, 64) };
                let cast = if funct & 1 == 0 { Cast::SExt } else { Cast::ZExt };
                let a = self.reg(block, rs, &ty);
                let b = self.reg(block, rt, &ty);
                let a = self.m.b.cast(block, cast, ty.clone(), a, wide.clone(), "a");
                let b = self.m.b.cast(block, cast, ty.clone(), b, wide.clone(), "b");
                let p = self.m.b.bin(block, BinOp::Mul, wide.clone(), a, b, "product");
                let lo = self.m.b.cast(block, Cast::Trunc, wide.clone(), p, ty.clone(), "lo");
                let high = self.m.b.bin(block, BinOp::LShr, wide.clone(), p, bits, "high");
                let hi = self.m.b.cast(block, Cast::Trunc, wide, high, ty.clone(), "hi");
                self.set_field(block, MIPS_LO, &ty, lo);
                self.set_field(block, MIPS_HI, &ty, hi);
            }
            26 | 27 | 30 | 31 => {
                let ty = if funct < 30 { Type::I32 } else { Type::I64 };
                return self.divide(block, w, ip, &ty, false);
            }
            32 | 34 | 44 | 46 => {
                let ty = if funct < 44 { Type::I32 } else { Type::I64 };
                let sub = funct & 2 != 0;
                let a = self.reg(block, rs, &ty);
                let b = self.reg(block, rt, &ty);
                let (block, r) = self.checked_sum(block, &ty, a, b, sub, ip);
                self.set(block, rd, &ty, r);
                return block;
            }
            33 | 35 | 45 | 47 => {
                let ty = if funct < 45 { Type::I32 } else { Type::I64 };
                let a = self.reg(block, rs, &ty);
                let b = self.reg(block, rt, &ty);
                let r = self.m.b.bin(block, if funct & 2 != 0 { BinOp::Sub } else { BinOp::Add }, ty.clone(), a, b, "sum");
                self.set(block, rd, &ty, r);
            }
            36..=39 => {
                let a = self.reg64(block, rs);
                let b = self.reg64(block, rt);
                let op = [BinOp::And, BinOp::Or, BinOp::Xor, BinOp::Or][(funct - 36) as usize];
                let mut r = self.m.b.bin(block, op, Type::I64, a, b, "logic");
                if funct == 39 {
                    r = self.m.b.bin(block, BinOp::Xor, Type::I64, r, -1i64, "nor");
                }
                self.set64(block, rd, r);
            }
            42 | 43 => {
                let a = self.reg64(block, rs);
                let b = self.reg64(block, rt);
                let cc = if funct == 42 { ICond::Slt } else { ICond::Ult };
                let c = self.m.b.icmp(block, cc, Type::I64, a, b, "lt");
                let r = self.m.b.cast(block, Cast::ZExt, Type::I1, c, Type::I64, "set");
                self.set64(block, rd, r);
            }
            48..=52 | 54 => {
                let a = self.reg64(block, rs);
                let b = self.reg64(block, rt);
                let cc = [ICond::Sge, ICond::Uge, ICond::Slt, ICond::Ult, ICond::Eq, ICond::Ne, ICond::Ne][(funct - 48) as usize];
                let c = self.m.b.icmp(block, cc, Type::I64, a, b, "trap");
                return self.trap_if(block, c, ip, trap::INT);
            }
            _ => return self.unsupported(block, w, ip),
        }
        block
    }

    /// `hi:lo ±= rs * rt`, the words' product at 64 bits, signed or not; each half lands
    /// sign-extended, as a 32-bit result does.
    fn multiply_accumulate(&mut self, block: BlockId, rs: u8, rt: u8, signed: bool, subtract: bool) {
        let cast = if signed { Cast::SExt } else { Cast::ZExt };
        let a = self.reg32(block, rs);
        let b = self.reg32(block, rt);
        let a = self.m.b.cast(block, cast, Type::I32, a, Type::I64, "a");
        let b = self.m.b.cast(block, cast, Type::I32, b, Type::I64, "b");
        let product = self.m.b.bin(block, BinOp::Mul, Type::I64, a, b, "product");
        let hi = self.field(block, MIPS_HI);
        let lo = self.field(block, MIPS_LO);
        let high = self.m.b.bin(block, BinOp::Shl, Type::I64, hi, 32i64, "acc.hi");
        let low = self.m.b.bin(block, BinOp::And, Type::I64, lo, 0xffff_ffff_i64, "acc.lo");
        let acc = self.m.b.bin(block, BinOp::Or, Type::I64, high, low, "acc");
        let acc = self.m.b.bin(block, if subtract { BinOp::Sub } else { BinOp::Add }, Type::I64, acc, product, "acc");
        let lo = self.m.b.cast(block, Cast::Trunc, Type::I64, acc, Type::I32, "lo");
        let high = self.m.b.bin(block, BinOp::LShr, Type::I64, acc, 32i64, "high");
        let hi = self.m.b.cast(block, Cast::Trunc, Type::I64, high, Type::I32, "hi");
        self.set_field(block, MIPS_LO, &Type::I32, lo);
        self.set_field(block, MIPS_HI, &Type::I32, hi);
    }

    /// The Allegrex's `SPECIAL3`: `ext` and `ins` on a range of bits, and the byte and bit
    /// shuffles of `BSHFL` (`wsbh`, `wsbw`, `seb`, `bitrev`, `seh`). A range past the word's
    /// end, and another shuffle, are unpredictable or reserved and trap.
    fn special3(&mut self, block: BlockId, w: Word, ip: u64) -> BlockId {
        let (rs, rt, rd, sa) = (w.rs(), w.rt(), w.rd(), u32::from(w.sa()));
        let msb = u32::from(rd);
        match w.funct() {
            0 if sa + msb < 32 => {
                let mask = (1u64 << (msb + 1)) - 1;
                let v = self.reg32(block, rs);
                let shifted = self.m.b.bin(block, BinOp::LShr, Type::I32, v, i64::from(sa), "ext.at");
                let r = self.m.b.bin(block, BinOp::And, Type::I32, shifted, i64::from(mask as u32 as i32), "ext");
                self.set32(block, rt, r);
            }
            4 if msb >= sa => {
                let mask = (((1u64 << (msb - sa + 1)) - 1) << sa) as u32;
                let from = self.reg32(block, rs);
                let old = self.reg32(block, rt);
                let moved = self.m.b.bin(block, BinOp::Shl, Type::I32, from, i64::from(sa), "ins.at");
                let field = self.m.b.bin(block, BinOp::And, Type::I32, moved, i64::from(mask as i32), "ins.field");
                let kept = self.m.b.bin(block, BinOp::And, Type::I32, old, i64::from(!mask as i32), "ins.kept");
                let r = self.m.b.bin(block, BinOp::Or, Type::I32, kept, field, "ins");
                self.set32(block, rt, r);
            }
            0x20 if rs == 0 => {
                let v = self.reg32(block, rt);
                let r = match sa {
                    0x02 => {
                        let up = self.m.b.bin(block, BinOp::Shl, Type::I32, v, 8i64, "wsbh.up");
                        let up = self.m.b.bin(block, BinOp::And, Type::I32, up, i64::from(0xff00_ff00_u32 as i32), "wsbh.hi");
                        let down = self.m.b.bin(block, BinOp::LShr, Type::I32, v, 8i64, "wsbh.down");
                        let down = self.m.b.bin(block, BinOp::And, Type::I32, down, 0x00ff_00ff_i64, "wsbh.lo");
                        self.m.b.bin(block, BinOp::Or, Type::I32, up, down, "wsbh")
                    }
                    0x03 => self.m.intrinsic(block, "llvm.bswap.i32", Type::I32, vec![(Type::I32, v.into())], "wsbw"),
                    0x10 | 0x18 => {
                        let narrow = if sa == 0x10 { Type::I8 } else { Type::I16 };
                        let part = self.m.b.cast(block, Cast::Trunc, Type::I32, v, narrow.clone(), "part");
                        self.m.b.cast(block, Cast::SExt, narrow, part, Type::I32, "sext")
                    }
                    // bitrev: the bits of each byte swapped in pairs, nibbles' pairs and nibbles,
                    // then the bytes.
                    0x14 => {
                        let mut v = v;
                        for (shift, mask) in [(1i64, 0x5555_5555_i64), (2, 0x3333_3333), (4, 0x0f0f_0f0f)] {
                            let down = self.m.b.bin(block, BinOp::LShr, Type::I32, v, shift, "bitrev.down");
                            let down = self.m.b.bin(block, BinOp::And, Type::I32, down, mask, "bitrev.lo");
                            let up = self.m.b.bin(block, BinOp::And, Type::I32, v, mask, "bitrev.kept");
                            let up = self.m.b.bin(block, BinOp::Shl, Type::I32, up, shift, "bitrev.up");
                            v = self.m.b.bin(block, BinOp::Or, Type::I32, up, down, "bitrev");
                        }
                        self.m.intrinsic(block, "llvm.bswap.i32", Type::I32, vec![(Type::I32, v.into())], "bitrev")
                    }
                    _ => return self.unsupported(block, w, ip),
                };
                self.set32(block, rd, r);
            }
            _ => return self.unsupported(block, w, ip),
        }
        block
    }

    /// The low bits of a register a variable shift takes its amount from.
    fn reg_masked(&mut self, block: BlockId, n: u8, ty: Type, mask: i64) -> ValueId {
        let v = self.reg(block, n, &ty);
        self.m.b.bin(block, BinOp::And, ty, v, mask, "amount")
    }

    /// `rd = c ? rs : rd`.
    fn move_if(&mut self, block: BlockId, c: ValueId, rs: u8, rd: u8) {
        let from = self.reg64(block, rs);
        let old = self.reg64(block, rd);
        let r = self.m.select(block, c, Type::I64, from, old, "moved");
        self.set64(block, rd, r);
    }

    /// `div` and `divu` (`ddiv` and `ddivu` at `ty` I64): the quotient to `lo` and the remainder
    /// to `hi`, or to the R5900's `lo1` and `hi1` with `second`.
    fn divide(&mut self, block: BlockId, w: Word, ip: u64, ty: &Type, second: bool) -> BlockId {
        let signed = w.funct() & 1 == 0;
        let a = self.reg(block, w.rs(), ty);
        let b = self.reg(block, w.rt(), ty);
        // The quotient of a division by zero, and of the most negative number by -1, is
        // unpredictable: the lift stops rather than pick one.
        let zero = self.m.b.icmp(block, ICond::Eq, ty.clone(), b, 0i64, "zero");
        let mut block = self.trap_if(block, zero, ip, trap::DIVIDE);
        if signed {
            let min = if *ty == Type::I32 { i64::from(i32::MIN) } else { i64::MIN };
            let is_min = self.m.b.icmp(block, ICond::Eq, ty.clone(), a, min, "min");
            let minus = self.m.b.icmp(block, ICond::Eq, ty.clone(), b, -1i64, "minus");
            let both = self.m.b.bin(block, BinOp::And, Type::I1, is_min, minus, "overflow");
            block = self.trap_if(block, both, ip, trap::DIVIDE_WIDE);
        }
        let (div, rem) = if signed { (BinOp::SDiv, BinOp::SRem) } else { (BinOp::UDiv, BinOp::URem) };
        let q = self.m.b.bin(block, div, ty.clone(), a, b, "quotient");
        let r = self.m.b.bin(block, rem, ty.clone(), a, b, "remainder");
        self.set_pair(block, second, ty, q, r);
        block
    }

    // endregion

    // region The R5900

    /// The upper half of the R5900's general register `n`, or past `$31` of `hi` (`hi1`) and `lo`
    /// (`lo1`); `$0`'s is zero.
    fn upper(&mut self, block: BlockId, n: u8) -> ValueId {
        if n == 0 {
            return self.m.value_of(block, Type::I64, 0i64);
        }
        let p = self.m.upper_ptr(block, u32::from(n));
        let name = format!("{}.u", self.m.gpr_name(u32::from(n)));
        self.m.b.load(block, Type::I64, p, &name)
    }

    fn set_upper(&mut self, block: BlockId, n: u8, value: impl Into<Operand>) {
        if n == 0 {
            return;
        }
        let p = self.m.upper_ptr(block, u32::from(n));
        self.m.b.store(block, Type::I64, value, p);
    }

    /// The 128 bits of register `n`.
    fn reg128(&mut self, block: BlockId, n: u8) -> ValueId {
        if n == 0 {
            return self.m.value_of(block, Type::I128, 0i64);
        }
        let low = self.reg64(block, n);
        let high = self.upper(block, n);
        let low = self.m.b.cast(block, Cast::ZExt, Type::I64, low, Type::I128, "q.lo");
        let high = self.m.b.cast(block, Cast::ZExt, Type::I64, high, Type::I128, "q.hi");
        let high = self.m.b.bin(block, BinOp::Shl, Type::I128, high, 64i64, "q.high");
        self.m.b.bin(block, BinOp::Or, Type::I128, high, low, "q")
    }

    /// Writes the 128 bits of register `n`; a write to `$0` is dropped.
    fn set128(&mut self, block: BlockId, n: u8, value: ValueId) {
        if n == 0 {
            return;
        }
        let low = self.m.b.cast(block, Cast::Trunc, Type::I128, value, Type::I64, "q.lo");
        let high = self.m.b.bin(block, BinOp::LShr, Type::I128, value, 64i64, "q.high");
        let high = self.m.b.cast(block, Cast::Trunc, Type::I128, high, Type::I64, "q.hi");
        self.set64(block, n, low);
        self.set_upper(block, n, high);
    }

    /// Register `n` as the vector `ty` of its 128 bits.
    fn vec(&mut self, block: BlockId, n: u8, ty: &Type) -> ValueId {
        let q = self.reg128(block, n);
        self.m.b.cast(block, Cast::Bitcast, Type::I128, q, ty.clone(), "v")
    }

    fn set_vec(&mut self, block: BlockId, n: u8, ty: &Type, value: ValueId) {
        if n == 0 {
            return;
        }
        let q = self.m.b.cast(block, Cast::Bitcast, ty.clone(), value, Type::I128, "q");
        self.set128(block, n, q);
    }

    /// Writes the `lo` and `hi` of a multiply or a divide: the first pair, or the R5900's second
    /// (`lo1`, `hi1`) with `second`; a 32-bit half is sign-extended into its register.
    fn set_pair(&mut self, block: BlockId, second: bool, ty: &Type, lo: ValueId, hi: ValueId) {
        if !second {
            self.set_field(block, MIPS_LO, ty, lo);
            self.set_field(block, MIPS_HI, ty, hi);
            return;
        }
        let lo = self.m.b.cast(block, Cast::SExt, Type::I32, lo, Type::I64, "lo1.x");
        let hi = self.m.b.cast(block, Cast::SExt, Type::I32, hi, Type::I64, "hi1.x");
        self.set_upper(block, MIPS_LO, lo);
        self.set_upper(block, MIPS_HI, hi);
    }

    /// The low word of `lo` or `hi` (`n`), of the first pair or the second.
    fn pair_word(&mut self, block: BlockId, second: bool, n: u8) -> ValueId {
        let whole = if second { self.upper(block, n) } else { self.reg64(block, n) };
        self.m.b.cast(block, Cast::Trunc, Type::I64, whole, Type::I32, "w")
    }

    /// The R5900's 32-bit multiplies: `rs * rt`, signed or not, into `lo` and `hi`, or the second
    /// pair with `second`; `accumulate` adds the product to the 64 bits the pair held (`madd`).
    /// Each half is sign-extended into its register, and `rd` takes the new `lo`.
    fn multiply(&mut self, block: BlockId, w: Word, signed: bool, accumulate: bool, second: bool) {
        let cast = if signed { Cast::SExt } else { Cast::ZExt };
        let a = self.reg32(block, w.rs());
        let b = self.reg32(block, w.rt());
        let a = self.m.b.cast(block, cast, Type::I32, a, Type::I64, "a");
        let b = self.m.b.cast(block, cast, Type::I32, b, Type::I64, "b");
        let mut p = self.m.b.bin(block, BinOp::Mul, Type::I64, a, b, "product");
        if accumulate {
            let lo = self.pair_word(block, second, MIPS_LO);
            let hi = self.pair_word(block, second, MIPS_HI);
            let lo = self.m.b.cast(block, Cast::ZExt, Type::I32, lo, Type::I64, "acc.lo");
            let hi = self.m.b.cast(block, Cast::ZExt, Type::I32, hi, Type::I64, "acc.hi");
            let high = self.m.b.bin(block, BinOp::Shl, Type::I64, hi, 32i64, "acc.high");
            let acc = self.m.b.bin(block, BinOp::Or, Type::I64, high, lo, "acc");
            p = self.m.b.bin(block, BinOp::Add, Type::I64, p, acc, "sum");
        }
        let lo = self.m.b.cast(block, Cast::Trunc, Type::I64, p, Type::I32, "lo");
        let high = self.m.b.bin(block, BinOp::LShr, Type::I64, p, 32i64, "high");
        let hi = self.m.b.cast(block, Cast::Trunc, Type::I64, high, Type::I32, "hi");
        self.set_pair(block, second, &Type::I32, lo, hi);
        self.set32(block, w.rd(), lo);
    }

    /// The `MMI` opcode: the multiplies of the second pipeline and the moves of `hi1` and `lo1`,
    /// the parallel shifts, and the four groups of the multimedia instructions on the 128-bit
    /// registers as vectors of words, halfwords or bytes. An operation the lift does not cover
    /// (the parallel multiplies and divides, `pmfhl`, `pmthl`, `qfsrv`, `plzcw`, `pext5`,
    /// `ppac5`, `padsbh`) traps with its address.
    fn mmi(&mut self, block: BlockId, w: Word, ip: u64) -> BlockId {
        let funct = w.funct();
        match funct {
            0x00 | 0x01 | 0x20 | 0x21 => self.multiply(block, w, funct & 1 == 0, true, funct >= 0x20),
            0x18 | 0x19 => self.multiply(block, w, funct == 0x18, false, true),
            0x1a | 0x1b => return self.divide(block, w, ip, &Type::I32, true),
            0x10 | 0x12 => {
                let v = self.upper(block, if funct == 0x10 { MIPS_HI } else { MIPS_LO });
                self.set64(block, w.rd(), v);
            }
            0x11 | 0x13 => {
                let v = self.reg64(block, w.rs());
                self.set_upper(block, if funct == 0x11 { MIPS_HI } else { MIPS_LO }, v);
            }
            // psllh, psrlh, psrah, psllw, psrlw, psraw: each lane by the shift amount, of which a
            // halfword takes the low four bits.
            0x34 | 0x36 | 0x37 | 0x3c | 0x3e | 0x3f => {
                let (ty, amount) = if funct < 0x3c {
                    (Type::vector(8, Type::I16), i64::from(w.sa() & 15))
                } else {
                    (Type::vector(4, Type::I32), i64::from(w.sa()))
                };
                let op = [BinOp::Shl, BinOp::Shl, BinOp::LShr, BinOp::AShr][(funct & 3) as usize];
                let v = self.vec(block, w.rt(), &ty);
                let r = self.m.b.bin(block, op, ty.clone(), v, amount, "pshift");
                self.set_vec(block, w.rd(), &ty, r);
            }
            0x08 => return self.mmi_lanes(block, w, ip, false),
            0x28 => return self.mmi_lanes(block, w, ip, true),
            0x09 => return self.mmi_whole(block, w, ip, false),
            0x29 => return self.mmi_whole(block, w, ip, true),
            _ => return self.unsupported(block, w, ip),
        }
        block
    }

    /// MMI0 and MMI1 (`one`), whose operation number in the shift-amount field says the lanes
    /// (bits 2 and 3: words, halfwords, bytes) and the operation: the sums, wrapping or saturated
    /// (signed in MMI0, unsigned in MMI1), the compares, the maxima and minima, the absolute
    /// values (saturated, as the EE's are), the extensions that interleave the low (MMI0) or the
    /// high (MMI1) halves of `rt` and `rs`, and the packs of their even lanes.
    fn mmi_lanes(&mut self, block: BlockId, w: Word, ip: u64, one: bool) -> BlockId {
        let sa = w.sa();
        let (count, bits) = match (sa >> 2) & 3 {
            0 => (4, 32),
            1 => (8, 16),
            2 => (16, 8),
            _ => return self.unsupported(block, w, ip),
        };
        let ty = Type::vector(count, Type::Int(bits));
        let a = self.vec(block, w.rs(), &ty);
        let b = self.vec(block, w.rt(), &ty);
        let half = count / 2;
        let r = match (one, sa >= 16, sa & 3) {
            (false, false, 0) => self.m.b.bin(block, BinOp::Add, ty.clone(), a, b, "padd"),
            (false, false, 1) => self.m.b.bin(block, BinOp::Sub, ty.clone(), a, b, "psub"),
            (false, false, 2) => self.lane_compare(block, ICond::Sgt, &ty, a, b),
            (false, false, 3) if bits != 8 => self.lane_pick(block, ICond::Sgt, &ty, a, b),
            (true, false, 1) if bits != 8 => self.lane_abs(block, &ty, b),
            (true, false, 2) => self.lane_compare(block, ICond::Eq, &ty, a, b),
            (true, false, 3) if bits != 8 => self.lane_pick(block, ICond::Slt, &ty, a, b),
            (_, true, 0 | 1) => self.lane_saturate(block, &ty, a, b, !one, sa & 1 == 1),
            (_, true, 2) => {
                let base = if one { half } else { 0 };
                let mask: Vec<i32> = (0..half).flat_map(|k| [(base + k) as i32, (count + base + k) as i32]).collect();
                self.shuffle(block, &ty, b, a, mask)
            }
            (false, true, 3) => {
                let mask: Vec<i32> = (0..count).map(|k| (2 * k) as i32).collect();
                self.shuffle(block, &ty, b, a, mask)
            }
            _ => return self.unsupported(block, w, ip),
        };
        self.set_vec(block, w.rd(), &ty, r);
        block
    }

    /// MMI2 and MMI3 (`three`): the moves of the 128-bit `hi` and `lo`, the logic on the whole
    /// register, the variable shifts of the even words, and the copies, interleaves and
    /// exchanges of lanes.
    fn mmi_whole(&mut self, block: BlockId, w: Word, ip: u64, three: bool) -> BlockId {
        let (rs, rt, rd) = (w.rs(), w.rt(), w.rd());
        let (w64, w32, w16) = (Type::vector(2, Type::I64), Type::vector(4, Type::I32), Type::vector(8, Type::I16));
        match (three, w.sa()) {
            (false, 2) => self.shift_words(block, w, BinOp::Shl),
            (false, 3) => self.shift_words(block, w, BinOp::LShr),
            (true, 3) => self.shift_words(block, w, BinOp::AShr),
            (false, 8 | 9) => {
                let v = self.reg128(block, if w.sa() == 8 { MIPS_HI } else { MIPS_LO });
                self.set128(block, rd, v);
            }
            (true, 8 | 9) => {
                let v = self.reg128(block, rs);
                self.set128(block, if w.sa() == 8 { MIPS_HI } else { MIPS_LO }, v);
            }
            (_, 18 | 19) => {
                let a = self.reg128(block, rs);
                let b = self.reg128(block, rt);
                let op = match (three, w.sa()) {
                    (false, 18) => BinOp::And,
                    (false, _) => BinOp::Xor,
                    _ => BinOp::Or,
                };
                let mut r = self.m.b.bin(block, op, Type::I128, a, b, "plogic");
                if three && w.sa() == 19 {
                    r = self.m.b.bin(block, BinOp::Xor, Type::I128, r, -1i64, "pnor");
                }
                self.set128(block, rd, r);
            }
            // pcpyld: rt's low doubleword, then rs's; pcpyud: rs's high doubleword, then rt's.
            (false, 14) => self.permute(block, rd, &w64, rt, rs, &[0, 2]),
            (true, 14) => self.permute(block, rd, &w64, rs, rt, &[1, 3]),
            // pinth: rt's low halfwords between rs's high ones; pinteh: the even halfwords of each.
            (false, 10) => self.permute(block, rd, &w16, rt, rs, &[0, 12, 1, 13, 2, 14, 3, 15]),
            (true, 10) => self.permute(block, rd, &w16, rt, rs, &[0, 8, 2, 10, 4, 12, 6, 14]),
            // pexeh, prevh, pexch, pcpyh: the halfwords of rt in each doubleword; pexew, prot3w,
            // pexcw: its words.
            (false, 26) => self.permute(block, rd, &w16, rt, rt, &[2, 1, 0, 3, 6, 5, 4, 7]),
            (false, 27) => self.permute(block, rd, &w16, rt, rt, &[3, 2, 1, 0, 7, 6, 5, 4]),
            (true, 26) => self.permute(block, rd, &w16, rt, rt, &[0, 2, 1, 3, 4, 6, 5, 7]),
            (true, 27) => self.permute(block, rd, &w16, rt, rt, &[0, 0, 0, 0, 4, 4, 4, 4]),
            (false, 30) => self.permute(block, rd, &w32, rt, rt, &[2, 1, 0, 3]),
            (false, 31) => self.permute(block, rd, &w32, rt, rt, &[1, 2, 0, 3]),
            (true, 30) => self.permute(block, rd, &w32, rt, rt, &[0, 2, 1, 3]),
            _ => return self.unsupported(block, w, ip),
        }
        block
    }

    /// The lanes `mask` picks of `a` and `b` taken together.
    fn shuffle(&mut self, block: BlockId, ty: &Type, a: ValueId, b: ValueId, mask: Vec<i32>) -> ValueId {
        self.m.b.value(block, Inst::ShuffleVector { ty: ty.clone(), a: a.into(), b: b.into(), mask }, "pshuf")
    }

    /// `rd` = the lanes `mask` picks of registers `first` and `second` as vectors `ty`.
    fn permute(&mut self, block: BlockId, rd: u8, ty: &Type, first: u8, second: u8, mask: &[i32]) {
        let a = self.vec(block, first, ty);
        let b = self.vec(block, second, ty);
        let r = self.shuffle(block, ty, a, b, mask.to_vec());
        self.set_vec(block, rd, ty, r);
    }

    /// All ones in each lane where `a cond b` holds.
    fn lane_compare(&mut self, block: BlockId, cond: ICond, ty: &Type, a: ValueId, b: ValueId) -> ValueId {
        let Type::Vector(n, _) = ty else { unreachable!("a lane compare on {ty}") };
        let c = self.m.b.icmp(block, cond, ty.clone(), a, b, "pc");
        self.m.b.cast(block, Cast::SExt, Type::vector(*n, Type::I1), c, ty.clone(), "pc.m")
    }

    /// `a` in each lane where `a cond b` holds, `b` elsewhere: the maxima and minima.
    fn lane_pick(&mut self, block: BlockId, cond: ICond, ty: &Type, a: ValueId, b: ValueId) -> ValueId {
        let c = self.m.b.icmp(block, cond, ty.clone(), a, b, "pc");
        self.m.select(block, c, ty.clone(), a, b, "ppick")
    }

    /// The absolute value of each lane, the most negative number's the most positive.
    fn lane_abs(&mut self, block: BlockId, ty: &Type, v: ValueId) -> ValueId {
        let bits = ty.bits().unwrap_or(0) / ty_lanes(ty);
        let (min, max) = (-(1i64 << (bits - 1)), (1i64 << (bits - 1)) - 1);
        let not = self.m.b.bin(block, BinOp::Xor, ty.clone(), v, -1i64, "pnot");
        let neg = self.m.b.bin(block, BinOp::Add, ty.clone(), not, 1i64, "pneg");
        let below = self.m.b.icmp(block, ICond::Slt, ty.clone(), v, 0i64, "pneg.c");
        let r = self.m.select(block, below, ty.clone(), neg, v, "pabs");
        let at_min = self.m.b.icmp(block, ICond::Eq, ty.clone(), v, min, "pmin.c");
        self.m.select(block, at_min, ty.clone(), max, r, "pabs.s")
    }

    /// Each lane's `a + b`, or `a - b` with `sub`, saturated to the lane's range, signed or not:
    /// the sum at twice the width, clamped.
    fn lane_saturate(&mut self, block: BlockId, ty: &Type, a: ValueId, b: ValueId, signed: bool, sub: bool) -> ValueId {
        let n = ty_lanes(ty);
        let bits = ty.bits().unwrap_or(0) / n;
        let wide = Type::vector(n, Type::Int((bits * 2) as u16));
        let cast = if signed { Cast::SExt } else { Cast::ZExt };
        let a = self.m.b.cast(block, cast, ty.clone(), a, wide.clone(), "sat.a");
        let b = self.m.b.cast(block, cast, ty.clone(), b, wide.clone(), "sat.b");
        let r = self.m.b.bin(block, if sub { BinOp::Sub } else { BinOp::Add }, wide.clone(), a, b, "sat");
        let (min, max) = if signed { (-(1i64 << (bits - 1)), (1i64 << (bits - 1)) - 1) } else { (0, (1i64 << bits) - 1) };
        let over = self.m.b.icmp(block, ICond::Sgt, wide.clone(), r, max, "sat.over");
        let r = self.m.select(block, over, wide.clone(), max, r, "sat.max");
        let under = self.m.b.icmp(block, ICond::Slt, wide.clone(), r, min, "sat.under");
        let r = self.m.select(block, under, wide.clone(), min, r, "sat.min");
        self.m.b.cast(block, Cast::Trunc, wide, r, ty.clone(), "sat.r")
    }

    /// `psllvw`, `psrlvw`, `psravw`: each even word of `rt` shifted by the low five bits of the
    /// same word of `rs`, its 32-bit result sign-extended into the doubleword.
    fn shift_words(&mut self, block: BlockId, w: Word, op: BinOp) {
        let (w32, pair) = (Type::vector(4, Type::I32), Type::vector(2, Type::I32));
        let v = self.vec(block, w.rt(), &w32);
        let s = self.vec(block, w.rs(), &w32);
        let v = self.shuffle(block, &w32, v, v, vec![0, 2]);
        let s = self.shuffle(block, &w32, s, s, vec![0, 2]);
        let s = self.m.b.bin(block, BinOp::And, pair.clone(), s, 31i64, "pamount");
        let r = self.m.b.bin(block, op, pair.clone(), v, s, "pshift");
        let w64 = Type::vector(2, Type::I64);
        let r = self.m.b.cast(block, Cast::SExt, pair, r, w64.clone(), "pshift.x");
        self.set_vec(block, w.rd(), &w64, r);
    }

    // endregion

    // region The GTE

    /// Coprocessor 2 of the R3000, the GTE: a command (bit 25 set) is a call on its runtime
    /// function with the GTE's registers and the command word; `mfc2`, `cfc2`, `mtc2`, `ctc2`,
    /// `lwc2` and `swc2` move a register.
    fn cop2(&mut self, block: BlockId, w: Word, ip: u64) -> BlockId {
        let (rt, rd) = (w.rt(), w.rd());
        match (w.op(), w.rs()) {
            (18, 16..=31) => {
                let Some(name) = crate::state::gte_command(w.funct()) else {
                    return self.unsupported(block, w, ip);
                };
                let p = self.m.field_ptr(block, MIPS_GTE, "p.gte");
                let args = vec![(Type::Ptr, p.into()), (Type::I64, i64::from(w.0).into())];
                self.m.b.call(block, crate::state::gte_type(), Callee::Direct(name), args, "");
            }
            (18, 0) => {
                let v = self.gte_data(block, rd);
                self.set32(block, rt, v);
            }
            (18, 2) => {
                let v = self.gte_slot(block, 32 + rd);
                self.set32(block, rt, v);
            }
            (18, 4) => {
                let v = self.reg32(block, rt);
                self.set_gte_data(block, rd, v);
            }
            (18, 6) => {
                let v = self.reg32(block, rt);
                self.set_gte_control(block, rd, v);
            }
            (50, _) => {
                let addr = self.address(block, w);
                let v = self.guest_load(block, Type::I32, addr, "ld");
                self.set_gte_data(block, rt, v);
            }
            (58, _) => {
                let addr = self.address(block, w);
                let v = self.gte_data(block, rt);
                self.guest_store(block, Type::I32, v, addr);
            }
            _ => return self.unsupported(block, w, ip),
        }
        block
    }

    fn gte_slot(&mut self, block: BlockId, slot: u8) -> ValueId {
        let p = self.m.gte_ptr(block, u32::from(slot));
        self.m.b.load(block, Type::I32, p, &format!("gte{slot}"))
    }

    fn set_gte_slot(&mut self, block: BlockId, slot: u8, value: impl Into<Operand>) {
        let p = self.m.gte_ptr(block, u32::from(slot));
        self.m.b.store(block, Type::I32, value, p);
    }

    /// Data register `n` as `mfc2` reads it. The field holds each register as it reads, but for
    /// those the GTE computes on the read: `SXYP` mirrors `SXY2`, `IRGB` and `ORGB` pack `IR1..3`
    /// saturated to five bits, `LZCR` counts the leading bits of `LZCS` that equal its sign.
    fn gte_data(&mut self, block: BlockId, n: u8) -> ValueId {
        match n {
            15 => self.gte_slot(block, 14),
            28 | 29 => {
                let mut packed = self.m.value_of(block, Type::I32, 0i64);
                for k in 0..3u8 {
                    let ir = self.gte_slot(block, 9 + k);
                    let s = self.m.b.bin(block, BinOp::AShr, Type::I32, ir, 7i64, "ir.s");
                    let low = self.m.b.icmp(block, ICond::Slt, Type::I32, s, 0i64, "ir.low");
                    let high = self.m.b.icmp(block, ICond::Sgt, Type::I32, s, 0x1fi64, "ir.high");
                    let c = self.m.select(block, high, Type::I32, 0x1fi64, s, "ir.top");
                    let c = self.m.select(block, low, Type::I32, 0i64, c, "ir.c");
                    let at = self.m.b.bin(block, BinOp::Shl, Type::I32, c, i64::from(5 * k), "ir.at");
                    packed = self.m.b.bin(block, BinOp::Or, Type::I32, packed, at, "orgb");
                }
                packed
            }
            31 => {
                let v = self.gte_slot(block, 30);
                let sign = self.m.b.bin(block, BinOp::AShr, Type::I32, v, 31i64, "lzcs.sign");
                let x = self.m.b.bin(block, BinOp::Xor, Type::I32, v, sign, "lzcs.x");
                let args = vec![(Type::I32, x.into()), (Type::I1, 0i64.into())];
                self.m.intrinsic(block, "llvm.ctlz.i32", Type::I32, args, "lzcr")
            }
            _ => self.gte_slot(block, n),
        }
    }

    /// Writes data register `n` as `mtc2` does: a 16-bit register keeps its half, sign- or
    /// zero-extended as it reads; `SXYP` pushes the screen-coordinate FIFO; `IRGB` sets `IR1..3`
    /// from its three five-bit fields; `ORGB` and `LZCR` are read-only.
    fn set_gte_data(&mut self, block: BlockId, n: u8, value: ValueId) {
        match n {
            1 | 3 | 5 | 8..=11 => {
                let half = self.m.b.cast(block, Cast::Trunc, Type::I32, value, Type::I16, "gte.h");
                let v = self.m.b.cast(block, Cast::SExt, Type::I16, half, Type::I32, "gte.x");
                self.set_gte_slot(block, n, v);
            }
            7 | 16..=19 => {
                let v = self.m.b.bin(block, BinOp::And, Type::I32, value, 0xffffi64, "gte.u");
                self.set_gte_slot(block, n, v);
            }
            15 => {
                let s1 = self.gte_slot(block, 13);
                let s2 = self.gte_slot(block, 14);
                self.set_gte_slot(block, 12, s1);
                self.set_gte_slot(block, 13, s2);
                self.set_gte_slot(block, 14, value);
            }
            28 => {
                for k in 0..3u8 {
                    let field = self.m.b.bin(block, BinOp::LShr, Type::I32, value, i64::from(5 * k), "irgb.f");
                    let field = self.m.b.bin(block, BinOp::And, Type::I32, field, 0x1fi64, "irgb.f");
                    let ir = self.m.b.bin(block, BinOp::Shl, Type::I32, field, 7i64, "irgb.ir");
                    self.set_gte_slot(block, 9 + k, ir);
                }
                self.set_gte_slot(block, 28, value);
            }
            29 | 31 => {}
            _ => self.set_gte_slot(block, n, value),
        }
    }

    /// Writes control register `n` as `ctc2` does: the 16-bit ones sign-extended as they read
    /// (`H` too, as the hardware reads it); `FLAG` keeps bits 12 to 30, with bit 31 the OR of its
    /// error bits.
    fn set_gte_control(&mut self, block: BlockId, n: u8, value: ValueId) {
        let v = match n {
            4 | 12 | 20 | 26 | 27 | 29 | 30 => {
                let half = self.m.b.cast(block, Cast::Trunc, Type::I32, value, Type::I16, "gte.h");
                self.m.b.cast(block, Cast::SExt, Type::I16, half, Type::I32, "gte.x")
            }
            31 => {
                let kept = self.m.b.bin(block, BinOp::And, Type::I32, value, 0x7fff_f000i64, "flag");
                let errors = self.m.b.bin(block, BinOp::And, Type::I32, value, 0x7f87_e000i64, "flag.err");
                let any = self.m.b.icmp(block, ICond::Ne, Type::I32, errors, 0i64, "flag.any");
                let top = self.m.select(block, any, Type::I32, i64::from(i32::MIN), 0i64, "flag.top");
                self.m.b.bin(block, BinOp::Or, Type::I32, kept, top, "flag")
            }
            _ => value,
        };
        self.set_gte_slot(block, 32 + n, v);
    }

    // endregion

    // region The FPU

    fn cop1(&mut self, block: BlockId, w: Word, ip: u64) -> BlockId {
        let (fmt, rt, fs) = (w.rs(), w.rt(), w.rd());
        match fmt {
            0 | 1 => {
                let v = self.fpr(block, fs, fmt == 1);
                if fmt == 0 { self.set32(block, rt, v) } else { self.set64(block, rt, v) }
            }
            4 => {
                let v = self.reg32(block, rt);
                self.set_fpr(block, fs, false, v);
            }
            5 => {
                let v = self.reg64(block, rt);
                self.set_fpr(block, fs, true, v);
            }
            // cfc1 of the FCSR: the field, with the condition codes in their bits.
            2 if fs == 31 => {
                let p = self.m.field_ptr(block, MIPS_FCSR, "p.fcsr");
                let v = self.m.b.load(block, Type::I32, p, "fcsr");
                let mut v = self.m.b.bin(block, BinOp::And, Type::I32, v, !FCC_BITS, "fcsr.rest");
                for cc in 0..8 {
                    let f = self.fcc(block, cc);
                    let f = self.m.b.cast(block, Cast::ZExt, Type::I1, f, Type::I32, "fcc.w");
                    let at = self.m.b.bin(block, BinOp::Shl, Type::I32, f, fcc_bit(cc), "fcc.at");
                    v = self.m.b.bin(block, BinOp::Or, Type::I32, v, at, "fcsr");
                }
                self.set32(block, rt, v);
            }
            6 if fs == 31 => {
                let v = self.reg32(block, rt);
                let p = self.m.field_ptr(block, MIPS_FCSR, "p.fcsr");
                self.m.b.store(block, Type::I32, v, p);
                for cc in 0..8 {
                    let at = self.m.b.bin(block, BinOp::LShr, Type::I32, v, fcc_bit(cc), "fcc.at");
                    let f = self.m.b.cast(block, Cast::Trunc, Type::I32, at, Type::I1, "fcc");
                    self.set_fcc(block, cc, f);
                }
            }
            16 | 17 => return self.fpu_arith(block, w, ip, fmt == 17),
            // cvt.s and cvt.d of a word (W) or a long (L).
            20 | 21 if matches!(w.funct(), 32 | 33) => {
                let long = fmt == 21;
                let bits = self.fpr(block, fs, long);
                let double = w.funct() == 33;
                let to = float_types(double).1;
                let v = self.m.b.cast(block, Cast::SIToFP, float_types(long).0, bits, to, "cvt");
                self.set_freg(block, w.sa(), double, v);
            }
            _ => return self.unsupported(block, w, ip),
        }
        block
    }

    /// The `S` and `D` formats: `fd = fs op ft`, the conversions, the moves and the compares.
    fn fpu_arith(&mut self, block: BlockId, w: Word, ip: u64, double: bool) -> BlockId {
        let (ft, fs, fd) = (w.rt(), w.rd(), w.sa());
        let (ity, fty) = float_types(double);
        let sign = if double { i64::MIN } else { i64::from(i32::MIN) };
        let suffix = if double { "f64" } else { "f32" };
        let funct = w.funct();
        match funct {
            0..=3 => {
                let op = [BinOp::FAdd, BinOp::FSub, BinOp::FMul, BinOp::FDiv][funct as usize];
                let a = self.freg(block, fs, double);
                let b = self.freg(block, ft, double);
                let r = self.m.b.bin(block, op, fty, a, b, "f");
                self.set_freg(block, fd, double, r);
            }
            4 => {
                let a = self.freg(block, fs, double);
                let r = self.m.intrinsic(block, &format!("llvm.sqrt.{suffix}"), fty.clone(), vec![(fty, a.into())], "sqrt");
                self.set_freg(block, fd, double, r);
            }
            // abs, mov, neg: on the bits, which keeps a NaN's payload as the FPU does.
            5..=7 => {
                let a = self.fpr(block, fs, double);
                let r = match funct {
                    5 => self.m.b.bin(block, BinOp::And, ity, a, !sign, "abs"),
                    6 => a,
                    _ => self.m.b.bin(block, BinOp::Xor, ity, a, sign, "neg"),
                };
                self.set_fpr(block, fd, double, r);
            }
            // round, trunc, ceil, floor to a long (8..=11) or a word; cvt.w and cvt.l round to nearest.
            8..=15 | 36 | 37 => {
                let a = self.freg(block, fs, double);
                let (mode, long) = if funct >= 36 { (0, funct == 37) } else { (funct & 3, funct < 12) };
                let r = self.float_to_int(block, a, double, mode, long);
                self.set_fpr(block, fd, long, r);
            }
            32 if double => {
                let a = self.freg(block, fs, true);
                let r = self.m.b.cast(block, Cast::FPTrunc, Type::Double, a, Type::Float, "cvt");
                self.set_freg(block, fd, false, r);
            }
            33 if !double => {
                let a = self.freg(block, fs, false);
                let r = self.m.b.cast(block, Cast::FPExt, Type::Float, a, Type::Double, "cvt");
                self.set_freg(block, fd, true, r);
            }
            17..=19 => {
                let c = if funct == 17 {
                    let f = self.fcc(block, w.branch_cc());
                    if w.tests_true() { f } else { self.m.b.bin(block, BinOp::Xor, Type::I1, f, 1i64, "not") }
                } else {
                    let t = self.reg64(block, ft);
                    let cc = if funct == 18 { ICond::Eq } else { ICond::Ne };
                    self.m.b.icmp(block, cc, Type::I64, t, 0i64, "c")
                };
                let from = self.fpr(block, fs, double);
                let old = self.fpr(block, fd, double);
                let r = self.m.select(block, c, ity, from, old, "moved");
                self.set_fpr(block, fd, double, r);
            }
            // c.cond.fmt: bit 0 of the condition takes unordered, bit 1 equal, bit 2 less; bit 3,
            // the signalling form, changes no result.
            48..=63 => {
                let a = self.freg(block, fs, double);
                let b = self.freg(block, ft, double);
                let mut r = self.m.value_of(block, Type::I1, 0i64);
                for (bit, cond) in [(1, FCond::Uno), (2, FCond::Oeq), (4, FCond::Olt)] {
                    if funct & bit != 0 {
                        let c = self.m.b.value(block, Inst::FCmp { cond, ty: fty.clone(), lhs: a.into(), rhs: b.into() }, cond.name());
                        r = self.m.b.bin(block, BinOp::Or, Type::I1, r, c, "fcc");
                    }
                }
                self.set_fcc(block, i64::from((w.0 >> 8) & 7), r);
            }
            _ => return self.unsupported(block, w, ip),
        }
        block
    }

    /// A float rounded by `mode` (to nearest, toward zero, up, down) to a word or a long; a value
    /// out of range or a NaN gives the FPU's invalid result, the largest positive integer.
    fn float_to_int(&mut self, block: BlockId, a: ValueId, double: bool, mode: u32, long: bool) -> ValueId {
        let fty = float_types(double).1;
        let suffix = if double { "f64" } else { "f32" };
        let rounding = ["roundeven", "trunc", "ceil", "floor"][mode as usize];
        let r = self.m.intrinsic(block, &format!("llvm.{rounding}.{suffix}"), fty.clone(), vec![(fty.clone(), a.into())], "rounded");
        let (ity, bits) = if long { (Type::I64, 64) } else { (Type::I32, 32) };
        let limit = 2f64.powi(bits - 1);
        let low = self.m.b.value(block, Inst::FCmp { cond: FCond::Oge, ty: fty.clone(), lhs: r.into(), rhs: Operand::Float(-limit) }, "ge");
        let high = self.m.b.value(block, Inst::FCmp { cond: FCond::Olt, ty: fty.clone(), lhs: r.into(), rhs: Operand::Float(limit) }, "lt");
        let fits = self.m.b.bin(block, BinOp::And, Type::I1, low, high, "fits");
        let v = self.m.intrinsic(block, &format!("llvm.fptosi.sat.i{bits}.{suffix}"), ity.clone(), vec![(fty, r.into())], "int");
        let invalid = if long { i64::MAX } else { i64::from(i32::MAX) };
        self.m.select(block, fits, ity, v, invalid, "cvt")
    }

    /// The R5900's FPU: the moves to and from the general registers and the control register as
    /// MIPS's; the `S` format's arithmetic, its conversions and its multiply-adds as calls on
    /// `ra_f32_ftz_*`; `mov`, `abs` and `neg` on the bits; the compares, `max` and `min` on the
    /// order of the values; and `cvt.s.w` of the `W` format.
    fn ee_cop1(&mut self, block: BlockId, w: Word, ip: u64) -> BlockId {
        let (fmt, ft, fs, fd, funct) = (w.rs(), w.rt(), w.rd(), w.sa(), w.funct());
        match (fmt, funct) {
            (0 | 2 | 4 | 6, _) => return self.cop1(block, w, ip),
            (16, 5..=7) => return self.fpu_arith(block, w, ip, false),
            (16, 0..=3) => {
                let r = self.ee_on(block, ["add", "sub", "mul", "div"][funct as usize], &[fs, ft]);
                self.set_fpr(block, fd, false, r);
            }
            // sqrt.s fd, ft and rsqrt.s fd, fs, ft: `fs / sqrt(ft)`.
            (16, 4) => {
                let r = self.ee_on(block, "sqrt", &[ft]);
                self.set_fpr(block, fd, false, r);
            }
            (16, 0x16) => {
                let r = self.ee_on(block, "rsqrt", &[fs, ft]);
                self.set_fpr(block, fd, false, r);
            }
            (16, 0x24) => {
                let r = self.ee_on(block, "cvt_w_s", &[fs]);
                self.set_fpr(block, fd, false, r);
            }
            (20, 0x20) => {
                let r = self.ee_on(block, "cvt_s_w", &[fs]);
                self.set_fpr(block, fd, false, r);
            }
            // adda, suba, mula: the accumulator takes `fs op ft`.
            (16, 0x18..=0x1a) => {
                let r = self.ee_on(block, ["add", "sub", "mul"][(funct - 0x18) as usize], &[fs, ft]);
                self.set_acc(block, r);
            }
            // madd, msub into `fd`, madda, msuba into the accumulator: the accumulator plus or
            // minus the product, which is rounded as `mul.s` rounds it.
            (16, 0x1c..=0x1f) => {
                let product = self.ee_on(block, "mul", &[fs, ft]);
                let acc = self.acc(block);
                let r = self.ee_fpu(block, if funct & 1 == 0 { "add" } else { "sub" }, vec![acc, product]);
                if funct < 0x1e {
                    self.set_fpr(block, fd, false, r);
                } else {
                    self.set_acc(block, r);
                }
            }
            // max, min: the operand itself, `fs` on a tie.
            (16, 0x28 | 0x29) => {
                let (a, b) = (self.fpr(block, fs, false), self.fpr(block, ft, false));
                let (x, y) = (self.ee_order(block, a), self.ee_order(block, b));
                let c = if funct == 0x28 {
                    self.m.b.icmp(block, ICond::Slt, Type::I32, x, y, "lt")
                } else {
                    self.m.b.icmp(block, ICond::Slt, Type::I32, y, x, "gt")
                };
                let r = self.m.select(block, c, Type::I32, b, a, if funct == 0x28 { "max" } else { "min" });
                self.set_fpr(block, fd, false, r);
            }
            // c.f, c.eq, c.lt, c.le: the one condition flag, no NaN to be unordered.
            (16, 0x30 | 0x32 | 0x34 | 0x36) => {
                let c = if funct == 0x30 {
                    self.m.value_of(block, Type::I1, 0i64)
                } else {
                    let (a, b) = (self.fpr(block, fs, false), self.fpr(block, ft, false));
                    let (x, y) = (self.ee_order(block, a), self.ee_order(block, b));
                    let cond = [ICond::Eq, ICond::Slt, ICond::Sle][((funct - 0x32) / 2) as usize];
                    self.m.b.icmp(block, cond, Type::I32, x, y, "fcc")
                };
                self.set_fcc(block, 0, c);
            }
            _ => return self.unsupported(block, w, ip),
        }
        block
    }

    /// `ra_f32_ftz_<name>` on the bits of `args`, the bits of its result.
    fn ee_fpu(&mut self, block: BlockId, name: &str, args: Vec<ValueId>) -> ValueId {
        let ty = crate::state::ee_fpu_type(args.len());
        let args = args.into_iter().map(|v| (Type::I32, Operand::from(v))).collect();
        self.m.b.call(block, ty, Callee::Direct(format!("ra_f32_ftz_{name}")), args, name).expect("an R5900 FPU function has a result")
    }

    /// `ra_f32_ftz_<name>` on the FPU registers `regs`.
    fn ee_on(&mut self, block: BlockId, name: &str, regs: &[u8]) -> ValueId {
        let args = regs.iter().map(|&n| self.fpr(block, n, false)).collect();
        self.ee_fpu(block, name, args)
    }

    /// A single's place in the order of the R5900's values as a signed word: its magnitude,
    /// negated when its sign is set, and zero's for a denormal, so both zeros are one value.
    fn ee_order(&mut self, block: BlockId, bits: ValueId) -> ValueId {
        let exponent = self.m.b.bin(block, BinOp::And, Type::I32, bits, 0x7f80_0000i64, "exp");
        let flushed = self.m.b.icmp(block, ICond::Eq, Type::I32, exponent, 0i64, "flushed");
        let magnitude = self.m.b.bin(block, BinOp::And, Type::I32, bits, 0x7fff_ffffi64, "mag");
        let magnitude = self.m.select(block, flushed, Type::I32, 0i64, magnitude, "mag");
        let negative = self.m.b.icmp(block, ICond::Slt, Type::I32, bits, 0i64, "neg");
        let minus = self.m.b.bin(block, BinOp::Sub, Type::I32, 0i64, magnitude, "minus");
        self.m.select(block, negative, Type::I32, minus, magnitude, "order")
    }

    fn acc(&mut self, block: BlockId) -> ValueId {
        let p = self.m.field_ptr(block, MIPS_ACC, "p.acc");
        self.m.b.load(block, Type::I32, p, "acc")
    }

    fn set_acc(&mut self, block: BlockId, value: ValueId) {
        let p = self.m.field_ptr(block, MIPS_ACC, "p.acc");
        self.m.b.store(block, Type::I32, value, p);
    }

    // endregion
}

/// The condition codes' bits in the FCSR: `cc0` at 23, `cc1` to `cc7` at 25 to 31.
const FCC_BITS: i64 = 0xfe80_0000_u32 as i32 as i64;

fn fcc_bit(cc: i64) -> i64 {
    if cc == 0 { 23 } else { 24 + cc }
}

/// The lanes of a vector type; one of a scalar.
fn ty_lanes(ty: &Type) -> u32 {
    if let Type::Vector(n, _) = ty { *n } else { 1 }
}

/// The integer and the float type of a single or a double.
fn float_types(double: bool) -> (Type, Type) {
    if double { (Type::I64, Type::Double) } else { (Type::I32, Type::Float) }
}

/// Builds the wrapper of an import for o32: the four words of `a0..a3` and the four the caller
/// left past their home slots on its stack go to the import as the adapter's eight integers, and
/// the word of its result lands in `v0`, sign-extended as a 32-bit result is.
pub fn import_wrapper(symbol: &str, x87: crate::x87::X87Unit, isa: Isa) -> ra_asm_ir::Function {
    let name = crate::import_wrapper_name(symbol, false);
    let mut b = FunctionBuilder::new(&name, FnType::new(Type::Void, vec![Type::Ptr]), &["state"]);
    b.set_linkage(ra_asm_ir::Linkage::Internal);
    let state = b.param(0);
    let block = b.block("entry");
    let mut m = Machine { b: &mut b, state, bits: 32, x87, isa };
    let mut args = Vec::with_capacity(8);
    for n in 4..8 {
        let p = m.gpr_ptr(block, n);
        let v = m.b.load(block, Type::I64, p, "arg");
        args.push((Type::I64, v.into()));
    }
    let p = m.gpr_ptr(block, 29);
    let sp = m.b.load(block, Type::I64, p, "sp");
    for slot in 4..8i64 {
        let addr = m.b.bin(block, BinOp::Add, Type::I64, sp, slot * 4, "arg.p");
        let addr = m.b.cast(block, Cast::Trunc, Type::I64, addr, Type::I32, "arg.w");
        let v = m.load(block, Type::I32, addr, "arg");
        let v = m.b.cast(block, Cast::SExt, Type::I32, v, Type::I64, "arg.x");
        args.push((Type::I64, v.into()));
    }
    let result = m.b.call(block, crate::state::import_adapter_type(), Callee::Direct(symbol.to_owned()), args, "ret");
    if let Some(result) = result {
        let word = m.b.cast(block, Cast::Trunc, Type::I64, result, Type::I32, "v0.w");
        let v0 = m.b.cast(block, Cast::SExt, Type::I32, word, Type::I64, "v0");
        let p = m.gpr_ptr(block, 2);
        m.b.store(block, Type::I64, v0, p);
    }
    b.terminate(block, Terminator::Ret(None));
    b.finish()
}
