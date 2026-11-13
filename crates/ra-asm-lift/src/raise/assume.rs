//! What a raise step assumed: the record a step hands back beside its result.
//!
//! The machine level promises only what the instructions do. A raise step reads more into them:
//! that a call comes back, with the stack where it was and the registers the convention says the
//! callee keeps. The raised program is right where those hold and wrong where one does not, so a
//! step lists each one it relied on, by the function and the address it relied on it at, rather
//! than leaving it implicit in what it produced.
//!
//! Two of them are made before the raise, where the control flow is recovered, and are listed
//! here from what that left: a jump table's index stays inside the table ([`tables`]), and the
//! functions a call through a vtable slot was resolved to are all it reaches ([`calls`]). One
//! the step proved itself, by the check it found in the code, is listed as discharged.

use std::collections::BTreeMap;

use ra_asm_analysis::rtti::Vtable;
use ra_asm_ir::Function;

use super::{Target, abi};

/// The step that made an assumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    /// The targets of an indirect jump from its table (see [`ra_asm_analysis::jumptable`]).
    JumpTable,
    /// A call through a vtable slot as calls to the functions that fill it (see
    /// [`super::abi::virtual_calls`]).
    Devirtualize,
    /// The signatures and the calls by the target's convention (see [`super::abi`]).
    Abi,
    /// The rewrite of a function by its signature (see [`super::promote`]).
    Promote,
    /// The stack pointer as a symbol (see [`super::stack`]).
    Stack,
    /// The frame as locals (see [`super::frame`]).
    Frame,
}

impl Step {
    /// The name the reports use for the step.
    pub fn name(self) -> &'static str {
        match self {
            Self::JumpTable => "jump-table",
            Self::Devirtualize => "devirtualize",
            Self::Abi => "abi",
            Self::Promote => "promote",
            Self::Stack => "stack",
            Self::Frame => "frame",
        }
    }
}

/// What was assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// The callee of the call keeps the registers the convention says a callee keeps.
    CalleeSaved,
    /// The callee does not write the return address the call pushed: it returns to the call.
    ReturnAddress,
    /// The callee returns with the stack pointer restored, past what its signature says it pops.
    StackPointer,
    /// A local whose address is not taken is not reached by another pointer: a function's frame
    /// by a callee that was not handed its address, a frame slot by a pointer to another slot.
    PrivateLocal,
    /// Pushes and pops balance on every path: the paths into a block arrive, and every return
    /// leaves, with the stack at the one depth the step works with.
    BalancedStack,
    /// The frame pointer is a frame pointer: the register that holds an address of the frame at
    /// a call holds it after, so what the function reads through it later is its frame.
    FramePointer,
    /// The index of a jump table stays inside the table: the jump goes to one of the entries
    /// that were read, and the port stops where the program would have gone past them.
    TableIndex,
    /// The candidates of a call through a vtable slot are complete: the slot holds one of the
    /// functions the vtables the image shows fill it with, and no class the analysis did not
    /// find is the receiver's.
    CompleteCandidates,
}

impl Kind {
    /// The name the reports use for the kind.
    pub fn name(self) -> &'static str {
        match self {
            Self::CalleeSaved => "callee-saved",
            Self::ReturnAddress => "return-address",
            Self::StackPointer => "stack-pointer",
            Self::PrivateLocal => "private-local",
            Self::BalancedStack => "balanced-stack",
            Self::FramePointer => "frame-pointer",
            Self::TableIndex => "table-index",
            Self::CompleteCandidates => "complete-candidates",
        }
    }
}

/// One thing a raise step assumed and the machine level does not promise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assumption {
    /// The IR name of the function the step was raising.
    pub function: String,
    pub step: Step,
    pub kind: Kind,
    /// The address of the instruction the assumption is made at, when the lift gave it one.
    pub site: Option<u64>,
    /// The address of the check in the code that makes it hold, when the step found one: the
    /// assumption is then discharged, listed for the record and relied on no further.
    pub discharged: Option<u64>,
}

impl Assumption {
    /// Whether the step found the check that makes the assumption hold.
    pub fn is_discharged(&self) -> bool {
        self.discharged.is_some()
    }
}

/// Orders a list by function, then site, then step and kind, so the same program lists the same
/// assumptions in the same order whatever order the functions were visited in.
pub fn sort(assumptions: &mut [Assumption]) {
    assumptions.sort_by(|a, b| (&a.function, a.site, a.step, a.kind).cmp(&(&b.function, b.site, b.step, b.kind)));
}

/// What the analysis assumed of the jump tables of `functions`, in a stable order: one entry per
/// indirect jump it resolved through a table, at the jump, under the IR name of its function.
/// The index is taken to stay inside the table; where a compare and a conditional jump before
/// the table code bound it, that is what the code checks, and the entry is discharged by the
/// compare's address. Without one the table was read while its entries pointed at code, so both
/// its length and the index's range are assumed. An indirect jump that did not resolve is not a
/// table and lists nothing.
pub fn tables<'a>(functions: impl IntoIterator<Item = &'a ra_asm_analysis::Function>) -> Vec<Assumption> {
    let mut out = Vec::new();
    for f in functions {
        for block in &f.blocks {
            let ra_asm_analysis::Terminator::Switch { bound, .. } = &block.terminator else { continue };
            out.push(Assumption {
                function: crate::function_name(f.entry),
                step: Step::JumpTable,
                kind: Kind::TableIndex,
                site: block.instrs.last().map(|i| i.address),
                discharged: *bound,
            });
        }
    }
    sort(&mut out);
    out
}

/// What devirtualization assumes of the machine function `f`, in the order of its calls: at
/// each call through a vtable slot that [`abi::virtual_calls`] resolves, that the functions the
/// slot was resolved to are all the call reaches. The rewrite calls the one candidate without a
/// test and traps on an address outside several, so a receiver of a class the analysis did not
/// find stops the port or runs another class's function. A call a traced run resolved (`traced`,
/// see [`abi::traced_calls`]) keeps the indirect call for any other target and assumes nothing.
pub fn calls(
    f: &Function,
    vtables: &[Vtable],
    target: Target,
    traced: &BTreeMap<u64, Vec<u64>>,
    lifted: &dyn Fn(&str) -> bool,
) -> Vec<Assumption> {
    let virtual_calls = abi::virtual_calls(f, vtables, target, lifted);
    let seen = abi::traced_calls(f, traced, lifted);
    let mut out = Vec::new();
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        if abi::virtual_candidates(&i.inst, &virtual_calls).is_some() && abi::virtual_candidates(&i.inst, &seen).is_none() {
            out.push(Assumption {
                function: f.name.clone(),
                step: Step::Devirtualize,
                kind: Kind::CompleteCandidates,
                site: i.address,
                discharged: None,
            });
        }
    }
    out
}
