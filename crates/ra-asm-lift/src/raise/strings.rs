//! Strings decrypted at lift time. An obfuscator keeps a string as ciphertext and calls a
//! decryptor at the use, which writes the plaintext into a buffer of the caller's frame. A call
//! to a lifted function whose arguments are constants, addresses in read-only data and pointers
//! into the caller's own frame, and whose result nothing reads, is run here in the IR
//! interpreter, over a copy of the module that holds the read-only globals alone: the run may
//! read those and write the frame buffers, and nothing else is mapped, so any other access, any
//! call out and any change to the read-only data stops it. Two runs over buffers filled with
//! different bytes must write the same bytes, which shows the result independent of what the
//! buffer held (a dependence the program could only have through uninitialized memory). A key
//! written to a register's low byte keeps the register's other bits, which no one set: the two
//! runs pass them clear and set, so the result shows independent of them too. The call
//! then becomes a copy of the plaintext from a new constant, which the output prints as the
//! literal it is.

use std::collections::HashMap;

use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{BinOp, CallConv, Callee, Cast, Const, Declaration, FnType, Global, Inst, Instruction, Linkage, Module, Operand, Type, ValueId};

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
    /// An integer whose `known` bits are `value` and whose other bits are what the register held
    /// before a byte or word write: `mov r9b, 0x78` over a value the caller left.
    Partial {
        value: u128,
        known: u128,
    },
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

/// Host memory for a run: anywhere for a 64-bit program, below 4 GiB for a 32-bit one, whose
/// pointers are the low halves of the addresses.
enum Backing {
    Heap(Vec<u8>),
    Low(crate::run::os::Allocation),
}

impl Backing {
    fn new(len: u64, low: bool) -> Option<Self> {
        if low { crate::run::os::allocate_low(len).map(Self::Low) } else { Some(Self::Heap(vec![0u8; len as usize])) }
    }

    fn start(&mut self) -> u64 {
        match self {
            Self::Heap(bytes) => bytes.as_mut_ptr() as u64,
            Self::Low(allocation) => allocation.start,
        }
    }
}

/// Runs the decryptor calls of `module` and replaces them; returns the functions it changed.
/// `low` is for a 32-bit program, whose runs need their memory below 4 GiB.
pub fn run(module: &mut Module, low: bool) -> Vec<usize> {
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
    let (Some(mut statics), Some(mut frames)) = (Backing::new(statics_len, low), Backing::new(frames_len, low)) else { return Vec::new() };
    let (statics_at, frames_at) = (statics.start(), frames.start());
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

    let mut plain: Vec<(Candidate, Vec<Option<Vec<u8>>>)> = Vec::new();
    for candidate in candidates {
        if let Some(outcome) = decrypt(&mut vm, &candidate, statics_at, &baseline, low) {
            plain.push((candidate, outcome));
        }
    }
    drop(vm);

    // Later calls first: a replacement of several instructions moves the ones after it.
    plain.sort_by_key(|(c, _)| std::cmp::Reverse((c.function, c.block, c.index)));
    let mut new_globals: Vec<Global> = Vec::new();
    let mut counter = 0usize;
    let mut changed: Vec<usize> = Vec::new();
    for (c, buffers) in plain {
        let mut names = Vec::new();
        for _ in buffers.iter().flatten() {
            loop {
                counter += 1;
                let name = format!("str_{counter}");
                if module.global(&name).is_none() && !new_globals.iter().any(|g| g.name == name) {
                    names.push(name);
                    break;
                }
            }
        }
        let mut names = names.into_iter();
        let f = &mut module.functions[c.function];
        let address = f.blocks[c.block].instrs[c.index].address;
        let mut out: Vec<Instruction> = Vec::new();
        let mut buffer = 0;
        for arg in &c.args {
            let Arg::Buffer(ty, operand, _) = arg else { continue };
            let bytes = buffers[buffer].clone();
            buffer += 1;
            let (Some(bytes), Some(name)) = (bytes, names.next()) else { continue };
            let dst = match ty {
                Type::Int(64) => operand.clone(),
                Type::Ptr | Type::Int(_) => {
                    let v = f.new_value("dst", Type::I64);
                    let cast = if *ty == Type::Ptr { Cast::PtrToInt } else { Cast::ZExt };
                    out.push(Instruction {
                        result: Some(v),
                        inst: Inst::Cast { cast, from: ty.clone(), value: operand.clone(), to: Type::I64 },
                        address,
                    });
                    Operand::Local(v)
                }
                _ => continue,
            };
            let len = bytes.len();
            new_globals.push(Global {
                name: name.clone(),
                ty: Type::array(len as u64, Type::I8),
                init: Some(Const::Bytes(bytes)),
                constant: true,
                linkage: Linkage::Internal,
                align: 1,
                address: None,
            });
            out.push(Instruction {
                result: None,
                inst: Inst::Call {
                    fn_ty: copy_type(),
                    callee: Callee::Direct("memcpy".to_owned()),
                    args: vec![(Type::I64, dst), (Type::Ptr, Operand::Global(name)), (Type::I64, Operand::Int(len as i128))],
                    tail: false,
                },
                address,
            });
        }
        if out.is_empty() {
            continue;
        }
        f.blocks[c.block].instrs.splice(c.index..=c.index, out);
        if !changed.contains(&c.function) {
            changed.push(c.function);
        }
    }
    module.globals.extend(new_globals);
    if !changed.is_empty() {
        module.declare(Declaration { name: "memcpy".to_owned(), ty: copy_type(), conv: CallConv::C, attrs: Vec::new() });
    }
    changed
}

/// The type the copy is called at: `memcpy` over the integer arguments the ABI passes.
fn copy_type() -> FnType {
    FnType::new(Type::I64, vec![Type::I64, Type::Ptr, Type::I64])
}

/// The calls to run: to a function of the module, with a result nothing reads, over constants,
/// read-only data (the ciphertext) and buffers of the caller's frame (the plaintext).
fn candidates(module: &Module) -> Vec<Candidate> {
    let mut out = Vec::new();
    for (function, f) in module.functions.iter().enumerate() {
        let defs = known::definitions(f);
        let uses = f.use_counts();
        for (block, b) in f.blocks.iter().enumerate() {
            for (index, i) in b.instrs.iter().enumerate() {
                let Inst::Call { callee: Callee::Direct(callee), args, .. } = &i.inst else { continue };
                if module.function(callee).is_none() || i.result.is_some_and(|v| uses.get(&v).is_some_and(|n| *n > 0)) {
                    continue;
                }
                let Some(args) = args.iter().map(|(ty, o)| classify(ty, o, module, &defs)).collect::<Option<Vec<Arg>>>() else { continue };
                let buffers = args.iter().filter(|a| matches!(a, Arg::Buffer(..))).count();
                let data = args.iter().filter(|a| matches!(a, Arg::Data(..))).count();
                if buffers == 0 || data == 0 {
                    continue;
                }
                out.push(Candidate { function, block, index, callee: callee.clone(), args });
            }
        }
    }
    out
}

/// An argument as the run takes it; `None` for one the run cannot stand in for.
fn classify(ty: &Type, o: &Operand, module: &Module, defs: &HashMap<ValueId, Inst>) -> Option<Arg> {
    let constant = |g: &str| module.global(g).is_some_and(|g| g.constant);
    match o {
        Operand::Int(k) => Some(Arg::Int(*k as u128)),
        Operand::Address { global, offset } if constant(global) => Some(Arg::Data(global.clone(), *offset)),
        Operand::Global(g) if constant(g) => Some(Arg::Data(g.clone(), 0)),
        Operand::Local(_) => {
            if let Some((value, known)) = partial(o, defs) {
                return Some(Arg::Partial { value, known });
            }
            // An address in read-only data the code narrowed or offset (a 32-bit program's
            // pointer is the low half of the address) is still that byte.
            let (root, offset) = known::place(o, defs)?;
            let a = match root {
                Root::Alloca(a) => a,
                Root::Global(g) if constant(&g) => return Some(Arg::Data(g, offset)),
                _ => return None,
            };
            let Some(Inst::Alloca { ty: slot, .. }) = defs.get(&a) else { return None };
            let size = slot.size(8)?;
            let offset = u64::try_from(offset).ok()?;
            (offset < size).then(|| Arg::Buffer(ty.clone(), o.clone(), size - offset))
        }
        _ => None,
    }
}

/// The value and the mask of the known bits of `(x & m) | k`, the shape a write of a register's
/// low byte or word leaves, in the 64 bits an argument has.
fn partial(o: &Operand, defs: &HashMap<ValueId, Inst>) -> Option<(u128, u128)> {
    const WORD: u128 = 0xffff_ffff_ffff_ffff;
    let Operand::Local(v) = o else { return None };
    let Some(Inst::Bin { op: BinOp::Or, lhs, rhs, .. }) = defs.get(v) else { return None };
    let ((Operand::Local(kept), Operand::Int(k)) | (Operand::Int(k), Operand::Local(kept))) = (lhs, rhs) else { return None };
    let (Some(Inst::Bin { op: BinOp::And, rhs: Operand::Int(m), .. }) | Some(Inst::Bin { op: BinOp::And, lhs: Operand::Int(m), .. })) =
        defs.get(kept)
    else {
        return None;
    };
    let (m, k) = (*m as u128 & WORD, *k as u128 & WORD);
    let known = !m & WORD;
    (known != 0 && k & m == 0).then_some((k, known))
}

/// Runs `c` twice over buffers filled differently. The bytes each buffer got, when both runs
/// wrote the same prefix of it and touched nothing else; `None` in the place of a buffer the
/// runs left alone, and no result at all when the runs disagree or stop.
fn decrypt(vm: &mut Vm<'_>, c: &Candidate, statics_at: u64, baseline: &[u8], low: bool) -> Option<Vec<Option<Vec<u8>>>> {
    let sizes: Vec<u64> = c.args.iter().filter_map(|a| if let Arg::Buffer(_, _, n) = a { Some(*n) } else { None }).collect();
    let mut buffers: Vec<Backing> = sizes.iter().map(|n| Backing::new(*n, low)).collect::<Option<_>>()?;
    let addresses: Vec<u64> = buffers.iter_mut().map(Backing::start).collect();
    for (a, n) in addresses.iter().zip(&sizes) {
        vm.memory.add_region(Region::new(*a, *n, "lift-time buffer"));
    }
    let runs = [0x00u8, 0xff].map(|fill| run_once(vm, c, &addresses, &sizes, fill, statics_at, baseline));
    for a in &addresses {
        vm.memory.remove_region(*a);
    }
    let [Some(first), Some(second)] = runs else { return None };
    let mut out = Vec::new();
    for (i, n) in sizes.iter().enumerate() {
        let (x, y) = (&first[i], &second[i]);
        let untouched = |j: usize| x[j] == 0x00 && y[j] == 0xff;
        let written = (0..*n as usize).take_while(|&j| !untouched(j)).count();
        if written == 0 {
            out.push(None);
            continue;
        }
        if (written..*n as usize).any(|j| !untouched(j)) || x[..written] != y[..written] {
            return None;
        }
        out.push(Some(x[..written].to_vec()));
    }
    if out.iter().all(Option::is_none) {
        return None;
    }
    Some(out)
}

/// One run of `c` with the buffers filled with `fill`: the buffers' bytes after it, when it ran
/// to its return and left the read-only data as it was.
fn run_once(
    vm: &mut Vm<'_>,
    c: &Candidate,
    addresses: &[u64],
    sizes: &[u64],
    fill: u8,
    statics_at: u64,
    baseline: &[u8],
) -> Option<Vec<Vec<u8>>> {
    for (a, n) in addresses.iter().zip(sizes) {
        vm.memory.fill(*a, *n, fill).ok()?;
    }
    let mut buffer = 0;
    let mut args = Vec::new();
    for arg in &c.args {
        args.push(Value::Int(match arg {
            Arg::Int(k) => *k,
            Arg::Partial { value, known } => {
                if fill == 0 {
                    *value
                } else {
                    *value | (!known & u128::from(u64::MAX))
                }
            }
            Arg::Data(g, offset) => u128::from(vm.symbol(g)?.wrapping_add(*offset as u64)),
            Arg::Buffer(..) => {
                buffer += 1;
                u128::from(addresses[buffer - 1])
            }
        }));
    }
    vm.steps = 0;
    vm.call_named(&mut NoHost, &c.callee, args).ok()?;
    if vm.memory.read_vec(statics_at, baseline.len() as u64).ok()? != baseline {
        return None;
    }
    addresses.iter().zip(sizes).map(|(a, n)| vm.memory.read_vec(*a, *n).ok()).collect()
}
