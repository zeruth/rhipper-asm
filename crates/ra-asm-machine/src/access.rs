//! What an instruction reads and writes, from the same semantics the lift runs: a [`Builder`] that
//! records the registers its row's function reads and writes and whether it loads or stores, so
//! the summary is never written by hand and cannot say other than the lift does.
//!
//! A register is read when the function reads it before writing it for sure; what follows a
//! conditional branch happens only when the branch is not taken, so a register touched only there
//! is read or written on some paths. The zero register counts as the row names it: whether it
//! holds anything is the state's to say, not the row's.

use ra_asm_arch::Access;

use crate::instr::Machine;
use crate::sem::{Builder, Cond, Fields, Op, Sem, Val};

/// The registers an instruction reads and writes, by their number in the machine's register file
/// and in the order the semantics first touches them, and whether it reads or writes memory.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub registers: Vec<(u32, Access)>,
    pub reads_memory: bool,
    pub writes_memory: bool,
}

impl Machine {
    /// What the instruction `word` at `address` reads and writes, by its row's semantics; a row
    /// without semantics lifts to a stop and touches nothing. `None` when no row accepts the word.
    pub fn access(&self, word: u32, address: u64) -> Option<Summary> {
        let decoded = self.table.decode(word)?;
        let mut recorder = Recorder::default();
        if let Some(Sem(lift)) = decoded.row.sem {
            lift(&mut recorder, &Fields::new(self, decoded.row, &decoded.values, address));
        }
        Some(recorder.summary())
    }
}

/// How the semantics has touched one register so far: whether it reads the value the instruction
/// starts with and whether it writes, each `Some(true)` when only on some paths.
#[derive(Debug, Clone, Copy)]
struct Touch {
    register: u32,
    read: Option<bool>,
    write: Option<bool>,
}

/// The builder that records instead of building: every value is a fresh number with no meaning.
#[derive(Debug, Default)]
struct Recorder {
    touches: Vec<Touch>,
    reads_memory: bool,
    writes_memory: bool,
    values: u32,
    /// Whether a conditional branch came before, so what follows runs on some paths only.
    conditional: bool,
}

/// `was` and `now` together: on every path when either is.
fn either(was: Option<bool>, now: bool) -> bool {
    was.map_or(now, |was| was && now)
}

impl Recorder {
    fn fresh(&mut self) -> Val {
        let value = Val(self.values);
        self.values += 1;
        value
    }

    fn touch(&mut self, register: u32) -> &mut Touch {
        let at = if let Some(at) = self.touches.iter().position(|touch| touch.register == register) {
            at
        } else {
            self.touches.push(Touch { register, read: None, write: None });
            self.touches.len() - 1
        };
        &mut self.touches[at]
    }

    fn summary(&self) -> Summary {
        let registers = self
            .touches
            .iter()
            .filter_map(|touch| {
                let access = match (touch.read, touch.write) {
                    (Some(_), Some(_)) => Access::ReadWrite,
                    (Some(false), None) => Access::Read,
                    (Some(true), None) => Access::CondRead,
                    (None, Some(false)) => Access::Write,
                    (None, Some(true)) => Access::CondWrite,
                    (None, None) => return None,
                };
                Some((touch.register, access))
            })
            .collect();
        Summary { registers, reads_memory: self.reads_memory, writes_memory: self.writes_memory }
    }
}

impl Builder for Recorder {
    fn imm(&mut self, _: u16, _: i64) -> Val {
        self.fresh()
    }

    fn r(&mut self, register: u32) -> Val {
        let conditional = self.conditional;
        let touch = self.touch(register);
        // A read after a write on every path sees the instruction's own value, not the one it starts with.
        if touch.write != Some(false) {
            touch.read = Some(either(touch.read, conditional));
        }
        self.fresh()
    }

    fn set(&mut self, register: u32, _: Val) {
        let conditional = self.conditional;
        let touch = self.touch(register);
        touch.write = Some(either(touch.write, conditional));
    }

    fn op(&mut self, _: Op, _: u16, _: Val, _: Val) -> Val {
        self.fresh()
    }

    fn cmp(&mut self, _: Cond, _: u16, _: Val, _: Val) -> Val {
        self.fresh()
    }

    fn sext(&mut self, _: u16, _: Val) -> Val {
        self.fresh()
    }

    fn zext(&mut self, _: u16, _: Val) -> Val {
        self.fresh()
    }

    fn trunc(&mut self, _: u16, _: Val) -> Val {
        self.fresh()
    }

    fn load(&mut self, _: u16, _: Val) -> Val {
        self.reads_memory = true;
        self.fresh()
    }

    fn store(&mut self, _: u16, _: Val, _: Val) {
        self.writes_memory = true;
    }

    fn branch_if(&mut self, _: Val, _: Val) {
        self.conditional = true;
    }

    fn jump(&mut self, _: Val) {}

    fn call(&mut self, _: Val) {}

    fn trap(&mut self, _: u64) {}
}
