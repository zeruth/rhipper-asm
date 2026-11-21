//! A lifted RISC-V program run over the IR's interpreter: the program's segments at their own
//! addresses, a stack, the state, and the Linux system calls a program with no C library makes
//! to print and to end.

use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{FnType, Module};

use crate::lift::{Code, ECALL, Functions, MAIN, RUN, SEGMENT, TRAP, lift};
use crate::{Error, Width};

/// A part of a program's file the system maps: where, its bytes, and the size it has in memory
/// (the rest zero).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub size: u64,
    pub executable: bool,
}

/// A RISC-V program as its ELF file has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub width: Width,
    pub entry: u64,
    pub segments: Vec<Segment>,
    /// The functions the file's symbol table names, by their addresses; none for a stripped file.
    pub symbols: Functions,
}

/// The bytes of `file` from `offset`, at most `len` of them, and none when that lies outside it.
fn at_bytes(file: &[u8], offset: usize, len: usize) -> &[u8] {
    file.get(offset..).map_or(&[][..], |rest| &rest[..len.min(rest.len())])
}

impl Program {
    /// Reads a little-endian ELF executable for RISC-V, of either width: its entry and the
    /// segments it loads.
    ///
    /// # Errors
    /// When the bytes are no such file, or a header points outside them.
    pub fn parse(file: &[u8]) -> Result<Self, Error> {
        let bad = |what: &str| Error(format!("no RISC-V program: {what}"));
        // Every number here is the file's own: a place past the file's end is refused before
        // anything is added to it, so no sum of two of them overflows.
        let at = |offset: usize, len: usize| {
            let end = offset.checked_add(len).filter(|&end| offset <= file.len() && end <= file.len());
            end.map(|end| &file[offset..end]).ok_or_else(|| bad("a header ends outside the file"))
        };
        let inside = |offset: u64| if offset <= file.len() as u64 { Ok(offset) } else { Err(bad("a table lies outside the file")) };
        let number = |offset: usize, len: usize| -> Result<u64, Error> {
            Ok(at(offset, len)?.iter().rev().fold(0u64, |value, &byte| value << 8 | u64::from(byte)))
        };
        if at(0, 4)? != b"\x7fELF" || at(5, 1)? != [1] {
            return Err(bad("it is no little-endian ELF file"));
        }
        if number(18, 2)? != 243 {
            return Err(bad("its machine is not RISC-V"));
        }
        let wide = at(4, 1)? == [2];
        // The fields of the two widths, in the order the file header and a program header keep them.
        let (entry, table, entry_size, count) = if wide {
            (number(24, 8)?, number(32, 8)?, number(54, 2)?, number(56, 2)?)
        } else {
            (number(24, 4)?, number(28, 4)?, number(42, 2)?, number(44, 2)?)
        };
        let table = inside(table)?;
        let mut segments = Vec::new();
        for n in 0..count {
            let header = (table + n * entry_size) as usize;
            let (kind, flags, offset, address, in_file, size) = if wide {
                (
                    number(header, 4)?,
                    number(header + 4, 4)?,
                    number(header + 8, 8)?,
                    number(header + 16, 8)?,
                    number(header + 32, 8)?,
                    number(header + 40, 8)?,
                )
            } else {
                (
                    number(header, 4)?,
                    number(header + 24, 4)?,
                    number(header + 4, 4)?,
                    number(header + 8, 4)?,
                    number(header + 16, 4)?,
                    number(header + 20, 4)?,
                )
            };
            // A segment the system loads, with memory of its own.
            if kind == 1 && size != 0 {
                segments.push(Segment {
                    address,
                    bytes: at(inside(offset)? as usize, inside(in_file)? as usize)?.to_vec(),
                    size,
                    executable: flags & 1 != 0,
                });
            }
        }
        // The functions the symbol table names, where the file has one: a stripped program has
        // none, and a table that points outside the file is as good as none.
        let mut symbols = Functions::new();
        let named = || -> Result<Functions, Error> {
            let mut found = Functions::new();
            let (table, entry_size, count) =
                if wide { (number(40, 8)?, number(58, 2)?, number(60, 2)?) } else { (number(32, 4)?, number(46, 2)?, number(48, 2)?) };
            let table = inside(table)?;
            let section = |n: u64| -> Result<(u64, u64, u64, u64), Error> {
                let n = n.min(0xffff);
                let header = (table + n * entry_size) as usize;
                Ok(if wide {
                    (number(header + 4, 4)?, number(header + 24, 8)?, number(header + 32, 8)?, number(header + 40, 4)?)
                } else {
                    (number(header + 4, 4)?, number(header + 16, 4)?, number(header + 20, 4)?, number(header + 24, 4)?)
                })
            };
            for n in 0..count {
                let (kind, offset, size, link) = section(n)?;
                if kind != 2 {
                    continue;
                }
                let (_, strings, strings_size, _) = section(link)?;
                let (offset, size, strings, strings_size) = (inside(offset)?, inside(size)?, inside(strings)?, inside(strings_size)?);
                let each = if wide { 24 } else { 16 };
                for symbol in 0..size / each {
                    let at = (offset + symbol * each) as usize;
                    let (name, info, value) = if wide {
                        (number(at, 4)?, number(at + 4, 1)?, number(at + 8, 8)?)
                    } else {
                        (number(at, 4)?, number(at + 12, 1)?, number(at + 4, 4)?)
                    };
                    if info & 0xf != 2 || name >= strings_size {
                        continue;
                    }
                    let text = at_bytes(file, (strings + name) as usize, (strings_size - name) as usize);
                    let text: Vec<u8> = text.iter().copied().take_while(|&byte| byte != 0).collect();
                    found.insert(value & !1, String::from_utf8_lossy(&text).into_owned());
                }
            }
            Ok(found)
        };
        if let Ok(found) = named() {
            symbols = found;
        }
        Ok(Self { width: if wide { Width::Rv64 } else { Width::Rv32 }, entry, segments, symbols })
    }

    /// The program's code lifted: the first executable segment, as [`lift`] makes it.
    ///
    /// # Errors
    /// When the program has no executable segment.
    pub fn lift(&self) -> Result<Module, Error> {
        self.lift_as(false)
    }

    /// The program lifted as its functions, as [`crate::lift::lift_functions`] makes them: the
    /// same program, a function of the IR for each of its own.
    ///
    /// # Errors
    /// When the program has no executable segment.
    pub fn lift_functions(&self) -> Result<Module, Error> {
        self.lift_as(true)
    }

    fn lift_as(&self, functions: bool) -> Result<Module, Error> {
        use ra_asm_ir::{CallConv, Callee, Const, Declaration, FunctionBuilder, Global, Linkage, Operand, Terminator, Type};
        let code = self.segments.iter().find(|s| s.executable).ok_or_else(|| Error("the program has no code".to_owned()))?;
        let code = Code { address: code.address, bytes: &code.bytes };
        let mut module =
            if functions { crate::lift::lift_named(code, self.width, self.entry, &self.symbols) } else { lift(code, self.width) };
        // The whole program: its segments as data the runtime is handed, then the code from
        // the entry.
        let segment_ty = FnType::new(Type::Void, vec![Type::I64, Type::Ptr, Type::I64, Type::I64]);
        module.declare(Declaration { name: SEGMENT.to_owned(), ty: segment_ty.clone(), conv: CallConv::C, attrs: Vec::new() });
        let mut b = FunctionBuilder::new(MAIN, FnType::new(Type::Void, vec![Type::Ptr]), &["state"]);
        let block = b.block("entry");
        for (n, segment) in self.segments.iter().enumerate() {
            let name = format!("rv_segment_{n}");
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
        let run_ty = FnType::new(Type::Void, vec![Type::Ptr, Type::I64]);
        let args = vec![(Type::Ptr, Operand::from(b.param(0))), (Type::I64, Operand::Int(i128::from(self.entry)))];
        b.call(block, run_ty, Callee::Direct(RUN.to_owned()), args, "");
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
            ECALL => {
                // The call's number in a7, its arguments from a0, its result in a0.
                let register = |vm: &mut Vm<'_>, n: u64| vm.memory.read_u64(first + 8 * n);
                let (number, a0, a1, a2) = (register(vm, 17)?, register(vm, 10)?, register(vm, 11)?, register(vm, 12)?);
                let result = match number {
                    // write
                    64 if a0 == 1 || a0 == 2 => {
                        self.out.extend(vm.memory.read_vec(a1, a2)?);
                        a2
                    }
                    // -EBADF
                    64 => 9u64.wrapping_neg(),
                    // exit, exit_group
                    93 | 94 => return Err(Stop::Exit(a0 as i32)),
                    _ => return Err(Stop::fault(format!("the system call {number} is not interpreted"))),
                };
                vm.memory.write_u64(first + 80, result)?;
                Ok(Value::Int(0))
            }
            // A segment: its bytes go where the program has them, which the run mapped.
            SEGMENT => {
                let (bytes, in_file) = (args[1].as_u64(), args[2].as_u64());
                let bytes = vm.memory.read_vec(bytes, in_file)?;
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
/// When the program has no code, or the run stops other than by the program's `exit`: a system
/// call the runtime has not, an address that is no instruction's, the limit.
pub fn run(program: &Program, limit: u64) -> Result<Outcome, Error> {
    run_module(program, &program.lift()?, limit)
}

/// Runs `program` lifted as its functions ([`Program::lift_functions`]).
///
/// # Errors
/// As [`run`].
pub fn run_functions(program: &Program, limit: u64) -> Result<Outcome, Error> {
    run_module(program, &program.lift_functions()?, limit)
}

fn run_module(program: &Program, module: &Module, limit: u64) -> Result<Outcome, Error> {
    let mut memory = Memory::new();
    for (name, (start, len)) in [("stack", STACK), ("statics", STATICS), ("frames", FRAMES)] {
        memory.add_region(Region::owned(start, len, name));
    }
    for segment in &program.segments {
        memory.add_region(Region::owned(segment.address, segment.size.max(segment.bytes.len() as u64), "segment"));
    }
    let mut vm = Vm::new(module, memory);
    vm.limit = limit;
    vm.set_static_region(STATICS.0, STATICS.1);
    vm.set_frame_region(FRAMES.0, FRAMES.1);
    let stopped = |stop: Stop| Error(format!("the run stopped: {stop:?}"));
    vm.place().map_err(stopped)?;
    // The state: thirty-two registers, the stack pointer at the stack's top.
    let state = vm.allocate_static(32 * 8, 8).map_err(stopped)?;
    vm.memory.fill(state, 32 * 8, 0).map_err(stopped)?;
    vm.memory.write_u64(state + 2 * 8, STACK.0 + STACK.1 - 64).map_err(stopped)?;
    let mut host = Linux { out: Vec::new() };
    let exit = match vm.call_named(&mut host, MAIN, vec![Value::Int(u128::from(state))]) {
        Ok(_) => None,
        Err(Stop::Exit(code)) => Some(code),
        Err(stop) => return Err(stopped(stop)),
    };
    Ok(Outcome { out: host.out, exit, steps: vm.steps })
}
