//! Strings decrypted at lift time. An obfuscator keeps a string as ciphertext and calls a
//! decryptor at the use, which writes the plaintext into a buffer of the caller's frame. A call
//! to a lifted function whose arguments are constants, addresses in read-only data and pointers
//! into the caller's own frame, and whose result nothing reads, is run here in the IR
//! interpreter, over a copy of the module that holds the read-only globals alone: the run may
//! read those and write the frame buffers, and nothing else is mapped, so any other access, any
//! call out and any change to the read-only data stops it. Two runs over buffers filled with
//! different bytes must write the same bytes, which shows the result independent of what the
//! buffer held (a dependence the program could only have through uninitialized memory). The call
//! then becomes a copy of the plaintext from a new constant, which the output prints as the
//! literal it is.

use std::collections::HashMap;

use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{CallConv, Callee, Cast, Const, Declaration, FnType, Global, Inst, Instruction, Linkage, Module, Operand, Type};

use super::known::{self, Root};

/// How many instructions a decryptor may run.
const LIMIT: u64 = 1 << 20;

/// A host that answers no call: a decryptor needs none.
struct NoHost;

impl Host for NoHost {
    fn call(&mut self, _vm: &mut Vm<'_>, name: &str, _ty: &FnType, _args: &[Value]) -> Result<Value, Stop> {
        Err(Stop::fault(format!("a call to {name} in a run at lift time")))
    }
}

/// An argument of a call to run.
enum Arg {
    Int(u128),
    /// A byte of a read-only global.
    Data(String, i64),
    /// A pointer into the caller's frame: the operand as the call passes it, and the bytes from
    /// it to the end of its slot.
    Buffer(Type, Operand, u64),
}

/// A call worth running: where it is, and its arguments.
struct Candidate {
    function: usize,
    block: usize,
    index: usize,
    callee: String,
    args: Vec<Arg>,
}

/// Runs the decryptor calls of `module` and replaces them; returns the functions it changed.
pub fn run(module: &mut Module) -> Vec<usize> {
    let candidates = candidates(module);
    if candidates.is_empty() {
        return Vec::new();
    }
    // The copy the runs see: the read-only globals, placed wherever the interpreter likes.
    let mut copy = module.clone();
    copy.globals.retain(|g| g.constant);
    for g in &mut copy.globals {
        g.address = None;
    }
    let statics_len = copy.globals.iter().map(|g| g.ty.size(8).unwrap_or(0) + 16).sum::<u64>()
        + 16 * (copy.functions.len() + copy.declarations.len()) as u64
        + (1 << 20);
    let frames_len = 8u64 << 20;
    let mut statics = vec![0u8; statics_len as usize];
    let mut frames = vec![0u8; frames_len as usize];
    let (statics_at, frames_at) = (statics.as_mut_ptr() as u64, frames.as_mut_ptr() as u64);
    let mut memory = Memory::new();
    memory.add_region(Region::new(statics_at, statics_len, "lift-time statics"));
    memory.add_region(Region::new(frames_at, frames_len, "lift-time frames"));
    let mut vm = Vm::new(&copy, memory);
    vm.limit = LIMIT;
    vm.set_static_region(statics_at, statics_len);
    vm.set_frame_region(frames_at, frames_len);
    if vm.place().is_err() {
        return Vec::new();
    }
    let Ok(baseline) = vm.memory.read_vec(statics_at, statics_len) else { return Vec::new() };

    let mut results: Vec<(Candidate, Vec<Option<Vec<u8>>>)> = Vec::new();
    for candidate in candidates {
        let sizes: Vec<u64> = candidate.args