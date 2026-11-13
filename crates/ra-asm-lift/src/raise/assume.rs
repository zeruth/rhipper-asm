//! What a raise step assumed: the record a step hands back beside its result.
//!
//! The machine level promises only what the instructions do. A raise step reads more into them:
//! that a call comes back, with the stack where it was and the registers the convention says the
//! callee keeps. The raised program is right where those hold and wrong where one does not, so a
//! step lists each one it relied on, by the function and the address it relied on it at, rather
//! than leaving it implicit in what it produced.

/// The raise step that made an assumption.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
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
}

/// Orders a list by function, then site, then step and kind, so the same program lists the same
/// assumptions in the same order whatever order the functions were visited in.
pub fn sort(assumptions: &mut [Assumption]) {
    assumptions.sort_by(|a, b| (&a.function, a.site, a.step, a.kind).cmp(&(&b.function, b.site, b.step, b.kind)));
}
