//! A lifted PowerPC program run over the IR's interpreter: the program's segments at their own
//! addresses, a stack, the state, and the Linux system calls a program with no C library makes
//! to print and to end (`sc`: the call's number in `r0`, its arguments from `r3`, the result in
//! `r3`).

use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{CallConv, Callee, Const, Declaration, FnType, FunctionBuilder, Global, Linkage, Module, Operand, Terminator, Type};

use crate::lift::{Code, MAIN, MASK, RUN, SC, SEGMENT, SLOTS, TRAP, lift};

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

/// A big-endian PowerPC program as its ELF file has it, of either width.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    pub entry: u64,
    pub segments: Vec<Segment>,
    /// The program is a 64-bit one: [`crate::lift64`] lifts its code.
    pub wide: bool,
    /// The bits of an address that name memory, for a console's program ([`Code::mask`]).
    pub mask: Option<u32>,
}

/// The sections a DOL file's header has places for, and how many of the first are code.
const DOL_SECTIONS: usize = 18;
const DOL_CODE: usize = 7;
/// The GameCube's memory is mapped at 0x80000000 and again at 0xc0000000: the low thirty bits
/// of an address name the memory.
const DOL_MASK: u32 = 0x3fff_ffff;

/// Whether a file reads as a DOL: the format has no mark of its own, so it is one when its
/// header's sections lie in the file after the header, at the console's addresses, and its
/// entry is such an address.
fn is_dol(file: &[u8]) -> bool {
    let word = |at: usize| file.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let console = |address: u32| (0x8000_0000..0x8180_0000).contains(&address);
    let mut any = false;
    for n in 0..DOL_SECTIONS {
        let (Some(offset), Some(address), Some(size)) = (word(4 * n), word(0x48 + 4 * n), word(0x90 + 4 * n)) else { return false };
        if size == 0 {
            continue;
        }
        any = true;
        if offset < 0x100 || u64::from(offset) + u64::from(size) > file.len() as u64 || !console(address) {
            return false;
        }
    }
    any && word(0xe0).is_some_and(console)
}

impl Program {
    /// Reads a big-endian 32-bit ELF executable for PowerPC: its entry and the segments it loads.
    ///
    /// # Errors
    /// When the bytes are no such file, or a header points outside them.
    pub fn parse(file: &[u8]) -> Result<Self, Error> {
        let bad = |what: &str| Error(format!("no PowerPC program: {what}"));
        // A place past the file's end is refused before anything is added to it.
        let at = |offset: u64, len: u64| {
            let end = offset.checked_add(len).filter(|&end| end <= file.len() as u64);
            end.map(|end| &file[offset as usize..end as usize]).ok_or_else(|| bad("a header ends outside the file"))
        };
        let number = |offset: u64, len: u64| -> Result<u64, Error> {
            Ok(at(offset, len)?.iter().fold(0u64, |value, &byte| value << 8 | u64::from(byte)))
        };
        if file.get(..4) != Some(b"\x7fELF") && is_dol(file) {
            return Self::dol(file);
        }
        if at(0, 4)? != b"\x7fELF" || !matches!(at(4, 1)?, [1 | 2]) || at(5, 1)? != [2] {
            return Err(bad("it is no big-endian ELF file"));
        }
        // A file of the 64-bit class is a 64-bit program's: its headers are laid out wider.
        let wide = at(4, 1)? == [2];
        if number(18, 2)? != if wide { 21 } else { 20 } {
            return Err(bad("its machine is not PowerPC"));
        }
        let (entry, table, entry_size, count) = if wide {
            (number(24, 8)?, number(32, 8)?, number(54, 2)?, number(56, 2)?)
        } else {
            (number(24, 4)?, number(28, 4)?, number(42, 2)?, number(44, 2)?)
        };
        let mut segments = Vec::new();
        for n in 0..count {
            let header = table.checked_add(n * entry_size).ok_or_else(|| bad("a header ends outside the file"))?;
            let field = |at: u64, len: u64| number(header.checked_add(at).ok_or_else(|| bad("a header ends outside the file"))?, len);
            let (kind, offset, address, in_file, size, flags) = if wide {
                (field(0, 4)?, field(8, 8)?, field(16, 8)?, field(32, 8)?, field(40, 8)?, field(4, 4)?)
            } else {
                (field(0, 4)?, field(4, 4)?, field(8, 4)?, field(16, 4)?, field(20, 4)?, field(24, 4)?)
            };
            if kind == 1 && size != 0 {
                // A segment with no bytes in the file (what the program clears) may say an
                // offset past the file's end: nothing is read there.
                let bytes = if in_file == 0 { Vec::new() } else { at(offset, in_file)?.to_vec() };
                segments.push(Segment { address, bytes, size, executable: flags & 1 != 0 });
            }
        }
        Ok(Self { entry, segments, wide, mask: None })
    }

    /// Reads a DOL file, the GameCube's and the Wii's executable: a header of eighteen
    /// sections (seven of code, eleven of data), each a place in the file, an address and a
    /// size, then what the program clears and its entry. The addresses are the console's, in
    /// the mapping of its memory at 0x80000000; [`Program::mask`] says which bits name memory.
    ///
    /// # Errors
    /// When a section lies outside the file or the entry outside the code.
    pub fn dol(file: &[u8]) -> Result<Self, Error> {
        let bad = |what: &str| Error(format!("no DOL program: {what}"));
        let word = |at: usize| {
            file.get(at..at + 4)
                .map(|b| u64::from(u32::from_be_bytes([b[0], b[1], b[2], b[3]])))
                .ok_or_else(|| bad("its header is cut short"))
        };
        let mut segments = Vec::new();
        for n in 0..DOL_SECTIONS {
            let (offset, address, size) = (word(4 * n)?, word(0x48 + 4 * n)?, word(0x90 + 4 * n)?);
            if size == 0 {
                continue;
            }
            let end =
                offset.checked_add(size).filter(|&end| end <= file.len() as u64).ok_or_else(|| bad("a section ends outside the file"))?;
            segments.push(Segment { address, bytes: file[offset as usize..end as usize].to_vec(), size, executable: n < DOL_CODE });
        }
        let (cleared, cleared_size, entry) = (word(0xd8)?, word(0xdc)?, word(0xe0)?);
        if cleared_size != 0 {
            segments.push(Segment { address: cleared, bytes: Vec::new(), size: cleared_size, executable: false });
        }
        if !segments.iter().any(|s| s.executable && entry >= s.address && entry < s.address + s.size) {
            return Err(bad("its entry is in no section of code"));
        }
        Ok(Self { entry, segments, wide: false, mask: Some(DOL_MASK) })
    }

    /// The whole program lifted: the code of its first executable segment as [`lift`] makes it,
    /// its segments as data, and [`MAIN`], which hands the runtime each segment and runs the
    /// code from the entry.
    ///
    /// # Errors
    /// When the program has no executable segment.
    pub fn lift(&self) -> Result<Module, Error> {
        let code = self.segments.iter().find(|s| s.executable).ok_or_else(|| Error("the program has no code".to_owned()))?;
        let code = Code { address: code.address, bytes: &code.bytes, mask: self.mask };
        let mut module = if self.wide { crate::lift64::lift(code) } else { lift(code) };
        let mask = self.mask.map_or(u64::MAX, u64::from);
        module.globals.push(Global {
            name: MASK.to_owned(),
            ty: Type::array(8, Type::I8),
            init: Some(Const::Bytes(mask.to_le_bytes().to_vec())),
            constant: false,
            linkage: Linkage::External,
            align: 8,
            address: None,
        });
        let segment_ty = FnType::new(Type::Void, vec![Type::I64, Type::Ptr, Type::I64, Type::I64]);
        module.declare(Declaration { name: SEGMENT.to_owned(), ty: segment_ty.clone(), conv: CallConv::C, attrs: Vec::new() });
        let mut b = FunctionBuilder::new(MAIN, FnType::new(Type::Void, vec![Type::Ptr]), &["state"]);
        let block = b.block("entry");
        for (n, segment) in self.segments.iter().enumerate() {
            let name = format!("ppc_segment_{n}");
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
                (Type::I64, Operand::Int(i128::from(segment.address & mask))),
                (Type::Ptr, Operand::Global(name)),
                (Type::I64, Operand::Int(segment.bytes.len() as i128)),
                (Type::I64, Operand::Int(i128::from(segment.size))),
            ];
            b.call(block, segment_ty.clone(), Callee::Direct(SEGMENT.to_owned()), args, "");
        }
        if self.wide {
            // A 64-bit program finds its table of contents from its entry's own address, which
            // the system leaves in r12.
            let r12 = b.gep(block, Type::I8, b.param(0), vec![(Type::I64, Operand::Int(8 * 12))], "r12");
            b.store(block, Type::I64, Operand::Int(i128::from(self.entry)), r12);
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
    /// The bits of an address the program hands the system that name memory.
    mask: u64,
}

impl Host for Linux {
    fn call(&mut self, vm: &mut Vm<'_>, name: &str, _ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        let first = args.first().map_or(0, Value::as_u64);
        match name {
            SC => {
                // A slot's eight bytes: a 32-bit program leaves the high four zero.
                let register = |vm: &mut Vm<'_>, n: u64| vm.memory.read_u64(first + 8 * n);
                let (number, a, b, c) = (register(vm, 0)?, register(vm, 3)?, register(vm, 4)?, register(vm, 5)?);
                let result: u32 = match number {
                    // write
                    4 if a == 1 || a == 2 => {
                        self.out.extend(vm.memory.read_vec(b & self.mask, c)?);
                        c as u32
                    }
                    // EBADF, which the kernel says by the summary bit; no program here looks
                    4 => 9,
                    // exit, exit_group
                    1 | 234 => return Err(Stop::Exit(a as i32)),
                    _ => return Err(Stop::fault(format!("the system call {number} is not interpreted"))),
                };
                vm.memory.write_u64(first + 8 * 3, u64::from(result))?;
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

/// Lifts `program` and runs it from its entry, to its `exit` or to `limit` instructions of the
/// IR (zero for no limit).
///
/// # Errors
/// When the program has no code, or the run stops other than by the program's `exit`.
pub fn run(program: &Program, limit: u64) -> Result<Outcome, Error> {
    let module = program.lift()?;
    let mut memory = Memory::new();
    let mask = program.mask.map_or(u64::MAX, u64::from);
    // The stack is where the program's own addresses of it land in memory.
    for (name, (start, len)) in [("stack", (STACK.0 & mask, STACK.1)), ("statics", STATICS), ("frames", FRAMES)] {
        memory.add_region(Region::owned(start, len, name));
    }
    for segment in &program.segments {
        // The system maps whole pages.
        let address = segment.address & mask;
        let start = address & !0xfff;
        let end = (address + segment.size.max(segment.bytes.len() as u64)).next_multiple_of(0x1000);
        memory.add_region(Region::owned(start, end - start, "segment"));
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
    vm.memory.write_u64(state + 8, STACK.0 + STACK.1 - 256).map_err(stopped)?;
    let mut host = Linux { out: Vec::new(), mask };
    let exit = match vm.call_named(&mut host, MAIN, vec![Value::Int(u128::from(state))]) {
        Ok(_) => None,
        Err(Stop::Exit(code)) => Some(code),
        Err(stop) => return Err(stopped(stop)),
    };
    Ok(Outcome { out: host.out, exit, steps: vm.steps })
}
