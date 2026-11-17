//! The recursive descent over one function: blocks from the entry, split where a jump lands in the
//! middle of one, bounded by the function's ranges. A jump out of the ranges or to another known
//! start is a tail call.
//!
//! A MIPS transfer takes effect after its delay slot, the instruction after it: the block ends past
//! the slot, and the fall-through and the return of a call are the address after it. A slot a jump
//! lands on is entered twice, on the way to the transfer's target and from the jump: the transfer's
//! block keeps its copy and a block of its own starts at the slot.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use ra_asm_arch::{Arch, Flow, Instr, Operand, Target};
use ra_asm_mips::{Delay, Mips};
use ra_asm_pe::{CxxFrame, Image};

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
    arch: &'a dyn Arch,
    entry: u64,
    ranges: &'a [(u64, u64)],
    known_starts: &'a BTreeSet<u64>,
    noreturn: &'a HashSet<u64>,
    /// The x64 C++ frame of the function, when it has one.
    frame: Option<&'a CxxFrame>,
    /// Whether the transfers of the machine take effect after a delay slot: MIPS.
    delays: bool,
    blocks: BTreeMap<u64, Draft>,
    worklist: Vec<u64>,
    calls: Vec<u64>,
    import_calls: Vec<String>,
    tail_calls: Vec<u64>,
    local_calls: Vec<u64>,
    warnings: Vec<String>,
    /// The indirect jumps that did not resolve with the blocks known when they were reached, by
    /// address: each is tried again once the rest of the function is, since a table's bound may
    /// lie on a way in found later (MSVC jumps into a `switch` from another's case, past its
    /// compare).
    unresolved: Vec<u64>,
}

/// Builds the function at `entry`.
pub fn build(
    image: &Image,
    arch: &dyn Arch,
    entry: u64,
    ranges: &[(u64, u64)],
    known_starts: &BTreeSet<u64>,
    noreturn: &HashSet<u64>,
    frame: Option<&CxxFrame>,
) -> Built {
    let mut b = Builder {
        image,
        arch,
        entry,
        ranges,
        known_starts,
        noreturn,
        frame,
        delays: matches!(arch.name(), "mips" | "mipsel"),
        blocks: BTreeMap::new(),
        worklist: vec![entry],
        calls: Vec::new(),
        import_calls: Vec::new(),
        tail_calls: Vec::new(),
        local_calls: Vec::new(),
        warnings: Vec::new(),
        unresolved: Vec::new(),
    };
    let mut tried = BTreeSet::new();
    loop {
        while let Some(start) = b.worklist.pop() {
            b.block(start);
        }
        // The jumps that did not resolve, with every way into them known now; one that resolves
        // adds its cases to the walk.
        if b.retry_unresolved() {
            continue;
        }
        // The funclets an exception frame registers are reached by the handler, not by the code;
        // they belong to the function that registers them.
        let roots: Vec<u64> = b.frame_funclets().into_iter().filter(|&va| !b.blocks.contains_key(&va) && tried.insert(va)).collect();
        if roots.is_empty() {
            break;
        }
        b.worklist.extend(roots);
    }
    b.local_calls = b.resolve_local_calls();
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

    /// Whether a jump to `target` crosses the `int 3` fill the linker and the compiler put between
    /// x86 functions, and so leaves this one: `jmp __report_gsfailure` after a cookie check, a
    /// vector deleting destructor jumping into the scalar one, a tail call to the next function.
    /// Nothing pads the inside of a function with `int 3` up to a 16-byte boundary: clang's lone
    /// `int 3` after a call that never returns precedes a block of the same function, which is
    /// not aligned. On x64 the exception table bounds a function exactly.
    fn beyond_fill(&self, target: u64) -> bool {
        !self.delays
            && self.arch.address_bits() == 32
            && target.is_multiple_of(16)
            && !self.blocks.contains_key(&target)
            && target != self.entry
            && self.image.u8(target - 1).is_ok_and(|b| b == 0xcc)
    }

    /// The funclets of the exception frame this function registers on x86. The prologue stores the
    /// handler and, for SEH, the scope table, then reads `fs:[0]`: MSVC pushes them, clang writes
    /// the record in place. A scope table (data) names filters and handlers; a C++ handler is the
    /// function's `__ehhandler$` stub (code), whose `FuncInfo` names the cleanup actions and the
    /// catch handlers; a catch handler returns its continuation, which is the function's too. A
    /// filter clang outlines into a function of its own follows `int 3` fill, as a funclet of the
    /// function never does, and stays a function. On x64 the exception table names the function's
    /// C++ frame: its funclets, and the continuation each catch funclet hands the handler with
    /// `lea rax, [rip + L]`.
    fn frame_funclets(&self) -> Vec<u64> {
        let mut out = Vec::new();
        if self.arch.address_bits() != 32 {
            let Some(frame) = self.frame else { return out };
            out.extend(frame.funclets.iter().map(|f| f.begin));
            for block in self.blocks.values().filter(|b| frame.funclets.iter().any(|f| f.contains(b.start))) {
                for instr in block.instrs.iter().filter(|i| i.mnemonic == "lea") {
                    for operand in &instr.operands {
                        if let Operand::Mem(m) = operand
                            && let Some(va) = m.absolute
                            && self.internal(va)
                        {
                            out.push(va);
                        }
                    }
                }
            }
            return out;
        }
        let after_fill = |va: &u64| self.image.u8(va - 1).is_ok_and(|b| b == 0xcc);
        // A scope table lies in data, or among the code of an image that keeps its read-only
        // data there (the system DLLs), where only its shape tells it from a handler stub.
        let data_in_code = self.image.data_in_code();
        for block in self.blocks.values() {
            for (n, instr) in block.instrs.iter().enumerate() {
                let reads_frame_chain = instr.mnemonic == "mov"
                    && instr.operands.iter().any(|o| {
                        matches!(o, Operand::Mem(m) if m.segment.map(|s| s.name) == Some("fs") && m.base.is_none() && m.index.is_none() && m.disp == 0)
                    });
                if !reads_frame_chain {
                    continue;
                }
                for stored in block.instrs[n.saturating_sub(6)..n].iter().flat_map(|i| &i.operands) {
                    let Operand::Imm { value, bits: 32 } = stored else { continue };
                    let va = *value as u64 & 0xffff_ffff;
                    if self.image.section_of(va).is_none() {
                        continue;
                    }
                    let code = self.image.is_code(va);
                    let seh = if code && !data_in_code { Vec::new() } else { self.seh_funclets(va) };
                    if !seh.is_empty() {
                        out.extend(seh);
                    } else if code {
                        out.extend(self.cxx_funclets(va));
                    }
                }
            }
        }
        out.retain(|va| !after_fill(va));
        out
    }

    /// The filters and handlers of an SEH scope table: `{level, filter, handler}` entries, where
    /// the level encloses the entry (-1, or an earlier index) and the handler is code of this
    /// function. `__except_handler3`'s table (clang) starts with the entries; `__except_handler4`'s
    /// (MSVC) puts the four cookie offsets first. A filter clang outlines into a function of its
    /// own fails the test of this function's code and stays one.
    fn seh_funclets(&self, table: u64) -> Vec<u64> {
        let entries = |mut at: u64| {
            let mut out = Vec::new();
            for index in 0..256u32 {
                let (Ok(level), Ok(filter), Ok(handler)) = (self.image.u32(at), self.image.u32(at + 4), self.image.u32(at + 8)) else {
                    break;
                };
                // -1 is the outermost level of `__except_handler3`, -2 that of `__except_handler4`.
                let level_fits = level == 0xffff_ffff || level == 0xffff_fffe || level < index;
                if !(level_fits && handler != 0 && self.internal(u64::from(handler))) {
                    break;
                }
                out.extend([u64::from(filter), u64::from(handler)].into_iter().filter(|&va| va != 0 && self.internal(va)));
                at += 12;
            }
            out
        };
        let plain = entries(table);
        if plain.is_empty() { entries(table + 16) } else { plain }
    }

    /// The funclets a C++ handler stub names: `mov eax, offset FuncInfo; jmp __CxxFrameHandler3`,
    /// after a cookie check when the function has one, the unwind map's actions and the try
    /// blocks' catch handlers of the `FuncInfo`, and the continuation each catch handler returns
    /// with `mov eax, offset L; ret`.
    fn cxx_funclets(&self, stub: u64) -> Vec<u64> {
        let mut out = Vec::new();
        let mut va = stub;
        let mut info = None;
        for _ in 0..16 {
            let Some(instr) = self.decode(va) else { return out };
            if let (Some(Operand::Reg(r)), Some(Operand::Imm { value, bits: 32 })) = (instr.operands.first(), instr.operands.get(1))
                && instr.mnemonic == "mov"
                && r.name == "eax"
            {
                let at = *value as u64 & 0xffff_ffff;
                if self.image.u32(at).is_ok_and(|magic| magic & 0xffff_ff00 == 0x1993_0500) {
                    info = Some(at);
                }
            }
            match instr.flow {
                Flow::Next | Flow::Call(_) => va = instr.end(),
                Flow::Jump(Target::Direct(_)) => break,
                _ => return out,
            }
        }
        let Some(info) = info else { return out };
        let field = |n: u64| self.image.u32(info + 4 * n).map_or(0, u64::from);
        let (states, unwind_map, try_blocks, try_map) = (field(1).min(4096), field(2), field(3).min(4096), field(4));
        for state in 0..states {
            if let Ok(action) = self.image.u32(unwind_map + state * 8 + 4)
                && action != 0
                && self.internal(u64::from(action))
            {
                out.push(u64::from(action));
            }
        }
        for block in 0..try_blocks {
            let entry = try_map + block * 20;
            let catches = self.image.u32(entry + 12).map_or(0, u64::from).min(256);
            let handlers = self.image.u32(entry + 16).map_or(0, u64::from);
            for n in 0..catches {
                let Ok(handler) = self.image.u32(handlers + n * 16 + 12) else { continue };
                if !self.internal(u64::from(handler)) {
                    continue;
                }
                out.push(u64::from(handler));
                out.extend(self.catch_continuations(u64::from(handler)));
            }
        }
        out
    }

    /// The addresses a catch handler returns to the frame handler: a `mov eax, offset L` in a
    /// returning block reachable from the handler, once its blocks are built.
    fn catch_continuations(&self, handler: u64) -> Vec<u64> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        let mut stack = vec![handler];
        while let Some(start) = stack.pop() {
            let Some(block) = self.blocks.get(&start) else { continue };
            if !seen.insert(start) {
                continue;
            }
            stack.extend(block.terminator.successors());
            if !matches!(block.terminator, Terminator::Return) {
                continue;
            }
            for instr in &block.instrs {
                if let (Some(Operand::Reg(r)), Some(Operand::Imm { value, bits: 32 })) = (instr.operands.first(), instr.operands.get(1))
                    && instr.mnemonic == "mov"
                    && r.name == "eax"
                {
                    let target = *value as u64 & 0xffff_ffff;
                    if self.internal(target) {
                        out.push(target);
                    }
                }
            }
        }
        out
    }

    fn decode(&self, va: u64) -> Option<Instr> {
        let bytes = self.image.read_to_section_end(va).ok()?;
        self.arch.decode(bytes, va)
    }

    /// When the flow of `instr` takes effect: at once on a machine without delay slots.
    fn delay(&self, instr: &Instr) -> Delay {
        if self.delays { Mips::delay(instr) } else { Delay::None }
    }

    /// The delay slot at `va`, which runs before its transfer takes effect. A transfer in a slot
    /// is unpredictable in the architecture, and bytes that are no instruction hold no slot: the
    /// block ends there as invalid.
    fn slot(&mut self, va: u64) -> Option<Instr> {
        let Some(slot) = self.decode(va) else {
            self.warnings.push(format!("no instruction in the delay slot at {va:#x} in the function at {:#x}", self.entry));
            return None;
        };
        if self.delay(&slot) != Delay::None || !matches!(slot.flow, Flow::Next | Flow::Interrupt) {
            self.warnings.push(format!("the delay slot at {va:#x} in the function at {:#x} holds a transfer", self.entry));
            return None;
        }
        Some(slot)
    }

    /// Splits the block at `block_start` before its instruction `index`: the head falls through to
    /// a block of the rest, which keeps the terminator.
    fn split(&mut self, block_start: u64, index: usize) {
        let mut head = self.blocks.remove(&block_start).unwrap();
        let tail = head.instrs.split_off(index);
        let start = tail[0].address;
        let terminator = std::mem::replace(&mut head.terminator, Terminator::FallThrough(start));
        head.end = start;
        let tail_end = tail.last().map_or(start, Instr::end);
        self.blocks.insert(block_start, head);
        self.blocks.insert(start, Draft { start, end: tail_end, instrs: tail, terminator });
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
            // A slot keeps its copy in the block of its transfer, and the block it starts is built
            // below; the instructions after the slot of a call rejoin that block where the slot ends.
            let in_slot = self.delay(&other.instrs[index - 1]) != Delay::None;
            let len = other.instrs.len();
            if !in_slot {
                self.split(other_start, index);
                return;
            }
            if index + 1 < len {
                self.split(other_start, index + 1);
            }
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
            let Some(mut instr) = self.decode(va) else {
                self.warnings.push(format!("no instruction at {va:#x} in the function at {:#x}", self.entry));
                draft.terminator = Terminator::Invalid;
                break;
            };
            self.arch.link(&draft.instrs, &mut instr);
            let mut next = instr.end();
            let flow = instr.flow;
            let delay = self.delay(&instr);
            let mnemonic_is_fastfail = instr.mnemonic == "int" && matches!(instr.operands.first(), Some(Operand::Imm { value: 0x29, .. }));
            // A branch on a register that calls and links: the call may not happen.
            let conditional_call = delay != Delay::None && matches!(instr.mnemonic.as_str(), "bltzal" | "bgezal" | "bltzall" | "bgezall");
            let at = draft.instrs.len();
            draft.instrs.push(instr);
            draft.end = next;
            if delay != Delay::None {
                let Some(slot) = self.slot(next) else {
                    draft.terminator = Terminator::Invalid;
                    break;
                };
                next = slot.end();
                draft.instrs.push(slot);
                draft.end = next;
            }
            match flow {
                Flow::Interrupt if mnemonic_is_fastfail => {
                    draft.terminator = Terminator::Trap;
                    break;
                }
                Flow::Next | Flow::Interrupt => {}
                Flow::Jump(Target::Direct(target)) => {
                    draft.terminator = if self.internal(target) && !self.beyond_fill(target) {
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
                    if self.noreturn.contains(&target) && !conditional_call {
                        draft.terminator = Terminator::NoReturnCall(target);
                        break;
                    }
                }
                Flow::Call(Target::Indirect) => {
                    if let Some(import) = self.import_of(&draft.instrs[at]) {
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
        // A delayed jump has its slot after it in the block.
        let body = if self.delays { &draft.instrs[..draft.instrs.len() - 1] } else { draft.instrs.as_slice() };
        let jump = body.last().unwrap();
        if let Some(import) = self.import_of(jump) {
            if !self.import_calls.contains(&import) {
                self.import_calls.push(import.clone());
            }
            return Terminator::ImportTailCall(import);
        }
        let predecessors: Vec<&[Instr]> =
            self.blocks.values().filter(|b| b.terminator.successors().contains(&draft.start)).map(|b| b.instrs.as_slice()).collect();
        let resolved = if self.delays {
            let function: Vec<&[Instr]> =
                self.blocks.values().map(|b| b.instrs.as_slice()).chain(std::iter::once(draft.instrs.as_slice())).collect();
            jumptable::resolve_mips(self.image, body, &predecessors, &function)
        } else {
            jumptable::resolve(self.image, body, &predecessors)
        };
        let Some(table) = resolved else {
            if !self.unresolved.contains(&jump.address) {
                self.unresolved.push(jump.address);
            }
            return Terminator::IndirectJump;
        };
        let mut targets = Vec::new();
        let mut cases = Vec::new();
        for (&n, target) in table.indices.iter().zip(table.targets.iter().copied()) {
            cases.push((n, target));
            if self.internal(target) {
                if !targets.contains(&target) {
                    targets.push(target);
                }
                self.worklist.push(target);
            } else if self.image.is_code(target) {
                self.tail_calls.push(target);
            }
        }
        Terminator::Switch {
            table: table.address,
            targets,
            index: table.index,
            load_at: table.load_at,
            cases,
            bound: table.bound,
            case_table: table.case_table,
        }
    }

    /// The direct calls into the function's own code, outside its flow from the entry: MSVC calls
    /// a `__finally` funclet on the normal way out of its `__try`, with the frame still the
    /// function's (the unwinder calls it through the handler, from the entry the scope table
    /// names, a few bytes before). The target is code of this function, not a function: a block
    /// starts there, for the lift to enter it, and the target leaves the calls.
    fn resolve_local_calls(&mut self) -> Vec<u64> {
        if self.arch.address_bits() != 32 || self.delays {
            return Vec::new();
        }
        let mut reached = BTreeSet::new();
        let mut work = vec![self.entry];
        while let Some(start) = work.pop() {
            if reached.insert(start)
                && let Some(block) = self.blocks.get(&start)
            {
                work.extend(block.terminator.successors());
            }
        }
        if self.blocks.keys().all(|start| reached.contains(start)) {
            return Vec::new();
        }
        let outside = |blocks: &BTreeMap<u64, Draft>, va: u64| {
            blocks.range(..=va).next_back().is_some_and(|(start, block)| va < block.end && !reached.contains(start))
        };
        let mut targets: Vec<u64> = self
            .blocks
            .values()
            .flat_map(|b| &b.instrs)
            .filter_map(|i| match i.flow {
                Flow::Call(Target::Direct(target)) if outside(&self.blocks, target) => Some(target),
                _ => None,
            })
            .collect();
        targets.sort_unstable();
        targets.dedup();
        for &target in &targets {
            self.block(target);
            self.calls.retain(|&c| c != target);
        }
        targets
    }

    /// Tries each unresolved indirect jump again with the blocks known now. Whether one resolved.
    fn retry_unresolved(&mut self) -> bool {
        let mut resolved = false;
        for jump in std::mem::take(&mut self.unresolved) {
            // The jump ends its block, or comes just before its delay slot.
            let ends = |d: &Draft| d.instrs.iter().rev().take(2).any(|i| i.address == jump);
            // Only a block still ending in the unresolved jump: one the walk made something else
            // of (a PLT stub's `br x17` as a jump to its import) keeps it.
            let Some(start) = self.blocks.values().find(|d| ends(d) && matches!(d.terminator, Terminator::IndirectJump)).map(|d| d.start)
            else {
                continue;
            };
            let Some(mut draft) = self.blocks.remove(&start) else { continue };
            let terminator = self.indirect_jump(&draft);
            if matches!(terminator, Terminator::IndirectJump) {
                self.blocks.insert(start, draft);
                continue;
            }
            resolved = true;
            draft.terminator = terminator;
            self.blocks.insert(start, draft);
        }
        resolved
    }

    fn finish(mut self) -> Built {
        for jump in std::mem::take(&mut self.unresolved) {
            self.warnings.push(format!("the indirect jump at {jump:#x} in the function at {:#x} did not resolve", self.entry));
        }
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
            symbol: None,
            object: None,
            kind,
            blocks,
            calls: self.calls,
            import_calls: self.import_calls,
            tail_calls: self.tail_calls,
            local_calls: self.local_calls,
            noreturn: false,
            origins: Vec::new(),
            strings: Vec::new(),
            register_import_calls: std::collections::HashMap::new(),
            traced_calls: std::collections::BTreeMap::new(),
            virtuals: Vec::new(),
            reach: crate::function::Reach::Named,
        };
        Built { function, warnings: self.warnings }
    }
}
