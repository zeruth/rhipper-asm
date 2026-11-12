//! The binary format: a module's sections read into a [`Module`], each function body into the
//! flat [`Op`]s the interpreter runs, with every block's end and else found once, here.

use crate::{Error, FuncType, ValType};

/// One instruction, its immediates read and its structured targets resolved to op indices.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Op {
    Unreachable,
    Nop,
    /// `end` is the index of the block's `End`.
    Block {
        params: u32,
        results: u32,
        end: u32,
    },
    Loop {
        params: u32,
    },
    /// `else_at` is the index of the `Else`, or of the `End` when there is none.
    If {
        params: u32,
        results: u32,
        else_at: u32,
        end: u32,
    },
    Else {
        end: u32,
    },
    End,
    Br(u32),
    BrIf(u32),
    BrTable(Box<[u32]>, u32),
    Return,
    Call(u32),
    CallIndirect(u32),
    Drop,
    Select,
    LocalGet(u32),
    LocalSet(u32),
    LocalTee(u32),
    GlobalGet(u32),
    GlobalSet(u32),
    /// A load or a store by its opcode, with the memory argument's offset.
    Memory(u8, u32),
    MemorySize,
    MemoryGrow,
    MemoryCopy,
    MemoryFill,
    /// Any constant, as its bits.
    Const(u64),
    /// A numeric instruction without immediates, by its opcode (`0x45..=0xc4`).
    Numeric(u8),
    /// A saturating truncation, by its `0xfc` sub-opcode (`0..=7`).
    TruncSat(u8),
}

#[derive(Debug, Clone)]
pub(crate) struct Import {
    pub module: String,
    pub name: String,
    pub ty: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct Body {
    /// The declared locals after the parameters, by type.
    pub locals: Vec<ValType>,
    pub ops: Vec<Op>,
}

#[derive(Debug, Clone)]
pub(crate) struct Global {
    pub mutable: bool,
    pub init: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExportKind {
    Func,
    Table,
    Memory,
    Global,
}

#[derive(Debug, Clone)]
pub(crate) struct Export {
    pub name: String,
    pub kind: ExportKind,
    pub index: u32,
}

/// The limits of a memory or a table: the initial size, and the maximum when declared.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Limits {
    pub min: u32,
    pub max: Option<u32>,
}

/// A decoded module: what instantiation and the calls of its exports need.
#[derive(Debug, Clone, Default)]
pub struct Module {
    pub(crate) types: Vec<FuncType>,
    pub(crate) imports: Vec<Import>,
    /// The type of each function the module defines, after the imported ones.
    pub(crate) funcs: Vec<u32>,
    pub(crate) bodies: Vec<Body>,
    pub(crate) table: Option<Limits>,
    pub(crate) memory: Option<Limits>,
    pub(crate) globals: Vec<Global>,
    pub(crate) exports: Vec<Export>,
    pub(crate) start: Option<u32>,
    /// The active element segments of table 0: the offset and the functions.
    pub(crate) elements: Vec<(u64, Vec<u32>)>,
    /// The active data segments of memory 0: the offset and the bytes.
    pub(crate) data: Vec<(u64, Vec<u8>)>,
}

/// The most locals a function may declare: what a plugin needs many times over, and a bound on
/// the memory a hostile count asks of the host.
const MAX_LOCALS: usize = 50_000;

impl Module {
    /// Decodes the binary module `bytes`. A feature past the core set the interpreter runs (the
    /// reference types, SIMD, threads, a second memory, an imported memory, table or global) is
    /// an error that names it.
    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        let mut r = Reader { bytes, at: 0 };
        if r.take(4)? != b"\0asm" {
            return Err(Error::new("not a WebAssembly module"));
        }
        if r.take(4)? != [1, 0, 0, 0] {
            return Err(Error::new("not version 1 of the WebAssembly binary format"));
        }
        let mut module = Module::default();
        while !r.done() {
            let id = r.byte()?;
            let size = r.u32()? as usize;
            let mut s = Reader { bytes: r.take(size)?, at: 0 };
            match id {
                0 | 12 => {} // custom sections and the data count
                1 => module.types = s.vec(Reader::func_type)?,
                2 => module.imports = s.vec(Reader::import)?,
                3 => module.funcs = s.vec(Reader::u32)?,
                4 => {
                    let tables = s.vec(Reader::table)?;
                    if tables.len() > 1 {
                        return Err(Error::new("more than one table"));
                    }
                    module.table = tables.first().copied();
                }
                5 => {
                    let memories = s.vec(Reader::limits)?;
                    if memories.len() > 1 {
                        return Err(Error::new("more than one memory"));
                    }
                    module.memory = memories.first().copied();
                }
                6 => {
                    let count = s.u32()?;
                    for _ in 0..count {
                        let _ = s.val_type()?;
                        let mutable = s.byte()? == 1;
                        let init = s.const_expr(&module.globals)?;
                        module.globals.push(Global { mutable, init });
                    }
                }
                7 => module.exports = s.vec(Reader::export)?,
                8 => module.start = Some(s.u32()?),
                9 => {
                    let count = s.u32()?;
                    for _ in 0..count {
                        if let Some(segment) = s.element(&module.globals)? {
                            module.elements.push(segment);
                        }
                    }
                }
                10 => {
                    let count = s.u32()?;
                    for _ in 0..count {
                        let size = s.u32()? as usize;
                        let mut b = Reader { bytes: s.take(size)?, at: 0 };
                        module.bodies.push(b.body(&module.types)?);
                    }
                }
                11 => {
                    let count = s.u32()?;
                    for _ in 0..count {
                        if let Some(segment) = s.data(&module.globals)? {
                            module.data.push(segment);
                        }
                    }
                }
                _ => return Err(Error::new(format!("section {id} is not one of the core format"))),
            }
        }
        if module.funcs.len() != module.bodies.len() {
            return Err(Error::new("the function and code sections differ in length"));
        }
        let types = module.types.len() as u32;
        if module.funcs.iter().chain(module.imports.iter().map(|i| &i.ty)).any(|&t| t >= types) {
            return Err(Error::new("a function of no type"));
        }
        Ok(module)
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn done(&self) -> bool {
        self.at >= self.bytes.len()
    }

    fn byte(&mut self) -> Result<u8, Error> {
        let b = *self.bytes.get(self.at).ok_or_else(|| Error::new("the module ends early"))?;
        self.at += 1;
        Ok(b)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len()).ok_or_else(|| Error::new("the module ends early"))?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    /// A LEB128 number of at most `bits` bits, sign-extended when `signed`.
    fn leb(&mut self, bits: u32, signed: bool) -> Result<u64, Error> {
        let mut value = 0u64;
        let mut shift = 0u32;
        loop {
            let b = self.byte()?;
            if shift < 64 {
                value |= u64::from(b & 0x7f) << shift;
            }
            shift += 7;
            if b & 0x80 == 0 {
                if signed && shift < 64 && b & 0x40 != 0 {
                    value |= !0u64 << shift;
                }
                return Ok(value);
            }
            if shift >= bits {
                return Err(Error::new("a LEB128 number is too long"));
            }
        }
    }

    fn u32(&mut self) -> Result<u32, Error> {
        Ok(self.leb(32, false)? as u32)
    }

    fn name(&mut self) -> Result<String, Error> {
        let n = self.u32()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| Error::new("a name is not UTF-8"))
    }

    fn vec<T>(&mut self, mut item: impl FnMut(&mut Self) -> Result<T, Error>) -> Result<Vec<T>, Error> {
        let count = self.u32()?;
        // A count past what the bytes could hold is a corrupt module, not a reason to reserve.
        let mut out = Vec::with_capacity((count as usize).min(self.bytes.len() - self.at));
        for _ in 0..count {
            out.push(item(self)?);
        }
        Ok(out)
    }

    fn val_type(&mut self) -> Result<ValType, Error> {
        match self.byte()? {
            0x7f => Ok(ValType::I32),
            0x7e => Ok(ValType::I64),
            0x7d => Ok(ValType::F32),
            0x7c => Ok(ValType::F64),
            0x7b => Err(Error::new("a SIMD value")),
            b => Err(Error::new(format!("a value of type 0x{b:02x}, a reference"))),
        }
    }

    fn func_type(&mut self) -> Result<FuncType, Error> {
        if self.byte()? != 0x60 {
            return Err(Error::new("a type that is not a function type"));
        }
        let params = self.vec(Reader::val_type)?;
        let results = self.vec(Reader::val_type)?;
        Ok(FuncType { params, results })
    }

    fn limits(&mut self) -> Result<Limits, Error> {
        match self.byte()? {
            0 => Ok(Limits { min: self.u32()?, max: None }),
            1 => Ok(Limits { min: self.u32()?, max: Some(self.u32()?) }),
            flags => Err(Error::new(format!("limits of flags 0x{flags:x}: a shared or 64-bit memory"))),
        }
    }

    fn table(&mut self) -> Result<Limits, Error> {
        if self.byte()? != 0x70 {
            return Err(Error::new("a table of references other than functions"));
        }
        self.limits()
    }

    fn import(&mut self) -> Result<Import, Error> {
        let module = self.name()?;
        let name = self.name()?;
        match self.byte()? {
            0 => Ok(Import { module, name, ty: self.u32()? }),
            _ => Err(Error::new(format!("the import {module}.{name} is not a function: a plugin owns its memory, table and globals"))),
        }
    }

    fn export(&mut self) -> Result<Export, Error> {
        let name = self.name()?;
        let kind = match self.byte()? {
            0 => ExportKind::Func,
            1 => ExportKind::Table,
            2 => ExportKind::Memory,
            3 => ExportKind::Global,
            b => return Err(Error::new(format!("the export {name} is of kind {b}"))),
        };
        Ok(Export { name, kind, index: self.u32()? })
    }

    /// A constant expression: one constant, or a global already defined, then `end`.
    fn const_expr(&mut self, globals: &[Global]) -> Result<u64, Error> {
        let value = match self.byte()? {
            0x41 => u64::from(self.leb(32, true)? as u32),
            0x42 => self.leb(64, true)?,
            0x43 => u64::from(u32::from_le_bytes(self.take(4)?.try_into().expect("4 bytes"))),
            0x44 => u64::from_le_bytes(self.take(8)?.try_into().expect("8 bytes")),
            0x23 => {
                let index = self.u32()? as usize;
                globals.get(index).ok_or_else(|| Error::new("a constant of no global"))?.init
            }
            op => return Err(Error::new(format!("a constant expression of opcode 0x{op:02x}"))),
        };
        if self.byte()? != 0x0b {
            return Err(Error::new("a constant expression of more than one instruction"));
        }
        Ok(value)
    }

    /// An element segment: the active ones of table 0 with their offset; a passive or a
    /// declarative one, which only `table.init` and `ref.func` read, is read past.
    fn element(&mut self, globals: &[Global]) -> Result<Option<(u64, Vec<u32>)>, Error> {
        match self.u32()? {
            0 => {
                let offset = self.const_expr(globals)?;
                Ok(Some((offset, self.vec(Reader::u32)?)))
            }
            flags @ (1 | 3) => {
                if self.byte()? != 0 {
                    return Err(Error::new(format!("an element segment of flags {flags} not of functions")));
                }
                self.vec(Reader::u32)?;
                Ok(None)
            }
            2 => {
                if self.u32()? != 0 {
                    return Err(Error::new("an element segment of a second table"));
                }
                let offset = self.const_expr(globals)?;
                if self.byte()? != 0 {
                    return Err(Error::new("an element segment not of functions"));
                }
                Ok(Some((offset, self.vec(Reader::u32)?)))
            }
            flags => Err(Error::new(format!("an element segment of expressions (flags {flags})"))),
        }
    }

    /// A data segment: the active ones of memory 0 with their offset; a passive one, which
    /// only `memory.init` reads, is read past.
    fn data(&mut self, globals: &[Global]) -> Result<Option<(u64, Vec<u8>)>, Error> {
        let offset = match self.u32()? {
            0 => Some(self.const_expr(globals)?),
            1 => None,
            2 => {
                if self.u32()? != 0 {
                    return Err(Error::new("a data segment of a second memory"));
                }
                Some(self.const_expr(globals)?)
            }
            flags => return Err(Error::new(format!("a data segment of flags {flags}"))),
        };
        let n = self.u32()? as usize;
        let bytes = self.take(n)?.to_vec();
        Ok(offset.map(|o| (o, bytes)))
    }

    /// A block type, as its parameter and result counts.
    fn block_type(&mut self, types: &[FuncType]) -> Result<(u32, u32), Error> {
        let first = *self.bytes.get(self.at).ok_or_else(|| Error::new("the module ends early"))?;
        match first {
            0x40 => {
                self.at += 1;
                Ok((0, 0))
            }
            0x7f | 0x7e | 0x7d | 0x7c => {
                self.at += 1;
                Ok((0, 1))
            }
            _ => {
                let index = self.leb(33, true)?;
                let ty = usize::try_from(index).ok().and_then(|i| types.get(i)).ok_or_else(|| Error::new("a block of no type"))?;
                Ok((ty.params.len() as u32, ty.results.len() as u32))
            }
        }
    }

    fn mem_arg(&mut self) -> Result<u32, Error> {
        let _align = self.u32()?;
        self.u32()
    }

    fn zero(&mut self) -> Result<(), Error> {
        match self.byte()? {
            0 => Ok(()),
            _ => Err(Error::new("an instruction of a second memory or table")),
        }
    }

    /// A function body: its locals, and its code with the ends and elses of every block found.
    fn body(&mut self, types: &[FuncType]) -> Result<Body, Error> {
        let mut locals = Vec::new();
        let groups = self.u32()?;
        for _ in 0..groups {
            let count = self.u32()? as usize;
            let ty = self.val_type()?;
            if locals.len() + count > MAX_LOCALS {
                return Err(Error::new(format!("a function of more than {MAX_LOCALS} locals")));
            }
            locals.extend(std::iter::repeat_n(ty, count));
        }
        let mut ops = Vec::new();
        // The open blocks, by the index of their opening op: the function's own is the bottom.
        let mut open: Vec<Option<usize>> = vec![None];
        while !open.is_empty() {
            let at = self.at;
            let op = self.op(types).map_err(|e| Error::new(format!("{e} (at byte {at} of a function body)")))?;
            let index = ops.len();
            match &op {
                Op::Block { .. } | Op::Loop { .. } | Op::If { .. } => open.push(Some(index)),
                Op::Else { .. } => {
                    let Some(&Some(opening)) = open.last() else { return Err(Error::new("an else outside an if")) };
                    match &mut ops[opening] {
                        Op::If { else_at, .. } if *else_at == u32::MAX => *else_at = index as u32,
                        _ => return Err(Error::new("an else outside an if")),
                    }
                }
                Op::End => {
                    if let Some(Some(opening)) = open.pop() {
                        let end = index as u32;
                        let mut else_op = None;
                        match &mut ops[opening] {
                            Op::Block { end: e, .. } => *e = end,
                            Op::If { else_at, end: e, .. } => {
                                if *else_at == u32::MAX {
                                    *else_at = end;
                                } else {
                                    else_op = Some(*else_at as usize);
                                }
                                *e = end;
                            }
                            _ => {}
                        }
                        if let Some(at) = else_op
                            && let Op::Else { end: e } = &mut ops[at]
                        {
                            *e = end;
                        }
                    }
                }
                _ => {}
            }
            ops.push(op);
        }
        if !self.done() {
            return Err(Error::new("bytes after the end of a function body"));
        }
        Ok(Body { locals, ops })
    }

    fn op(&mut self, types: &[FuncType]) -> Result<Op, Error> {
        let code = self.byte()?;
        Ok(match code {
            0x00 => Op::Unreachable,
            0x01 => Op::Nop,
            0x02 => {
                let (params, results) = self.block_type(types)?;
                Op::Block { params, results, end: u32::MAX }
            }
            0x03 => Op::Loop { params: self.block_type(types)?.0 },
            0x04 => {
                let (params, results) = self.block_type(types)?;
                Op::If { params, results, else_at: u32::MAX, end: u32::MAX }
            }
            0x05 => Op::Else { end: u32::MAX },
            0x0b => Op::End,
            0x0c => Op::Br(self.u32()?),
            0x0d => Op::BrIf(self.u32()?),
            0x0e => {
                let targets = self.vec(Reader::u32)?;
                Op::BrTable(targets.into_boxed_slice(), self.u32()?)
            }
            0x0f => Op::Return,
            0x10 => Op::Call(self.u32()?),
            0x11 => {
                let ty = self.u32()?;
                self.zero()?;
                Op::CallIndirect(ty)
            }
            0x1a => Op::Drop,
            0x1b => Op::Select,
            0x1c => {
                self.vec(Reader::val_type)?;
                Op::Select
            }
            0x20 => Op::LocalGet(self.u32()?),
            0x21 => Op::LocalSet(self.u32()?),
            0x22 => Op::LocalTee(self.u32()?),
            0x23 => Op::GlobalGet(self.u32()?),
            0x24 => Op::GlobalSet(self.u32()?),
            0x28..=0x3e => Op::Memory(code, self.mem_arg()?),
            0x3f => {
                self.zero()?;
                Op::MemorySize
            }
            0x40 => {
                self.zero()?;
                Op::MemoryGrow
            }
            0x41 => Op::Const(u64::from(self.leb(32, true)? as u32)),
            0x42 => Op::Const(self.leb(64, true)?),
            0x43 => Op::Const(u64::from(u32::from_le_bytes(self.take(4)?.try_into().expect("4 bytes")))),
            0x44 => Op::Const(u64::from_le_bytes(self.take(8)?.try_into().expect("8 bytes"))),
            0x45..=0xc4 => Op::Numeric(code),
            0xfc => match self.u32()? {
                sub @ 0..=7 => Op::TruncSat(sub as u8),
                10 => {
                    self.zero()?;
                    self.zero()?;
                    Op::MemoryCopy
                }
                11 => {
                    self.zero()?;
                    Op::MemoryFill
                }
                sub => return Err(Error::new(format!("the instruction 0xfc {sub}, not one the interpreter runs"))),
            },
            _ => return Err(Error::new(format!("the instruction 0x{code:02x}, not one the interpreter runs"))),
        })
    }
}
