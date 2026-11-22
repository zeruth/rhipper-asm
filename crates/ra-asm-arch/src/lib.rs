//! What every architecture gives the analysis: an [`Instr`] record with its operands, its flow and
//! the registers it touches, and the [`Arch`] trait that decodes one. The x86 decoder fills these
//! from iced-x86; a second architecture fills them from its own decoder. The lift of each
//! architecture keeps its decoder's own instruction beside the record, because the semantics are
//! not neutral.

pub mod abi;
pub mod state;

use serde::Serialize;

/// A register, or a part of one. `full` names the register that holds it, and `shift` and `bits`
/// say where: `ah` is `rax` with shift 8 and 8 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct Register {
    /// The decoder's own number for the register: stable within an architecture.
    pub id: u16,
    pub name: &'static str,
    pub class: RegClass,
    /// The id of the full register.
    pub full: u16,
    /// The width of the register, in bits.
    pub bits: u16,
    /// The bit offset in the full register.
    pub shift: u16,
}

impl Register {
    pub fn is_full(&self) -> bool {
        self.id == self.full
    }
}

/// The kind of a register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegClass {
    General,
    StackPointer,
    InstructionPointer,
    Flags,
    Segment,
    Vector,
    Float,
    Other,
}

/// A memory operand: `segment:[base + index * scale + disp]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemOperand {
    pub segment: Option<Register>,
    pub base: Option<Register>,
    pub index: Option<Register>,
    pub scale: u8,
    pub disp: i64,
    /// The width of the access, in bits; zero when the instruction takes the address only (`lea`).
    pub bits: u16,
    /// The address when it is a constant: no base and no index, or a base of the instruction
    /// pointer, which the decoder resolves against the next instruction.
    pub absolute: Option<u64>,
}

/// An operand of an instruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Operand {
    Reg(Register),
    /// An immediate, sign-extended to 64 bits, with the width the encoding gave it.
    Imm {
        value: i64,
        bits: u16,
    },
    Mem(MemOperand),
    /// A code address: the target of a direct branch or call.
    Addr(u64),
}

/// Where control goes after an instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Flow {
    /// The next instruction.
    Next,
    /// A jump that always goes to the target.
    Jump(Target),
    /// A jump that goes to the target or falls through.
    CondJump(Target),
    /// A call: control returns to the next instruction unless the callee never returns.
    Call(Target),
    Return,
    /// `int3`, `ud2` and their kin: execution does not continue here.
    Trap,
    /// `hlt`.
    Halt,
    /// A software interrupt or a system call: control comes back to the next instruction.
    Interrupt,
}

/// The target of a jump or a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    Direct(u64),
    /// Through a register or a memory operand: the first operand says which.
    Indirect,
}

impl Flow {
    pub fn direct_target(self) -> Option<u64> {
        match self {
            Self::Jump(Target::Direct(t)) | Self::CondJump(Target::Direct(t)) | Self::Call(Target::Direct(t)) => Some(t),
            _ => None,
        }
    }

    /// Whether the next instruction can run after this one.
    pub fn falls_through(self) -> bool {
        matches!(self, Self::Next | Self::CondJump(_) | Self::Call(_) | Self::Interrupt)
    }

    pub fn is_call(self) -> bool {
        matches!(self, Self::Call(_))
    }
}

/// When the flow of a transfer takes effect, on a machine whose transfers have a delay slot (MIPS,
/// SuperH): the record keeps the transfer's own flow, and this says when it happens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Delay {
    /// At once: the instruction transfers nothing, or has no slot.
    None,
    /// After the next instruction, the delay slot, which runs whether or not a branch is taken.
    Slot,
    /// After the delay slot, which runs only when the branch is taken: a likely branch skips its
    /// slot when it falls through.
    Likely,
}

/// How an instruction touches a register.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    Read,
    Write,
    ReadWrite,
    /// Read on some paths, as the index of a conditional move is.
    CondRead,
    /// Written on some paths.
    CondWrite,
}

impl Access {
    pub fn reads(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite | Self::CondRead)
    }

    pub fn writes(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite | Self::CondWrite)
    }
}

/// Where the code of a 32-bit ARM program is ARM code and where Thumb: the places a set is known
/// to start at. The code at an address is of the set of the nearest place at or below it. A
/// file's mapping symbols are the first places; the decoder adds where a branch goes (the set
/// the branch is in, or the other for a branch that changes it), so a program without the
/// symbols is told apart as its code is followed. A place keeps the set it was first given.
#[derive(Debug, Default)]
pub struct ArmMap {
    places: std::sync::RwLock<std::collections::BTreeMap<u64, bool>>,
}

impl ArmMap {
    /// Says the code at `va` starts a run of ARM code (`arm`) or of Thumb, unless it is said already.
    pub fn mark(&self, va: u64, arm: bool) {
        if self.places.read().is_ok_and(|places| places.contains_key(&va)) {
            return;
        }
        if let Ok(mut places) = self.places.write() {
            places.entry(va).or_insert(arm);
        }
    }

    /// Whether the code at `va` is ARM code; `None` before the first place.
    pub fn at(&self, va: u64) -> Option<bool> {
        self.places.read().ok()?.range(..=va).next_back().map(|(_, &arm)| arm)
    }

    /// Whether the map has places of both sets.
    pub fn both(&self) -> bool {
        self.places.read().is_ok_and(|places| places.values().any(|&arm| arm) && places.values().any(|&arm| !arm))
    }
}

/// The repeat and lock prefixes of an instruction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Prefixes {
    pub rep: bool,
    pub repne: bool,
    pub lock: bool,
    /// The instruction runs under a condition of its own, as every instruction of ARM code may:
    /// a call it makes may not happen.
    #[serde(skip)]
    pub conditional: bool,
    /// Under that condition the instruction leaves the function, by a return or a jump through a
    /// register; its flow says only what happens when the condition fails.
    #[serde(skip)]
    pub conditional_leave: bool,
}

/// One decoded instruction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Instr {
    pub address: u64,
    pub len: u8,
    /// The mnemonic in the decoder's spelling, lower case.
    pub mnemonic: String,
    /// The whole instruction as text, in the architecture's usual syntax.
    pub text: String,
    pub operands: Vec<Operand>,
    pub flow: Flow,
    pub prefixes: Prefixes,
    /// Every register the instruction reads or writes, the implicit ones included.
    pub registers: Vec<(Register, Access)>,
    pub reads_memory: bool,
    pub writes_memory: bool,
    /// The decoder's own code for the exact form of the instruction, for the lift.
    pub code: u32,
}

impl Instr {
    /// The address after the instruction, wrapping at the top of the space as the instruction
    /// pointer does.
    pub fn end(&self) -> u64 {
        self.address.wrapping_add(u64::from(self.len))
    }

    /// The memory operand the instruction branches or calls through, if any.
    pub fn indirect_operand(&self) -> Option<&Operand> {
        match self.flow {
            Flow::Jump(Target::Indirect) | Flow::Call(Target::Indirect) => self.operands.first(),
            _ => None,
        }
    }

    /// The constant memory address of an operand, as a RIP-relative or absolute access gives it.
    pub fn absolute_memory_operand(&self) -> Option<u64> {
        self.operands.iter().find_map(|o| match o {
            Operand::Mem(m) => m.absolute,
            _ => None,
        })
    }

    pub fn reads(&self, register: Register) -> bool {
        self.registers.iter().any(|(r, a)| r.full == register.full && a.reads())
    }

    pub fn writes(&self, register: Register) -> bool {
        self.registers.iter().any(|(r, a)| r.full == register.full && a.writes())
    }
}

/// A decoder of one architecture. A decoder is shared by the threads that lift an image's
/// functions in parallel.
pub trait Arch: Send + Sync {
    fn name(&self) -> &'static str;

    /// The width of an address, in bits.
    fn address_bits(&self) -> u16;

    fn stack_pointer(&self) -> Register;

    fn instruction_pointer(&self) -> Register;

    /// Decodes the instruction at `address` from `bytes`, which start at that address. `None` when
    /// the bytes hold no valid instruction.
    fn decode(&self, bytes: &[u8], address: u64) -> Option<Instr>;

    /// Completes `instr` from the instructions before it in the same block: an address an
    /// architecture forms in two instructions (ARM64's `adrp` page and the `ldr` or `add` that
    /// adds the low bits) becomes the absolute address of the second. Nothing by default.
    fn link(&self, before: &[Instr], instr: &mut Instr) {
        let _ = (before, instr);
    }

    /// When the flow of `instr`, decoded by this architecture, takes effect: at once by default,
    /// after its delay slot on a machine that has them.
    fn delay(&self, instr: &Instr) -> Delay {
        let _ = instr;
        Delay::None
    }

    /// Decodes instructions in sequence until the bytes end or an instruction fails to decode. The
    /// address wraps at the top of the space, as the instruction pointer does.
    fn decode_all(&self, bytes: &[u8], address: u64) -> Vec<Instr> {
        let mut out: Vec<Instr> = Vec::new();
        let mut offset = 0usize;
        while offset < bytes.len() {
            let Some(mut instr) = self.decode(&bytes[offset..], address.wrapping_add(offset as u64)) else { break };
            self.link(&out, &mut instr);
            offset += instr.len as usize;
            out.push(instr);
        }
        out
    }
}
