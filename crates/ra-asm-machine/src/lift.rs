//! A function lifted from the rows of its instructions, which every machine described by rows
//! shares: each block of the function is its instructions' semantics in turn, a jump to a block of
//! the function is a branch to it, and any other leaves through the program counter.
//!
//! A delayed row ([`Flow::Delayed`]) computes its condition and its target, then its slot runs,
//! then control goes where the row said; a likely row runs its slot only when taken. A transfer in
//! a slot stops at the slot's address, since nothing says where control would go; a slot that is
//! also the start of a block is lifted twice, once in the transfer and once as its own block.

use std::collections::BTreeSet;

use ra_asm_ir::{Function, Terminator};

use crate::instr::Machine;
use crate::outline::Outlines;
use crate::sem::{Fields, Ir, Lift, Pending, Sem, StateSpec};
use crate::table::{Decoded, Flow};

/// The words of a machine's code from `base` on, one per instruction.
#[derive(Debug, Clone, Copy)]
pub struct Code<'a> {
    pub base: u64,
    pub words: &'a [u32],
}

impl Code<'_> {
    /// The word at `address`; `None` outside the code or between two words.
    pub fn word(&self, address: u64) -> Option<u32> {
        let offset = address.checked_sub(self.base)?;
        if offset % 4 != 0 {
            return None;
        }
        self.words.get(usize::try_from(offset / 4).ok()?).copied()
    }
}

impl Machine {
    /// The function at `entry` lifted into `void @F_<entry>(ptr %state)` over the state `spec`
    /// lays out, one block per address of `blocks` and `entry`. A block runs until control leaves
    /// it or reaches another block's start; an address with no word or no row, a row without
    /// semantics and a transfer in a delay slot stop with `ra_unsupported` at their address. Every
    /// row is lifted inline, an outlined one too.
    pub fn lift_function(&self, spec: &StateSpec, code: &Code<'_>, entry: u64, blocks: &[u64]) -> Function {
        self.lift_blocks(spec, code, entry, blocks, None)
    }

    /// The function at `entry` as [`Machine::lift_function`] lifts it, with each instruction of
    /// an outlined row a call to the row's function in `outlines`, built there when no function
    /// lifted before has needed it.
    pub fn lift_function_outlined(
        &self,
        spec: &StateSpec,
        code: &Code<'_>,
        entry: u64,
        blocks: &[u64],
        outlines: &mut Outlines,
    ) -> Function {
        self.lift_blocks(spec, code, entry, blocks, Some(outlines))
    }

    fn lift_blocks(&self, spec: &StateSpec, code: &Code<'_>, entry: u64, blocks: &[u64], mut outlines: Option<&mut Outlines>) -> Function {
        let mut ir = Ir::new(spec, self.registers, entry);
        let first = ir.block;
        let starts: BTreeSet<u64> = blocks.iter().copied().chain([entry]).collect();
        for &start in &starts {
            ir.place(start);
            let block = ir.b.block(&format!("b_{start:x}"));
            ir.blocks.insert(start, block);
        }
        // The entry block has no predecessor, so a loop back to the entry goes to a block of its own.
        ir.b.terminate(first, Terminator::Br(ir.blocks[&entry]));
        for &start in &starts {
            ir.block = ir.blocks[&start];
            let mut at = start;
            while let Some(next) = self.instruction(&mut ir, &mut outlines, code, at) {
                if let Some(&to) = ir.blocks.get(&next) {
                    ir.b.terminate(ir.block, Terminator::Br(to));
                    break;
                }
                at = next;
            }
        }
        ir.b.finish()
    }

    /// Lifts the instruction at `at` into the open block, with its slot when its row is delayed:
    /// the address control goes on at in the block then open, or `None` when control has left.
    fn instruction(&self, ir: &mut Ir<'_>, outlines: &mut Option<&mut Outlines>, code: &Code<'_>, at: u64) -> Option<u64> {
        ir.enter(at);
        let (decoded, lift) = self.decoded(ir, code, at)?;
        let fields = Fields::new(self, decoded.row, &decoded.values, at);
        let Flow::Delayed { likely, .. } = decoded.row.flow else {
            semantics(ir, outlines, &decoded, lift, &fields);
            return (!ir.left).then_some(fields.next);
        };
        ir.delayed = true;
        lift(&mut *ir, &fields);
        ir.delayed = false;
        let pending = ir.pending.take();
        if ir.left && pending.is_none() {
            return None;
        }
        ir.left = false;
        let slot = self.wrap(at.wrapping_add(4));
        match pending {
            None => self.slot(ir, outlines, code, slot).then_some(fields.next),
            Some(Pending::Jump(target)) => {
                if self.slot(ir, outlines, code, slot) {
                    ir.place(at);
                    ir.leave(target);
                }
                None
            }
            Some(Pending::Call(target)) => {
                if !self.slot(ir, outlines, code, slot) {
                    return None;
                }
                ir.place(at);
                ir.call_now(target);
                Some(fields.next)
            }
            Some(Pending::Branch { cond, target }) if likely => {
                // Not taken annuls the slot: control goes straight past it.
                let cond = ir.at(cond, 1);
                let taken = ir.b.block(&format!("slot_{at:x}"));
                let next = ir.b.block(&format!("next_{at:x}"));
                ir.b.terminate(ir.block, Terminator::CondBr { cond, then: taken, els: next });
                ir.block = taken;
                if self.slot(ir, outlines, code, slot) {
                    ir.place(at);
                    ir.leave(target);
                }
                ir.block = next;
                ir.enter(at);
                Some(fields.next)
            }
            Some(Pending::Branch { cond, target }) => {
                if !self.slot(ir, outlines, code, slot) {
                    return None;
                }
                ir.place(at);
                ir.branch_now(cond, target);
                Some(fields.next)
            }
        }
    }

    /// Lifts the delay slot at `at` into the open block; whether control goes on after it.
    fn slot(&self, ir: &mut Ir<'_>, outlines: &mut Option<&mut Outlines>, code: &Code<'_>, at: u64) -> bool {
        ir.enter(at);
        let Some((decoded, lift)) = self.decoded(ir, code, at) else { return false };
        if decoded.row.flow.transfers() {
            ir.stop("ra_unsupported", code.word(at).map_or(0, u64::from));
            return false;
        }
        semantics(ir, outlines, &decoded, lift, &Fields::new(self, decoded.row, &decoded.values, at));
        !ir.left
    }

    /// The word at `at` decoded, with its row's semantics; a stop at `at` when there is no word,
    /// no row accepts it or the row has no semantics.
    fn decoded(&self, ir: &mut Ir<'_>, code: &Code<'_>, at: u64) -> Option<(Decoded<'_>, Lift)> {
        let word = code.word(at);
        if let Some(decoded) = word.and_then(|word| self.table.decode(word))
            && let Some(Sem(lift)) = decoded.row.sem
        {
            return Some((decoded, lift));
        }
        ir.stop("ra_unsupported", word.map_or(0, u64::from));
        None
    }
}

/// Lifts the semantics `lift` of the instruction `decoded` into `ir`: a call to its row's function
/// when the row is outlined and the lift has `outlines`, and inline otherwise.
fn semantics(ir: &mut Ir<'_>, outlines: &mut Option<&mut Outlines>, decoded: &Decoded<'_>, lift: Lift, fields: &Fields<'_>) {
    match outlines {
        Some(outlines) if decoded.row.outlined => outlines.call(ir, decoded, lift, fields),
        _ => lift(&mut *ir, fields),
    }
}
