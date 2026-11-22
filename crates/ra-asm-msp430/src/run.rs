//! A lifted MSP430 program run over the IR's interpreter: the machine's sixty-four kilobytes
//! with the program's segments in them, the state, and the little of a microcontroller's world
//! a program with no library needs to print and to end.
//!
//! A microcontroller has no system to call. The program prints by storing a byte to a serial
//! port's transmit register ([`TRANSMIT`], where the F5 series has its second port's), and it
//! ends by leaving its code in the first output port's register ([`PORT`]) and stopping the
//! processor, the bit `CPUOFF` of the status register set: nothing wakes it without an
//! interrupt, and the run ends there with the port's byte as its code.

use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{CallConv, Callee, Const, Declaration, FnType, FunctionBuilder, Global, Linkage, Module, Operand, Terminator, Type};

use crate::lift::{Code, MAIN, OUT, RUN, SEGMENT, SLOTS, STATUS, TRAP, lift};

/// The serial port's transmit register, and the first output port's register.
pub const TRANSMIT: u64 = 0x060e;
pub const PORT: u64 = 0x0202;
/// The bit of the status register that stops the processor.
pub const CPUOFF: u64 = 0x10;
/// Where the stack starts: the top of the memory below the program.
pub const STACK_TOP: u64 = 0x4000;

/// What is wrong with a program, or how a run of one stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// A part of a program's file that is loaded: where, its bytes, and the size it has in memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub address: u64,
    pub bytes: Vec<u8>,
    pub size: u64,
    pub executable: bool,
}

/// An MSP430 program as its ELF file has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub entry: u64,
    pub segments: Vec<Segment>,
}

impl Program {
    /// Reads a little-endian 32-bit ELF executable for the MSP430: its entry and the segments
    /// it loads, each inside the machine's sixty-four kilobytes.
    ///
    /// # Errors
    /// When the bytes are no such file, or a header points outside them or outside the machine.
    pub fn parse(file: &[u8]) -> Result<Self, Error> {
        let bad = |what: &str| Error(format!("no MSP430 program: {what}"));
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
        if number(18, 2)? != 105 {
            return Err(bad("its machine is not the MSP430"));
        }
        let (entry, table, entry_size, count) = (number(24, 4)?, number(28, 4)?, number(42, 2)?, number(44, 2)?);
        let mut segments = Vec::new();
        for n in 0..count {
            let header = table.checked_add(n * entry_size).ok_or_else(|| bad("a header ends outside the file"))?;
            let field = |at: u64| number(header.checked_add(at).ok_or_else(|| bad("a header ends outside the file"))?, 4);
            let (kind, offset, address, in_file, size, flags) = (field(0)?, field(4)?, field(8)?, field(16)?, field(20)?, field(24)?);
            if kind == 1 && size != 0 {
                if address.checked_add(size.max(in_file)).is_none_or(|end| end > 0x1_0000) {
                    return Err(bad("a segment lies outside the machine's memory"));
                }
                let bytes = if in_file == 0 { Vec::new() } else { at(offset, in_file)?.to_vec() };
                segments.push(Segment { address, bytes, size, executable: flags & 1 != 0 });
            }
        }
        Ok(Self { entry, segments })
    }

    /// The whole program lifted: the code of its first executable segment as [`lift`] makes
    /// it, its segments as data, and [`MAIN`], which hands the runtime each segment and runs
    /// the code from the entry.
    ///
    /// # Errors
    /// When the program has no executable segment.
    pub fn lift(&self) -> Result<Module, Error> {
        let code = self.segments.iter().find(|s| s.executable).ok_or_else(|| Error("the program has no code".to_owned()))?;
        let mut module = lift(Code { address: code.address as u16, bytes: &code.bytes });
        let segment_ty = FnType::new(Type::Void, vec![Type::I64, Type::Ptr, Type::I64, Type::I64]);
        module.declare(Declaration { name: SEGMENT.to_owned(), ty: segment_ty.clone(), conv: CallConv::C, attrs: Vec::new() });
        let mut b = FunctionBuilder::new(MAIN, FnType::new(Type::Void, vec![Type::Ptr]), &["state"]);
        let block = b.block("entry");
        for (n, segment) in self.segments.iter().enumerate() {
            let name = format!("msp430_segment_{n}");
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
    /// The byte the program left in the output port when it stopped its processor; `None` when
    /// the run ended another way.
    pub exit: Option<i32>,
    pub steps: u64,
}

const STATICS: (u64, u64) = (0x6000_0000, 0x10_0000);
const FRAMES: (u64, u64) = (0x6800_0000, 0x10_0000);

#[derive(Default)]
struct Board {
    out: Vec<u8>,
    /// What the output port holds.
    port: u8,
}

impl Host for Board {
    fn call(&mut self, vm: &mut Vm<'_>, name: &str, _ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        let first = args.first().map_or(0, Value::as_u64);
        match name {
            OUT => {
                let (address, value) = (args[1].as_u64(), args[2].as_u64() as u8);
                match address {
                    TRANSMIT => self.out.push(value),
                    PORT => self.port = value,
                    _ => return Err(Stop::fault(format!("a store to the peripheral register at {address:#x}, which the board has not"))),
                }
                Ok(Value::Int(0))
            }
            STATUS => {
                // The status register is the third slot.
                if vm.memory.read_u64(first + 8 * 2)? & CPUOFF != 0 {
                    return Err(Stop::Exit(i32::from(self.port)));
                }
                Ok(Value::Int(0))
            }
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

/// Lifts `program` and runs it from its entry, to where it stops its processor or to `limit`
/// instructions of the IR (zero for no limit).
///
/// # Errors
/// When the program has no code, or the run stops other than by the program stopping itself.
pub fn run(program: &Program, limit: u64) -> Result<Outcome, Error> {
    let module = program.lift()?;
    let mut memory = Memory::new();
    for (name, (start, len)) in [("statics", STATICS), ("frames", FRAMES), ("machine", (0, 0x1_0000))] {
        memory.add_region(Region::owned(start, len, name));
    }
    let mut vm = Vm::new(&module, memory);
    vm.limit = limit;
    vm.set_static_region(STATICS.0, STATICS.1);
    vm.set_frame_region(FRAMES.0, FRAMES.1);
    let stopped = |stop: Stop| Error(format!("the run stopped: {stop:?}"));
    vm.place().map_err(stopped)?;
    let state = vm.allocate_static(SLOTS * 8, 8).map_err(stopped)?;
    vm.memory.fill(state, SLOTS * 8, 0).map_err(stopped)?;
    // The stack pointer is r1.
    vm.memory.write_u64(state + 8, STACK_TOP).map_err(stopped)?;
    let mut host = Board::default();
    let exit = match vm.call_named(&mut host, MAIN, vec![Value::Int(u128::from(state))]) {
        Ok(_) => None,
        Err(Stop::Exit(code)) => Some(code),
        Err(stop) => return Err(stopped(stop)),
    };
    Ok(Outcome { out: host.out, exit, steps: vm.steps })
}
