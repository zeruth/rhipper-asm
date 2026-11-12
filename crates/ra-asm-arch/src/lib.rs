//! What every architecture gives the analysis: an [`Instr`] record with its operands, its flow and
//! the registers it touches, and the [`Arch`] trait that decodes one. The x86 decoder fills these
//! from iced-x86; a second architecture fills them from its own decoder. The lift of each
//! architecture keeps its decoder's own instruction beside the record, because the semantics are
//! not neutral.

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

/// The repeat and lock prefixes of an instruction.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Prefixes {
    pub rep: bool,
    pub repne: bool,
    pub lock: bool,
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
    pub fn end(&self) -> u64 {
        self.address + u64::from(self.len)
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

    /// Decodes instructions in sequence until the bytes end or an instruction fails to decode.
    fn decode_all(&self, bytes: &[u8], address: u64) -> Vec<Instr> {
        let mut out: Vec<Instr> = Vec::new();
        let mut offset = 0usize;
        while offset < bytes.len() {
            let Some(mut instr) = self.decode(&bytes[offset..], address + offset as u64) else { break };
            self.link(&out, &mut instr);
            offset += instr.len as usize;
            out.push(instr);
        }
        out
    }
}
