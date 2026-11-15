//! The C++ exception frames of 32-bit x86: the `FuncInfo` a function's registration names and the
//! state the function is in at each of its instructions.
//!
//! The prologue links a registration `{next, handler, state}` into `fs:[0]`, whose handler is the
//! function's `__ehhandler$` stub: `mov eax, offset FuncInfo; jmp __CxxFrameHandler3`, after a
//! cookie check when the function has one. MSVC pushes the state and the handler after
//! `push ebp; mov ebp, esp`; clang writes them in place, under `ebp`, or under `esi` in a frame it
//! realigns. The `FuncInfo` is the x64 one with absolute addresses and no map from addresses to
//! states: the function says which state it is in by the constant it stores in the registration,
//! so the states here are read from those stores along the control flow, as the ranges of an SEH
//! scope are ([`crate::seh32`]).
//!
//! The frame handler runs the funclets with `ebp` 12 bytes past the registration: an unwind action
//! in the state its entry of the unwind map leaves to, a catch in the state past its try block's
//! body, and the continuation a catch returns in the state the catch left. A funclet finds the
//! registration through that `ebp`, so the frame registers are followed through each funclet as
//! offsets from the registration.
//!
//! The read is exact or it fails: a write of the state that is no constant, or no state of the
//! `FuncInfo`, a funclet that is no block of the function, or a catch that returns no
//! continuation makes the frame unread with the reason, never a frame of guesses. The state is
//! written through the frame registers only, as both compilers do.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail, ensure};
use ra_asm_arch::{Arch, Flow, Instr, Operand, Target};
use ra_asm_pe::{FuncInfo, Image};
use serde::Serialize;

use crate::function::{Block, Function};

/// The offset of the state in the registration.
const STATE: i64 = 8;

/// The offset from the registration of the `ebp` the frame handler runs a funclet with: where
/// MSVC's prologue leaves its own frame pointer.
const FUNCLET_FRAME: i64 = 12;

/// The state outside every object and try block.
const OUTSIDE: i32 = -1;

/// The states the function may be in over a range of its code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateRange {
    pub begin: u64,
    pub end: u64,
    /// More than one where paths in different states join, in ascending order.
    pub states: Vec<i32>,
}

/// The C++ exception frame of a 32-bit function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CxxFrame32 {
    /// The entry of the function that registers the frame.
    pub function: u64,
    /// The `__ehhandler$` stub the registration names.
    pub handler: u64,
    /// The tables the stub hands `__CxxFrameHandler3`.
    pub info: FuncInfo,
    /// The register the function's body addresses the registration through, and the offset of
    /// the registration from it.
    pub base: &'static str,
    pub registration: i64,
    /// The states of the code the flow reaches, the funclets' included, in address order: an
    /// instruction is in the states that hold before it runs.
    pub states: Vec<StateRange>,
}

impl CxxFrame32 {
    /// The states the function may be in at the instruction at `va`; empty for code the flow
    /// does not reach.
    pub fn states_at(&self, va: u64) -> &[i32] {
        let i = self.states.partition_point(|r| r.end <= va);
        match self.states.get(i) {
            Some(r) if r.begin <= va => &r.states,
            _ => &[],
        }
    }
}

/// The frames of `functions`, by entry, and the functions whose registration names a C++ handler
/// whose frame was not read, with the reason.
pub fn frames(image: &Image, arch: &dyn Arch, functions: &BTreeMap<u64, Function>) -> (BTreeMap<u64, CxxFrame32>, BTreeMap<u64, String>) {
    let (mut read, mut unread) = (BTreeMap::new(), BTreeMap::new());
    for f in functions.values() {
        match frame_of(image, arch, f) {
            Ok(Some(frame)) => {
                read.insert(f.entry, frame);
            }
            Ok(None) => {}
            Err(e) => {
                unread.insert(f.entry, format!("{e:#}"));
            }
        }
    }
    (read, unread)
}

/// The frame `f` registers: `None` when its prologue registers no C++ handler, an error when it
/// does and the tables or the function's use of the state cannot be read exactly.
pub fn frame_of(image: &Image, arch: &dyn Arch, f: &Function) -> Result<Option<CxxFrame32>> {
    let Some(registration) = registration(image, arch, f) else { return Ok(None) };
    let info = FuncInfo::parse(image, registration.info)?;
    let walk = Walk { f, registration: &registration, count: info.unwind_map.len() };
    let facts = walk.run(roots(f, &info)?)?;
    let mut states: Vec<StateRange> = Vec::new();
    for block in &f.blocks {
        let Some(mut fact) = facts.get(&block.start).cloned() else { continue };
        for instr in &block.instrs {
            let here: Vec<i32> = fact.states.iter().copied().collect();
            match states.last_mut() {
                Some(last) if last.end == instr.address && last.states == here => last.end = instr.end(),
                _ => states.push(StateRange { begin: instr.address, end: instr.end(), states: here }),
            }
            walk.step(instr, &mut fact)?;
        }
    }
    states.sort_by_key(|r| r.begin);
    Ok(Some(CxxFrame32 {
        function: f.entry,
        handler: registration.handler,
        info,
        base: registration.base,
        registration: registration.offset,
        states,
    }))
}

/// What a prologue stores of its registration.
struct Registration {
    /// The instruction that stores the handler: the frame register holds the registration's
    /// offset from it on.
    at: u64,
    base: &'static str,
    /// The offset of the registration from `base`.
    offset: i64,
    handler: u64,
    info: u64,
}

/// What holds at the start of a block.
#[derive(Debug, Clone)]
struct Fact {
    states: BTreeSet<i32>,
    /// The registers that hold an address of the frame, by their 32-bit name: the offset of the
    /// address from the registration.
    frame: BTreeMap<&'static str, i64>,
    /// Whether the block is code of a catch, whose return names the continuation.
    in_catch: bool,
}

impl Fact {
    /// What holds where the frame handler starts a funclet or a continuation.
    fn funclet(states: BTreeSet<i32>, in_catch: bool) -> Self {
        Self { states, frame: BTreeMap::from([("ebp", FUNCLET_FRAME)]), in_catch }
    }

    /// Joins `other` into this, which holds what either holds: whether that changed it.
    fn join(&mut self, other: &Fact) -> bool {
        let before = (self.states.len(), self.frame.len(), self.in_catch);
        self.states.extend(&other.states);
        self.frame.retain(|r, offset| other.frame.get(r) == Some(&*offset));
        self.in_catch |= other.in_catch;
        before != (self.states.len(), self.frame.len(), self.in_catch)
    }
}

/// Where the flow starts: the entry outside every state, and each funclet in the state the frame
/// handler runs it in.
fn roots(f: &Function, info: &FuncInfo) -> Result<Vec<(u64, Fact)>> {
    let mut roots = vec![(f.entry, Fact { states: BTreeSet::from([OUTSIDE]), frame: BTreeMap::new(), in_catch: false })];
    for entry in &info.unwind_map {
        if let Some(action) = entry.action {
            roots.push((action, Fact::funclet(BTreeSet::from([entry.to_state]), false)));
        }
    }
    for try_block in &info.try_blocks {
        for handler in &try_block.handlers {
            roots.push((handler.handler, Fact::funclet(BTreeSet::from([try_block.high + 1]), true)));
        }
    }
    for &(va, _) in &roots {
        ensure!(f.block_at(va).is_some(), "the funclet at {va:#x} is no block of the function at {:#x}", f.entry);
    }
    Ok(roots)
}

/// The flow of the states through one function.
struct Walk<'a> {
    f: &'a Function,
    registration: &'a Registration,
    /// The number of states the `FuncInfo` has.
    count: usize,
}

impl Walk<'_> {
    /// What holds at the start of each block the flow reaches from `roots`.
    fn run(&self, roots: Vec<(u64, Fact)>) -> Result<BTreeMap<u64, Fact>> {
        fn join(facts: &mut BTreeMap<u64, Fact>, work: &mut Vec<u64>, start: u64, fact: &Fact) {
            let changed = match facts.entry(start) {
                Entry::Vacant(e) => {
                    e.insert(fact.clone());
                    true
                }
                Entry::Occupied(mut e) => e.get_mut().join(fact),
            };
            if changed && !work.contains(&start) {
                work.push(start);
            }
        }

        let mut facts = BTreeMap::new();
        let mut work = Vec::new();
        for (start, fact) in roots {
            join(&mut facts, &mut work, start, &fact);
        }
        while let Some(start) = work.pop() {
            let Some(block) = self.f.block_at(start) else { continue };
            let mut fact = facts[&start].clone();
            for instr in &block.instrs {
                self.step(instr, &mut fact)?;
            }
            for next in block.successors() {
                if self.f.block_at(next).is_some() {
                    join(&mut facts, &mut work, next, &fact);
                }
            }
            if fact.in_catch && block.terminator.is_return() {
                let continuation = self.continuation(block)?;
                join(&mut facts, &mut work, continuation, &Fact::funclet(fact.states, false));
            }
        }
        Ok(facts)
    }

    /// Runs `instr` over `fact`.
    fn step(&self, instr: &Instr, fact: &mut Fact) -> Result<()> {
        if instr.address == self.registration.at {
            fact.frame.insert(self.registration.base, -self.registration.offset);
        }
        if let Some(state) = self.state_store(instr, fact)? {
            fact.states = BTreeSet::from([state]);
        }
        move_frame(instr, fact);
        Ok(())
    }

    /// The state `instr` stores, `None` when it does not write the state: `mov` of a constant, and
    /// the `and slot, 0` and `or slot, -1` a build for size writes 0 and -1 with.
    fn state_store(&self, instr: &Instr, fact: &Fact) -> Result<Option<i32>> {
        let Some(Operand::Mem(m)) = instr.operands.first() else { return Ok(None) };
        // A push or a call through the slot writes the stack, not the slot.
        if !instr.writes_memory || matches!(instr.mnemonic.as_str(), "push" | "call") || m.segment.is_some() || m.index.is_some() {
            return Ok(None);
        }
        let Some(&offset) = m.base.and_then(|b| fact.frame.get(family(b.name))) else { return Ok(None) };
        let at = offset + i64::from(m.disp as i32);
        if at >= STATE + 4 || at + i64::from(m.bits / 8) <= STATE {
            return Ok(None);
        }
        ensure!(at == STATE && m.bits == 32, "`{}` at {:#x} writes part of the state", instr.text, instr.address);
        let state = match (instr.mnemonic.as_str(), immediate(instr.operands.get(1))) {
            ("mov", Some(state)) => state,
            ("and", Some(0)) => 0,
            ("or", Some(-1)) => -1,
            _ => bail!("`{}` at {:#x} writes the state and no constant", instr.text, instr.address),
        };
        let known = usize::try_from(state).map_or(state == OUTSIDE, |s| s < self.count);
        ensure!(known, "`{}` at {:#x} stores the state {state}, and the FuncInfo has {}", instr.text, instr.address, self.count);
        Ok(Some(state))
    }

    /// The continuation a catch's returning block hands the frame handler: `mov eax, offset L`,
    /// the last write of `eax` before the return.
    fn continuation(&self, block: &Block) -> Result<u64> {
        let last = block.instrs.iter().rev().find(|i| i.registers.iter().any(|(r, a)| a.writes() && family(r.name) == "eax"));
        let target = match last.map(|i| (i.mnemonic.as_str(), immediate(i.operands.get(1)))) {
            Some(("mov", Some(target))) => u64::from(target as u32),
            _ => bail!("the catch block at {:#x} returns no continuation", block.start),
        };
        ensure!(self.f.block_at(target).is_some(), "the catch block at {:#x} returns {target:#x}, no block of the function", block.start);
        Ok(target)
    }
}

/// Follows the frame registers through `instr`: an `add` or a `sub` of a constant moves one, a
/// `lea` off one or a `mov` of one makes another, any other write ends one, and a call ends the
/// registers it does not preserve.
fn move_frame(instr: &Instr, fact: &mut Fact) {
    let moved = match (instr.mnemonic.as_str(), instr.operands.first(), instr.operands.get(1)) {
        (op @ ("add" | "sub"), Some(Operand::Reg(d)), Some(Operand::Imm { value, .. })) => {
            fact.frame.get(family(d.name)).map(|&o| (*d, if op == "add" { o + value } else { o - value }))
        }
        ("lea", Some(Operand::Reg(d)), Some(Operand::Mem(m))) if m.index.is_none() && m.segment.is_none() => {
            m.base.and_then(|b| fact.frame.get(family(b.name))).map(|&o| (*d, o + i64::from(m.disp as i32)))
        }
        ("mov", Some(Operand::Reg(d)), Some(Operand::Reg(s))) => fact.frame.get(family(s.name)).map(|&o| (*d, o)),
        _ => None,
    };
    for (r, access) in &instr.registers {
        if access.writes() {
            fact.frame.remove(family(r.name));
        }
    }
    if matches!(instr.flow, Flow::Call(_)) {
        fact.frame.retain(|r, _| matches!(*r, "ebx" | "esi" | "edi" | "ebp"));
    }
    if let Some((d, offset)) = moved
        && d.bits == 32
    {
        fact.frame.insert(family(d.name), offset);
    }
}

/// The 32-bit register a general register's name is part of.
fn family(name: &'static str) -> &'static str {
    match name {
        "ax" | "al" | "ah" => "eax",
        "cx" | "cl" | "ch" => "ecx",
        "dx" | "dl" | "dh" => "edx",
        "bx" | "bl" | "bh" => "ebx",
        "sp" | "spl" => "esp",
        "bp" | "bpl" => "ebp",
        "si" | "sil" => "esi",
        "di" | "dil" => "edi",
        other => other,
    }
}

/// The low 32 bits of an immediate, signed, which the decoder sign-extends.
fn immediate(operand: Option<&Operand>) -> Option<i32> {
    match operand {
        Some(Operand::Imm { value, .. }) => Some(*value as i32),
        _ => None,
    }
}

fn is_register(operand: Option<&Operand>, name: &str) -> bool {
    matches!(operand, Some(Operand::Reg(r)) if r.name == name)
}

/// A doubleword of the frame: `[base + offset]`, the offset signed, since the decoder gives a
/// 32-bit displacement under a register as its unsigned word.
fn frame_slot(operand: Option<&Operand>) -> Option<(&'static str, i64)> {
    match operand {
        Some(Operand::Mem(m)) if m.segment.is_none() && m.index.is_none() && m.bits == 32 => {
            let base = m.base.filter(|b| b.bits == 32 && b.name != "esp")?;
            Some((base.name, i64::from(m.disp as i32)))
        }
        _ => None,
    }
}

/// The registration the entry block stores before it reads the head of the chain at `fs:[0]`: a
/// C++ handler stub and the state outside every object, in adjacent slots.
fn registration(image: &Image, arch: &dyn Arch, f: &Function) -> Option<Registration> {
    let block = f.block_at(f.entry)?;
    let reads_chain = |i: &Instr| {
        i.mnemonic == "mov"
            && matches!(i.operands.get(1), Some(Operand::Mem(m))
                if m.segment.map(|s| s.name) == Some("fs") && m.base.is_none() && m.index.is_none() && m.disp == 0)
    };
    let n = block.instrs.iter().position(reads_chain)?;
    stored(image, arch, &block.instrs[n.saturating_sub(6)..n]).or_else(|| pushed(image, arch, &block.instrs[..n]))
}

/// clang's prologue: `mov [base + slot], offset stub` and `mov [base + slot + 4], -1`, which put
/// the registration at `slot - 4`.
fn stored(image: &Image, arch: &dyn Arch, window: &[Instr]) -> Option<Registration> {
    let stores: Vec<(u64, &'static str, i64, i32)> = window
        .iter()
        .filter(|i| i.mnemonic == "mov")
        .filter_map(|i| {
            let (base, slot) = frame_slot(i.operands.first())?;
            Some((i.address, base, slot, immediate(i.operands.get(1))?))
        })
        .collect();
    stores.iter().find_map(|&(at, base, slot, value)| {
        let handler = u64::from(value as u32);
        let info = cxx_stub(image, arch, handler)?;
        let outside = stores.iter().any(|&(_, b, s, v)| b == base && s == slot + 4 && v == OUTSIDE);
        outside.then_some(Registration { at, base, offset: slot - 4, handler, info })
    })
}

/// MSVC's prologue: `push ebp; mov ebp, esp; push -1; push offset stub`, which leaves the
/// registration 12 bytes under the frame pointer once the head of the chain is pushed.
fn pushed(image: &Image, arch: &dyn Arch, before: &[Instr]) -> Option<Registration> {
    let [frame, set, state, handler] = before.last_chunk::<4>()?;
    let pushes = |i: &Instr| if i.mnemonic == "push" { immediate(i.operands.first()) } else { None };
    let sets_frame = frame.mnemonic == "push"
        && is_register(frame.operands.first(), "ebp")
        && set.mnemonic == "mov"
        && is_register(set.operands.first(), "ebp")
        && is_register(set.operands.get(1), "esp");
    if !sets_frame || pushes(state)? != OUTSIDE {
        return None;
    }
    let stub = u64::from(pushes(handler)? as u32);
    let info = cxx_stub(image, arch, stub)?;
    Some(Registration { at: handler.address, base: "ebp", offset: -FUNCLET_FRAME, handler: stub, info })
}

/// The `FuncInfo` a C++ handler stub hands `__CxxFrameHandler3`: the `mov eax, offset FuncInfo`
/// on the straight way to its jump, past a cookie check.
fn cxx_stub(image: &Image, arch: &dyn Arch, stub: u64) -> Option<u64> {
    if !image.is_code(stub) {
        return None;
    }
    let (mut va, mut info) = (stub, None);
    for _ in 0..16 {
        let instr = arch.decode(image.read_to_section_end(va).ok()?, va)?;
        if instr.mnemonic == "mov"
            && is_register(instr.operands.first(), "eax")
            && let Some(at) = immediate(instr.operands.get(1))
            && FuncInfo::magic_at(image, u64::from(at as u32))
        {
            info = Some(u64::from(at as u32));
        }
        match instr.flow {
            Flow::Next | Flow::Call(_) => va = instr.end(),
            Flow::Jump(Target::Direct(_)) => return info,
            _ => return None,
        }
    }
    None
}
