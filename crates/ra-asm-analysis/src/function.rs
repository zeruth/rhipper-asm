//! A function and its blocks.

use ra_asm_arch::Instr;
use ra_asm_pe::Image;
use serde::Serialize;

/// How the analysis found a function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    Entry,
    Export(String),
    TlsCallback,
    ExceptionTable,
    /// A direct call from the function at the address.
    Call(u64),
    /// A jump out of the function at the address.
    TailCall(u64),
    /// A pointer-sized slot at the address holds the entry.
    Pointer(u64),
    /// A slot of the vtable at the address holds the entry.
    Vtable(u64),
    /// Code nothing names, after the `int 3` fill that follows the function at the address: a
    /// function the linker kept without a caller.
    Gap(u64),
}

/// A slot of a class's vtable that holds the function.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VirtualSlot {
    pub class: String,
    /// The offset of the vtable's subobject in the complete object: zero for the primary vtable.
    pub offset: u32,
    pub slot: usize,
}

/// What a function is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Normal,
    /// `jmp [slot]` to an import: the name is the import's.
    ImportThunk(String),
    /// A function of a C runtime linked into the image, known from the library its object came
    /// from: the host's runtime supplies it under the name, and its body is not ported.
    Runtime(String),
}

/// How a block ends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Terminator {
    /// The block runs into the next one: another block starts there.
    FallThrough(u64),
    Jump(u64),
    CondJump {
        taken: u64,
        fallthrough: u64,
    },
    /// An indirect jump through a table with the targets it resolved to. `cases` maps each index
    /// to its target; `index` is the register that holds the index at `load_at`, the instruction
    /// that reads the table entry (the jump itself on x86), so a lift can switch on the index and
    /// never read the table.
    Switch {
        table: u64,
        targets: Vec<u64>,
        index: ra_asm_arch::Register,
        load_at: u64,
        cases: Vec<(u64, u64)>,
    },
    /// An indirect jump the analysis could not resolve.
    IndirectJump,
    /// A jump to another function.
    TailCall(u64),
    /// A jump through an import slot: a tail call to the import.
    ImportTailCall(String),
    /// A call to a function that never returns.
    NoReturnCall(u64),
    /// A call through an import slot to an import that never returns.
    NoReturnImportCall(String),
    Return,
    Trap,
    Halt,
    /// The bytes hold no instruction.
    Invalid,
}

impl Terminator {
    /// The blocks control can reach next, inside the function.
    pub fn successors(&self) -> Vec<u64> {
        match self {
            Self::FallThrough(t) | Self::Jump(t) => vec![*t],
            Self::CondJump { taken, fallthrough } => vec![*taken, *fallthrough],
            Self::Switch { targets, .. } => targets.clone(),
            _ => Vec::new(),
        }
    }

    pub fn is_return(&self) -> bool {
        matches!(self, Self::Return)
    }
}

/// A basic block: a straight run of instructions with one entry and one exit.
#[derive(Debug, Clone, Serialize)]
pub struct Block {
    pub start: u64,
    /// The first address after the block.
    pub end: u64,
    pub instrs: Vec<Instr>,
    pub terminator: Terminator,
}

impl Block {
    pub fn contains(&self, va: u64) -> bool {
        va >= self.start && va < self.end
    }

    pub fn successors(&self) -> Vec<u64> {
        self.terminator.successors()
    }
}

/// A function: its blocks in address order and what the analysis knows about it.
#[derive(Debug, Clone, Serialize)]
pub struct Function {
    pub entry: u64,
    pub name: String,
    /// The name the build's symbols (a PDB or a link map) give the function, as they have it:
    /// `_rhtest_run`, `?area@Rect@@UEBAHXZ`; `None` when nothing names it.
    pub symbol: Option<String>,
    pub kind: Kind,
    pub blocks: Vec<Block>,
    /// The functions it calls directly, in first-call order.
    pub calls: Vec<u64>,
    /// The imports it calls or jumps to, by display name.
    pub import_calls: Vec<String>,
    /// The functions it jumps to.
    pub tail_calls: Vec<u64>,
    pub noreturn: bool,
    pub origins: Vec<Origin>,
    /// The string literals its instructions address, in first-use order: the address and the text.
    pub strings: Vec<(u64, String)>,
    /// The calls through a register that holds an import's slot value, by the address of the
    /// call: calls to that import, with its convention.
    pub register_import_calls: std::collections::HashMap<u64, String>,
    /// The vtable slots that hold the function: a virtual function of these classes.
    pub virtuals: Vec<VirtualSlot>,
}

impl Function {
    pub fn block_at(&self, start: u64) -> Option<&Block> {
        self.blocks.iter().find(|b| b.start == start)
    }

    pub fn block_containing(&self, va: u64) -> Option<&Block> {
        self.blocks.iter().find(|b| b.contains(va))
    }

    pub fn contains(&self, va: u64) -> bool {
        self.blocks.iter().any(|b| b.contains(va))
    }

    /// The lowest address of the function.
    pub fn start(&self) -> u64 {
        self.blocks.first().map_or(self.entry, |b| b.start)
    }

    /// The first address after the function's last block.
    pub fn end(&self) -> u64 {
        self.blocks.last().map_or(self.entry, |b| b.end)
    }

    /// The span from the first block to the end of the last one, as a listing shows it.
    pub fn size(&self) -> u64 {
        self.end() - self.start()
    }

    /// The bytes the blocks hold.
    pub fn code_bytes(&self) -> u64 {
        self.blocks.iter().map(|b| b.end - b.start).sum()
    }

    pub fn instr_count(&self) -> usize {
        self.blocks.iter().map(|b| b.instrs.len()).sum()
    }

    pub fn is_thunk(&self) -> bool {
        matches!(self.kind, Kind::ImportThunk(_))
    }
}

/// `F_<address>`: the name of a function nothing in the image names, the same the lift and the
/// backends give it.
pub fn default_name(va: u64) -> String {
    format!("F_{va:08x}")
}

/// The name of a function: the export name, `entry`, the import of a thunk, else the default.
pub fn name_for(image: &Image, function: &Function) -> String {
    for origin in &function.origins {
        if let Origin::Export(name) = origin {
            return name.clone();
        }
    }
    if let Kind::ImportThunk(import) = &function.kind {
        return format!("thunk_{import}");
    }
    if image.entry_point() == Some(function.entry) {
        return "entry".to_owned();
    }
    default_name(function.entry)
}
