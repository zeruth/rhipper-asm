//! A lifted SH-4 program run over the IR's interpreter: the program's segments at their own
//! addresses, a stack, the state, and the Linux system calls a program with no C library makes
//! to print and to end (`trapa`: the call's number in `r3`, its arguments from `r4`, the result
//! in `r0`).

use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{FnType, Module};

use crate::lift::{Code, MAIN, RUN, SEGMENT, SLOTS, TRAP, TRAPA, lift};

/// What is wrong with a program, or how a run of one stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// A part of a program's file the system maps: where, its bytes, and the size it has in memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub size: u64,
    pub executable: bool,
}

/// A little-endian SH program as its ELF file has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub entry: u64,
    pub segments: Vec<Segment>,
}

impl Program {
    /// Reads a little-endian 32-bit ELF executable for SuperH: its entry and the segments it
    /// loads.
    ///
    /// # Errors
    /// When the bytes are no such file, or a header points outside them.
    pub fn parse(file: &[u8]) -> Result<Self, Error> {
        let bad = |what: &str| Error(format!("no SH-4 program: {what}"));
        // A place past the file's end is refused before anything is added to it.
        let at = |offset: u64, len: u64| {
            let end = offset.checked_add(len).filter(|&end| end <= file.len() as u64);
            end.map(|end| &file[offset as usize..end as usize]).ok_or_else(|| bad("a header ends outside the file"))
        };
        let number = |offset: u64, len: u64| -> Result<u64, Error> {
            Ok(at(offset, len)?.iter().rev().fold(0u64, |value, &byte| value << 8 | u64::from(byte)))
        };
        if at(0, 4)? != b"\x7fELF" || at(4, 2)? != [1, 1] {
            return Err(bad("it is no little-endian 32-bit ELF file"));
        }
        if number(18, 2)? != 42 {
            return Err(bad("its machine is not SuperH"));
        }
        let (entry, table, entry_size, count) = (number(24, 4)?, number(28, 4)?, number(42, 2)?, number(44, 2)?);
        let mut segments = Vec::new();
        for n in 0..count {
            let header = table + n * entry_size;
            let (kind, offset, address, in_file, size, flags) = (
                number(header, 4)?,
                number(header + 4, 4)?,
                number(header + 8, 4)?,
                number(header + 16, 4)?,
                number(header + 20, 4)?,
                number(header + 24, 4)?,
            );
            if kind == 1 && size != 0 {
                segments.push(Segment { address, bytes: at(offset, in_file)?.to_vec(), size, executable: flags & 1 != 0 });
            }
        }
        Ok(Self { entry, segments })
    }

    /// The program's code lifted: the first executable segment, as [`lift`] makes it. The
    /// segment starts with the file's headers where the file maps them, and the lift reads those
    /// as it reads the constants between the functions: words that are never run.
    ///
    /// # Errors
    /// When the program has no executable segment.
    pub fn lift(&self) -> Result<Module, Error> {
        use ra_asm_ir::{CallConv, Callee, Const, Declaration, FunctionBuilder, Global, Linkage, Operand, Terminator, Type};
        let code = self.segments.iter().find(|s| s.executable).ok_or_else(|| Error("the program has no code".to_owned()))?;
        let mut module = lift(Code { address: code.address, bytes: &code.bytes });
        // The whole program: its segments as data the runtime is handed, then the code from
        // the entry.
        let segment_ty = FnType::new(Type::Void, vec![Type::I64, Type::Ptr, Type::I64, Type::I64]);
        module.declare(Declaration { name: SEGMENT.to_owned(), ty: segment_ty.clone(), conv: CallConv::C, attrs: Vec::new() });
        let mut b = FunctionBuilder::new(MAIN, FnType::new(Type::Void, vec![Type::Ptr]), &["state"]);
        let block = b.block("entry");
        for (n, segment) in self.segments.iter().enumerate() {
            let name = format!("sh_segment_{n}");
            module.globals.push(Global {
                name: name.clone(),
                ty: Type::array(segment.bytes.len().max(1) as u64, Type::I8),
                init: Some(Const::Bytes(if segment.bytes.is_empty() { vec![0] } else { segment.bytes.clone() })),
                constant: true,
                linkage: Linkage::Internal,
                align: 1,
                address: None,
            });
            let args = vec![
                (Type::I64, Operand::Int(i128::from(segment.address))),
                (Type::Ptr, Operand::Global(name)),
                (Type::I64, Operand::Int(segment.bytes.len() as i128)),
                (Type::I64, Operand::Int(i128::from(segment.size))),
            ];
            b.call(block, segment_ty.clone(), Callee::Direct(SEGMENT.to_owned()), args, "");
        }
        let args = vec![(Type::Ptr, Operand::from(b.param(0))), (Type::I64, Operand::Int(i128::from(self.entry)))];
        b.call(block, FnType::new(Type::Void, vec![Type::Ptr, Type::I64]), Callee::Direct(RUN.to_owned()), args, "");
        b.terminate(block, Terminator::Ret(None));
        module.functions.push(b.finish());
        Ok(module)
    }
}

/// What a run printed and how it ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    pub out: Vec<u8>,
    /// The code the program ended itself with; `None` when the run ended another way.
    pub exit: Option<i32>,
    pub steps: u64,
}

const STACK: (u64, u64) = (0x7ff0_0000, 0x10_0000);
const STATICS: (u64, u64) = (0x6000_0000, 0x10_0000);
const FRAMES: (u64, u64) = (0x6800_0000, 0x10_0000);

struct Linux {
    out: Vec<u8>,
}

impl Host for Linux {
    fn call(&mut self, vm: &mut Vm<'_>, name: &str, _ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        let first = args.first().map_or(0, Value::as_u64);
        match name {
            TRAPA => {
                let register = |vm: &mut Vm<'_>, n: u64| vm.memory.read_u32(first + 8 * n).map(u64::from);
                let (number, a, b, c) = (register(vm, 3)?, register(vm, 4)?, register(vm, 5)?, register(vm, 6)?);
                let result: u32 = match number {
                    // write
                    4 if a == 1 || a == 2 => {
                        self.out.extend(vm.memory.read_vec(b, c)?);
                        c as u32
                    }
                    // -EBADF
                    4 => 9u32.wrapping_neg(),
                    // exit, exit_group
                    1 | 252 => return Err(Stop::Exit(a as i32)),
                    _ => return Err(Stop::fault(format!("the system call {number} is not interpreted"))),
                };
                vm.memory.write_u32(first, result)?;
                Ok(Value::Int(0))
            }
            // A segment: its bytes go where the program has them, which the run mapped.
            SEGMENT => {
                let bytes = vm.memory.read_vec(args[1].as_u64(), args[2].as_u64())?;
                vm.memory.write(first, &bytes)?;
                Ok(Value::Int(0))
            }
            TRAP => Err(Stop::fault(format!("no instruction at {first:#x}"))),
            _ => Err(Stop::fault(format!("the runtime has no function {name}"))),
        }
    }
}

/// Lifts `program` and runs it from its entry, to its `exit` or to `limit` instructions of the
/// IR (zero for no limit).
///
/// # Errors
/// When the program has no code, or the run stops other than by the program's `exit`.
pub fn run(program: &Program, limit: u64) -> Result<Outcome, Error> {
    run_module(program, &program.lift()?, limit)
}

fn run_module(program: &Program, module: &Module, limit: u64) -> Result<Outcome, Error> {
    let mut memory = Memory::new();
    for (name, (start, len)) in [("stack", STACK), ("statics", STATICS), ("frames", FRAMES)] {
        memory.add_region(Region::owned(start, len, name));
    }
    for segment in &program.segments {
        // The system maps whole pages.
        let start = segment.address & !0xfff;
        let end = (segment.address + segment.size.max(segment.bytes.len() as u64)).next_multiple_of(0x1000);
        memory.add_region(Region::owned(start, end - start, "segment"));
    }
    let mut vm = Vm::new(module, memory);
    vm.limit = limit;
    vm.set_static_region(STATICS.0, STATICS.1);
    vm.set_frame_region(FRAMES.0, FRAMES.1);
    let stopped = |stop: Stop| Error(format!("the run stopped: {stop:?}"));
    vm.place().map_err(stopped)?;
    let state = vm.allocate_static(SLOTS * 8, 8).map_err(stopped)?;
    vm.memory.fill(state, SLOTS * 8, 0).map_err(stopped)?;
    vm.memory.write_u64(state + 15 * 8, STACK.0 + STACK.1 - 64).map_err(stopped)?;
    let mut host = Linux { out: Vec::new() };
    let exit = match vm.call_named(&mut host, MAIN, vec![Value::Int(u128::from(state))]) {
        Ok(_) => None,
        Err(Stop::Exit(code)) => Some(code),
        Err(stop) => return Err(stopped(stop)),
    };
    Ok(Outcome { out: host.out, exit, steps: vm.steps })
}
