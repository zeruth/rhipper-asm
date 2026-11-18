//! The micro mode of the `PlayStation 2`'s vector units as IR: a program of 64-bit pairs, each an
//! upper instruction (the FMAC: four lanes of float arithmetic, the accumulator, the flags) and a
//! lower one (the integer registers, the loads and stores, the branches, the divider, `xgkick`),
//! lifted to one function `void @name(ptr %vu, ptr %mem)` over the unit's state and its data
//! memory.
//!
//! The two halves of a pair issue together: each reads the registers as they were before the
//! pair, and when both write one register the upper's write is the one that stays. The lift reads
//! the upper's operands and computes its results, lifts the lower, then stores the upper's results
//! and flags, so the IR observes what the hardware does. A pair's results are seen by the next pair
//! in program order: the unit interlocks a read of a vector register on the write before it, and
//! the program waits on `Q` and `P` (`waitq`, `waitp`) before it reads them.
//!
//! The FMACs round as the R5900's FPU does, toward zero with no infinities and denormals as zero,
//! through the same `ra_f32_ftz_*` functions. The MAC flags hold the zero and sign of each lane the
//! last FMAC instruction wrote, and the status flags their summary and its sticky copy; the clip
//! flags shift in six bits per `clipw`. The `d` and `t` bits break only when the core enables them,
//! which a port's core never does, and are no-ops. An instruction the lift does not cover is a trap
//! that names its address.

use std::collections::{BTreeMap, BTreeSet};

use ra_asm_ir::{
    BinOp, BlockId, CallConv, Callee, Cast, Declaration, FnType, Function, FunctionBuilder, ICond, Module, Operand, Terminator, Type,
    ValueId,
};

/// The vector registers in the unit's state: `vf<n>` at `VF + 16 * n`, lane `x` first.
pub const VF: u64 = 0;
/// The accumulator, four lanes.
pub const ACC: u64 = 0x200;
/// The integer registers, 16 bits each: `vi<n>` at `VI + 2 * n`.
pub const VI: u64 = 0x210;
/// The divider's result.
pub const Q: u64 = 0x230;
/// The elementary function unit's result.
pub const P: u64 = 0x234;
/// The immediate register `loi` loads.
pub const I: u64 = 0x238;
/// The status flags: zero, sign, underflow and overflow of the last FMAC instruction in bits 0 to
/// 3, the divider's invalid and divide-by-zero in 4 and 5, the sticky copies of all six in 6 to 11.
pub const STATUS: u64 = 0x240;
/// The MAC flags of the last FMAC instruction: zero in bits 0 to 3, sign in 4 to 7, lane `x` in
/// the highest bit of each group.
pub const MAC: u64 = 0x244;
/// The clip flags, 24 bits: the last four `clipw` judgements, the latest in the low six bits.
pub const CLIP: u64 = 0x248;
/// VU1's `TOP` and `ITOP`, the double buffer's base the VIF sets, read by `xtop` and `xitop`.
pub const TOP: u64 = 0x24c;
pub const ITOP: u64 = 0x250;
/// The bytes of the state.
pub const STATE_SIZE: u64 = 0x260;

/// `void @ra_vu_xgkick(ptr packet)`: the GIF takes the packet at the address in the unit's data
/// memory.
pub const XGKICK: &str = "ra_vu_xgkick";

/// The lanes in the order of the state and the text.
const LANES: [&str; 4] = ["x", "y", "z", "w"];

/// Declares what a lifted VU program calls: the FPU's arithmetic, [`XGKICK`] and the trap.
pub fn declare(module: &mut Module) {
    crate::state::declare_ee_fpu(module);
    module.declare(Declaration { name: XGKICK.into(), ty: FnType::new(Type::Void, vec![Type::Ptr]), conv: CallConv::C, attrs: vec![] });
    module.declare(Declaration {
        name: "ra_unsupported".into(),
        ty: FnType::new(Type::Void, vec![Type::I64, Type::I64]),
        conv: CallConv::C,
        attrs: vec![],
    });
}

/// A lifted micro program.
pub struct Program {
    pub function: Function,
    /// The address of each pair the lift trapped on.
    pub traps: Vec<u64>,
}

/// Lifts the micro program in `code`, the unit's micro memory from address zero, entered at
/// `entry`, as the function `name`; `vu0` for VU0's smaller memories.
pub fn lift_program(name: &str, code: &[u8], entry: u64, vu0: bool) -> Program {
    let mask = if vu0 { 0xfff } else { 0x3fff };
    let fetch = |at: u64| {
        let at = usize::try_from(at).ok()?;
        Some(u64::from_le_bytes(code.get(at..at + 8)?.try_into().ok()?))
    };
    let leaders = leaders(entry, mask, &fetch);
    let mut b = FunctionBuilder::new(name, FnType::new(Type::Void, vec![Type::Ptr, Type::Ptr]), &["vu", "mem"]);
    let start = b.block("entry");
    let blocks: BTreeMap<u64, BlockId> = leaders.iter().map(|&a| (a, b.block(&format!("vu_{a:x}")))).collect();
    b.terminate(start, Terminator::Br(blocks[&entry]));
    let (vu, mem) = (b.param(0), b.param(1));
    let mut lift = Lift { b: &mut b, vu, mem, qwords: if vu0 { 0x100 } else { 0x400 }, traps: Vec::new() };
    for (&leader, &block) in &blocks {
        lift.block(leader, block, &blocks, &fetch);
    }
    let traps = lift.traps;
    Program { function: b.finish(), traps }
}

/// Where a pair sends control after its delay slot.
enum Next {
    Fall,
    Branch {
        target: u64,
        returns: bool,
    },
    Indirect {
        link: bool,
    },
    /// The `e` bit: the program ends after the next pair.
    End,
}

fn sign(value: u32, bits: u32) -> i64 {
    i64::from(((value << (32 - bits)) as i32) >> (32 - bits))
}

fn next_of(word: u64, ip: u64, mask: u64) -> Next {
    let (low, high) = (word as u32, (word >> 32) as u32);
    if high & (1 << 30) != 0 {
        return Next::End;
    }
    if high & (1 << 31) != 0 {
        return Next::Fall;
    }
    let target = ip.wrapping_add(8).wrapping_add((sign(low & 0x7ff, 11) * 8) as u64) & mask;
    match low >> 25 {
        0x20 => Next::Branch { target, returns: false },
        0x21 | 0x28 | 0x29 | 0x2c..=0x2f => Next::Branch { target, returns: true },
        0x24 => Next::Indirect { link: false },
        0x25 => Next::Indirect { link: true },
        _ => Next::Fall,
    }
}

/// The addresses that start a block: the entry, every branch's target, and the pair after a
/// conditional branch's or a call's delay slot, where control comes back.
fn leaders(entry: u64, mask: u64, fetch: &dyn Fn(u64) -> Option<u64>) -> BTreeSet<u64> {
    let mut leaders = BTreeSet::from([entry]);
    let mut work = vec![entry];
    let mut walked = BTreeSet::new();
    while let Some(start) = work.pop() {
        if !walked.insert(start) {
            continue;
        }
        let mut at = start;
        let mut add = |a: u64, work: &mut Vec<u64>| {
            if leaders.insert(a) {
                work.push(a);
            }
        };
        while let Some(word) = fetch(at) {
            match next_of(word, at, mask) {
                Next::Fall => at += 8,
                Next::End => break,
                Next::Branch { target, returns } => {
                    add(target, &mut work);
                    if returns {
                        add(at + 16, &mut work);
                    }
                    break;
                }
                Next::Indirect { link } => {
                    if link {
                        add(at + 16, &mut work);
                    }
                    break;
                }
            }
        }
    }
    leaders
}

/// What the lower instruction decides of control.
enum Control {
    None,
    /// A conditional branch's condition, an `i1`.
    Cond(ValueId),
    /// The byte address a `jr` or `jalr` goes to, an `i64`.
    Target(ValueId),
}

/// A write of the upper instruction, held until the lower one has run.
enum Pending {
    Store(u64, ValueId),
    /// The MAC flags of an FMAC instruction, and the status they summarize.
    Flags(ValueId),
}

#[derive(Clone, Copy)]
enum Arith {
    Add,
    Sub,
    Madd,
    Msub,
    Max,
    Mini,
    Mul,
    OpMul,
    OpMsub,
}

/// The third operand of an upper instruction.
#[derive(Clone, Copy)]
enum Src {
    Broadcast(usize),
    Q,
    I,
    Vector,
}

fn rt(w: u32) -> u32 {
    (w >> 16) & 0x1f
}

fn rs(w: u32) -> u32 {
    (w >> 11) & 0x1f
}

fn rd(w: u32) -> u32 {
    (w >> 6) & 0x1f
}

/// The lanes of the `dest` mask, `x` first.
fn dest(w: u32) -> impl Iterator<Item = usize> {
    (0..4).filter(move |l| (w >> 21) & (8 >> l) != 0)
}

fn vf_at(n: u32, l: usize) -> u64 {
    VF + 16 * u64::from(n) + 4 * l as u64
}

struct Lift<'b> {
    b: &'b mut FunctionBuilder,
    vu: ValueId,
    mem: ValueId,
    /// The data memory in quadwords, a power of two an address wraps within.
    qwords: i64,
    traps: Vec<u64>,
}

impl Lift<'_> {
    // region Blocks

    /// Lifts the pairs from `leader` to the next block or the end of the flow.
    fn block(&mut self, leader: u64, block: BlockId, blocks: &BTreeMap<u64, BlockId>, fetch: &dyn Fn(u64) -> Option<u64>) {
        let mask = (self.qwords as u64) * 16 - 1;
        let mut at = leader;
        loop {
            if at != leader
                && let Some(&next) = blocks.get(&at)
            {
                self.b.terminate(block, Terminator::Br(next));
                return;
            }
            let Some(word) = fetch(at) else {
                self.trap(block, at, 0);
                return;
            };
            let next = next_of(word, at, mask);
            let Some(control) = self.pair(block, word, at) else { return };
            if matches!(next, Next::Fall) {
                at += 8;
                continue;
            }
            // The delay slot runs before the transfer, and is no transfer itself.
            match fetch(at + 8) {
                Some(slot) if matches!(next_of(slot, at + 8, mask), Next::Fall) => {
                    if self.pair(block, slot, at + 8).is_none() {
                        return;
                    }
                }
                _ => {
                    self.trap(block, at + 8, 0);
                    return;
                }
            }
            let after = blocks.get(&(at + 16)).copied();
            match (next, control) {
                (Next::End, _) => self.b.terminate(block, Terminator::Ret(None)),
                (Next::Branch { target, .. }, Control::Cond(cond)) => {
                    let els = after.expect("a conditional branch's fall-through is a leader");
                    self.b.terminate(block, Terminator::CondBr { cond: cond.into(), then: blocks[&target], els });
                }
                (Next::Branch { target, .. }, _) => self.b.terminate(block, Terminator::Br(blocks[&target])),
                (Next::Indirect { .. }, Control::Target(to)) => {
                    // A target outside the program's pairs stops the run; the jump itself is lifted.
                    let wild = self.b.block(&format!("wild_{at:x}"));
                    self.stop(wild, at, 0);
                    let cases = blocks.iter().map(|(&a, &blk)| (i128::from(a), blk)).collect();
                    self.b.terminate(block, Terminator::Switch { ty: Type::I64, value: to.into(), default: wild, cases });
                }
                _ => self.trap(block, at, word),
            }
            return;
        }
    }

    /// `ra_unsupported(address, code)` and `unreachable`, for a pair the lift does not cover.
    fn trap(&mut self, block: BlockId, at: u64, code: u64) {
        self.traps.push(at);
        self.stop(block, at, code);
    }

    /// `ra_unsupported(address, code)` and `unreachable`.
    fn stop(&mut self, block: BlockId, at: u64, code: u64) {
        self.b.call(
            block,
            FnType::new(Type::Void, vec![Type::I64, Type::I64]),
            Callee::Direct("ra_unsupported".into()),
            vec![(Type::I64, at.into()), (Type::I64, code.into())],
            "",
        );
        self.b.terminate(block, Terminator::Unreachable);
    }

    /// One pair: the upper's results, then the lower, then the upper's writes. `None` after a trap.
    fn pair(&mut self, block: BlockId, word: u64, at: u64) -> Option<Control> {
        let (low, high) = (word as u32, (word >> 32) as u32);
        let Some(pending) = self.upper(block, high) else {
            self.trap(block, at, word);
            return None;
        };
        let control = if high & (1 << 31) != 0 {
            self.put(block, I, i64::from(low));
            Control::None
        } else {
            let Some(control) = self.lower(block, low, at) else {
                self.trap(block, at, word);
                return None;
            };
            control
        };
        for write in pending {
            match write {
                Pending::Store(offset, v) => self.put(block, offset, v),
                Pending::Flags(mac) => self.flags(block, mac),
            }
        }
        Some(control)
    }

    // endregion

    // region State

    fn at(&mut self, block: BlockId, offset: u64) -> ValueId {
        self.b.gep(block, Type::I8, self.vu, vec![(Type::I64, Operand::Int(i128::from(offset)))], "p")
    }

    fn get(&mut self, block: BlockId, offset: u64, hint: &str) -> ValueId {
        let p = self.at(block, offset);
        self.b.load(block, Type::I32, p, hint)
    }

    fn put(&mut self, block: BlockId, offset: u64, v: impl Into<Operand>) {
        let p = self.at(block, offset);
        self.b.store(block, Type::I32, v, p);
    }

    fn konst(&mut self, block: BlockId, ty: Type, v: i64) -> ValueId {
        self.b.bin(block, BinOp::Or, ty, v, 0i64, "k")
    }

    /// The bits of lane `l` of `vf<n>`; `vf00` reads `(0, 0, 0, 1)`.
    fn lane(&mut self, block: BlockId, n: u32, l: usize) -> ValueId {
        if n == 0 {
            return self.konst(block, Type::I32, if l == 3 { 0x3f80_0000 } else { 0 });
        }
        self.get(block, vf_at(n, l), &format!("vf{n:02}{}", LANES[l]))
    }

    /// Writes lane `l` of `vf<n>`; a write to `vf00` is dropped.
    fn set_lane(&mut self, block: BlockId, n: u32, l: usize, v: ValueId) {
        if n != 0 {
            self.put(block, vf_at(n, l), v);
        }
    }

    /// `vi<n>`, an `i16`; `vi00` reads zero.
    fn vi(&mut self, block: BlockId, n: u32) -> ValueId {
        if n == 0 {
            return self.konst(block, Type::I16, 0);
        }
        let p = self.at(block, VI + 2 * u64::from(n));
        self.b.load(block, Type::I16, p, &format!("vi{n:02}"))
    }

    fn set_vi(&mut self, block: BlockId, n: u32, v: impl Into<Operand>) {
        if n != 0 {
            let p = self.at(block, VI + 2 * u64::from(n));
            self.b.store(block, Type::I16, v, p);
        }
    }

    /// The pointer to the quadword `base + offset` of the data memory, wrapped within it.
    fn qword(&mut self, block: BlockId, base: ValueId, offset: i64) -> ValueId {
        let base = self.b.cast(block, Cast::ZExt, Type::I16, base, Type::I64, "qw");
        let a = self.b.bin(block, BinOp::Add, Type::I64, base, offset, "qw");
        let a = self.b.bin(block, BinOp::And, Type::I64, a, self.qwords - 1, "qw");
        let a = self.b.bin(block, BinOp::Shl, Type::I64, a, 4i64, "addr");
        self.b.gep(block, Type::I8, self.mem, vec![(Type::I64, a.into())], "pq")
    }

    fn lane_ptr(&mut self, block: BlockId, q: ValueId, l: usize) -> ValueId {
        self.b.gep(block, Type::I8, q, vec![(Type::I64, Operand::Int(4 * l as i128))], "pl")
    }

    // endregion

    // region Upper

    /// `ra_f32_ftz_<name>` on the bits of `args`.
    fn ftz(&mut self, block: BlockId, name: &str, args: &[ValueId]) -> ValueId {
        let ty = crate::state::ee_fpu_type(args.len());
        let args = args.iter().map(|&v| (Type::I32, Operand::from(v))).collect();
        self.b.call(block, ty, Callee::Direct(format!("ra_f32_ftz_{name}")), args, name).expect("an FPU function has a result")
    }

    /// A value's place in the unit's order as a signed word: its magnitude, negated when its sign
    /// is set, a denormal's as zero's.
    fn order(&mut self, block: BlockId, bits: ValueId) -> ValueId {
        let exponent = self.b.bin(block, BinOp::And, Type::I32, bits, 0x7f80_0000i64, "exp");
        let flushed = self.b.icmp(block, ICond::Eq, Type::I32, exponent, 0i64, "flushed");
        let magnitude = self.b.bin(block, BinOp::And, Type::I32, bits, 0x7fff_ffffi64, "mag");
        let magnitude = self.b.select(block, flushed, Type::I32, 0i64, magnitude, "mag");
        let negative = self.b.icmp(block, ICond::Slt, Type::I32, bits, 0i64, "neg");
        let minus = self.b.bin(block, BinOp::Sub, Type::I32, 0i64, magnitude, "minus");
        self.b.select(block, negative, Type::I32, minus, magnitude, "order")
    }

    /// The upper instruction `w`'s writes, its operands read; `None` for one the lift lacks.
    fn upper(&mut self, block: BlockId, w: u32) -> Option<Vec<Pending>> {
        use Arith::{Add, Madd, Max, Mini, Msub, Mul, OpMsub, Sub};
        let op = w & 0x3f;
        let (kind, src) = match op {
            0x00..=0x1b => ([Add, Sub, Madd, Msub, Max, Mini, Mul][(op >> 2) as usize], Src::Broadcast((w & 3) as usize)),
            0x1c => (Mul, Src::Q),
            0x1d => (Max, Src::I),
            0x1e => (Mul, Src::I),
            0x1f => (Mini, Src::I),
            0x20 => (Add, Src::Q),
            0x21 => (Madd, Src::Q),
            0x22 => (Add, Src::I),
            0x23 => (Madd, Src::I),
            0x24 => (Sub, Src::Q),
            0x25 => (Msub, Src::Q),
            0x26 => (Sub, Src::I),
            0x27 => (Msub, Src::I),
            0x28 => (Add, Src::Vector),
            0x29 => (Madd, Src::Vector),
            0x2a => (Mul, Src::Vector),
            0x2b => (Max, Src::Vector),
            0x2c => (Sub, Src::Vector),
            0x2d => (Msub, Src::Vector),
            0x2e => (OpMsub, Src::Vector),
            0x2f => (Mini, Src::Vector),
            0x3c..=0x3f => return self.special_upper(block, w),
            _ => return None,
        };
        self.arith(block, w, kind, src, false)
    }

    /// The upper instructions whose opcode goes on into the `fd` field.
    fn special_upper(&mut self, block: BlockId, w: u32) -> Option<Vec<Pending>> {
        use Arith::{Add, Madd, Msub, Mul, OpMul, Sub};
        let lane = (w & 3) as usize;
        let (kind, src) = match w & 0x7ff {
            0x03c..=0x03f => (Add, Src::Broadcast(lane)),
            0x07c..=0x07f => (Sub, Src::Broadcast(lane)),
            0x0bc..=0x0bf => (Madd, Src::Broadcast(lane)),
            0x0fc..=0x0ff => (Msub, Src::Broadcast(lane)),
            0x1bc..=0x1bf => (Mul, Src::Broadcast(lane)),
            0x13c..=0x13f => return Some(self.itof(block, w, lane)),
            0x17c..=0x17f => return Some(self.ftoi(block, w, lane)),
            0x1fc => (Mul, Src::Q),
            0x1fd => return Some(self.abs(block, w)),
            0x1fe => (Mul, Src::I),
            0x1ff => return Some(self.clip(block, w)),
            0x23c => (Add, Src::Q),
            0x23d => (Madd, Src::Q),
            0x23e => (Add, Src::I),
            0x23f => (Madd, Src::I),
            0x27c => (Sub, Src::Q),
            0x27d => (Msub, Src::Q),
            0x27e => (Sub, Src::I),
            0x27f => (Msub, Src::I),
            0x2bc => (Add, Src::Vector),
            0x2bd => (Madd, Src::Vector),
            0x2be => (Mul, Src::Vector),
            0x2fc => (Sub, Src::Vector),
            0x2fd => (Msub, Src::Vector),
            0x2fe => (OpMul, Src::Vector),
            0x2ff => return Some(Vec::new()),
            _ => return None,
        };
        self.arith(block, w, kind, src, true)
    }

    /// `fd` (or the accumulator) from `fs` and the source, lane by lane, and the flags of an FMAC
    /// instruction (`max` and `mini` set none).
    fn arith(&mut self, block: BlockId, width: u32, kind: Arith, src: Src, into_acc: bool) -> Option<Vec<Pending>> {
        let (fd, fs, ft) = (rd(width), rs(width), rt(width));
        let scalar = match src {
            Src::Broadcast(k) => Some(self.lane(block, ft, k)),
            Src::Q => Some(self.get(block, Q, "q")),
            Src::I => Some(self.get(block, I, "i")),
            Src::Vector => None,
        };
        let flags = !matches!(kind, Arith::Max | Arith::Mini);
        let mut out = Vec::new();
        let mut mac = self.konst(block, Type::I32, 0);
        for lane in dest(width) {
            let result = if matches!(kind, Arith::OpMul | Arith::OpMsub) {
                // The cross product's halves: `fs.yzx * ft.zxy`, of three lanes.
                let (left, right) = *[(1, 2), (2, 0), (0, 1)].get(lane)?;
                let (lhs, rhs) = (self.lane(block, fs, left), self.lane(block, ft, right));
                let product = self.ftz(block, "mul", &[lhs, rhs]);
                if matches!(kind, Arith::OpMul) {
                    product
                } else {
                    let acc = self.get(block, ACC + 4 * lane as u64, "acc");
                    self.ftz(block, "sub", &[acc, product])
                }
            } else {
                let lhs = self.lane(block, fs, lane);
                let rhs = match scalar {
                    Some(value) => value,
                    None => self.lane(block, ft, lane),
                };
                match kind {
                    Arith::Add => self.ftz(block, "add", &[lhs, rhs]),
                    Arith::Sub => self.ftz(block, "sub", &[lhs, rhs]),
                    Arith::Mul => self.ftz(block, "mul", &[lhs, rhs]),
                    Arith::Madd | Arith::Msub => {
                        let product = self.ftz(block, "mul", &[lhs, rhs]);
                        let acc = self.get(block, ACC + 4 * lane as u64, "acc");
                        self.ftz(block, if matches!(kind, Arith::Madd) { "add" } else { "sub" }, &[acc, product])
                    }
                    _ => {
                        let (lhs_order, rhs_order) = (self.order(block, lhs), self.order(block, rhs));
                        let rhs_wins = if matches!(kind, Arith::Max) {
                            self.b.icmp(block, ICond::Slt, Type::I32, lhs_order, rhs_order, "lt")
                        } else {
                            self.b.icmp(block, ICond::Slt, Type::I32, rhs_order, lhs_order, "gt")
                        };
                        self.b.select(block, rhs_wins, Type::I32, rhs, lhs, "pick")
                    }
                }
            };
            if into_acc {
                out.push(Pending::Store(ACC + 4 * lane as u64, result));
            } else if fd != 0 {
                out.push(Pending::Store(vf_at(fd, lane), result));
            }
            if flags {
                let exponent = self.b.bin(block, BinOp::And, Type::I32, result, 0x7f80_0000i64, "exp");
                let zero = self.b.icmp(block, ICond::Eq, Type::I32, exponent, 0i64, "zero");
                let zero = self.b.cast(block, Cast::ZExt, Type::I1, zero, Type::I32, "z");
                let zero = self.b.bin(block, BinOp::Shl, Type::I32, zero, 3 - lane as i64, "z");
                let negative = self.b.bin(block, BinOp::LShr, Type::I32, result, 31i64, "s");
                let negative = self.b.bin(block, BinOp::Shl, Type::I32, negative, 7 - lane as i64, "s");
                let both = self.b.bin(block, BinOp::Or, Type::I32, zero, negative, "zs");
                mac = self.b.bin(block, BinOp::Or, Type::I32, mac, both, "mac");
            }
        }
        if flags {
            out.push(Pending::Flags(mac));
        }
        Some(out)
    }

    /// Writes the MAC flags and the status flags they set: bits 0 and 1 the zero and sign of any
    /// lane, 6 and 7 their sticky copies; the divider's bits stay.
    fn flags(&mut self, block: BlockId, mac: ValueId) {
        self.put(block, MAC, mac);
        let zero = self.b.bin(block, BinOp::And, Type::I32, mac, 0xfi64, "z");
        let zero = self.b.icmp(block, ICond::Ne, Type::I32, zero, 0i64, "z");
        let zero = self.b.cast(block, Cast::ZExt, Type::I1, zero, Type::I32, "z");
        let negative = self.b.bin(block, BinOp::And, Type::I32, mac, 0xf0i64, "s");
        let negative = self.b.icmp(block, ICond::Ne, Type::I32, negative, 0i64, "s");
        let negative = self.b.cast(block, Cast::ZExt, Type::I1, negative, Type::I32, "s");
        let negative = self.b.bin(block, BinOp::Shl, Type::I32, negative, 1i64, "s");
        let now = self.b.bin(block, BinOp::Or, Type::I32, zero, negative, "now");
        let sticky = self.b.bin(block, BinOp::Shl, Type::I32, now, 6i64, "sticky");
        let status = self.get(block, STATUS, "status");
        let status = self.b.bin(block, BinOp::And, Type::I32, status, 0xff0i64, "status");
        let status = self.b.bin(block, BinOp::Or, Type::I32, status, now, "status");
        let status = self.b.bin(block, BinOp::Or, Type::I32, status, sticky, "status");
        self.put(block, STATUS, status);
    }

    /// `itof0`, `itof4`, `itof12`, `itof15`: `ft` from the fixed-point words of `fs`.
    fn itof(&mut self, block: BlockId, w: u32, form: usize) -> Vec<Pending> {
        let (ft, fs) = (rt(w), rs(w));
        let scale = [0x3f80_0000i64, 0x3d80_0000, 0x3980_0000, 0x3800_0000][form];
        let mut out = Vec::new();
        for l in dest(w) {
            let a = self.lane(block, fs, l);
            let mut r = self.ftz(block, "cvt_s_w", &[a]);
            if form != 0 {
                let k = self.konst(block, Type::I32, scale);
                r = self.ftz(block, "mul", &[r, k]);
            }
            if ft != 0 {
                out.push(Pending::Store(vf_at(ft, l), r));
            }
        }
        out
    }

    /// `ftoi0`, `ftoi4`, `ftoi12`, `ftoi15`: `ft` the fixed-point words of `fs`, truncated and
    /// clamped.
    fn ftoi(&mut self, block: BlockId, w: u32, form: usize) -> Vec<Pending> {
        let (ft, fs) = (rt(w), rs(w));
        let scale = [0x3f80_0000i64, 0x4180_0000, 0x4580_0000, 0x4700_0000][form];
        let mut out = Vec::new();
        for l in dest(w) {
            let mut a = self.lane(block, fs, l);
            if form != 0 {
                let k = self.konst(block, Type::I32, scale);
                a = self.ftz(block, "mul", &[a, k]);
            }
            let r = self.ftz(block, "cvt_w_s", &[a]);
            if ft != 0 {
                out.push(Pending::Store(vf_at(ft, l), r));
            }
        }
        out
    }

    fn abs(&mut self, block: BlockId, w: u32) -> Vec<Pending> {
        let (ft, fs) = (rt(w), rs(w));
        let mut out = Vec::new();
        for l in dest(w) {
            let a = self.lane(block, fs, l);
            let r = self.b.bin(block, BinOp::And, Type::I32, a, 0x7fff_ffffi64, "abs");
            if ft != 0 {
                out.push(Pending::Store(vf_at(ft, l), r));
            }
        }
        out
    }

    /// `clipw.xyz fs, ft.w`: six bits, each lane of `fs` above `+|w|` and below `-|w|`, shifted in.
    fn clip(&mut self, block: BlockId, w: u32) -> Vec<Pending> {
        let (ft, fs) = (rt(w), rs(w));
        let bound = self.lane(block, ft, 3);
        let bound = self.b.bin(block, BinOp::And, Type::I32, bound, 0x7fff_ffffi64, "w");
        let high = self.order(block, bound);
        let low = self.b.bin(block, BinOp::Sub, Type::I32, 0i64, high, "lo");
        let old = self.get(block, CLIP, "clip");
        let mut clip = self.b.bin(block, BinOp::Shl, Type::I32, old, 6i64, "clip");
        clip = self.b.bin(block, BinOp::And, Type::I32, clip, 0xff_ffffi64, "clip");
        for l in 0..3 {
            let v = self.lane(block, fs, l);
            let v = self.order(block, v);
            for (bit, cond, against) in [(2 * l, ICond::Sgt, high), (2 * l + 1, ICond::Slt, low)] {
                let c = self.b.icmp(block, cond, Type::I32, v, against, "out");
                let c = self.b.cast(block, Cast::ZExt, Type::I1, c, Type::I32, "out");
                let c = self.b.bin(block, BinOp::Shl, Type::I32, c, bit as i64, "out");
                clip = self.b.bin(block, BinOp::Or, Type::I32, clip, c, "clip");
            }
        }
        vec![Pending::Store(CLIP, clip)]
    }

    // endregion

    // region Lower

    /// The lower instruction `w` at `at`; `None` for one the lift lacks.
    fn lower(&mut self, block: BlockId, w: u32, at: u64) -> Option<Control> {
        let (it, is) = (rt(w), rs(w));
        let offset = sign(w & 0x7ff, 11);
        let imm15 = i64::from(((w >> 10) & 0x7800) | (w & 0x7ff));
        let imm12 = i64::from(((w >> 10) & 0x800) | (w & 0x7ff));
        let imm24 = i64::from(w & 0xff_ffff);
        match w >> 25 {
            // lq, sq
            0x00 => {
                let base = self.vi(block, is);
                let q = self.qword(block, base, offset);
                self.load_lanes(block, w, q, it);
            }
            0x01 => {
                let base = self.vi(block, it);
                let q = self.qword(block, base, offset);
                self.store_lanes(block, w, q, is);
            }
            // ilw, isw: one word of the quadword, the register's 16 bits.
            0x04 => {
                let base = self.vi(block, is);
                let q = self.qword(block, base, offset);
                self.load_integer(block, w, q, it)?;
            }
            0x05 => {
                let base = self.vi(block, is);
                let q = self.qword(block, base, offset);
                self.store_integer(block, w, q, it);
            }
            0x08 | 0x09 => {
                let a = self.vi(block, is);
                let op = if w >> 25 == 0x08 { BinOp::Add } else { BinOp::Sub };
                let r = self.b.bin(block, op, Type::I16, a, imm15, "vi");
                self.set_vi(block, it, r);
            }
            // fceq, fcand, fcor into vi01; fcset; fcget.
            0x10 | 0x12 | 0x13 => {
                let clip = self.get(block, CLIP, "clip");
                let c = match w >> 25 {
                    0x10 => {
                        let v = self.b.bin(block, BinOp::And, Type::I32, clip, 0xff_ffffi64, "clip");
                        self.b.icmp(block, ICond::Eq, Type::I32, v, imm24, "eq")
                    }
                    0x12 => {
                        let v = self.b.bin(block, BinOp::And, Type::I32, clip, imm24, "and");
                        self.b.icmp(block, ICond::Ne, Type::I32, v, 0i64, "any")
                    }
                    _ => {
                        let v = self.b.bin(block, BinOp::Or, Type::I32, clip, imm24, "or");
                        let v = self.b.bin(block, BinOp::And, Type::I32, v, 0xff_ffffi64, "or");
                        self.b.icmp(block, ICond::Eq, Type::I32, v, 0xff_ffffi64, "all")
                    }
                };
                let r = self.b.cast(block, Cast::ZExt, Type::I1, c, Type::I16, "vi01");
                self.set_vi(block, 1, r);
            }
            0x11 => self.put(block, CLIP, imm24),
            0x1c => {
                let clip = self.get(block, CLIP, "clip");
                let r = self.b.cast(block, Cast::Trunc, Type::I32, clip, Type::I16, "clip");
                let r = self.b.bin(block, BinOp::And, Type::I16, r, 0xfffi64, "clip");
                self.set_vi(block, it, r);
            }
            // fseq, fsand, fsor; fsset writes the sticky bits.
            0x14 | 0x16 | 0x17 => {
                let status = self.get(block, STATUS, "status");
                let status = self.b.bin(block, BinOp::And, Type::I32, status, 0xfffi64, "status");
                let r = match w >> 25 {
                    0x14 => {
                        let c = self.b.icmp(block, ICond::Eq, Type::I32, status, imm12, "eq");
                        self.b.cast(block, Cast::ZExt, Type::I1, c, Type::I32, "eq")
                    }
                    0x16 => self.b.bin(block, BinOp::And, Type::I32, status, imm12, "and"),
                    _ => {
                        let v = self.b.bin(block, BinOp::Or, Type::I32, status, imm12, "or");
                        let c = self.b.icmp(block, ICond::Eq, Type::I32, v, 0xfffi64, "all");
                        self.b.cast(block, Cast::ZExt, Type::I1, c, Type::I32, "all")
                    }
                };
                let r = self.b.cast(block, Cast::Trunc, Type::I32, r, Type::I16, "vi");
                self.set_vi(block, it, r);
            }
            0x15 => {
                let status = self.get(block, STATUS, "status");
                let status = self.b.bin(block, BinOp::And, Type::I32, status, 0x3fi64, "status");
                let status = self.b.bin(block, BinOp::Or, Type::I32, status, imm12 & 0xfc0, "status");
                self.put(block, STATUS, status);
            }
            // fmeq, fmand, fmor against vi[is].
            0x18 | 0x1a | 0x1b => {
                let mac = self.get(block, MAC, "mac");
                let mac = self.b.cast(block, Cast::Trunc, Type::I32, mac, Type::I16, "mac");
                let v = self.vi(block, is);
                let r = match w >> 25 {
                    0x18 => {
                        let c = self.b.icmp(block, ICond::Eq, Type::I16, mac, v, "eq");
                        self.b.cast(block, Cast::ZExt, Type::I1, c, Type::I16, "eq")
                    }
                    0x1a => self.b.bin(block, BinOp::And, Type::I16, mac, v, "and"),
                    _ => {
                        let o = self.b.bin(block, BinOp::Or, Type::I16, mac, v, "or");
                        let c = self.b.icmp(block, ICond::Eq, Type::I16, o, -1i64, "all");
                        self.b.cast(block, Cast::ZExt, Type::I1, c, Type::I16, "all")
                    }
                };
                self.set_vi(block, it, r);
            }
            0x20 => {}
            // bal: the link is the pair after the delay slot, in pairs.
            0x21 => self.set_vi(block, it, i64::try_from((at + 16) / 8).ok()?),
            0x24 | 0x25 => {
                let to = self.vi(block, is);
                let to = self.b.cast(block, Cast::ZExt, Type::I16, to, Type::I64, "to");
                let to = self.b.bin(block, BinOp::Shl, Type::I64, to, 3i64, "to");
                let mask = self.qwords * 16 - 1;
                let to = self.b.bin(block, BinOp::And, Type::I64, to, mask, "to");
                if w >> 25 == 0x25 {
                    self.set_vi(block, it, i64::try_from((at + 16) / 8).ok()?);
                }
                return Some(Control::Target(to));
            }
            0x28 | 0x29 => {
                let (a, b) = (self.vi(block, it), self.vi(block, is));
                let cond = if w >> 25 == 0x28 { ICond::Eq } else { ICond::Ne };
                return Some(Control::Cond(self.b.icmp(block, cond, Type::I16, a, b, "taken")));
            }
            0x2c..=0x2f => {
                let a = self.vi(block, is);
                let cond = [ICond::Slt, ICond::Sgt, ICond::Sle, ICond::Sge][((w >> 25) & 3) as usize];
                return Some(Control::Cond(self.b.icmp(block, cond, Type::I16, a, 0i64, "taken")));
            }
            0x40 => self.special_lower(block, w)?,
            _ => return None,
        }
        Some(Control::None)
    }

    /// The lower instructions of opcode `0x40`.
    fn special_lower(&mut self, block: BlockId, w: u32) -> Option<()> {
        let (it, is, id) = (rt(w), rs(w), rd(w));
        let op = match w & 0x3f {
            0x30 => BinOp::Add,
            0x31 => BinOp::Sub,
            0x34 => BinOp::And,
            0x35 => BinOp::Or,
            0x32 => {
                let a = self.vi(block, is);
                let r = self.b.bin(block, BinOp::Add, Type::I16, a, sign(id, 5), "vi");
                self.set_vi(block, it, r);
                return Some(());
            }
            0x3c..=0x3f => return self.extended_lower(block, w),
            _ => return None,
        };
        let (a, b) = (self.vi(block, is), self.vi(block, it));
        let r = self.b.bin(block, op, Type::I16, a, b, "vi");
        self.set_vi(block, id, r);
        Some(())
    }

    /// The lower instructions whose opcode goes on into the `id` field.
    fn extended_lower(&mut self, block: BlockId, w: u32) -> Option<()> {
        let (it, is) = (rt(w), rs(w));
        let (fsf, ftf) = (((w >> 21) & 3) as usize, ((w >> 23) & 3) as usize);
        match w & 0x7ff {
            // move, and mr32's lanes turned one toward `x`.
            0x33c | 0x33d => {
                let turn = usize::from(w & 0x7ff == 0x33d);
                let mut lanes = Vec::new();
                for l in dest(w) {
                    lanes.push((l, self.lane(block, is, (l + turn) % 4)));
                }
                for (l, v) in lanes {
                    self.set_lane(block, it, l, v);
                }
            }
            // lqi, lqd, sqi, sqd: the register steps after the access, or before it.
            0x37c..=0x37f => {
                let store = w & 1 != 0;
                let decrement = w & 2 != 0;
                let base_reg = if store { it } else { is };
                let mut base = self.vi(block, base_reg);
                if decrement {
                    base = self.b.bin(block, BinOp::Sub, Type::I16, base, 1i64, "vi");
                }
                let q = self.qword(block, base, 0);
                if store {
                    self.store_lanes(block, w, q, is);
                } else {
                    self.load_lanes(block, w, q, it);
                }
                let next = if decrement { base } else { self.b.bin(block, BinOp::Add, Type::I16, base, 1i64, "vi") };
                self.set_vi(block, base_reg, next);
            }
            0x3bc => self.divide(block, is, fsf, it, ftf),
            0x3bd => {
                let a = self.lane(block, it, ftf);
                let r = self.ftz(block, "sqrt", &[a]);
                self.put(block, Q, r);
                // A negative operand is invalid; the root is its magnitude's.
                let exponent = self.b.bin(block, BinOp::And, Type::I32, a, 0x7f80_0000i64, "exp");
                let nonzero = self.b.icmp(block, ICond::Ne, Type::I32, exponent, 0i64, "nonzero");
                let negative = self.b.icmp(block, ICond::Slt, Type::I32, a, 0i64, "neg");
                let invalid = self.b.bin(block, BinOp::And, Type::I1, nonzero, negative, "invalid");
                let zero = self.konst(block, Type::I1, 0);
                self.divider_flags(block, invalid, zero);
            }
            0x3bf | 0x7bf => {}
            0x3fc => {
                let v = self.lane(block, is, fsf);
                let v = self.b.cast(block, Cast::Trunc, Type::I32, v, Type::I16, "vi");
                self.set_vi(block, it, v);
            }
            0x3fd => {
                let v = self.vi(block, is);
                let v = self.b.cast(block, Cast::SExt, Type::I16, v, Type::I32, "word");
                for l in dest(w) {
                    self.set_lane(block, it, l, v);
                }
            }
            0x3fe => {
                let base = self.vi(block, is);
                let q = self.qword(block, base, 0);
                self.load_integer(block, w, q, it)?;
            }
            0x3ff => {
                let base = self.vi(block, is);
                let q = self.qword(block, base, 0);
                self.store_integer(block, w, q, it);
            }
            0x67c => {
                let p = self.get(block, P, "p");
                for l in dest(w) {
                    self.set_lane(block, it, l, p);
                }
            }
            0x6bc | 0x6bd => {
                let v = self.get(block, if w & 1 == 0 { TOP } else { ITOP }, "top");
                let v = self.b.cast(block, Cast::Trunc, Type::I32, v, Type::I16, "top");
                let v = self.b.bin(block, BinOp::And, Type::I16, v, 0x3ffi64, "top");
                self.set_vi(block, it, v);
            }
            0x6fc => {
                let base = self.vi(block, is);
                let q = self.qword(block, base, 0);
                self.b.call(
                    block,
                    FnType::new(Type::Void, vec![Type::Ptr]),
                    Callee::Direct(XGKICK.into()),
                    vec![(Type::Ptr, q.into())],
                    "",
                );
            }
            // esadd: `x * x + y * y + z * z`, in that order.
            0x73c => {
                let mut sum = None;
                for l in 0..3 {
                    let v = self.lane(block, is, l);
                    let square = self.ftz(block, "mul", &[v, v]);
                    sum = Some(match sum {
                        Some(s) => self.ftz(block, "add", &[s, square]),
                        None => square,
                    });
                }
                self.put(block, P, sum?);
            }
            _ => return None,
        }
        Some(())
    }

    /// `div q, fs.fsf, ft.ftf`: a zero divisor gives the largest magnitude of the operands' signs,
    /// and the divide-by-zero flag, or the invalid one when the dividend is zero too.
    fn divide(&mut self, block: BlockId, is: u32, fsf: usize, it: u32, ftf: usize) {
        let (a, b) = (self.lane(block, is, fsf), self.lane(block, it, ftf));
        let quotient = self.ftz(block, "div", &[a, b]);
        let ea = self.b.bin(block, BinOp::And, Type::I32, a, 0x7f80_0000i64, "exp");
        let eb = self.b.bin(block, BinOp::And, Type::I32, b, 0x7f80_0000i64, "exp");
        let a_zero = self.b.icmp(block, ICond::Eq, Type::I32, ea, 0i64, "zero");
        let b_zero = self.b.icmp(block, ICond::Eq, Type::I32, eb, 0i64, "zero");
        let signs = self.b.bin(block, BinOp::Xor, Type::I32, a, b, "sign");
        let signs = self.b.bin(block, BinOp::And, Type::I32, signs, i64::from(i32::MIN), "sign");
        let largest = self.b.bin(block, BinOp::Or, Type::I32, signs, 0x7fff_ffffi64, "largest");
        let q = self.b.select(block, b_zero, Type::I32, largest, quotient, "q");
        self.put(block, Q, q);
        let invalid = self.b.bin(block, BinOp::And, Type::I1, a_zero, b_zero, "invalid");
        let a_nonzero = self.b.icmp(block, ICond::Ne, Type::I32, ea, 0i64, "nonzero");
        let by_zero = self.b.bin(block, BinOp::And, Type::I1, b_zero, a_nonzero, "byzero");
        self.divider_flags(block, invalid, by_zero);
    }

    /// The divider's status bits: invalid (4) and divide-by-zero (5) as the last division set
    /// them, and their sticky copies (10, 11).
    fn divider_flags(&mut self, block: BlockId, invalid: ValueId, by_zero: ValueId) {
        let i = self.b.cast(block, Cast::ZExt, Type::I1, invalid, Type::I32, "i");
        let i = self.b.bin(block, BinOp::Shl, Type::I32, i, 4i64, "i");
        let d = self.b.cast(block, Cast::ZExt, Type::I1, by_zero, Type::I32, "d");
        let d = self.b.bin(block, BinOp::Shl, Type::I32, d, 5i64, "d");
        let now = self.b.bin(block, BinOp::Or, Type::I32, i, d, "now");
        let sticky = self.b.bin(block, BinOp::Shl, Type::I32, now, 6i64, "sticky");
        let status = self.get(block, STATUS, "status");
        let status = self.b.bin(block, BinOp::And, Type::I32, status, !0x30i64 & 0xfff, "status");
        let status = self.b.bin(block, BinOp::Or, Type::I32, status, now, "status");
        let status = self.b.bin(block, BinOp::Or, Type::I32, status, sticky, "status");
        self.put(block, STATUS, status);
    }

    /// The `dest` lanes of `vf<to>` from the quadword at `q`.
    fn load_lanes(&mut self, block: BlockId, w: u32, q: ValueId, to: u32) {
        for l in dest(w) {
            let p = self.lane_ptr(block, q, l);
            let v = self.b.load(block, Type::I32, p, "ld");
            self.set_lane(block, to, l, v);
        }
    }

    /// The `dest` lanes of `vf<from>` into the quadword at `q`.
    fn store_lanes(&mut self, block: BlockId, w: u32, q: ValueId, from: u32) {
        for l in dest(w) {
            let v = self.lane(block, from, l);
            let p = self.lane_ptr(block, q, l);
            self.b.store(block, Type::I32, v, p);
        }
    }

    /// `vi<to>` from the low half of the one `dest` lane of the quadword at `q`; `None` for a mask
    /// of any other count, which names no one word.
    fn load_integer(&mut self, block: BlockId, width: u32, quad: ValueId, to: u32) -> Option<()> {
        let mut lanes = dest(width);
        let (Some(lane), None) = (lanes.next(), lanes.next()) else { return None };
        let ptr = self.lane_ptr(block, quad, lane);
        let val = self.b.load(block, Type::I16, ptr, "ld");
        self.set_vi(block, to, val);
        Some(())
    }

    /// `vi<from>`, zero-extended, into the `dest` lanes of the quadword at `q`.
    fn store_integer(&mut self, block: BlockId, w: u32, q: ValueId, from: u32) {
        let v = self.vi(block, from);
        let v = self.b.cast(block, Cast::ZExt, Type::I16, v, Type::I32, "word");
        for l in dest(w) {
            let p = self.lane_ptr(block, q, l);
            self.b.store(block, Type::I32, v, p);
        }
    }

    // endregion
}
