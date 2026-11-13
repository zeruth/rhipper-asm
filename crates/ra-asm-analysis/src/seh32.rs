//! The structured exception frames of 32-bit x86: the scope table a `__try` leaves in the image
//! and the registration its function links into `fs:[0]`.
//!
//! The prologue stores the registration `{next, handler, scope table, try level}` under the frame
//! pointer: MSVC pushes the level, the table and the handler after `push ebp; mov ebp, esp`, clang
//! writes them in place. The table is an array of `{enclosing level, filter, handler}`, one entry
//! per `__try`, where a filter of zero marks a `__finally`; `_except_handler4`'s table puts the
//! four cookie offsets before the entries and calls the outermost level -2, `_except_handler3`'s
//! starts with the entries and calls it -1. No entry carries an address range, as the scope table
//! of the x64 unwind data does: the function says which scope is active by the constant it stores
//! in the try level, so the ranges here are read from those stores along the control flow.
//!
//! The read is exact or it fails: a table with an entry that is no scope, or a write of the try
//! level that is no constant, makes the frame unread with the reason, never a frame of guesses.
//! A prologue that calls `__SEH_prolog4` to register stores nothing itself and is not read here.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail, ensure};
use ra_asm_arch::{Instr, Operand};
use ra_asm_pe::Image;
use serde::Serialize;

use crate::function::Function;

/// The try level outside every scope, whichever handler's table spells it.
const OUTSIDE: u32 = u32::MAX;

/// More scopes than a function has: a try level past it is not one.
const MAX_SCOPES: u32 = 4096;

/// The size of a scope table entry and of the cookie offsets before `_except_handler4`'s entries.
const ENTRY_SIZE: u64 = 12;
const COOKIES_SIZE: u64 = 16;

/// Whose scope table a frame has: the layout of the table and the level outside every scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// `_except_handler3`: the entries alone, the outermost level -1. What clang emits.
    Handler3,
    /// `_except_handler4`: the cookie offsets, then the entries, the outermost level -2.
    Handler4,
}

impl Layout {
    /// The try level outside every scope, as the table and the prologue spell it.
    pub fn outermost(self) -> u32 {
        match self {
            Self::Handler3 => 0xffff_ffff,
            Self::Handler4 => 0xffff_fffe,
        }
    }
}

/// One `__try` of a function: an entry of its scope table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Scope {
    /// The index of the entry: the try level the function stores to enter the scope.
    pub index: u32,
    /// The scope this one is nested in, `None` for an outermost one.
    pub parent: Option<u32>,
    /// The filter of a `__try`/`__except`, `None` for a `__try`/`__finally`.
    pub filter: Option<u64>,
    /// The `__except` block, or the `__finally` funclet.
    pub handler: u64,
    /// The code the scope protects, in address order: where the try level may be this scope or
    /// one nested in it, the instruction that stores the next level included. A scope's ranges
    /// hold those of the scopes nested in it, as the ranges of the x64 scope table do.
    pub ranges: Vec<(u64, u64)>,
}

impl Scope {
    /// Whether the scope protects the instruction at `va`.
    pub fn protects(&self, va: u64) -> bool {
        self.ranges.iter().any(|&(begin, end)| va >= begin && va < end)
    }
}

/// The structured exception frame of a 32-bit function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SehFrame {
    /// The entry of the function that registers the frame.
    pub function: u64,
    /// The handler the registration names: `_except_handler3`, `_except_handler4`, or the stub
    /// of the image that calls it.
    pub handler: u64,
    pub layout: Layout,
    /// The address of the scope table.
    pub table: u64,
    /// The offset of the try level from `ebp`.
    pub level_slot: i64,
    /// The scopes, by index.
    pub scopes: Vec<Scope>,
}

/// The frames of `functions`, by entry, and the functions whose registration was found but not
/// read, with the reason.
pub fn frames(image: &Image, functions: &BTreeMap<u64, Function>) -> (BTreeMap<u64, SehFrame>, BTreeMap<u64, String>) {
    let (mut read, mut unread) = (BTreeMap::new(), BTreeMap::new());
    for f in functions.values() {
        match frame_of(image, f) {
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

/// The frame `f` registers: `None` when its prologue registers no scope table, an error when it
/// does and the table or the function's use of the try level cannot be read exactly.
pub fn frame_of(image: &Image, f: &Function) -> Result<Option<SehFrame>> {
    let Some(registration) = registration(image, f) else { return Ok(None) };
    let mut stores = BTreeMap::new();
    for instr in f.blocks.iter().flat_map(|b| &b.instrs) {
        if let Some(level) = level_store(instr, registration.level_slot)? {
            stores.insert(instr.address, level);
        }
    }
    // The table has no count: the function enters every scope by storing its index.
    let count = stores.values().filter(|&&level| level != OUTSIDE).max().map_or(0, |level| level + 1);
    ensure!(count <= MAX_SCOPES, "the function at {:#x} stores the try level {}", f.entry, count - 1);
    let (layout, mut scopes) = scope_table(image, &registration, count)?;

    let levels = block_levels(f, &scopes, &stores);
    for block in &f.blocks {
        let Some(mut state) = levels.get(&block.start).cloned() else { continue };
        for instr in &block.instrs {
            let active: BTreeSet<u32> =
                state.iter().filter(|&&level| level != OUTSIDE).flat_map(|&level| enclosing(&scopes, level)).collect();
            for index in active {
                let ranges = &mut scopes[index as usize].ranges;
                match ranges.last_mut() {
                    Some(last) if last.1 == instr.address => last.1 = instr.end(),
                    _ => ranges.push((instr.address, instr.end())),
                }
            }
            if let Some(&level) = stores.get(&instr.address) {
                state = BTreeSet::from([level]);
            }
        }
    }
    Ok(Some(SehFrame {
        function: f.entry,
        handler: registration.handler,
        layout,
        table: registration.table,
        level_slot: registration.level_slot,
        scopes,
    }))
}

/// What a prologue stores of its registration.
struct Registration {
    level: u32,
    table: u64,
    handler: u64,
    /// The offset of the try level from `ebp`.
    level_slot: i64,
}

/// The low 32 bits of an immediate, which the decoder sign-extends.
fn low32(value: i64) -> u32 {
    value as u32
}

/// The offset of a memory operand that is a doubleword of the frame: `[ebp + offset]`, signed,
/// since the decoder gives a 32-bit displacement under the frame pointer as its unsigned word.
fn frame_slot(operand: Option<&Operand>) -> Option<i64> {
    match operand {
        Some(Operand::Mem(m)) if m.segment.is_none() && m.index.is_none() && m.bits == 32 && m.base.is_some_and(|r| r.name == "ebp") => {
            Some(i64::from(m.disp as i32))
        }
        _ => None,
    }
}

fn immediate(operand: Option<&Operand>) -> Option<u32> {
    match operand {
        Some(Operand::Imm { value, .. }) => Some(low32(*value)),
        _ => None,
    }
}

fn is_register(operand: Option<&Operand>, name: &str) -> bool {
    matches!(operand, Some(Operand::Reg(r)) if r.name == name)
}

/// The registration the entry block stores before it reads the head of the chain at `fs:[0]`: a
/// level outside every scope, a table in data and a handler in code, in adjacent slots. A C++
/// frame stores a handler and a state and no table, and is none.
fn registration(image: &Image, f: &Function) -> Option<Registration> {
    let block = f.block_at(f.entry)?;
    let reads_chain = |i: &Instr| {
        i.mnemonic == "mov"
            && matches!(i.operands.get(1), Some(Operand::Mem(m))
                if m.segment.map(|s| s.name) == Some("fs") && m.base.is_none() && m.index.is_none() && m.disp == 0)
    };
    let n = block.instrs.iter().position(reads_chain)?;
    let before = &block.instrs[..n];
    let fits = |r: &Registration| {
        r.level >= Layout::Handler4.outermost()
            && image.is_code(r.handler)
            && image.section_of(r.table).is_some()
            && !image.is_code(r.table)
    };
    pushed(before).filter(&fits).or_else(|| stored(&before[n.saturating_sub(6)..], fits))
}

/// MSVC's prologue: `push ebp; mov ebp, esp; push level; push table; push handler`, which leaves
/// the level in the first slot under the frame pointer.
fn pushed(before: &[Instr]) -> Option<Registration> {
    let [frame, set, level, table, handler] = before.last_chunk::<5>()?;
    let pushes = |i: &Instr| if i.mnemonic == "push" { immediate(i.operands.first()) } else { None };
    let sets_frame = frame.mnemonic == "push"
        && is_register(frame.operands.first(), "ebp")
        && set.mnemonic == "mov"
        && is_register(set.operands.first(), "ebp")
        && is_register(set.operands.get(1), "esp");
    if !sets_frame {
        return None;
    }
    Some(Registration { level: pushes(level)?, table: u64::from(pushes(table)?), handler: u64::from(pushes(handler)?), level_slot: -4 })
}

/// clang's prologue: the three fields written in place with `mov [ebp + slot], constant`, the
/// handler one slot under the table and the level one over it.
fn stored(window: &[Instr], fits: impl Fn(&Registration) -> bool) -> Option<Registration> {
    let stores: Vec<(i64, u32)> = window
        .iter()
        .filter(|i| i.mnemonic == "mov")
        .filter_map(|i| Some((frame_slot(i.operands.first())?, immediate(i.operands.get(1))?)))
        .collect();
    let at = |slot: i64| stores.iter().find(|&&(s, _)| s == slot).map(|&(_, value)| value);
    stores.iter().find_map(|&(slot, table)| {
        let r = Registration { level: at(slot + 4)?, table: u64::from(table), handler: u64::from(at(slot - 4)?), level_slot: slot + 4 };
        fits(&r).then_some(r)
    })
}

/// The try level `instr` leaves in the slot, `None` when it does not write the slot: `mov` of a
/// constant, and the `and slot, 0` and `or slot, -1` a build for size writes 0 and -1 with.
fn level_store(instr: &Instr, slot: i64) -> Result<Option<u32>> {
    // A push or a call through the slot writes the stack, not the slot.
    if frame_slot(instr.operands.first()) != Some(slot) || !instr.writes_memory || matches!(instr.mnemonic.as_str(), "push" | "call") {
        return Ok(None);
    }
    let level = match (instr.mnemonic.as_str(), immediate(instr.operands.get(1))) {
        ("mov", Some(level)) => level,
        ("and", Some(0)) => 0,
        ("or", Some(0xffff_ffff)) => 0xffff_ffff,
        _ => bail!("`{}` at {:#x} writes the try level and no constant", instr.text, instr.address),
    };
    Ok(Some(if level >= Layout::Handler4.outermost() { OUTSIDE } else { level }))
}

/// The first `count` entries of the registration's table, without their ranges. The first
/// doubleword tells the layouts apart: the enclosing level of `_except_handler3`'s first entry is
/// always -1, the cookie offset `_except_handler4`'s table starts with is -2 or an offset under
/// the frame pointer.
fn scope_table(image: &Image, registration: &Registration, count: u32) -> Result<(Layout, Vec<Scope>)> {
    let table = registration.table;
    let first = image.u32(table)?;
    let (layout, entries) = if first == Layout::Handler3.outermost() {
        (Layout::Handler3, table)
    } else {
        ensure!(first >= 0x8000_0000 && image.u32(table + 8)? >= 0x8000_0000, "the table at {table:#x} starts with no cookie offsets");
        (Layout::Handler4, table + COOKIES_SIZE)
    };
    let outermost = layout.outermost();
    ensure!(registration.level == outermost, "the prologue stores the level {:#x} beside the table at {table:#x}", registration.level);
    let mut scopes = Vec::new();
    for index in 0..count {
        let at = entries + u64::from(index) * ENTRY_SIZE;
        let (parent, filter, handler) = (image.u32(at)?, image.u32(at + 4)?, image.u32(at + 8)?);
        let is_scope = (parent == outermost || parent < index)
            && image.is_code(u64::from(handler))
            && (filter == 0 || image.is_code(u64::from(filter)));
        ensure!(is_scope, "entry {index} of the scope table at {table:#x} is no scope");
        scopes.push(Scope {
            index,
            parent: (parent != outermost).then_some(parent),
            filter: (filter != 0).then(|| u64::from(filter)),
            handler: u64::from(handler),
            ranges: Vec::new(),
        });
    }
    Ok((layout, scopes))
}

/// The scope at `level` and the scopes around it, innermost first.
fn enclosing(scopes: &[Scope], level: u32) -> impl Iterator<Item = u32> + '_ {
    std::iter::successors(Some(level), move |&l| scopes[l as usize].parent)
}

/// The try levels that may hold at the start of each block the function's flow reaches. The
/// entry starts outside every scope. The handler runs an `__except` block and a `__finally`
/// funclet with the level of the scope around theirs, and a filter with the level the exception
/// was raised at: its scope or one nested in it. A block two paths reach at different levels
/// holds both, which is what the code does: clang's `__except` block joins the code after its
/// `__try` before that code stores the level it already has.
fn block_levels(f: &Function, scopes: &[Scope], stores: &BTreeMap<u64, u32>) -> BTreeMap<u64, BTreeSet<u32>> {
    fn join(levels: &mut BTreeMap<u64, BTreeSet<u32>>, work: &mut Vec<u64>, start: u64, more: impl IntoIterator<Item = u32>) {
        let known = levels.entry(start).or_default();
        let before = known.len();
        known.extend(more);
        if known.len() != before && !work.contains(&start) {
            work.push(start);
        }
    }

    let mut levels = BTreeMap::new();
    let mut work = Vec::new();
    join(&mut levels, &mut work, f.entry, [OUTSIDE]);
    for scope in scopes {
        if f.block_at(scope.handler).is_some() {
            join(&mut levels, &mut work, scope.handler, [scope.parent.unwrap_or(OUTSIDE)]);
        }
        if let Some(filter) = scope.filter
            && f.block_at(filter).is_some()
        {
            let within = scopes.iter().map(|s| s.index).filter(|&level| enclosing(scopes, level).any(|e| e == scope.index));
            join(&mut levels, &mut work, filter, within);
        }
    }
    while let Some(start) = work.pop() {
        let Some(block) = f.block_at(start) else { continue };
        let mut state = levels.get(&start).cloned().unwrap_or_default();
        for instr in &block.instrs {
            if let Some(&level) = stores.get(&instr.address) {
                state = BTreeSet::from([level]);
            }
        }
        for next in block.successors() {
            if f.block_at(next).is_some() {
                join(&mut levels, &mut work, next, state.iter().copied());
            }
        }
    }
    levels
}
