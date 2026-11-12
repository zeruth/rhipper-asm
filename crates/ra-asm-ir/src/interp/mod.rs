//! The interpreter: a module's functions run over a flat memory, with the calls the module
//! cannot answer (its declarations, and the addresses the host bound) handed to a [`Host`].
//!
//! The machine is small: every value is a [`Value`], every local lives in its frame, memory is
//! the host's own at the program's addresses (see [`Memory`]), and every function and declaration
//! has an address of its own, so a pointer to one round-trips through memory and an indirect call
//! finds it again. A call adapts its arguments and result to the callee's own types, since a call
//! through a pointer names the callee's types only as the caller knew them. A trap, a fault or a
//! limit ends the run as a [`Stop`] that names the place.

#![allow(clippy::many_single_char_names)]

pub mod memory;
pub mod value;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write;

pub use memory::{Memory, Region};
pub use value::{Layout, Value, mask, sext};

use crate::inst::{BinOp, Callee, Cast, FCond, ICond, Inst, Instruction, Operand, Terminator};
use crate::module::{BlockId, Const, Function, Module};
use crate::types::{FnType, Type};

/// Why a run ended before its entry returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stop {
    /// The program ended itself with an exit code.
    Exit(i32),
    /// The run limit was reached after this many instructions.
    Limit(u64),
    /// A trap of the lifted code, a fault of the machine, or a thing the interpreter or its host
    /// cannot do: the message names it and, when known, the address.
    Fault(String),
}

impl Stop {
    pub fn fault(message: impl Into<String>) -> Self {
        Self::Fault(message.into())
    }

    /// The fault with the address of the IR instruction it happened at.
    fn at(self, address: Option<u64>) -> Self {
        match (self, address) {
            (Self::Fault(m), Some(a)) if !m.contains(" at 0x") => Self::Fault(format!("{m} at {a:#x}")),
            (s, _) => s,
        }
    }
}

/// What an address names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Symbol {
    /// A function of the module, by index.
    Function(usize),
    /// A function the host answers for, by the name the host bound it under.
    External(String),
}

/// What the interpreter asks of its host: a call it cannot answer itself. The host gets the
/// machine back, to read and write its memory and to call into the module again (a callback).
pub trait Host {
    fn call(&mut self, vm: &mut Vm<'_>, name: &str, ty: &FnType, args: &[Value]) -> Result<Value, Stop>;
}

/// What a run saw that no reading of the program shows: the facts of the trace, which the static
/// analysis takes on its next pass.
#[derive(Debug, Clone, Default)]
pub struct Seen {
    /// The targets of every indirect call and jump, by the address of the instruction, as the
    /// addresses the memory held: a function's own, or an alias the host gave it.
    pub calls: BTreeMap<u64, BTreeSet<u64>>,
    /// The functions entered, by index in the module.
    pub entered: BTreeSet<usize>,
}

/// What a run calls at each entry of a recorded function, with the function, its arguments and
/// the machine's memory, before the function runs: a stepping host stops the run there, reading
/// the memory while it waits, and returns to let it go on.
pub type Pause = std::sync::Arc<dyn Fn(&Function, &[Value], &mut Memory) + Send + Sync>;

/// A bump allocator over a region: the statics grow up, the frames grow down.
#[derive(Debug, Clone, Copy, Default)]
struct Bump {
    start: u64,
    end: u64,
    at: u64,
}

/// The call depth the interpreter allows before it stops a runaway recursion.
const MAX_DEPTH: usize = 20_000;

pub struct Vm<'m> {
    pub module: &'m Module,
    pub memory: Memory,
    /// Print every function entered.
    pub trace: bool,
    /// Stop after this many instructions; zero for no limit.
    pub limit: u64,
    /// The instructions executed so far.
    pub steps: u64,
    /// The address of the instruction running, for a host that records what it did.
    pub site: Option<u64>,
    /// What the run saw.
    pub seen: Seen,
    /// Where the record of the run is written, when one is kept: one line per call and return of
    /// the recorded functions, in the shape the port's trace hooks write (`> name(args)`,
    /// `< name = value:kind`), so a run of the reference's IR compares with a port's trace.
    pub record: Option<Box<dyn std::io::Write>>,
    /// The functions the record covers, by index; every function when empty.
    pub recorded: HashSet<usize>,
    /// Called at each entry of a recorded function, when set: the stops of a run stepped in
    /// lockstep with another.
    pub pause: Option<Pause>,
    functions: HashMap<String, usize>,
    named: HashMap<String, Vec<Type>>,
    /// The index of each block of each function, by id.
    blocks: Vec<HashMap<BlockId, usize>>,
    symbols: HashMap<String, u64>,
    by_address: HashMap<u64, Symbol>,
    statics: Bump,
    frames: Bump,
    depth: usize,
}

impl<'m> Vm<'m> {
    pub fn new(module: &'m Module, memory: Memory) -> Self {
        let functions = module.functions.iter().enumerate().map(|(i, f)| (f.name.clone(), i)).collect();
        let named = module.types.iter().cloned().collect();
        let blocks = module.functions.iter().map(|f| f.blocks.iter().enumerate().map(|(i, b)| (b.id, i)).collect()).collect();
        Self {
            module,
            memory,
            trace: false,
            limit: 0,
            steps: 0,
            site: None,
            seen: Seen::default(),
            record: None,
            recorded: HashSet::new(),
            pause: None,
            functions,
            named,
            blocks,
            symbols: HashMap::new(),
            by_address: HashMap::new(),
            statics: Bump::default(),
            frames: Bump::default(),
            depth: 0,
        }
    }

    pub fn layout(&self) -> Layout<'_> {
        Layout { named: &self.named }
    }

    // region Setup

    /// The region the globals without an address of their own, the function addresses and the
    /// host's own allocations of static data come from.
    pub fn set_static_region(&mut self, start: u64, len: u64) {
        self.statics = Bump { start, end: start + len, at: start };
    }

    /// The region the `alloca`s come from.
    pub fn set_frame_region(&mut self, start: u64, len: u64) {
        self.frames = Bump { start, end: start + len, at: start + len };
    }

    /// `size` bytes of static data, aligned to `align`.
    pub fn allocate_static(&mut self, size: u64, align: u64) -> Result<u64, Stop> {
        let align = align.max(1);
        let at = self.statics.at.div_ceil(align) * align;
        if at + size > self.statics.end {
            return Err(Stop::fault("the static region is full"));
        }
        self.statics.at = at + size.max(1);
        Ok(at)
    }

    /// Gives every global, function and declaration an address and writes the initializers. A
    /// global with an address of the image keeps it; the rest come from the static region. A
    /// function or a declaration gets sixteen bytes of `int3`, so a pointer to it is a value of
    /// its own that nothing else holds.
    pub fn place(&mut self) -> Result<(), Stop> {
        let module = self.module;
        for (i, f) in module.functions.iter().enumerate() {
            let address = self.allocate_static(16, 16)?;
            self.memory.fill(address, 16, 0xcc)?;
            self.symbols.insert(f.name.clone(), address);
            self.by_address.insert(address, Symbol::Function(i));
        }
        for d in &module.declarations {
            self.bind_external(&d.name)?;
        }
        for g in &module.globals {
            let layout = Layout { named: &self.named };
            let size = layout.size(&g.ty);
            let align = u64::from(g.align).max(layout.align(&g.ty));
            let address = match g.address {
                Some(a) => a,
                None => self.allocate_static(size, align)?,
            };
            self.symbols.insert(g.name.clone(), address);
        }
        for g in &module.globals {
            let address = self.symbols[&g.name];
            if let Some(init) = &g.init {
                self.write_const(address, &g.ty, init)?;
            } else {
                let size = self.layout().size(&g.ty);
                self.memory.fill(address, size, 0)?;
            }
        }
        Ok(())
    }

    /// An address for a function of the host's, called back by `name`; the same one each time.
    pub fn bind_external(&mut self, name: &str) -> Result<u64, Stop> {
        if let Some(&a) = self.symbols.get(name) {
            return Ok(a);
        }
        let address = self.allocate_static(16, 16)?;
        self.memory.fill(address, 16, 0xcc)?;
        self.symbols.insert(name.to_owned(), address);
        self.by_address.insert(address, Symbol::External(name.to_owned()));
        Ok(address)
    }

    /// Makes `address` name the function or the external `name` as well: the image address of a
    /// lifted function, which the code and the data of the program hold.
    pub fn alias(&mut self, address: u64, name: &str) {
        if let Some(&i) = self.functions.get(name) {
            self.by_address.insert(address, Symbol::Function(i));
        } else if self.symbols.contains_key(name) {
            self.by_address.insert(address, Symbol::External(name.to_owned()));
        }
    }

    pub fn symbol(&self, name: &str) -> Option<u64> {
        self.symbols.get(name).copied()
    }

    pub fn symbol_at(&self, address: u64) -> Option<&Symbol> {
        self.by_address.get(&address)
    }

    /// The name of what `address` names, when it is a function or an external.
    pub fn name_at(&self, address: u64) -> Option<&str> {
        match self.by_address.get(&address)? {
            Symbol::Function(i) => Some(&self.module.functions[*i].name),
            Symbol::External(name) => Some(name),
        }
    }

    pub fn function(&self, name: &str) -> Option<&'m Function> {
        let module = self.module;
        self.functions.get(name).map(|&i| &module.functions[i])
    }

    /// The frames region, for a host that gives a thread of its own a stack.
    pub fn frame_pointer(&self) -> u64 {
        self.frames.at
    }

    /// Records that the instruction running called or jumped to `target` through a pointer.
    pub fn record_call(&mut self, target: u64) {
        if let Some(site) = self.site {
            self.seen.calls.entry(site).or_default().insert(target);
        }
    }

    fn write_const(&mut self, address: u64, ty: &Type, c: &Const) -> Result<(), Stop> {
        let layout = Layout { named: &self.named };
        let size = layout.size(ty);
        match (ty, c) {
            (_, Const::Zero | Const::Null) => self.memory.fill(address, size, 0),
            (_, Const::Bytes(bytes)) => self.memory.write(address, bytes),
            (Type::Float, Const::Float(f)) => self.memory.write_u32(address, (*f as f32).to_bits()),
            (Type::Double, Const::Float(f)) => self.memory.write_u64(address, f.to_bits()),
            (_, Const::Int(v)) => {
                let bytes = (*v as u128).to_le_bytes();
                self.memory.write(address, &bytes[..(size as usize).min(16)])
            }
            (_, Const::Global(name)) => {
                let target =
                    self.symbols.get(name).copied().ok_or_else(|| Stop::fault(format!("a reference to @{name}, which is not placed")))?;
                self.memory.write(address, &u128::from(target).to_le_bytes()[..(size as usize).min(16)])
            }
            (_, Const::Address { global, offset }) => {
                let target = self
                    .symbols
                    .get(global)
                    .copied()
                    .ok_or_else(|| Stop::fault(format!("a reference to @{global}, which is not placed")))?;
                let value = target.wrapping_add(*offset as u64);
                self.memory.write(address, &u128::from(value).to_le_bytes()[..(size as usize).min(16)])
            }
            (Type::Array(_, e) | Type::Vector(_, e), Const::Array(items) | Const::Struct(items)) => {
                let stride = layout.size(e);
                for (i, item) in items.iter().enumerate() {
                    self.write_const(address + i as u64 * stride, e, item)?;
                }
                Ok(())
            }
            (Type::Struct(_) | Type::Packed(_) | Type::Named(_), Const::Struct(items) | Const::Array(items)) => {
                let fields = layout.fields(ty);
                let offsets = layout.offsets(ty);
                self.memory.fill(address, size, 0)?;
                for ((item, field), at) in items.iter().zip(&fields).zip(offsets) {
                    self.write_const(address + at, field, item)?;
                }
                Ok(())
            }
            (_, other) => Err(Stop::fault(format!("an initializer {other:?} for a global of type {ty}"))),
        }
    }

    // endregion

    // region Calls

    /// Calls the function `name` with `args`, as a host or an entry does.
    pub fn call_named(&mut self, host: &mut dyn Host, name: &str, args: Vec<Value>) -> Result<Value, Stop> {
        let module = self.module;
        match self.functions.get(name).copied() {
            Some(i) => self.invoke(host, i, &module.functions[i].ty, args),
            None => Err(Stop::fault(format!("no function @{name} to call"))),
        }
    }

    /// Calls whatever `address` names with `args`, which the caller typed as `ty`.
    pub fn call_address(&mut self, host: &mut dyn Host, address: u64, ty: &FnType, args: Vec<Value>) -> Result<Value, Stop> {
        match self.by_address.get(&address).cloned() {
            Some(Symbol::Function(i)) => self.invoke(host, i, ty, args),
            Some(Symbol::External(name)) => {
                let v = host.call(self, &name, ty, &args)?;
                Ok(v.coerce(&ty.ret, &self.layout()))
            }
            None => Err(Stop::fault(format!("a call to {address:#x}, which is no function"))),
        }
    }

    /// Runs function `index` with the arguments adapted to its parameters and the result to what
    /// the call site expects.
    fn invoke(&mut self, host: &mut dyn Host, index: usize, site: &FnType, args: Vec<Value>) -> Result<Value, Stop> {
        let module = self.module;
        let f = &module.functions[index];
        let layout = Layout { named: &self.named };
        let mut args = args;
        args.resize(f.ty.params.len().max(args.len()), Value::Int(0));
        args.truncate(f.ty.params.len());
        let args = args.into_iter().zip(&f.ty.params).map(|(a, p)| a.coerce(p, &layout)).collect();
        let result = self.run_function(host, index, args)?;
        Ok(result.coerce(&site.ret, &Layout { named: &self.named }))
    }

    fn run_function(&mut self, host: &mut dyn Host, index: usize, args: Vec<Value>) -> Result<Value, Stop> {
        let module = self.module;
        let f = &module.functions[index];
        if self.trace {
            eprintln!("> {}", f.name);
        }
        if self.depth >= MAX_DEPTH {
            return Err(Stop::fault(format!("the call depth passed {MAX_DEPTH} in @{}", f.name)));
        }
        self.depth += 1;
        self.seen.entered.insert(index);
        let watched = self.recorded.is_empty() || self.recorded.contains(&index);
        let recorded = self.record.is_some() && watched;
        if recorded {
            self.record_entry(f, &args)?;
        }
        if watched && let Some(pause) = self.pause.clone() {
            pause(f, &args, &mut self.memory);
        }
        let mark = self.frames.at;
        let mut values: Vec<Option<Value>> = vec![None; f.value_names.len()];
        for (p, a) in f.params.iter().zip(args) {
            values[p.0 as usize] = Some(a);
        }
        let result = self.run_blocks(host, index, f, &mut values);
        self.frames.at = mark;
        self.depth -= 1;
        // A function that never returns (an exit, a fault) leaves no return in the record, as the
        // port's hooks leave none.
        if recorded && let Ok(value) = &result {
            self.record_return(f, value)?;
        }
        result
    }

    /// Writes the entry of `f` to the record: each argument as the port's hook writes it, an
    /// integer or a pointer in hexadecimal, a floating value as `f:` and its bits, `?` for what
    /// the hook does not record.
    fn record_entry(&mut self, f: &Function, args: &[Value]) -> Result<(), Stop> {
        let mut line = format!("> {}(", f.name);
        for (i, (ty, a)) in f.ty.params.iter().zip(args).enumerate() {
            if i > 0 {
                line.push_str(", ");
            }
            match ty {
                Type::Int(_) | Type::Ptr | Type::Func(_) => {
                    let _ = write!(line, "{:#x}", a.as_u64());
                }
                Type::Double => {
                    let _ = write!(line, "f:{:#x}", a.as_f64().to_bits());
                }
                Type::Float => {
                    let _ = write!(line, "f:{:#x}", a.as_f32().to_bits());
                }
                _ => line.push('?'),
            }
        }
        line.push(')');
        self.record_line(&line)
    }

    /// Writes the return of `f` with `value` at the kind of its result type: `:N` an integer of
    /// N bits, `:p` a pointer, `f:...:f64` and `f:...:f32` the bits of a floating result, and the
    /// name alone for a result the hooks do not record.
    fn record_return(&mut self, f: &Function, value: &Value) -> Result<(), Stop> {
        let line = match &f.ty.ret {
            Type::Int(bits) => format!("< {} = {:#x}:{bits}", f.name, (value.as_u128() & mask(u32::from(*bits))) as u64),
            Type::Ptr | Type::Func(_) => format!("< {} = {:#x}:p", f.name, value.as_u64()),
            Type::Double => format!("< {} = f:{:#x}:f64", f.name, value.as_f64().to_bits()),
            Type::Float => format!("< {} = f:{:#x}:f32", f.name, value.as_f32().to_bits()),
            _ => format!("< {}", f.name),
        };
        self.record_line(&line)
    }

    fn record_line(&mut self, line: &str) -> Result<(), Stop> {
        if let Some(out) = &mut self.record {
            out.write_all(line.as_bytes())
                .and_then(|()| out.write_all(b"\n"))
                .map_err(|e| Stop::fault(format!("cannot write the record of the run: {e}")))?;
        }
        Ok(())
    }

    fn run_blocks(&mut self, host: &mut dyn Host, index: usize, f: &'m Function, values: &mut [Option<Value>]) -> Result<Value, Stop> {
        let mut block = 0usize;
        let mut prev: Option<BlockId> = None;
        loop {
            let b = f.blocks.get(block).ok_or_else(|| Stop::fault(format!("@{} has no block to run", f.name)))?;
            // The phis read their incoming values together, before any is written.
            let mut first = 0;
            let mut phis = Vec::new();
            while let Some(i) = b.instrs.get(first)
                && let Inst::Phi { ty, incoming } = &i.inst
            {
                let from = prev.ok_or_else(|| Stop::fault(format!("a phi in the entry block of @{}", f.name)))?;
                let (op, _) = incoming
                    .iter()
                    .find(|(_, p)| *p == from)
                    .ok_or_else(|| Stop::fault(format!("a phi in @{} without a value from {}", f.name, from.0)))?;
                phis.push((i.result, self.eval(f, values, op, ty)?));
                first += 1;
            }
            for (result, v) in phis {
                if let Some(r) = result {
                    values[r.0 as usize] = Some(v);
                }
            }
            for i in &b.instrs[first..] {
                self.steps += 1;
                if self.limit != 0 && self.steps > self.limit {
                    return Err(Stop::Limit(self.steps));
                }
                self.site = i.address;
                let v = self.exec(host, f, values, i).map_err(|s| s.at(i.address))?;
                if let Some(r) = i.result {
                    values[r.0 as usize] = Some(v);
                }
            }
            prev = Some(b.id);
            let next = match &b.terminator {
                Terminator::Ret(None) => return Ok(Value::Int(0)),
                Terminator::Ret(Some((ty, op))) => return self.eval(f, values, op, ty),
                Terminator::Br(t) => *t,
                Terminator::CondBr { cond, then, els } => {
                    if self.eval(f, values, cond, &Type::I1)?.as_bool() {
                        *then
                    } else {
                        *els
                    }
                }
                Terminator::Switch { ty, value, default, cases } => {
                    let bits = self.layout().bits(ty);
                    let v = self.eval(f, values, value, ty)?.as_u128() & mask(bits);
                    cases.iter().find(|(c, _)| (*c as u128) & mask(bits) == v).map_or(*default, |(_, t)| *t)
                }
                Terminator::Unreachable => {
                    let at = b.instrs.last().and_then(|i| i.address).or(b.address);
                    return Err(Stop::fault(format!("unreachable code in @{} was reached", f.name)).at(at));
                }
            };
            block = *self.blocks[index].get(&next).ok_or_else(|| Stop::fault(format!("@{} has no block {}", f.name, next.0)))?;
        }
    }

    // endregion

    // region Operands and instructions

    fn eval(&self, f: &Function, values: &[Option<Value>], op: &Operand, ty: &Type) -> Result<Value, Stop> {
        let layout = Layout { named: &self.named };
        Ok(match op {
            Operand::Local(v) => {
                return values
                    .get(v.0 as usize)
                    .and_then(Clone::clone)
                    .ok_or_else(|| Stop::fault(format!("%{} of @{} is read before it is written", f.value_name(*v), f.name)));
            }
            Operand::Int(i) => match ty {
                Type::Float => Value::F32(f32::from_bits(*i as u32)),
                Type::Double => Value::F64(f64::from_bits(*i as u64)),
                Type::Vector(n, e) => Value::Vector(vec![Value::int(*i as u128, layout.bits(e)); *n as usize]),
                _ => Value::int(*i as u128, layout.bits(ty)),
            },
            Operand::Float(x) => match ty {
                Type::Float => Value::F32(*x as f32),
                Type::Vector(n, e) => Value::Vector(vec![Value::F64(*x).coerce(e, &layout); *n as usize]),
                _ => Value::F64(*x),
            },
            Operand::Global(name) => {
                let a = self.symbols.get(name).ok_or_else(|| Stop::fault(format!("a reference to @{name}, which is not placed")))?;
                Value::int(u128::from(*a), layout.bits(ty))
            }
            Operand::Address { global, offset } => {
                let a = self.symbols.get(global).ok_or_else(|| Stop::fault(format!("a reference to @{global}, which is not placed")))?;
                Value::int(u128::from(a.wrapping_add(*offset as u64)), layout.bits(ty))
            }
            Operand::Null | Operand::Undef | Operand::Zero => Value::zero(ty, &layout),
        })
    }

    fn exec(&mut self, host: &mut dyn Host, f: &Function, values: &[Option<Value>], i: &Instruction) -> Result<Value, Stop> {
        match &i.inst {
            Inst::Alloca { ty, align } => {
                let size = self.layout().size(ty).max(1);
                let align = u64::from(*align).max(1);
                let at = (self.frames.at.wrapping_sub(size)) & !(align - 1);
                if at < self.frames.start || at > self.frames.at {
                    return Err(Stop::fault(format!("the frames overflowed in @{}", f.name)));
                }
                self.frames.at = at;
                Ok(Value::Int(u128::from(at)))
            }
            Inst::Load { ty, ptr, .. } => {
                let address = self.eval(f, values, ptr, &Type::Ptr)?.as_u64();
                let size = self.layout().size(ty);
                let bytes = self.memory.read_vec(address, size)?;
                Ok(Value::from_bytes(ty, &bytes, &self.layout()))
            }
            Inst::Store { ty, value, ptr, .. } => {
                let address = self.eval(f, values, ptr, &Type::Ptr)?.as_u64();
                let v = self.eval(f, values, value, ty)?;
                let bytes = v.to_bytes(ty, &self.layout());
                self.memory.write(address, &bytes)?;
                Ok(Value::Int(0))
            }
            Inst::Bin { op, ty, lhs, rhs } => {
                let a = self.eval(f, values, lhs, ty)?;
                let b = self.eval(f, values, rhs, ty)?;
                self.binary(*op, ty, &a, &b)
            }
            Inst::ICmp { cond, ty, lhs, rhs } => {
                let a = self.eval(f, values, lhs, ty)?;
                let b = self.eval(f, values, rhs, ty)?;
                Ok(Self::lanes(ty, &a, &b, &mut |e, a, b| Ok(Value::bool(icmp(*cond, self.layout().bits(e), a.as_u128(), b.as_u128()))))?)
            }
            Inst::FCmp { cond, ty, lhs, rhs } => {
                let a = self.eval(f, values, lhs, ty)?;
                let b = self.eval(f, values, rhs, ty)?;
                Ok(Self::lanes(ty, &a, &b, &mut |_, a, b| Ok(Value::bool(fcmp(*cond, a.as_f64(), b.as_f64()))))?)
            }
            Inst::Select { cond, ty, then, els } => {
                let c = self.eval(f, values, cond, &Type::I1)?;
                let a = self.eval(f, values, then, ty)?;
                let b = self.eval(f, values, els, ty)?;
                match (&c, &a, &b) {
                    (Value::Vector(cs), Value::Vector(xs), Value::Vector(ys)) => Ok(Value::Vector(
                        cs.iter().zip(xs).zip(ys).map(|((c, x), y)| if c.as_bool() { x.clone() } else { y.clone() }).collect(),
                    )),
                    _ => Ok(if c.as_bool() { a } else { b }),
                }
            }
            Inst::Cast { cast, from, value, to } => {
                let v = self.eval(f, values, value, from)?;
                self.cast(*cast, from, &v, to)
            }
            Inst::Gep { base_ty, ptr, indices, .. } => {
                let layout = Layout { named: &self.named };
                let mut address = self.eval(f, values, ptr, &Type::Ptr)?.as_u64();
                let mut ty = base_ty.clone();
                for (k, (ity, iop)) in indices.iter().enumerate() {
                    let index = sext(self.eval(f, values, iop, ity)?.as_u128(), layout.bits(ity)) as i64;
                    if k == 0 {
                        address = address.wrapping_add((index as u64).wrapping_mul(layout.size(base_ty)));
                        continue;
                    }
                    match &ty {
                        Type::Array(_, e) | Type::Vector(_, e) => {
                            address = address.wrapping_add((index as u64).wrapping_mul(layout.size(e)));
                            ty = (**e).clone();
                        }
                        Type::Struct(_) | Type::Packed(_) | Type::Named(_) => {
                            let fields = layout.fields(&ty);
                            let offsets = layout.offsets(&ty);
                            let n = index as usize;
                            let (Some(field), Some(at)) = (fields.get(n), offsets.get(n)) else {
                                return Err(Stop::fault(format!("a getelementptr into field {index} of {ty}")));
                            };
                            address = address.wrapping_add(*at);
                            ty = field.clone();
                        }
                        other => return Err(Stop::fault(format!("a getelementptr through {other}"))),
                    }
                }
                Ok(Value::Int(u128::from(address)))
            }
            Inst::ExtractElement { ty, vector, index } => {
                let v = self.eval(f, values, vector, ty)?;
                let n = self.eval(f, values, index, &Type::I64)?.as_u64() as usize;
                v.lanes().get(n).cloned().ok_or_else(|| Stop::fault(format!("an extractelement of lane {n} of {ty}")))
            }
            Inst::InsertElement { ty, vector, element, index } => {
                let mut lanes = self.eval(f, values, vector, ty)?.lanes().to_vec();
                let n = self.eval(f, values, index, &Type::I64)?.as_u64() as usize;
                let e = match ty {
                    Type::Vector(_, e) => self.eval(f, values, element, e)?,
                    _ => self.eval(f, values, element, ty)?,
                };
                match lanes.get_mut(n) {
                    Some(slot) => *slot = e,
                    None => return Err(Stop::fault(format!("an insertelement into lane {n} of {ty}"))),
                }
                Ok(Value::Vector(lanes))
            }
            Inst::ShuffleVector { ty, a, b, mask } => {
                let layout = Layout { named: &self.named };
                let mut lanes = self.eval(f, values, a, ty)?.lanes().to_vec();
                lanes.extend(self.eval(f, values, b, ty)?.lanes().iter().cloned());
                let zero = match ty {
                    Type::Vector(_, e) => Value::zero(e, &layout),
                    _ => Value::Int(0),
                };
                Ok(Value::Vector(
                    mask.iter()
                        .map(|&m| if m < 0 { zero.clone() } else { lanes.get(m as usize).cloned().unwrap_or(zero.clone()) })
                        .collect(),
                ))
            }
            Inst::Freeze { ty, value } => self.eval(f, values, value, ty),
            Inst::Phi { .. } => Err(Stop::fault(format!("a phi after the first instructions of a block of @{}", f.name))),
            Inst::Call { fn_ty, callee, args, .. } => {
                let mut evaluated = Vec::with_capacity(args.len());
                for (ty, op) in args {
                    evaluated.push(self.eval(f, values, op, ty)?);
                }
                match callee {
                    Callee::Direct(name) => {
                        if let Some(&index) = self.functions.get(name.as_str()) {
                            return self.invoke(host, index, fn_ty, evaluated);
                        }
                        if let Some(result) = self.intrinsic(name, fn_ty, &evaluated)? {
                            return Ok(result);
                        }
                        let v = host.call(self, name, fn_ty, &evaluated)?;
                        Ok(v.coerce(&fn_ty.ret, &self.layout()))
                    }
                    Callee::Indirect(op) => {
                        let address = self.eval(f, values, op, &Type::Ptr)?.as_u64();
                        self.record_call(address);
                        self.call_address(host, address, fn_ty, evaluated)
                    }
                }
            }
        }
    }

    /// Applies `op` lane by lane when `ty` is a vector, else once.
    fn lanes(ty: &Type, a: &Value, b: &Value, op: &mut dyn FnMut(&Type, &Value, &Value) -> Result<Value, Stop>) -> Result<Value, Stop> {
        match (ty, a, b) {
            (Type::Vector(_, e), Value::Vector(xs), Value::Vector(ys)) => {
                Ok(Value::Vector(xs.iter().zip(ys).map(|(x, y)| op(e, x, y)).collect::<Result<_, _>>()?))
            }
            _ => op(ty, a, b),
        }
    }

    fn binary(&self, op: BinOp, ty: &Type, a: &Value, b: &Value) -> Result<Value, Stop> {
        Self::lanes(ty, a, b, &mut |e, a, b| {
            if op.is_float() {
                return Ok(match e {
                    Type::Float => Value::F32(float_op(op, f64::from(a.as_f32()), f64::from(b.as_f32())) as f32),
                    _ => Value::F64(float_op(op, a.as_f64(), b.as_f64())),
                });
            }
            let bits = self.layout().bits(e);
            let (x, y) = (a.as_u128() & mask(bits), b.as_u128() & mask(bits));
            let v = match op {
                BinOp::Add => x.wrapping_add(y),
                BinOp::Sub => x.wrapping_sub(y),
                BinOp::Mul => x.wrapping_mul(y),
                BinOp::UDiv | BinOp::URem if y == 0 => return Err(Stop::fault("a division by zero")),
                BinOp::UDiv => x / y,
                BinOp::URem => x % y,
                BinOp::SDiv | BinOp::SRem => {
                    let (sx, sy) = (sext(x, bits), sext(y, bits));
                    if sy == 0 {
                        return Err(Stop::fault("a division by zero"));
                    }
                    (if op == BinOp::SDiv { sx.wrapping_div(sy) } else { sx.wrapping_rem(sy) }) as u128
                }
                BinOp::Shl => {
                    if y >= u128::from(bits) {
                        0
                    } else {
                        x << y
                    }
                }
                BinOp::LShr => {
                    if y >= u128::from(bits) {
                        0
                    } else {
                        x >> y
                    }
                }
                BinOp::AShr => {
                    let sx = sext(x, bits);
                    (if y >= u128::from(bits) { sx >> 127 } else { sx >> y }) as u128
                }
                BinOp::And => x & y,
                BinOp::Or => x | y,
                BinOp::Xor => x ^ y,
                BinOp::FAdd | BinOp::FSub | BinOp::FMul | BinOp::FDiv | BinOp::FRem => unreachable!(),
            };
            Ok(Value::int(v, bits))
        })
    }

    // A 128-bit integer to a double loses what a double cannot hold: that is the instruction.
    #[allow(clippy::cast_precision_loss)]
    fn cast(&self, cast: Cast, from: &Type, v: &Value, to: &Type) -> Result<Value, Stop> {
        let layout = Layout { named: &self.named };
        if cast == Cast::Bitcast {
            let bytes = v.to_bytes(from, &layout);
            if bytes.len() != layout.size(to) as usize {
                return Err(Stop::fault(format!("a bitcast from {from} to {to}, which differ in size")));
            }
            return Ok(Value::from_bytes(to, &bytes, &layout));
        }
        if let (Type::Vector(_, fe), Type::Vector(_, te), Value::Vector(lanes)) = (from, to, v) {
            return Ok(Value::Vector(lanes.iter().map(|l| self.cast(cast, fe, l, te)).collect::<Result<_, _>>()?));
        }
        let (fb, tb) = (layout.bits(from), layout.bits(to));
        Ok(match cast {
            Cast::Trunc | Cast::ZExt | Cast::PtrToInt | Cast::IntToPtr => Value::int(v.as_u128(), tb),
            Cast::SExt => Value::int(sext(v.as_u128(), fb) as u128, tb),
            Cast::FPTrunc => Value::F32(v.as_f64() as f32),
            Cast::FPExt => Value::F64(v.as_f64()),
            Cast::FPToSI => Value::int(fptosi(v.as_f64(), tb) as u128, tb),
            Cast::FPToUI => Value::int(fptoui(v.as_f64(), tb), tb),
            Cast::UIToFP => float_of(v.as_u128() as f64, to),
            Cast::SIToFP => float_of(sext(v.as_u128(), fb) as f64, to),
            Cast::Bitcast => unreachable!(),
        })
    }

    /// The LLVM intrinsics the lift calls, answered here; `None` for any other name.
    fn intrinsic(&self, name: &str, ty: &FnType, args: &[Value]) -> Result<Option<Value>, Stop> {
        let Some(rest) = name.strip_prefix("llvm.") else { return Ok(None) };
        let (op, _suffix) = rest.split_once('.').unwrap_or((rest, ""));
        let layout = Layout { named: &self.named };
        let bits = layout.bits(&ty.ret);
        let arg = |n: usize| args.get(n).map_or(0, Value::as_u128) & mask(bits);
        let farg = |n: usize| args.get(n).map_or(0.0, Value::as_f64);
        let int = |v: u128| Some(Value::int(v, bits));
        let float = |v: f64| Some(float_of(v, &ty.ret));
        Ok(match op {
            "ctpop" => int(u128::from(arg(0).count_ones())),
            "cttz" => int(u128::from(if arg(0) == 0 { bits } else { arg(0).trailing_zeros() })),
            "ctlz" => int(u128::from(if arg(0) == 0 { bits } else { arg(0).leading_zeros() - (128 - bits) })),
            "bswap" => int(arg(0).swap_bytes() >> (128 - bits)),
            "bitreverse" => int(arg(0).reverse_bits() >> (128 - bits)),
            "fshl" | "fshr" => {
                let (a, b, c) = (arg(0), arg(1), arg(2) % u128::from(bits));
                int(if c == 0 {
                    if op == "fshl" { a } else { b }
                } else if op == "fshl" {
                    (a << c) | (b >> (u128::from(bits) - c))
                } else {
                    (a << (u128::from(bits) - c)) | (b >> c)
                })
            }
            "sqrt" => float(farg(0).sqrt()),
            "fabs" => float(farg(0).abs()),
            "minnum" => float(farg(0).min(farg(1))),
            "maxnum" => float(farg(0).max(farg(1))),
            "trunc" => float(farg(0).trunc()),
            "floor" => float(farg(0).floor()),
            "ceil" => float(farg(0).ceil()),
            "rint" | "nearbyint" | "roundeven" => float(farg(0).round_ties_even()),
            "round" => float(farg(0).round()),
            "fma" | "fmuladd" => float(farg(0).mul_add(farg(1), farg(2))),
            "copysign" => float(farg(0).copysign(farg(1))),
            "abs" => int(sext(arg(0), bits).unsigned_abs()),
            "umax" => int(arg(0).max(arg(1))),
            "umin" => int(arg(0).min(arg(1))),
            "smax" => int(sext(arg(0), bits).max(sext(arg(1), bits)) as u128),
            "smin" => int(sext(arg(0), bits).min(sext(arg(1), bits)) as u128),
            _ => return Err(Stop::fault(format!("the intrinsic @{name} is not interpreted"))),
        })
    }

    // endregion
}

/// A float of `ty`'s precision.
fn float_of(v: f64, ty: &Type) -> Value {
    match ty {
        Type::Float => Value::F32(v as f32),
        _ => Value::F64(v),
    }
}

fn float_op(op: BinOp, a: f64, b: f64) -> f64 {
    match op {
        BinOp::FAdd => a + b,
        BinOp::FSub => a - b,
        BinOp::FMul => a * b,
        BinOp::FDiv => a / b,
        _ => a % b,
    }
}

fn icmp(cond: ICond, bits: u32, a: u128, b: u128) -> bool {
    let (a, b) = (a & mask(bits), b & mask(bits));
    let (sa, sb) = (sext(a, bits), sext(b, bits));
    match cond {
        ICond::Eq => a == b,
        ICond::Ne => a != b,
        ICond::Ugt => a > b,
        ICond::Uge => a >= b,
        ICond::Ult => a < b,
        ICond::Ule => a <= b,
        ICond::Sgt => sa > sb,
        ICond::Sge => sa >= sb,
        ICond::Slt => sa < sb,
        ICond::Sle => sa <= sb,
    }
}

#[allow(clippy::float_cmp)]
fn fcmp(cond: FCond, a: f64, b: f64) -> bool {
    let ordered = !a.is_nan() && !b.is_nan();
    match cond {
        FCond::Oeq => ordered && a == b,
        FCond::Ogt => ordered && a > b,
        FCond::Oge => ordered && a >= b,
        FCond::Olt => ordered && a < b,
        FCond::Ole => ordered && a <= b,
        FCond::One => ordered && a != b,
        FCond::Ord => ordered,
        FCond::Ueq => !ordered || a == b,
        FCond::Ugt => !ordered || a > b,
        FCond::Uge => !ordered || a >= b,
        FCond::Ult => !ordered || a < b,
        FCond::Ule => !ordered || a <= b,
        FCond::Une => !ordered || a != b,
        FCond::Uno => !ordered,
    }
}

/// A float to a signed integer of `bits` bits as `cvttsd2si` makes it: truncated, and the integer
/// indefinite (the minimum) when it is not a number or does not fit.
fn fptosi(v: f64, bits: u32) -> i128 {
    let bits = bits.clamp(1, 64);
    let limit = 2f64.powi(bits as i32 - 1);
    let t = v.trunc();
    if t.is_nan() || t < -limit || t >= limit { -(1i128 << (bits - 1)) } else { t as i128 }
}

/// A float to an unsigned integer as the compilers make it on x86: through the signed conversion
/// of the next width, and for 64 bits through the signed conversion of the value less 2^63.
fn fptoui(v: f64, bits: u32) -> u128 {
    if bits <= 32 {
        return (fptosi(v, 64) as u128) & mask(bits);
    }
    let limit = 2f64.powi(63);
    if v.is_nan() || v < limit { fptosi(v, 64) as u128 & mask(64) } else { (fptosi(v - limit, 64) as u128 ^ (1u128 << 63)) & mask(64) }
}
