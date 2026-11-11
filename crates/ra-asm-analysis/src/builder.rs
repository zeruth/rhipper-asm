//! The recursive descent over one function: blocks from the entry, split where a jump lands in the
//! middle of one, bounded by the function's ranges. A jump out of the ranges or to another known
//! start is a tail call.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use ra_asm_arch::{Arch, Flow, Instr, Operand, Target};
use ra_asm_pe::Image;
use ra_asm_x86::X86;

use crate::function::{Block, Function, Kind, Terminator};
use crate::{jumptable, noreturn};

/// What one build produced.
pub struct Built {
    pub function: Function,
    pub warnings: Vec<String>,
}

/// A block under construction.
struct Draft {
    start: u64,
    end: u64,
    instrs: Vec<Instr>,
    terminator: Terminator,
}

pub(crate) struct Builder<'a> {
    image: &'a Image,
    arch: X86,
    entry: u64,
    ranges: &'a [(u64, u64)],
    known_starts: &'a BTreeSet<u64>,
    noreturn: &'a HashSet<u64>,
    blocks: BTreeMap<u64, Draft>,
    worklist: Vec<u64>,
    calls: Vec<u64>,
    import_calls: Vec<String>,
    tail_calls: Vec<u64>,
    warnings: Vec<String>,
}

/// Builds the function at `entry`.
pub fn build(image: &Image, arch: X86, entry: u64, ranges: &[(u64, u64)], known_starts: &BTreeSet<u64>, noreturn: &HashSet<u64>) -> Built {
    let mut b = Builder {
        image,
        arch,
        entry,
        ranges,
        known_starts,
        noreturn,
        blocks: BTreeMap::new(),
        worklist: vec![entry],
        calls: Vec::new(),
        import_calls: Vec::new(),
        tail_calls: Vec::new(),
        warnings: Vec::new(),
    };
    while let Some(start) = b.worklist.pop() {
        b.block(start);
    }
    b.finish()
}

impl Builder<'_> {
    /// Whether `va` belongs to this function: inside its ranges, and not the start of another.
    fn internal(&self, va: u64) -> bool {
        if va == self.entry {
            return true;
        }
        if self.known_starts.contains(&va) {
            return false;
        }
        self.ranges.iter().any(|&(begin, end)| va >= begin && va < end)
    }

    fn decode(&self, va: u64) -> Option<Instr> {
        let bytes = self.image.read_to_section_end(va).ok()?;
        self.arch.decode(bytes, va)
    }

    fn block(&mut self, start: u64) {
        if self.blocks.contains_key(&start) {
            return;
        }
        // A start inside a built block splits that block at the start.
        if let Some((&other_start, other)) = self.blocks.range(..start).next_back()
            && start < other.end
        {
            let Some(index) = other.instrs.iter().position(|i| i.address == start) else {
                self.warnings.push(format!("{start:#x} is inside an instruction of the block at {other_start:#x}"));
                return;
            };
            let mut other = self.blocks.remove(&other_start).unwrap();
            let tail = other.instrs.split_off(index);
            let terminator = std::mem::replace(&mut other.terminator, Terminator::FallThrough(start));
            other.end = start;
            let tail_end = tail.last().map_or(start, Instr::end);
            self.blocks.insert(other_start, other);
            self.blocks.insert(start, Draft { start, end: tail_end, instrs: tail, terminator });
            return;
        }

        let mut draft = Draft { start, end: start, instrs: Vec::new(), terminator: Terminator::Invalid };
        let mut va = start;
        loop {
            if va != start && self.blocks.contains_key(&va) {
                draft.terminator = Terminator::FallThrough(va);
                break;
            }
            if !self.internal(va) {
                // The code runs out of the function: into another function, or past the range.
                draft.terminator = self.leave(va);
                break;
            }
            let Some(instr) = self.decode(va) else {
                self.warnings.push(format!("no instruction at {va:#x} in the function at {:#x}", self.entry));
                draft.terminator = Terminator::Invalid;
                break;
            };
            let next = instr.end();
            let flow = instr.flow;
            let mnemonic_is_fastfail = instr.mnemonic == "int" && matches!(instr.operands.first(), Some(Operand::Imm { value: 0x29, .. }));
            draft.instrs.push(instr);
            draft.end = next;
            match flow {
                Flow::Interrupt if mnemonic_is_fastfail => {
                    draft.terminator = Terminator::Trap;
                    break;
                }
                Flow::Next | Flow::Interrupt => {}
                Flow::Jump(Target::Direct(target)) => {
                    draft.terminator = if self.internal(target) {
                        self.worklist.push(target);
                        Terminator::Jump(target)
                    } else {
                        self.leave(target)
                    };
                    break;
                }
                Flow::Jump(Target::Indirect) => {
                    draft.terminator = self.indirect_jump(&draft);
                    break;
                }
                Flow::CondJump(Target::Direct(taken)) => {
                    if self.internal(taken) {
                        self.worklist.push(taken);
                    } else {
                        // A conditional jump to another function: rare, kept as a tail call edge.
                        self.tail_calls.push(taken);
                    }
                    self.worklist.push(next);
                    draft.terminator = Terminator::CondJump { taken, fallthrough: next };
                    break;
                }
                Flow::CondJump(Target::Indirect) => unreachable!("x86 has no indirect conditional jump"),
                Flow::Call(Target::Direct(target)) => {
                    if !self.calls.contains(&target) {
                        self.calls.push(target);
                    }
                    if self.noreturn.contains(&target) {
                        draft.terminator = Terminator::NoReturnCall(target);
                        break;
                    }
                }
                Flow::Call(Target::Indirect) => {
                    if let Some(import) = self.import_of(draft.instrs.last().unwrap()) {
                        if !self.import_calls.contains(&import) {
                            self.import_calls.push(import.clone());
                        }
                        if noreturn::import_never_returns(&import) {
                            draft.terminator = Terminator::NoReturnImportCall(import);
                            break;
                        }
                    }
                }
                Flow::Return => {
                    draft.terminator = Terminator::Return;
                    break;
                }
                Flow::Trap => {
                    draft.terminator = Terminator::Trap;
                    break;
                }
                Flow::Halt => {
                    draft.terminator = Terminator::Halt;
                    break;
                }
            }
            va = next;
        }
        self.blocks.insert(start, draft);
    }

    /// The terminator of a block whose control leaves the function at `target`.
    fn leave(&mut self, target: u64) -> Terminator {
        if self.image.is_code(target) {
            if !self.tail_calls.contains(&target) {
                self.tail_calls.push(target);
            }
            Terminator::TailCall(target)
        } else {
            self.warnings.push(format!("the function at {:#x} jumps to {target:#x}, which holds no code", self.entry));
            Terminator::IndirectJump
        }
    }

    /// The import an instruction calls or jumps through, when its memory operand is an IAT slot.
    fn import_of(&self, instr: &Instr) -> Option<String> {
        let slot = instr.absolute_memory_operand()?;
        self.image.import_at(slot).map(ra_asm_pe::Import::display_name)
    }

    fn indirect_jump(&mut self, draft: &Draft) -> Terminator {
        let jump = draft.instrs.last().unwrap();
        if let Some(import) = self.import_of(jump) {
            if !self.import_calls.contains(&import) {
                self.import_calls.push(import.clone());
            }
            return Terminator::ImportTailCall(import);
        }
        let predecessors: Vec<&[Instr]> =
            self.blocks.values().filter(|b| b.terminator.successors().contains(&draft.start)).map(|b| b.instrs.as_slice()).collect();
        let Some(table) = jumptable::resolve(self.image, &draft.instrs, &predecessors) else {
            self.warnings.push(format!("the indirect jump at {:#x} in the function at {:#x} did not resolve", jump.address, self.entry));
            return Terminator::IndirectJump;
        };
        let mut targets = Vec::new();
        let mut cases = Vec::new();
        for (n, target) in table.targets.iter().copied().enumerate() {
            cases.push((n as u64, target));
            if self.internal(target) {
                if !targets.contains(&target) {
                    targets.push(target);
                }
                self.worklist.push(target);
            } else if self.image.is_code(target) {
                self.tail_calls.push(target);
            }
        }
        Terminator::Switch { table: table.address, targets, index: table.index, load_at: table.load_at, cases }
    }

    fn finish(self) -> Built {
        let blocks: Vec<Block> =
            self.blocks.into_values().map(|d| Block { start: d.start, end: d.end, instrs: d.instrs, terminator: d.terminator }).collect();
        // A thunk is one block: `jmp [slot]`.
        let kind = match blocks.as_slice() {
            [one] if one.instrs.len() == 1 => match &one.terminator {
                Terminator::ImportTailCall(import) => Kind::ImportThunk(import.clone()),
                _ => Kind::Normal,
            },
            _ => Kind::Normal,
        };
        let function = Function {
            entry: self.entry,
            name: String::new(),
            kind,
            blocks,
            calls: self.calls,
            import_calls: self.import_calls,
            tail_calls: self.tail_calls,
            noreturn: false,
            origins: Vec::new(),
        };
        Built { function, warnings: self.warnings }
    }
}
