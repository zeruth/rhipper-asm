//! The host of a run: what the driver does for a port, done for the interpreter. It binds the
//! import slots, keeps the heap the allocating imports hand out of, the threads with their
//! environment blocks and thread-local storage, the exceptions in flight and the exact x87 unit,
//! and answers every call the IR cannot: the `ra_*` runtime, the imports it intercepts (the
//! process exit, the console writes it captures, the threads it runs itself, the allocations) and
//! the rest through the host's own DLLs with the eight-integer adapter or the import's prototype.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ra_asm_ir::interp::{Host, Region, Stop, Symbol, Value, Vm, mask};
use ra_asm_ir::{FnType, Type};
use ra_asm_pe::Image;

use super::{HostCall, f80, os, printf};
use crate::Lifted;
use crate::raise::abi::Kind;
use crate::raise::prototypes;
use crate::state;

/// The handles the runtime makes up: a thread it ran, its heap, a stream, a module of a 32-bit
/// program. The values stay clear of the small even numbers the system uses.
/// The key the runtime's `ra_x86_encode_pointer` flips, as the port's runtime does.
const X86_POINTER_KEY: u32 = 0x5ca1_ab1e;
const THREAD_HANDLES: u64 = 0x5a00_0000;
const HEAP_HANDLE: u64 = 0x5a10_0000;
const STREAMS: u64 = 0x5a20_0000;
/// The `HRSRC` of each resource of the image: its index, from this base.
const RESOURCE_HANDLES: u64 = 0x5a30_0000;
const MODULE_HANDLES: u64 = 0x5a30_0000;

/// The DLLs a function of the host's C runtime is looked for in, when the image does not import
/// it: a statically linked runtime's functions, which the lift leaves to the host's.
const RUNTIME_DLLS: [&str; 6] = ["ucrtbase.dll", "msvcrt.dll", "vcruntime140.dll", "kernel32.dll", "ntdll.dll", "user32.dll"];

/// The bytes of an x86 `%State`: the general registers, `rip`, the flags, the vectors (`xmm0..15`
/// and the upper halves of `ymm0..15`).
pub(super) const STATE_SIZE: u64 = 16 * 8 + 8 + 8 + 32 * 16;
/// The bytes of an ARM64 `%State`: `x0..x30` and `sp`, `pc`, the flags, `v0..v31`.
const ARM64_STATE_SIZE: u64 = 32 * 8 + 8 + 8 + 32 * 16;
/// The bytes of a MIPS `%State`: `$0..$31`, `hi` and `lo`, `pc`, the condition codes, `$f0..$f31`,
/// the FPU's control and status register, the GTE's 64 registers, and the R5900's upper halves of
/// `$0..$31`, `hi` and `lo`, the R5900 FPU's accumulator, and the VFPU's 128 registers and its
/// condition bits, each field at its alignment and the whole padded to the vectors'.
const MIPS_STATE_SIZE: u64 = 34 * 8 + 8 + 8 + 32 * 16 + 4 + 64 * 4 + 4 + 34 * 8 + 4 + 128 * 4 + 4;

/// The end of the `PlayStation 2`'s hardware registers, which begin at 256 MB: the EE's, the
/// vector units' memories and the graphics synthesizer's privileged registers after them.
pub(super) const PS2_HARDWARE_END: u64 = 0x1300_0000;

/// A thread of the program as the interpreter runs it: one at a time, a created one to its end
/// inside `CreateThread`.
struct Thread {
    teb: u64,
    stack_top: u64,
    stack_bottom: u64,
    id: u32,
}

/// The ids the PSP kernel layer hands out, each kind's from a base of its own, positive as a
/// kernel's `SceUID` is: the loader's thread, which runs `module_start`, is the threads' base.
const PSP_THREADS: u64 = 0x0410_0000;
const PSP_SEMAPHORES: u64 = 0x0420_0000;
const PSP_BLOCKS: u64 = 0x0430_0000;
const PSP_LW_MUTEXES: u64 = 0x0440_0000;
/// The user partition of the PSP's RAM, which a module shares with the memory it allocates.
const PSP_USER_MEMORY: u64 = 24 << 20;
/// `SceKernelThreadInfo`, which `sceKernelReferThreadStatus` fills to the size its caller set.
const PSP_THREAD_INFO: u64 = 104;
/// A light mutex's attribute that lets its owner lock it again.
const PSP_LW_RECURSIVE: u32 = 0x200;
/// `SCE_ERROR_ERRNO_EBADF`.
const PSP_EBADF: u64 = 0x8001_0009;
/// `SCE_KERNEL_ERROR_MEMBLOCK_ALLOC_FAILED`.
const PSP_NO_MEMORY: u64 = 0x8002_00d9;

/// What the PSP kernel keeps for a module: the threads it created, its semaphores and its blocks of
/// partition memory, each by its index from its kind's base, less one.
#[derive(Default)]
struct Psp {
    threads: Vec<PspThread>,
    /// The thread running: `None` for the loader's.
    current: Option<usize>,
    /// Each semaphore's count and most.
    semaphores: Vec<(u32, u32)>,
    blocks: Vec<u64>,
    lw_mutexes: u64,
}

/// A thread `sceKernelCreateThread` made: its name (31 bytes at most), entry, priority and stack.
struct PspThread {
    name: Vec<u8>,
    entry: u64,
    priority: u64,
    stack: u64,
    stack_size: u64,
}

impl Psp {
    /// The index behind an id of the kind from `base`, among `len`.
    fn index(id: u64, base: u64, len: usize) -> Option<usize> {
        let i = id.checked_sub(base + 1)?;
        (i < len as u64).then_some(i as usize)
    }

    fn current_id(&self) -> u64 {
        self.current.map_or(PSP_THREADS, |i| PSP_THREADS + 1 + i as u64)
    }

    fn semaphore(&mut self, id: u64) -> Result<&mut (u32, u32), Stop> {
        match Self::index(id, PSP_SEMAPHORES, self.semaphores.len()) {
            Some(i) => Ok(&mut self.semaphores[i]),
            None => Err(Stop::fault(format!("the semaphore {id:#x}, which the kernel did not make"))),
        }
    }
}

/// A C++ exception: the thrown object's copy and the types it can be caught as, held by the
/// catches and the flight.
struct Exception {
    object: u64,
    types: Vec<(u64, u64, u64)>,
    /// The copy constructor of each type, zero for a type copied by its bytes.
    copies: Vec<u64>,
    /// The destructor the end of the exception runs on the object, zero for none.
    destructor: u64,
}

#[derive(Default)]
struct Exceptions {
    arena: Vec<Exception>,
    in_flight: Option<usize>,
    staged: Option<usize>,
    caught: Vec<usize>,
    /// Beside each catch being run, the `esp` the x86 frame handler saved from under the
    /// registration when the catch began: an unwind out of the catch puts it back, as the
    /// handler's own unwind does, so a catch further out resumes on the parent's stack.
    catch_sp: Vec<u64>,
    suspended: Vec<Option<usize>>,
    /// The structured exception in flight: its code and the `EXCEPTION_POINTERS` its filters read.
    structured: Option<(u32, u64)>,
}

/// The heap: a bump allocator with the size ahead of each block, never reused.
struct Heap {
    next: u64,
    end: u64,
}

pub(super) struct Runtime<'a> {
    pub(super) image: &'a Image,
    pub(super) lifted: &'a Lifted,
    bits: u32,
    /// The program's path and arguments.
    args: Vec<String>,
    live: bool,
    pub(super) output: Vec<u8>,
    allocations: Vec<os::Allocation>,
    heap: Heap,
    /// The host's standard handles: input, output, error.
    std_handles: [u64; 3],
    /// The host's `FILE` of its standard output and error, to know an `fprintf` to them.
    std_files: [u64; 2],
    /// The DLL and name of each import symbol of the image.
    imports: HashMap<String, (String, String)>,
    /// The host functions resolved so far, by symbol.
    procedures: HashMap<String, usize>,
    /// The host handles behind the module handles a 32-bit program holds.
    modules: Vec<usize>,
    /// The name the program loaded each host module by, for the functions it asks of the module.
    libraries: HashMap<usize, String>,
    /// The functions the program asked the host for by name, as it asked for them.
    pub(super) resolved: Vec<super::Resolved>,
    threads: Vec<Option<u32>>,
    thread: Thread,
    next_thread_id: u32,
    /// The copies of the image's resources the program loaded, by resource index: `.rsrc` is no
    /// global of the lift, so a `LockResource` is answered from the heap.
    resources: HashMap<usize, u64>,
    tls: [u64; 5],
    eh: Exceptions,
    f80: f80::Unit,
    onexit: Vec<(u64, Vec<u64>)>,
    /// The addresses of the runtime's globals: the x87 status and control words, `ra_pops`.
    sw: u64,
    cw: u64,
    pops: u64,
    command_line: [u64; 2],
    argv: u64,
    p_argv: u64,
    environment: u64,
    /// The C runtime's globals a 32-bit program reaches through a pointer (`__p__commode`,
    /// `__p__fmode`, `__p___argc`, `_errno`): a cell of the program's own memory each, made on
    /// first ask, since the host's pointer is a 64-bit one outside the program's memory.
    cells: BTreeMap<&'static str, u64>,
    /// What runs the code of the image no lifted function has, and the entries it ran.
    pub(super) residual: Option<super::Residual>,
    pub(super) residual_ran: BTreeSet<u64>,
    /// The calls across the platform boundary, while the run keeps them.
    pub(super) boundary: Option<Boundary>,
    /// What stands in for the host, when the run has none: the host's part of every call is its.
    pub(super) answers: Option<super::Answers>,
    /// The index the stand-in gave the call across the boundary in flight.
    event: Option<usize>,
    /// The calls the run made into the host's DLLs.
    pub(super) hosted: u64,
    psp: Psp,
    /// The machine of a console's program, when its console has a platform runtime.
    pub(super) machine: Option<super::console::Machine>,
}

/// The calls across the platform boundary a run made, and those still running.
#[derive(Default)]
pub(super) struct Boundary {
    pub(super) calls: Vec<HostCall>,
    /// Each running call, innermost last: its index in `calls` and the bytes written in it before
    /// a nested call took the memory's log.
    open: Vec<(usize, BTreeMap<u64, u8>)>,
}

impl Boundary {
    /// Starts the record of a call: what the log holds belongs to the call it interrupts.
    fn begin(&mut self, vm: &mut Vm<'_>, name: &str, args: &[Value]) {
        let written = vm.memory.disarm();
        if let Some((_, bytes)) = self.open.last_mut() {
            keep(bytes, written.writes);
        }
        self.calls.push(HostCall { name: name.to_owned(), args: args.to_vec(), result: Ok(Value::Int(0)), writes: Vec::new() });
        self.open.push((self.calls.len() - 1, BTreeMap::new()));
        vm.memory.arm();
    }

    /// Ends the record of the innermost call, whose writes are those of the call around it too.
    fn end(&mut self, vm: &mut Vm<'_>, result: &Result<Value, Stop>) {
        let written = vm.memory.disarm();
        let Some((index, mut bytes)) = self.open.pop() else {
            return;
        };
        keep(&mut bytes, written.writes);
        let mut writes: Vec<(u64, Vec<u8>)> = Vec::new();
        for (&address, &byte) in &bytes {
            match writes.last_mut() {
                Some((start, run)) if start.checked_add(run.len() as u64) == Some(address) => run.push(byte),
                _ => writes.push((address, vec![byte])),
            }
        }
        let call = &mut self.calls[index];
        call.result.clone_from(result);
        call.writes = writes;
        if let Some((_, outer)) = self.open.last_mut() {
            outer.extend(bytes);
            vm.memory.arm();
        }
    }
}

impl Boundary {
    /// Bytes the innermost running call wrote past the memory's log: a host DLL's writes.
    fn saw(&mut self, runs: Vec<(u64, Vec<u8>)>) {
        if let Some((_, bytes)) = self.open.last_mut() {
            keep(bytes, runs);
        }
    }
}

/// The most of the memory behind one pointer argument [`watch`] reads.
const WATCHED: u64 = 0x1000;

/// The memory each pointer-like argument of a host call points into, as it is before the call:
/// up to [`WATCHED`] bytes, or to the end of its page when the next one is not mapped. An
/// argument that is no mapped address is passed over.
fn watch(vm: &mut Vm<'_>, args: &[Value]) -> Vec<(u64, Vec<u8>)> {
    let mut watched: Vec<(u64, Vec<u8>)> = Vec::new();
    for address in args.iter().map(Value::as_u64).filter(|&a| a >= 0x1_0000) {
        if watched.iter().any(|(start, run)| (*start..start + run.len() as u64).contains(&address)) {
            continue;
        }
        let to_page_end = WATCHED - (address & (WATCHED - 1));
        let bytes = vm.memory.read_vec(address, WATCHED).or_else(|_| vm.memory.read_vec(address, to_page_end));
        if let Ok(bytes) = bytes {
            watched.push((address, bytes));
        }
    }
    watched
}

/// The bytes of `watched` that the call changed, as runs of adjacent addresses.
fn changed(vm: &mut Vm<'_>, watched: &[(u64, Vec<u8>)]) -> Vec<(u64, Vec<u8>)> {
    let mut runs: Vec<(u64, Vec<u8>)> = Vec::new();
    for (start, before) in watched {
        let Ok(after) = vm.memory.read_vec(*start, before.len() as u64) else { continue };
        for (offset, (&old, &new)) in before.iter().zip(&after).enumerate() {
            if old == new {
                continue;
            }
            let address = start + offset as u64;
            match runs.last_mut() {
                Some((at, run)) if at.checked_add(run.len() as u64) == Some(address) => run.push(new),
                _ => runs.push((address, vec![new])),
            }
        }
    }
    runs
}

/// `runs` laid over `bytes`, the later write of a byte over the earlier.
fn keep(bytes: &mut BTreeMap<u64, u8>, runs: Vec<(u64, Vec<u8>)>) {
    for (start, run) in runs {
        bytes.extend((start..).zip(run));
    }
}

/// Whether a call is one across the platform boundary: an import, or a `ra_*` function that
/// answers for the system; not the lift's own machinery (an indirect call, the x87 unit, the
/// exceptions in flight, a trap).
fn crosses(name: &str) -> bool {
    !(name.starts_with("ra_f80_") || name.starts_with("ra_eh_") || matches!(name, "ra_call" | "ra_pops_of" | "ra_trap" | "ra_unsupported"))
}

fn trap_name(code: u64) -> &'static str {
    match code {
        1 => "int3",
        2 => "ud2",
        3 => "hlt",
        4 => "a divide error (a zero divisor or a quotient too large)",
        5 => "128-bit division",
        6 => "__fastfail",
        7 => "a switch to an address outside its table",
        8 => "bytes that are no instruction",
        9 => "a software interrupt",
        10 => "a call through a vtable slot to a function outside the vtables",
        _ => "a trap",
    }
}

fn arg(args: &[Value], n: usize) -> u64 {
    args.get(n).map_or(0, Value::as_u64)
}

/// Whether a resource's id is the one a call asked for: a name is matched without case, as the
/// system does.
fn resource_ids_match(have: &ra_asm_pe::ResourceId, want: &ra_asm_pe::ResourceId) -> bool {
    match (have, want) {
        (ra_asm_pe::ResourceId::Name(a), ra_asm_pe::ResourceId::Name(b)) => a.eq_ignore_ascii_case(b),
        _ => have == want,
    }
}

fn farg(args: &[Value], n: usize) -> f64 {
    args.get(n).map_or(0.0, Value::as_f64)
}

fn slot(vm: &mut Vm<'_>, address: u64) -> Result<f80::Slot, Stop> {
    let bytes = vm.memory.read_vec(address, 16)?;
    Ok(bytes.try_into().unwrap())
}

/// A call to the software `f80` (`ra_f80_*`) over the interpreter's memory, answered by `unit`,
/// which keeps the flags the call raised for the caller to merge into the status word.
pub(super) fn f80_dispatch(unit: &mut f80::Unit, vm: &mut Vm<'_>, name: &str, args: &[Value]) -> Result<Value, Stop> {
    let a = |n: usize| arg(args, n);
    Ok(match name {
        "ra_f80_load" => {
            let bytes = vm.memory.read_vec(a(1), 10)?;
            vm.memory.write(a(0), &f80::Unit::load(&bytes))?;
            Value::Int(0)
        }
        "ra_f80_store" => {
            let x = slot(vm, a(1))?;
            vm.memory.write(a(0), &f80::Unit::store(&x))?;
            Value::Int(0)
        }
        "ra_f80_pack" => {
            vm.memory.write(a(0), &f80::Unit::pack(a(1), a(2)))?;
            Value::Int(0)
        }
        "ra_f80_from_f64" => {
            vm.memory.write(a(0), &f80::Unit::from_f64(farg(args, 1)))?;
            Value::Int(0)
        }
        "ra_f80_store_f64" => {
            vm.memory.write(a(0), &f80::Unit::from_f64(farg(args, 1))[..10])?;
            Value::Int(0)
        }
        "ra_f80_from_i64" => {
            vm.memory.write(a(0), &f80::Unit::from_i64(a(1)))?;
            Value::Int(0)
        }
        "ra_f80_to_f64" => Value::F64(unit.f64_of(&slot(vm, a(0))?, a(1))),
        "ra_f80_to_f32" => Value::F32(unit.f32_of(&slot(vm, a(0))?, a(1))),
        "ra_f80_to_int" => Value::Int(u128::from(unit.int_of(&slot(vm, a(0))?, a(1), a(2)))),
        "ra_f80_add" | "ra_f80_sub" | "ra_f80_mul" | "ra_f80_div" => {
            let (x, y) = (slot(vm, a(1))?, slot(vm, a(2))?);
            let r = match name {
                "ra_f80_add" => unit.add(&x, &y, a(3)),
                "ra_f80_sub" => unit.sub(&x, &y, a(3)),
                "ra_f80_mul" => unit.mul(&x, &y, a(3)),
                _ => unit.div(&x, &y, a(3)),
            };
            vm.memory.write(a(0), &r)?;
            Value::Int(0)
        }
        "ra_f80_sqrt" | "ra_f80_rndint" => {
            let x = slot(vm, a(1))?;
            let r = if name == "ra_f80_sqrt" { unit.sqrt(&x, a(2)) } else { unit.rndint(&x, a(2)) };
            vm.memory.write(a(0), &r)?;
            Value::Int(0)
        }
        "ra_f80_chs" | "ra_f80_abs" => {
            let x = slot(vm, a(1))?;
            let r = if name == "ra_f80_chs" { f80::Unit::chs(&x) } else { f80::Unit::abs(&x) };
            vm.memory.write(a(0), &r)?;
            Value::Int(0)
        }
        "ra_f80_cmp" => Value::Int(u128::from(f80::Unit::cmp(&slot(vm, a(0))?, &slot(vm, a(1))?))),
        _ => return Err(Stop::fault(format!("the runtime function {name} is not interpreted"))),
    })
}

// The arms of the host's big matches all answer with a `Result`; this one wraps for them.
#[allow(clippy::unnecessary_wraps)]
fn int(v: u64) -> Result<Value, Stop> {
    Ok(Value::Int(u128::from(v)))
}

impl<'a> Runtime<'a> {
    pub(super) fn new(
        image: &'a Image,
        lifted: &'a Lifted,
        vm: &mut Vm<'_>,
        args: Vec<String>,
        live: bool,
        stack: os::Allocation,
        heap: os::Allocation,
    ) -> Self {
        let bits = lifted.bits;
        let stack_top = (stack.start + stack.len - 64) & !15;
        let (pid, tid) = os::ids();
        let _ = pid;
        let imports = image
            .imports
            .iter()
            .map(|i| {
                let name = i.name.clone().unwrap_or_else(|| format!("#{}", i.ordinal.unwrap_or(0)));
                (crate::x86::import_symbol(&i.display_name()), (i.dll.clone(), name))
            })
            .collect();
        let mut runtime = Self {
            image,
            lifted,
            bits,
            args,
            live,
            output: Vec::new(),
            heap: Heap { next: heap.start, end: heap.start + heap.len },
            allocations: vec![stack, heap],
            std_handles: [0; 3],
            std_files: [0, 0],
            imports,
            procedures: HashMap::new(),
            modules: Vec::new(),
            libraries: HashMap::new(),
            resolved: Vec::new(),
            threads: Vec::new(),
            thread: Thread { teb: 0, stack_top, stack_bottom: 0, id: tid },
            next_thread_id: tid.wrapping_add(1),
            resources: HashMap::new(),
            tls: [0; 5],
            eh: Exceptions::default(),
            f80: f80::Unit::default(),
            onexit: Vec::new(),
            sw: vm.symbol(state::X87_SW).unwrap_or(0),
            cw: vm.symbol(state::X87_CW).unwrap_or(0),
            pops: vm.symbol(state::POPS).unwrap_or(0),
            command_line: [0, 0],
            argv: 0,
            p_argv: 0,
            environment: 0,
            cells: BTreeMap::new(),
            residual: None,
            residual_ran: BTreeSet::new(),
            boundary: None,
            answers: None,
            event: None,
            hosted: 0,
            psp: Psp::default(),
            machine: None,
        };
        runtime.thread.stack_bottom = runtime.allocations[0].start;
        runtime
    }

    /// The host's standard handles, and its own standard streams, so a `printf` through a `FILE`
    /// the host handed the program still lands in the captured output. A run with a stand-in for
    /// the host takes the handles from the `GetStdHandle` calls the stand-in answers instead.
    pub(super) fn attach_host(&mut self) {
        self.std_handles = [os::std_handle(-10i32 as u32), os::std_handle(-11i32 as u32), os::std_handle(-12i32 as u32)];
        if let Some(iob) = os::library("ucrtbase.dll").and_then(|m| os::procedure(m, "__acrt_iob_func")) {
            self.std_files = [unsafe { os::call8(iob, &[1, 0, 0, 0, 0, 0, 0, 0]) }, unsafe { os::call8(iob, &[2, 0, 0, 0, 0, 0, 0, 0]) }];
        }
    }

    /// The stand-in's answer to the host's part of the call across the boundary in flight.
    fn replayed(&self, vm: &mut Vm<'_>) -> Result<Value, Stop> {
        match (&self.answers, self.event) {
            (Some(answers), Some(index)) => answers.answer(&mut vm.memory, index),
            _ => Err(Stop::fault("a call into the host outside the calls the stand-in for the host was told of")),
        }
    }

    // region Setup

    /// Binds every import slot of the image to an address of the interpreter's that names the
    /// import, so a call through the slot comes back here, and makes the lifted functions
    /// reachable at their image addresses through the table the lift wrote.
    pub(super) fn bind(&mut self, vm: &mut Vm<'_>) -> Result<(), Stop> {
        let ptr = u64::from(self.bits / 8);
        for import in &self.image.imports {
            let symbol = crate::x86::import_symbol(&import.display_name());
            let address = vm.bind_external(&symbol)?;
            // A slot outside the mapped image (a delay-load slot in a section the lift did not
            // keep) is left alone.
            let _ = vm.memory.write_word(import.iat_va, ptr, address);
        }
        if let Some(ra_asm_ir::Const::Array(entries)) = self.lifted.module.global("ra_functions").and_then(|g| g.init.as_ref()) {
            for entry in entries {
                if let ra_asm_ir::Const::Struct(fields) = entry
                    && let (Some(ra_asm_ir::Const::Int(va)), Some(ra_asm_ir::Const::Global(name))) = (fields.first(), fields.get(1))
                {
                    vm.alias(*va as u64, name);
                }
            }
        }
        if let Some(tls) = vm.symbol(state::TLS) {
            for (i, word) in self.tls.iter_mut().enumerate() {
                *word = vm.memory.read_u64(tls + 8 * i as u64)?;
            }
        }
        Ok(())
    }

    /// The loader's work before the entry: `__tls_index` written, the main thread's block, and
    /// the TLS callbacks with `DLL_PROCESS_ATTACH`.
    pub(super) fn attach_process(&mut self, vm: &mut Vm<'_>) -> Result<(), Stop> {
        if self.tls[0] != 0 && self.tls[2] != 0 {
            vm.memory.write_u32(self.tls[2], 0)?;
        }
        self.attach_thread(vm, 1)
    }

    /// The thread environment block of the current thread, in the layout of the program's width:
    /// the stack bounds, the block's own address, the process and thread ids, and the pointer to
    /// the thread-local storage array, whose one slot is the thread's copy of the template.
    fn attach_thread(&mut self, vm: &mut Vm<'_>, reason: u64) -> Result<(), Stop> {
        let teb = self.allocate(vm, 4096, 4096)?;
        let slot = self.allocate(vm, 8, 8)?;
        let (pid, _) = os::ids();
        let (base, limit, id) = (self.thread.stack_top, self.thread.stack_bottom, self.thread.id);
        if self.bits == 32 {
            vm.memory.write_u32(teb + 0x04, base as u32)?;
            vm.memory.write_u32(teb + 0x08, limit as u32)?;
            vm.memory.write_u32(teb + 0x18, teb as u32)?;
            vm.memory.write_u32(teb + 0x20, pid)?;
            vm.memory.write_u32(teb + 0x24, id)?;
            vm.memory.write_u32(teb + 0x2c, slot as u32)?;
        } else {
            vm.memory.write_u64(teb + 0x08, base)?;
            vm.memory.write_u64(teb + 0x10, limit)?;
            vm.memory.write_u64(teb + 0x30, teb)?;
            vm.memory.write_u64(teb + 0x40, u64::from(pid))?;
            vm.memory.write_u64(teb + 0x48, u64::from(id))?;
            vm.memory.write_u64(teb + 0x58, slot)?;
        }
        self.thread.teb = teb;
        if self.tls[0] == 0 {
            return Ok(());
        }
        let initialized = self.tls[1].saturating_sub(self.tls[0]);
        let block = self.allocate(vm, initialized + self.tls[3] + 16, 16)?;
        let template = vm.memory.read_vec(self.tls[0], initialized)?;
        vm.memory.write(block, &template)?;
        vm.memory.write_u64(slot, block)?;
        let width = u64::from(self.bits / 8);
        let mut p = self.tls[4];
        while p != 0 {
            let target = vm.memory.read_word(p, width)?;
            if target == 0 {
                break;
            }
            self.call_back(vm, target, [self.image.image_base(), reason, 0])?;
            p += width;
        }
        Ok(())
    }

    /// `size` bytes of the heap, zeroed, aligned to `align`.
    fn allocate(&mut self, vm: &mut Vm<'_>, size: u64, align: u64) -> Result<u64, Stop> {
        let align = align.clamp(16, 4096);
        let at = (self.heap.next + 16).div_ceil(align) * align;
        let end = (at + size.max(1)).div_ceil(16) * 16;
        if end > self.heap.end {
            return Err(Stop::fault("the heap of the interpreter is full"));
        }
        vm.memory.write_u64(at - 16, size)?;
        self.heap.next = end;
        Ok(at)
    }

    fn in_heap(&self, address: u64) -> bool {
        address >= self.allocations[1].start && address < self.heap.end
    }

    /// Whether a module handle names the program itself: `NULL` or its image base.
    fn is_image_module(&self, module: u64) -> bool {
        module == 0 || module == self.image.image_base()
    }

    /// The index of the resource behind an `HRSRC` the runtime handed out.
    fn resource_index(&self, handle: u64) -> Option<usize> {
        let index = handle.checked_sub(RESOURCE_HANDLES)? / 16;
        (handle.is_multiple_of(16) && index >= 1 && index <= self.image.resources.len() as u64).then(|| index as usize - 1)
    }

    /// A resource's type or name as the call passed it: an ordinal below 0x10000
    /// (`MAKEINTRESOURCE`), a string else, `#123` standing for an ordinal.
    fn resource_id(vm: &mut Vm<'_>, arg: u64, wide: bool) -> Result<ra_asm_pe::ResourceId, Stop> {
        if arg < 0x10000 {
            return Ok(ra_asm_pe::ResourceId::Id(arg as u32));
        }
        let text = if wide {
            String::from_utf16_lossy(&vm.memory.read_wstr(arg, 1 << 12)?)
        } else {
            String::from_utf8_lossy(&vm.memory.read_cstr(arg, 1 << 12)?).into_owned()
        };
        Ok(match text.strip_prefix('#').and_then(|n| n.parse().ok()) {
            Some(id) => ra_asm_pe::ResourceId::Id(id),
            None => ra_asm_pe::ResourceId::Name(text),
        })
    }

    /// The program's copy of resource `index`, made on the first load.
    fn resource_copy(&mut self, vm: &mut Vm<'_>, index: usize) -> Result<u64, Stop> {
        if let Some(&at) = self.resources.get(&index) {
            return Ok(at);
        }
        let bytes = self.image.resources[index].data(self.image).map_err(|e| Stop::fault(format!("{e:#}")))?.to_vec();
        let at = self.allocate(vm, bytes.len() as u64, 16)?;
        vm.memory.write(at, &bytes)?;
        self.resources.insert(index, at);
        Ok(at)
    }

    fn reallocate(&mut self, vm: &mut Vm<'_>, old: u64, size: u64) -> Result<u64, Stop> {
        let new = self.allocate(vm, size, 16)?;
        if old != 0 && self.in_heap(old) {
            let old_size = vm.memory.read_u64(old - 16)?;
            let bytes = vm.memory.read_vec(old, old_size.min(size))?;
            vm.memory.write(new, &bytes)?;
        }
        Ok(new)
    }

    /// A block of `size` bytes in the heap, for the run's own use.
    pub(super) fn new_block(&mut self, vm: &mut Vm<'_>, size: u64) -> Result<u64, Stop> {
        self.allocate(vm, size, 16)
    }

    /// A fresh `%State` in the heap.
    pub(super) fn new_state(&mut self, vm: &mut Vm<'_>) -> Result<u64, Stop> {
        let size = match self.lifted.isa {
            crate::state::Isa::X86 | crate::state::Isa::Arm32 => STATE_SIZE,
            crate::state::Isa::Arm64 => ARM64_STATE_SIZE,
            crate::state::Isa::Mips | crate::state::Isa::MipsN32 => MIPS_STATE_SIZE,
        };
        self.allocate(vm, size, 16)
    }

    pub(super) fn stack_top(&self) -> u64 {
        self.thread.stack_top
    }

    /// The current thread's environment block, which ARM64 code reaches through `x18`.
    pub(super) fn teb(&self) -> u64 {
        self.thread.teb
    }

    /// The string `text` in the heap, NUL-terminated.
    fn string(&mut self, vm: &mut Vm<'_>, text: &[u8]) -> Result<u64, Stop> {
        let at = self.allocate(vm, text.len() as u64 + 1, 16)?;
        vm.memory.write(at, text)?;
        Ok(at)
    }

    /// The program's arguments as an `argv` of its pointer width, NUL-terminated, in the heap.
    pub(super) fn argv(&mut self, vm: &mut Vm<'_>) -> Result<(u64, u64), Stop> {
        if self.argv == 0 {
            let width = u64::from(self.bits / 8);
            let args = self.args.clone();
            let table = self.allocate(vm, width * (args.len() as u64 + 1), 16)?;
            for (i, a) in args.iter().enumerate() {
                let s = self.string(vm, a.as_bytes())?;
                vm.memory.write_word(table + width * i as u64, width, s)?;
            }
            self.argv = table;
            self.p_argv = self.allocate(vm, 8, 8)?;
            vm.memory.write_word(self.p_argv, width, table)?;
        }
        Ok((self.args.len() as u64, self.argv))
    }

    fn command_line(&mut self, vm: &mut Vm<'_>, wide: bool) -> Result<u64, Stop> {
        let i = usize::from(wide);
        if self.command_line[i] == 0 {
            let quoted: Vec<String> =
                self.args.iter().map(|a| if a.contains(' ') && !a.starts_with('"') { format!("\"{a}\"") } else { a.clone() }).collect();
            let text = quoted.join(" ");
            self.command_line[i] = if wide {
                let units: Vec<u8> = text.encode_utf16().chain(std::iter::once(0)).flat_map(u16::to_le_bytes).collect();
                let at = self.allocate(vm, units.len() as u64, 16)?;
                vm.memory.write(at, &units)?;
                at
            } else {
                self.string(vm, text.as_bytes())?
            };
        }
        Ok(self.command_line[i])
    }

    // endregion

    // region Calls into the program

    /// Calls the function at `target` as the system calls a callback (a TLS callback, a thread's
    /// start): a function of the machine shape gets a state with the arguments where the
    /// convention puts them and a return address of zero; any other gets them as a universal call.
    pub(super) fn call_back(&mut self, vm: &mut Vm<'_>, target: u64, args: [u64; 3]) -> Result<u64, Stop> {
        let Some(name) = vm.name_at(target).map(str::to_owned) else {
            return Err(Stop::fault(format!("a callback at {target:#x}, which is no function")));
        };
        let machine_shape = vm.function(&name).is_some_and(|f| f.ty.params == [Type::Ptr] && f.ty.ret == Type::Void);
        if !machine_shape {
            let ty = state::import_adapter_type();
            let values = [args[0], args[1], args[2], 0, 0, 0, 0, 0].iter().map(|&a| Value::Int(u128::from(a))).collect();
            return vm.call_address(self, target, &ty, values).map(|v| v.as_u64());
        }
        let state = self.new_state(vm)?;
        let mut sp = self.thread.stack_top;
        if self.bits == 32 {
            for v in [args[2], args[1], args[0], 0] {
                sp -= 4;
                vm.memory.write_u32(sp, v as u32)?;
            }
        } else {
            sp -= 32;
            sp -= 8;
            vm.memory.write_u64(sp, 0)?;
            vm.memory.write_u64(state + 8, args[0])?;
            vm.memory.write_u64(state + 16, args[1])?;
            vm.memory.write_u64(state + 64, args[2])?;
        }
        vm.memory.write_u64(state + 32, sp)?;
        vm.call_named(self, &name, vec![Value::Int(u128::from(state))])?;
        Ok(vm.memory.read_u64(state)? & mask(self.bits) as u64)
    }

    /// After the entry: an exception nothing caught ends the program, as terminate would.
    pub(super) fn check_uncaught(&self) -> Result<(), Stop> {
        match (self.eh.in_flight, self.eh.structured) {
            (Some(_), _) => Err(Stop::fault("an exception the program did not catch")),
            (None, Some((code, _))) => Err(Stop::fault(format!("the structured exception {code:#x} was left in flight"))),
            (None, None) => Ok(()),
        }
    }

    /// Runs `start(parameter)` as a thread of its own: a stack and a block of its own, attached
    /// with `DLL_THREAD_ATTACH`, to its end before the creator goes on. The result is the thread's
    /// exit code.
    fn run_thread(&mut self, vm: &mut Vm<'_>, start: u64, parameter: u64) -> Result<u32, Stop> {
        let Some(stack) = os::allocate_low(1 << 20) else { return Err(Stop::fault("no room for a thread's stack")) };
        vm.memory.add_region(Region::new(stack.start, stack.len, "stack"));
        let top = (stack.start + stack.len - 64) & !15;
        let id = self.next_thread_id;
        self.next_thread_id += 1;
        let previous = std::mem::replace(&mut self.thread, Thread { teb: 0, stack_top: top, stack_bottom: stack.start, id });
        self.allocations.push(stack);
        let result = self.attach_thread(vm, 2).and_then(|()| self.call_back(vm, start, [parameter, 0, 0]));
        self.thread = previous;
        result.map(|r| r as u32)
    }

    // endregion

    // region The runtime functions

    fn runtime_call(&mut self, vm: &mut Vm<'_>, name: &str, args: &[Value]) -> Result<Value, Stop> {
        match name {
            "ra_trap" => Err(Stop::fault(format!("{} at {:#x}", trap_name(arg(args, 1)), arg(args, 0)))),
            "ra_unsupported" => {
                Err(Stop::fault(format!("an instruction the lift does not cover at {:#x} (code {})", arg(args, 0), arg(args, 1))))
            }
            "ra_call" => self.machine_call(vm, arg(args, 0), arg(args, 1)),
            "ra_pops_of" => {
                let recorded = vm.memory.read_u64(self.pops)?;
                vm.memory.write_u64(self.pops, 0)?;
                let target = arg(args, 0);
                match vm.symbol_at(target).cloned() {
                    Some(Symbol::External(symbol)) => int(self.pops_of(&symbol).unwrap_or(recorded)),
                    _ => int(recorded),
                }
            }
            "ra_teb" => int(self.thread.teb),
            // The processor's queries are the host's: its stand-in answers them.
            "ra_cpuid" | "ra_xgetbv" | "ra_rdtsc" if self.answers.is_some() => self.replayed(vm),
            "ra_cpuid" => {
                let regs = cpuid(arg(args, 0) as u32, arg(args, 1) as u32);
                let out = arg(args, 2);
                for (i, r) in regs.iter().enumerate() {
                    vm.memory.write_u32(out + 4 * i as u64, *r)?;
                }
                int(0)
            }
            "ra_xgetbv" => int(xgetbv(arg(args, 0) as u32)),
            "ra_rdtsc" => int(rdtsc()),
            "ra_pause" => int(0),
            // The machine's own stack has the room: the stack pointer moves down by the bytes.
            "ra_stack_probe" => int(arg(args, 0).wrapping_sub(arg(args, 1)) & !(arg(args, 2).max(1) - 1) & 0xffff_ffff),
            "ra_svc" => self.svc(vm, arg(args, 0)),
            "ra_syscall" => {
                let number = (arg(args, 0) as u32).wrapping_sub(4000);
                let regs = [arg(args, 1) as u32, arg(args, 2) as u32, arg(args, 3) as u32];
                let result = self.linux(vm, number, true, regs)?;
                Ok(Value::Int(u128::from(i64::from(result as i32) as u64)))
            }
            n if n.starts_with("ra_f32_ftz_") => match super::ftz_call(n, &[arg(args, 0) as u32, arg(args, 1) as u32]) {
                Some(r) => int(u64::from(r)),
                None => Err(Stop::fault(format!("the runtime function {name} is not interpreted"))),
            },
            n if n.starts_with("ra_f80_") => self.f80_call(vm, n, args),
            // A console's BIOS, kernel and devices are its machine's.
            n if n.starts_with("ra_bios_") => self.console_call(vm, &n["ra_bios_".len()..], args),
            n if n.starts_with("ra_ee_") => self.console_call(vm, &n["ra_ee_".len()..], args),
            n if n.starts_with("ra_gte_") => super::console::gte(vm, args),
            crate::state::PSX_EXCEPTION => {
                self.psx_raised(vm, arg(args, 0), arg(args, 1) as u32, arg(args, 2) as u32)?;
                int(0)
            }
            crate::state::PSX_JUMP => {
                self.psx_jump(arg(args, 1))?;
                self.machine_call(vm, arg(args, 0), arg(args, 1))
            }
            crate::state::PSX_RETURN => {
                self.psx_returns(arg(args, 1) as u32)?;
                int(0)
            }
            crate::state::PSX_STATUS => {
                self.psx_status(vm)?;
                int(0)
            }
            n if let Some(call) = crate::data::DeviceCall::parse(n) => self.console_access(vm, call.write, call.bits, args),
            n if n.starts_with("ra_eh_") => self.eh_call(vm, n, args),
            n if n.starts_with("ra_x86_") => self.shim_call(vm, n, args),
            _ => Err(Stop::fault(format!("the runtime function {name} is not interpreted"))),
        }
    }

    /// A system call of a 32-bit ARM Linux program, by the EABI: the number in `r7`, the
    /// arguments from `r0`, the result in `r0`.
    fn svc(&mut self, vm: &mut Vm<'_>, state: u64) -> Result<Value, Stop> {
        let r = |vm: &mut Vm<'_>, n: u64| vm.memory.read_u32(state + n * 8);
        let number = r(vm, 7)?;
        let regs = [r(vm, 0)?, r(vm, 1)?, r(vm, 2)?];
        let result = self.linux(vm, number, false, regs)?;
        vm.memory.write_u64(state, u64::from(result))?;
        int(0)
    }

    /// A Linux system call by its generic number (o32's less its `4000` when `mips`) on its first
    /// three arguments: the kernel's result, negative with the error number on a failure. The calls
    /// a program without a C library makes to print and to end are here; another stops the run with
    /// its number.
    fn linux(&mut self, vm: &mut Vm<'_>, number: u32, mips: bool, [fd, buffer, len]: [u32; 3]) -> Result<u32, Stop> {
        match (mips, number) {
            // exit, exit_group
            (false, 1 | 248) | (true, 1 | 246) => Err(Stop::Exit(fd as i32)),
            // write
            (_, 4) => {
                let bytes = vm.memory.read_vec(u64::from(buffer), u64::from(len))?;
                Ok(match fd {
                    1 | 2 => {
                        self.emit(fd == 2, &bytes);
                        len
                    }
                    // -EBADF
                    _ => 9u32.wrapping_neg(),
                })
            }
            _ => Err(Stop::fault(format!("the system call {number} is not interpreted"))),
        }
    }

    /// A function of the PSP kernel a module imports, by the name its NID stands for, on the words
    /// of `a0..a3`: the threads (a started one runs to its end inside `sceKernelStartThread`), the
    /// light mutexes and semaphores of a run with one thread, the standard descriptors and the
    /// writes to output and error, partition memory out of the interpreter's heap, the timezone of
    /// a console set to UTC, and the game's exit. Another function, an argument the PSP's
    /// convention passes past `a3` (in `t0` and up, which the import's wrapper does not read), or
    /// a wait no other thread could end, stops the run.
    #[allow(clippy::too_many_lines)]
    fn psp(&mut self, vm: &mut Vm<'_>, name: &str, args: &[Value]) -> Result<Value, Stop> {
        let word = |i: usize| u64::from(arg(args, i) as u32);
        match name {
            "sceKernelExitGame" => Err(Stop::Exit(0)),
            "sceIoWrite" => match word(0) {
                fd @ (1 | 2) => {
                    let bytes = vm.memory.read_vec(word(1), word(2))?;
                    self.emit(fd == 2, &bytes);
                    int(word(2))
                }
                _ => int(PSP_EBADF),
            },
            "sceKernelStdin" => int(3),
            "sceKernelStdout" => int(1),
            "sceKernelStderr" => int(2),
            "sceKernelCreateThread" => {
                let name = vm.memory.read_cstr(word(0), 31)?;
                let stack_size = word(3);
                let stack = self.allocate(vm, stack_size, 256)?;
                self.psp.threads.push(PspThread { name, entry: word(1), priority: word(2), stack, stack_size });
                int(PSP_THREADS + self.psp.threads.len() as u64)
            }
            "sceKernelStartThread" => self.psp_start_thread(vm, word(0), word(1), word(2)),
            "sceKernelGetThreadId" => int(self.psp.current_id()),
            "sceKernelReferThreadStatus" => {
                let id = if word(0) == 0 { self.psp.current_id() } else { word(0) };
                let Some(i) = Psp::index(id, PSP_THREADS, self.psp.threads.len()) else {
                    return Err(Stop::fault(format!("sceKernelReferThreadStatus of {id:#x}, a thread the run did not create")));
                };
                let info = word(1);
                let size = u64::from(vm.memory.read_u32(info)?).min(PSP_THREAD_INFO) as usize;
                let t = &self.psp.threads[i];
                // Running, or dormant: one not started, or one that ended.
                let status = if self.psp.current == Some(i) { 1 } else { 16 };
                let gp = self.image.global_pointer.unwrap_or(0);
                let mut bytes = vec![0u8; PSP_THREAD_INFO as usize];
                bytes[4..4 + t.name.len()].copy_from_slice(&t.name);
                for (offset, v) in
                    [(40, status), (44, t.entry), (48, t.stack), (52, t.stack_size), (56, gp), (60, t.priority), (64, t.priority)]
                {
                    bytes[offset..offset + 4].copy_from_slice(&(v as u32).to_le_bytes());
                }
                if size > 4 {
                    vm.memory.write(info + 4, &bytes[4..size])?;
                }
                int(0)
            }
            "sceKernelDelayThread"
            | "sceKernelDeleteThread"
            | "sceKernelDeleteLwMutex"
            | "sceKernelDeleteSema"
            | "sceKernelFreePartitionMemory" => int(0),
            // The work area: the lock count, the owner, the attributes, the waiting threads, the id.
            "sceKernelCreateLwMutex" => {
                let (work, count) = (word(0), word(3));
                let owner = if count > 0 { self.psp.current_id() } else { 0 };
                self.psp.lw_mutexes += 1;
                for (offset, v) in [(0, count), (4, owner), (8, word(2)), (12, 0), (16, PSP_LW_MUTEXES + self.psp.lw_mutexes)] {
                    vm.memory.write_u32(work + offset, v as u32)?;
                }
                int(0)
            }
            "sceKernelLockLwMutex" | "sceKernelUnlockLwMutex" => {
                let (work, count) = (word(0), word(1) as u32);
                let (level, owner, attr) = (vm.memory.read_u32(work)?, vm.memory.read_u32(work + 4)?, vm.memory.read_u32(work + 8)?);
                let me = self.psp.current_id() as u32;
                let (level, owner) = if name == "sceKernelLockLwMutex" {
                    if level != 0 && (owner != me || attr & PSP_LW_RECURSIVE == 0) {
                        return Err(Stop::fault(format!("a lock of the light mutex at {work:#x}, which its owner {owner:#x} holds")));
                    }
                    (level.saturating_add(count), me)
                } else {
                    if level < count || owner != me {
                        return Err(Stop::fault(format!("an unlock of the light mutex at {work:#x}, which the thread does not hold")));
                    }
                    (level - count, if level == count { 0 } else { me })
                };
                vm.memory.write_u32(work, level)?;
                vm.memory.write_u32(work + 4, owner)?;
                int(0)
            }
            "sceKernelCreateSema" => {
                self.psp.semaphores.push((word(2) as u32, word(3) as u32));
                int(PSP_SEMAPHORES + self.psp.semaphores.len() as u64)
            }
            "sceKernelWaitSema" => {
                let (id, take) = (word(0), word(1) as u32);
                let semaphore = self.psp.semaphore(id)?;
                if semaphore.0 < take {
                    return Err(Stop::fault(format!("a wait on the semaphore {id:#x}, which no other thread can signal")));
                }
                semaphore.0 -= take;
                int(0)
            }
            "sceKernelSignalSema" => {
                let (id, give) = (word(0), word(1) as u32);
                let semaphore = self.psp.semaphore(id)?;
                if semaphore.0.checked_add(give).is_none_or(|count| count > semaphore.1) {
                    return Err(Stop::fault(format!("a signal of the semaphore {id:#x} past its most")));
                }
                semaphore.0 += give;
                int(0)
            }
            "sceKernelMaxFreeMemSize" => int(self.psp_free_memory()),
            // Low or high in the partition: the interpreter's heap has no sides.
            "sceKernelAllocPartitionMemory" if word(2) <= 1 => {
                let size = word(3);
                if size > self.psp_free_memory() {
                    return int(PSP_NO_MEMORY);
                }
                let at = self.allocate(vm, size, 256)?;
                self.psp.blocks.push(at);
                int(PSP_BLOCKS + self.psp.blocks.len() as u64)
            }
            "sceKernelGetBlockHeadAddr" => match Psp::index(word(0), PSP_BLOCKS, self.psp.blocks.len()) {
                Some(i) => int(self.psp.blocks[i]),
                None => Err(Stop::fault(format!("the memory block {:#x}, which the kernel did not allocate", word(0)))),
            },
            // The timezone's offset in minutes and the daylight saving.
            "sceUtilityGetSystemParamInt" if matches!(word(0), 6 | 7) => {
                vm.memory.write_u32(word(1), 0)?;
                int(0)
            }
            // The rest is the console's machine: the display, the controller, the sound.
            _ if self.machine.is_some() => self.console_call(vm, name, args),
            _ => Err(Stop::fault(format!("the PSP kernel function {name} is not interpreted"))),
        }
    }

    /// What `sceKernelMaxFreeMemSize` answers: the user partition less the module, or what the
    /// interpreter's heap has left when that is less, in the kernel's 256-byte units.
    fn psp_free_memory(&self) -> u64 {
        let partition = PSP_USER_MEMORY - u64::from(self.image.size_of_image());
        partition.min(self.heap.end.saturating_sub(self.heap.next + 4096)) & !0xff
    }

    /// `sceKernelStartThread`: the kernel copies the `length` bytes at `argp` to the top of the
    /// thread's stack and calls its entry with the length and the copy, `gp` the module's. The
    /// thread runs here to its end, the one thread the run has while it runs.
    fn psp_start_thread(&mut self, vm: &mut Vm<'_>, id: u64, length: u64, argp: u64) -> Result<Value, Stop> {
        let Some(index) = Psp::index(id, PSP_THREADS, self.psp.threads.len()) else {
            return Err(Stop::fault(format!("sceKernelStartThread of {id:#x}, a thread the run did not create")));
        };
        let (entry, stack, stack_size) = {
            let t = &self.psp.threads[index];
            (t.entry, t.stack, t.stack_size)
        };
        if length + 256 > stack_size {
            return Err(Stop::fault(format!("sceKernelStartThread of {id:#x} with {length} bytes, past its stack")));
        }
        let copy = (stack + stack_size - length) & !15;
        if argp != 0 && length != 0 {
            let bytes = vm.memory.read_vec(argp, length)?;
            vm.memory.write(copy, &bytes)?;
        }
        let Some(name) = vm.name_at(entry).map(str::to_owned) else {
            return Err(Stop::fault(format!("a thread's entry at {entry:#x}, which is no function")));
        };
        if !vm.function(&name).is_some_and(|f| f.ty.params == [Type::Ptr] && f.ty.ret == Type::Void) {
            return Err(Stop::fault(format!("a thread's entry @{name}, which is not of the machine shape")));
        }
        let state = self.new_state(vm)?;
        vm.memory.write_u64(state + 4 * 8, length)?;
        vm.memory.write_u64(state + 5 * 8, if argp == 0 { 0 } else { copy })?;
        if let Some(gp) = self.image.global_pointer {
            vm.memory.write_u64(state + 28 * 8, gp)?;
        }
        vm.memory.write_u64(state + 29 * 8, (copy - 64) & !15)?;
        let outer = self.psp.current.replace(index);
        let result = vm.call_named(self, &name, vec![Value::Int(u128::from(state))]);
        self.psp.current = outer;
        result.map(|_| Value::Int(0))
    }

    /// An indirect call of the machine model: a lifted function at `target` takes the state; a
    /// function of the host's is called with the arguments where the convention puts them, and
    /// the return address the caller pushed is popped here, with what a 32-bit import pops. Code
    /// of the image no lifted function has is the run's [`super::Residual`]'s, unless the run wrote
    /// its page, where the run stops.
    fn machine_call(&mut self, vm: &mut Vm<'_>, state: u64, target: u64) -> Result<Value, Stop> {
        // A jump of a kernel's handler back to where its exception was taken ends the handler,
        // whatever is at the address.
        if self.r3000() && self.r3000_leaves(target as u32)? {
            return Err(Stop::fault(super::r3000::RETURNED));
        }
        vm.record_call(target);
        // A PlayStation's code is the lifted function of its address, whatever segment names
        // it; code the lift has no function for is its processor's.
        if self.r3000() {
            let depth = self.psx_nest(vm, target as u32)?;
            let result = (|| {
                match self.psx_lifted(vm, target)? {
                    Some(at) => {
                        let name = vm.name_at(at).unwrap_or_default().to_owned();
                        let link = vm.memory.read_u64(state + 8 * 31)? as u32;
                        vm.call_named(self, &name, vec![Value::Int(u128::from(state))])?;
                        self.psx_returned(vm, state, link)?;
                    }
                    None => self.r3000_call(vm, state, target)?,
                }
                Ok(())
            })();
            self.psx_unnest(depth);
            result?;
            return int(0);
        }
        match vm.symbol_at(target).cloned() {
            Some(Symbol::Function(_)) => {
                let name = vm.name_at(target).unwrap_or_default().to_owned();
                vm.call_named(self, &name, vec![Value::Int(u128::from(state))])?;
                int(0)
            }
            Some(Symbol::External(symbol)) if self.lifted.isa == state::Isa::Arm64 => {
                // `x0..x7` carry the arguments and `x0` the result; the return is through `x30`,
                // so the stack is the caller's as it was.
                let mut args = Vec::with_capacity(8);
                for i in 0..8u64 {
                    args.push(Value::Int(u128::from(vm.memory.read_u64(state + 8 * i)?)));
                }
                let result = self.import_call(vm, &symbol, &state::import_adapter_type(), &args)?.as_u64();
                vm.memory.write_u64(state, result)?;
                int(0)
            }
            Some(Symbol::External(symbol)) => {
                let rsp = vm.memory.read_u64(state + 32)?;
                let ty = state::import_adapter_type();
                if self.bits == 64 {
                    let mut values = [0u64; 8];
                    for (i, v) in values.iter_mut().enumerate() {
                        *v = match i {
                            0 => vm.memory.read_u64(state + 8)?,
                            1 => vm.memory.read_u64(state + 16)?,
                            2 => vm.memory.read_u64(state + 64)?,
                            3 => vm.memory.read_u64(state + 72)?,
                            _ => vm.memory.read_u64(rsp + 8 + 8 * i as u64)?,
                        };
                    }
                    let args: Vec<Value> = values.iter().map(|&v| Value::Int(u128::from(v))).collect();
                    let result = self.import_call(vm, &symbol, &ty, &args)?.as_u64();
                    vm.memory.write_u64(state, result)?;
                    vm.memory.write_u64(state + 32, rsp + 8)?;
                } else {
                    let mut args = Vec::with_capacity(8);
                    for i in 0..8u64 {
                        args.push(Value::Int(u128::from(vm.memory.read_u32(rsp + 4 + 4 * i)?)));
                    }
                    let result = self.import_call(vm, &symbol, &ty, &args)?.as_u64();
                    vm.memory.write_u64(state, u64::from(result as u32))?;
                    vm.memory.write_u64(state + 16, result >> 32)?;
                    let pops = self.pops_of(&symbol).unwrap_or(0);
                    vm.memory.write_u64(state + 32, u64::from((rsp as u32).wrapping_add(4 + pops as u32)))?;
                }
                int(0)
            }
            // What the program wrote there is no code the lift or the engine's image holds.
            None if self.in_image(target) && vm.memory.was_written(target) => Err(Stop::Unpacked(target)),
            None if self.in_image(target) => {
                let Some(residual) = &self.residual else {
                    return Err(Stop::fault(format!("an indirect call to {target:#x}, which is no lifted function")));
                };
                self.residual_ran.insert(target);
                residual.call(&mut vm.memory, state, target)?;
                int(0)
            }
            None => Err(Stop::fault(format!("an indirect call to {target:#x}, which is no function the interpreter knows"))),
        }
    }

    fn in_image(&self, address: u64) -> bool {
        address >= self.image.image_base() && address < self.image.image_base() + u64::from(self.image.size_of_image())
    }

    /// What a call to the import `symbol` pops in 32 bits: by its DLL and name.
    fn pops_of(&self, symbol: &str) -> Option<u64> {
        let (dll, name) = self.imports.get(symbol)?;
        prototypes::stdcall_pops(dll, name)
    }

    fn f80_call(&mut self, vm: &mut Vm<'_>, name: &str, args: &[Value]) -> Result<Value, Stop> {
        let result = f80_dispatch(&mut self.f80, vm, name, args)?;
        let flags = std::mem::take(&mut self.f80.flags);
        if flags != 0 && self.sw != 0 {
            let sw = vm.memory.read_u16(self.sw)?;
            vm.memory.write_u16(self.sw, sw | flags)?;
        }
        Ok(result)
    }

    /// Writes the `EXCEPTION_RECORD` of an access violation at `site` and the `EXCEPTION_POINTERS`
    /// to it, in the program's layout, and gives the pointers' address. There is no context: a
    /// filter that reads one faults rather than reads made-up registers.
    fn access_violation(&mut self, vm: &mut Vm<'_>, address: u64, write: bool, site: u64) -> Result<u64, Stop> {
        self.exception_record(vm, 0xC000_0005, 0, site, &[u64::from(write), address])
    }

    /// Writes the `EXCEPTION_RECORD` of the exception `code` at `site`, with its flags and its
    /// parameters (at most fifteen, as the record holds), and the `EXCEPTION_POINTERS` to it; gives
    /// the pointers' address.
    fn exception_record(&mut self, vm: &mut Vm<'_>, code: u32, flags: u32, site: u64, parameters: &[u64]) -> Result<u64, Stop> {
        let word = if self.bits == 32 { 4 } else { 8 };
        let information = if word == 4 { 20 } else { 32 };
        let size = information + 15 * word;
        let record = self.allocate(vm, size, 16)?;
        vm.memory.fill(record, size, 0)?;
        vm.memory.write_u32(record, code)?;
        vm.memory.write_u32(record + 4, flags)?;
        vm.memory.write_word(record + 8 + word, word, site)?;
        let parameters = &parameters[..parameters.len().min(15)];
        vm.memory.write_u32(record + 8 + 2 * word, parameters.len() as u32)?;
        for (n, &p) in parameters.iter().enumerate() {
            vm.memory.write_word(record + information + n as u64 * word, word, p)?;
        }
        let pointers = self.allocate(vm, 2 * word, 16)?;
        vm.memory.write_word(pointers, word, record)?;
        vm.memory.write_word(pointers + word, word, 0)?;
        Ok(pointers)
    }

    fn eh_call(&mut self, vm: &mut Vm<'_>, name: &str, args: &[Value]) -> Result<Value, Stop> {
        let a = |n: usize| arg(args, n);
        match name {
            "ra_eh_throw" => {
                if self.eh.in_flight.is_some() {
                    return Err(Stop::fault("a throw while an exception is in flight"));
                }
                let size = a(1);
                let copy = self.allocate(vm, size.max(1), 16)?;
                let bytes = vm.memory.read_vec(a(0), size)?;
                vm.memory.write(copy, &bytes)?;
                self.eh.arena.push(Exception { object: copy, types: Vec::new(), copies: Vec::new(), destructor: 0 });
                self.eh.in_flight = Some(self.eh.arena.len() - 1);
                int(0)
            }
            "ra_eh_throw_type" => {
                let Some(e) = self.eh.in_flight else { return Err(Stop::fault("a catchable type with no throw to belong to")) };
                self.eh.arena[e].types.push((a(0), a(1), a(2)));
                self.eh.arena[e].copies.push(0);
                int(0)
            }
            "ra_eh_throw_copy" => {
                let Some(copy) = self.eh.in_flight.and_then(|e| self.eh.arena[e].copies.last_mut()) else {
                    return Err(Stop::fault("a copy constructor with no catchable type to belong to"));
                };
                *copy = a(0);
                int(0)
            }
            "ra_eh_throw_destructor" => {
                let Some(e) = self.eh.in_flight else { return Err(Stop::fault("a destructor with no throw to belong to")) };
                self.eh.arena[e].destructor = a(0);
                int(0)
            }
            // The end of the catch ends the exception unless it is in flight again, or held by a
            // catch or an unwind further out.
            "ra_eh_ending_object" | "ra_eh_ending_destructor" => {
                let Some(&e) = self.eh.caught.last() else { return Err(Stop::fault("the end of a catch body with no exception caught")) };
                let held = self.eh.in_flight == Some(e)
                    || self.eh.staged == Some(e)
                    || self.eh.caught[..self.eh.caught.len() - 1].contains(&e)
                    || self.eh.suspended.contains(&Some(e));
                let destructor = if held { 0 } else { self.eh.arena[e].destructor };
                int(if name == "ra_eh_ending_object" { self.eh.arena[e].object } else { destructor })
            }
            "ra_eh_rethrow" => {
                if self.eh.in_flight.is_some() {
                    return Err(Stop::fault("a rethrow while an exception is in flight"));
                }
                let Some(&e) = self.eh.caught.last() else { return Err(Stop::fault("a rethrow with no exception caught")) };
                self.eh.in_flight = Some(e);
                int(0)
            }
            "ra_eh_pending" => int(u64::from(self.eh.in_flight.is_some())),
            "ra_eh_catches" => {
                let Some(e) = self.eh.in_flight else { return Err(Stop::fault("a catch with no exception in flight")) };
                let (key, slot, flags) = (a(0), a(1), a(2));
                let Some(n) = self.eh.arena[e].types.iter().position(|t| key == 0 || t.0 == key) else { return int(0) };
                let (_, displacement, size) = self.eh.arena[e].types[n];
                if slot != 0 && flags & 8 == 0 && self.eh.arena[e].copies[n] != 0 {
                    return Err(Stop::fault("a catch by value of an object with a copy constructor is not interpreted yet"));
                }
                if slot != 0 {
                    let subobject = self.eh.arena[e].object + displacement;
                    if flags & 8 != 0 {
                        // By reference: the slot holds a pointer of the program's width.
                        vm.memory.write_word(slot, if flags >> 8 == 32 { 4 } else { 8 }, subobject)?;
                    } else {
                        let bytes = vm.memory.read_vec(subobject, size)?;
                        vm.memory.write(slot, &bytes)?;
                    }
                }
                self.eh.staged = Some(e);
                self.eh.in_flight = None;
                int(1)
            }
            "ra_eh_enter_catch" => {
                if self.eh.in_flight.is_some() {
                    return Err(Stop::fault("an exception escaped a destructor on the way to a catch"));
                }
                let Some(e) = self.eh.staged.take() else { return Err(Stop::fault("a catch body with no exception matched")) };
                self.eh.caught.push(e);
                self.eh.catch_sp.push(0);
                int(0)
            }
            "ra_eh_catch_sp" => {
                let Some(sp) = self.eh.catch_sp.last_mut() else { return Err(Stop::fault("a saved esp with no catch being run")) };
                *sp = a(0);
                int(0)
            }
            "ra_eh_ending_sp" => match self.eh.catch_sp.last() {
                Some(&sp) => int(sp),
                None => Err(Stop::fault("the end of a catch body with no exception caught")),
            },
            "ra_eh_leave_catch" => {
                if self.eh.caught.pop().is_none() {
                    return Err(Stop::fault("the end of a catch body with no exception caught"));
                }
                self.eh.catch_sp.pop();
                int(0)
            }
            "ra_eh_raise" => {
                let (code, flags, count, arguments, site) = (a(0), a(1), a(2), a(3), a(4));
                if self.eh.structured.is_some() {
                    return Err(Stop::fault(format!("RaiseException at {site:#x} while a structured exception is in flight")));
                }
                let word = if self.bits == 32 { 4 } else { 8 };
                let mut parameters = Vec::new();
                for n in 0..count.min(15) {
                    parameters.push(vm.memory.read_word(arguments + n * word, word)?);
                }
                let pointers = self.exception_record(vm, code as u32, flags as u32, site, &parameters)?;
                self.eh.structured = Some((code as u32, pointers));
                int(0)
            }
            "ra_eh_suspend" => {
                let e = self.eh.in_flight.take();
                self.eh.suspended.push(e);
                int(0)
            }
            "ra_eh_resume" => {
                if self.eh.in_flight.is_some() {
                    return Err(Stop::fault("an exception escaped a destructor while another was unwinding"));
                }
                let Some(e) = self.eh.suspended.pop() else { return Err(Stop::fault("a resume with no exception suspended")) };
                self.eh.in_flight = e;
                int(0)
            }
            "ra_eh_probe" => {
                let (address, size, write, site) = (a(0), a(1).max(1), a(2) != 0, a(3));
                let mapped =
                    vm.memory.region_of(address).is_some_and(|r| address.checked_add(size).is_some_and(|end| end <= r.start + r.len));
                if mapped {
                    return int(0);
                }
                if self.eh.structured.is_some() {
                    return Err(Stop::fault(format!("a fault at {site:#x} while a structured exception is in flight")));
                }
                let pointers = self.access_violation(vm, address, write, site)?;
                self.eh.structured = Some((0xC000_0005, pointers));
                int(1)
            }
            "ra_eh_pointers" => match self.eh.structured {
                Some((_, pointers)) => int(pointers),
                None => Err(Stop::fault("a filter with no structured exception in flight")),
            },
            "ra_eh_handled" => match self.eh.structured.take() {
                Some(_) => int(0),
                None => Err(Stop::fault("a handler with no structured exception in flight")),
            },
            "ra_eh_unhandled" => {
                let code = self.eh.structured.map_or(0, |(code, _)| code);
                Err(Stop::fault(format!("the exception {code:#x} at {:#x} passes a frame the lift does not unwind yet", a(0))))
            }
            _ => Err(Stop::fault(format!("the runtime function {name} is not interpreted"))),
        }
    }

    /// The C runtime functions a 32-bit program calls that walk or hand back a table of its own
    /// pointers, and the floating-point environment of the x87 words.
    fn shim_call(&mut self, vm: &mut Vm<'_>, name: &str, args: &[Value]) -> Result<Value, Stop> {
        let a = |n: usize| arg(args, n);
        match name {
            "ra_x86_initterm" | "ra_x86_initterm_e" => {
                let mut p = a(0);
                while p < a(1) {
                    let f = u64::from(vm.memory.read_u32(p)?);
                    if f != 0 {
                        let result = self.call_back(vm, f, [0, 0, 0])? as u32;
                        if name == "ra_x86_initterm_e" && result != 0 {
                            return int(u64::from(result));
                        }
                    }
                    p += 4;
                }
                int(0)
            }
            "ra_x86_initialize_onexit_table" => {
                if a(0) == 0 {
                    return int(u64::MAX);
                }
                vm.memory.fill(a(0), 12, 0)?;
                match self.onexit.iter_mut().find(|(t, _)| *t == a(0)) {
                    Some(entry) => entry.1.clear(),
                    None => self.onexit.push((a(0), Vec::new())),
                }
                int(0)
            }
            "ra_x86_register_onexit_function" => {
                match self.onexit.iter_mut().find(|(t, _)| *t == a(0)) {
                    Some(entry) => entry.1.push(a(1)),
                    None => self.onexit.push((a(0), vec![a(1)])),
                }
                int(0)
            }
            "ra_x86_execute_onexit_table" => {
                let functions = self.onexit.iter_mut().find(|(t, _)| *t == a(0)).map(|e| std::mem::take(&mut e.1)).unwrap_or_default();
                for f in functions.into_iter().rev() {
                    if f != 0 {
                        self.call_back(vm, f, [0, 0, 0])?;
                    }
                }
                int(0)
            }
            "ra_x86_initialize_slist_head" => {
                vm.memory.fill(a(0), 8, 0)?;
                int(0)
            }
            // The program's list: an 8-byte head of the first entry, the depth and the sequence,
            // and a dword link at the start of each entry.
            "ra_x86_interlocked_push_entry_slist" => {
                let (first, depth, sequence) = (vm.memory.read_u32(a(0))?, vm.memory.read_u16(a(0) + 4)?, vm.memory.read_u16(a(0) + 6)?);
                vm.memory.write_u32(a(1), first)?;
                vm.memory.write_u32(a(0), a(1) as u32)?;
                vm.memory.write_u16(a(0) + 4, depth.wrapping_add(1))?;
                vm.memory.write_u16(a(0) + 6, sequence.wrapping_add(1))?;
                int(u64::from(first))
            }
            "ra_x86_interlocked_pop_entry_slist" => {
                let first = vm.memory.read_u32(a(0))?;
                if first != 0 {
                    let next = vm.memory.read_u32(u64::from(first))?;
                    let depth = vm.memory.read_u16(a(0) + 4)?;
                    vm.memory.write_u32(a(0), next)?;
                    vm.memory.write_u16(a(0) + 4, depth.wrapping_sub(1))?;
                }
                int(u64::from(first))
            }
            "ra_x86_interlocked_flush_slist" => {
                let first = vm.memory.read_u32(a(0))?;
                vm.memory.write_u32(a(0), 0)?;
                vm.memory.write_u16(a(0) + 4, 0)?;
                int(u64::from(first))
            }
            "ra_x86_query_depth_slist" => int(u64::from(vm.memory.read_u16(a(0) + 4)?)),
            // The program's INIT_ONCE is a dword: done is bit 1, the context above the reserved
            // bits; the callback writes its context into the program's own dword (or the state's).
            "ra_x86_init_once_execute_once" => {
                if vm.memory.read_u32(a(0))? & 2 == 0 {
                    let out = if a(3) != 0 { a(3) } else { a(0) };
                    if self.call_back(vm, a(1), [a(0), a(2), out])? as u32 == 0 {
                        return int(0);
                    }
                    let made = vm.memory.read_u32(out)?;
                    vm.memory.write_u32(a(0), (made & !3) | 2)?;
                }
                if a(3) != 0 {
                    let state = vm.memory.read_u32(a(0))?;
                    vm.memory.write_u32(a(3), state & !3)?;
                }
                int(1)
            }
            // One thread at a time: a critical section is never contended, and the program's 24
            // bytes are left as it zeroed them.
            "ra_x86_initialize_critical_section" => {
                vm.memory.fill(a(0), 24, 0)?;
                int(0)
            }
            "ra_x86_initialize_critical_section_and_spin_count" | "ra_x86_initialize_critical_section_ex" => {
                vm.memory.fill(a(0), 24, 0)?;
                int(1)
            }
            // (and operator delete: the interpreter's heap is never given back)
            "ra_x86_enter_critical_section"
            | "ra_x86_leave_critical_section"
            | "ra_x86_delete_critical_section"
            | "ra_x86_operator_delete" => int(0),
            "ra_x86_try_enter_critical_section" => int(1),
            // Each thread ran to its end inside CreateThread: a wait on threads is over at once.
            "ra_x86_wait_for_multiple_objects" => {
                for i in 0..a(0).min(64) {
                    let handle = u64::from(vm.memory.read_u32(a(1) + 4 * i)?);
                    if self.thread_index(handle).is_none() {
                        return Err(Stop::fault(format!("WaitForMultipleObjects on {handle:#x}, which is no thread of the run")));
                    }
                }
                int(0)
            }
            // The runtime's encoding of a pointer, which keeps to the program's dword.
            "ra_x86_encode_pointer" | "ra_x86_decode_pointer" => int(u64::from(a(0) as u32 ^ X86_POINTER_KEY)),
            // The interpreter runs one thread at a time: each is a plain read and write.
            "ra_x86_interlocked_increment" | "ra_x86_interlocked_decrement" => {
                let old = vm.memory.read_u32(a(0))?;
                let new = if name == "ra_x86_interlocked_increment" { old.wrapping_add(1) } else { old.wrapping_sub(1) };
                vm.memory.write_u32(a(0), new)?;
                int(u64::from(new))
            }
            "ra_x86_interlocked_exchange" | "ra_x86_interlocked_exchange_add" => {
                let old = vm.memory.read_u32(a(0))?;
                let new = if name == "ra_x86_interlocked_exchange" { a(1) as u32 } else { old.wrapping_add(a(1) as u32) };
                vm.memory.write_u32(a(0), new)?;
                int(u64::from(old))
            }
            "ra_x86_interlocked_compare_exchange" => {
                let old = vm.memory.read_u32(a(0))?;
                if old == a(2) as u32 {
                    vm.memory.write_u32(a(0), a(1) as u32)?;
                }
                int(u64::from(old))
            }
            "ra_x86_operator_new" => int(self.allocate(vm, a(0).max(1), 16)?),
            "ra_x86_p_argv" => {
                self.argv(vm)?;
                int(self.p_argv)
            }
            "ra_x86_p_commode" | "ra_x86_p_fmode" | "ra_x86_p_argc" | "ra_x86_errno" => {
                let key: &'static str = match name {
                    "ra_x86_p_commode" => "commode",
                    "ra_x86_p_fmode" => "fmode",
                    "ra_x86_p_argc" => "argc",
                    _ => "errno",
                };
                if let Some(&cell) = self.cells.get(key) {
                    return int(cell);
                }
                let cell = self.allocate(vm, 4, 16)?;
                let value = if key == "argc" { self.argv(vm)?.0 as u32 } else { 0 };
                vm.memory.write_u32(cell, value)?;
                self.cells.insert(key, cell);
                int(cell)
            }
            "ra_x86_initial_narrow_environment" => {
                if self.environment == 0 {
                    // The host's environment, as dword pointers to copies in the heap.
                    let vars: Vec<String> = std::env::vars().map(|(k, v)| format!("{k}={v}")).collect();
                    let table = self.allocate(vm, 4 * (vars.len() as u64 + 1), 16)?;
                    for (i, v) in vars.iter().enumerate() {
                        let s = self.string(vm, v.as_bytes())?;
                        vm.memory.write_u32(table + 4 * i as u64, s as u32)?;
                    }
                    self.environment = table;
                }
                int(self.environment)
            }
            "ra_x86_controlfp" => int(u64::from(self.controlfp(vm, a(0) as u32, a(1) as u32)?)),
            "ra_x86_controlfp_s" => {
                let word = self.controlfp(vm, a(1) as u32, a(2) as u32)?;
                if a(0) != 0 {
                    vm.memory.write_u32(a(0), word)?;
                }
                int(0)
            }
            "ra_x86_fesetround" => {
                if a(0) & !u64::from(MCW_RC) != 0 {
                    return int(1);
                }
                self.controlfp(vm, a(0) as u32, MCW_RC)?;
                int(0)
            }
            "ra_x86_statusfp" | "ra_x86_clearfp" => {
                let sw = vm.memory.read_u16(self.sw)?;
                if name == "ra_x86_clearfp" {
                    vm.memory.write_u16(self.sw, sw & !0xff)?;
                }
                int(u64::from(crt_flags(u32::from(sw) & 0x3f)))
            }
            "ra_x86_fetestexcept" => {
                let sw = vm.memory.read_u16(self.sw)?;
                int(u64::from(crt_flags(u32::from(sw) & 0x3f) & a(0) as u32))
            }
            "ra_x86_feclearexcept" => {
                let sw = vm.memory.read_u16(self.sw)?;
                vm.memory.write_u16(self.sw, sw & !(x87_flags(a(0) as u32) as u16))?;
                int(0)
            }
            "ra_x86_sqrt" => {
                // The x86 C runtime's square root is the x87's: rounded to the precision control.
                let r = farg(args, 0).sqrt();
                let cw = vm.memory.read_u16(self.cw)?;
                Ok(Value::F64(if cw & 0x300 == 0 { f64::from(r as f32) } else { r }))
            }
            "ra_x86_pow_sse2" => Ok(Value::F64(farg(args, 0).powf(farg(args, 1)))),
            "ra_x86_sqrt_sse2" => Ok(Value::F64(farg(args, 0).sqrt())),
            "ra_x86_sin_sse2" => Ok(Value::F64(farg(args, 0).sin())),
            "ra_x86_cos_sse2" => Ok(Value::F64(farg(args, 0).cos())),
            "ra_x86_tan_sse2" => Ok(Value::F64(farg(args, 0).tan())),
            "ra_x86_asin_sse2" => Ok(Value::F64(farg(args, 0).asin())),
            "ra_x86_acos_sse2" => Ok(Value::F64(farg(args, 0).acos())),
            "ra_x86_atan_sse2" => Ok(Value::F64(farg(args, 0).atan())),
            "ra_x86_exp_sse2" => Ok(Value::F64(farg(args, 0).exp())),
            "ra_x86_log_sse2" => Ok(Value::F64(farg(args, 0).ln())),
            "ra_x86_log10_sse2" => Ok(Value::F64(farg(args, 0).log10())),
            "ra_x86_report_gsfailure" => Err(Stop::fault("the stack cookie check failed")),
            "ra_x86_report_rangecheckfailure" => Err(Stop::fault("an array index was out of range")),
            "ra_x86_setjmp3" | "ra_x86_longjmp" => Err(Stop::fault("a setjmp or longjmp, which the interpreter does not run yet")),
            "ra_x86_strtol" | "ra_x86_strtoul" | "ra_x86_strtoll" | "ra_x86_strtoull" | "ra_x86_strtod" => {
                let text = vm.memory.read_cstr(a(0), 1 << 16)?;
                let (value, used) = if name == "ra_x86_strtod" {
                    let (v, n) = parse_double(&text);
                    (v.to_bits(), n)
                } else {
                    let (v, n) = parse_integer(&text, a(2) as u32);
                    let v = match name {
                        "ra_x86_strtol" => i64::from(v.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32) as u64,
                        "ra_x86_strtoul" => u64::from(v.clamp(0, i128::from(u32::MAX)) as u32),
                        "ra_x86_strtoll" => v.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as u64,
                        _ => v.clamp(0, i128::from(u64::MAX)) as u64,
                    };
                    (v, n)
                };
                if a(1) != 0 {
                    vm.memory.write_u32(a(1), (a(0) + used as u64) as u32)?;
                }
                if name == "ra_x86_strtod" { Ok(Value::F64(f64::from_bits(value))) } else { int(value) }
            }
            "ra_x86_vsprintf" | "ra_x86_vsprintf_s" => {
                let text = self.format_list(vm, a(3), a(5), 4)?;
                Self::write_formatted(vm, a(1), a(2), &text)
            }
            "ra_x86_vsnprintf_s" => {
                let text = self.format_list(vm, a(4), a(6), 4)?;
                Self::write_formatted(vm, a(1), a(2).min(a(3).saturating_add(1)), &text)
            }
            "ra_x86_vfprintf" => {
                let text = self.format_list(vm, a(2), a(4), 4)?;
                self.write_stream(vm, a(1), &text)
            }
            // The interpreter's module handles and function addresses fit a dword: it answers
            // these as the imports themselves.
            "ra_x86_get_module_handle_a"
            | "ra_x86_get_module_handle_w"
            | "ra_x86_load_library_a"
            | "ra_x86_load_library_w"
            | "ra_x86_load_library_ex_a"
            | "ra_x86_load_library_ex_w"
            | "ra_x86_get_proc_address" => {
                let import = match name {
                    "ra_x86_get_module_handle_a" => "GetModuleHandleA",
                    "ra_x86_get_module_handle_w" => "GetModuleHandleW",
                    "ra_x86_load_library_a" => "LoadLibraryA",
                    "ra_x86_load_library_w" => "LoadLibraryW",
                    "ra_x86_load_library_ex_a" => "LoadLibraryExA",
                    "ra_x86_load_library_ex_w" => "LoadLibraryExW",
                    _ => "GetProcAddress",
                };
                self.intercept(vm, import, args)?.ok_or_else(|| Stop::fault(format!("{import} is not interpreted")))
            }
            _ => Err(Stop::fault(format!("the runtime function {name} is not interpreted"))),
        }
    }

    /// `_controlfp` over the lifted control word alone: the masks, the rounding mode and the
    /// precision; the host's own arithmetic keeps its default rounding.
    fn controlfp(&mut self, vm: &mut Vm<'_>, new: u32, mask: u32) -> Result<u32, Stop> {
        let cw = vm.memory.read_u16(self.cw)?;
        let word = (crt_control(u32::from(cw)) & !mask) | (new & mask);
        vm.memory.write_u16(self.cw, x87_control(word, cw))?;
        Ok(word & (MCW_EM | MCW_RC | MCW_PC))
    }

    // endregion

    // region The imports

    fn import_call(&mut self, vm: &mut Vm<'_>, name: &str, ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        if self.image.platform() == Some("psp") {
            return self.psp(vm, name, args);
        }
        if self.bits == 32
            && let Some(shim) = prototypes::x86_shim(name)
        {
            return self.runtime_call(vm, shim, args);
        }
        if let Some(v) = self.intercept(vm, name, args)? {
            return Ok(v);
        }
        self.native_call(vm, name, ty, args)
    }

    /// The imports the interpreter answers itself: the process exit, the console writes (so the
    /// output is captured), the threads, the allocations, the module handles and the stdio of
    /// the C runtime. `None` for an import left to the host.
    #[allow(clippy::too_many_lines)]
    fn intercept(&mut self, vm: &mut Vm<'_>, name: &str, args: &[Value]) -> Result<Option<Value>, Stop> {
        let a = |n: usize| arg(args, n);
        let some = |v: u64| Ok(Some(Value::Int(u128::from(v))));
        match name {
            "ExitProcess" | "exit" | "_exit" | "_Exit" | "quick_exit" | "_cexit" => Err(Stop::Exit(a(0) as i32)),
            "TerminateProcess" => Err(Stop::Exit(a(1) as i32)),
            // The copy the lift puts in place of a string's decryptor (see `raise::strings`).
            "memcpy" | "memmove" => {
                let bytes = vm.memory.read_vec(a(1), a(2))?;
                vm.memory.write(a(0), &bytes)?;
                some(a(0))
            }
            // The C runtime's writers through a pointer: the host's would write past the memory's
            // log, so a session would hold none of their bytes and its replay would lose them.
            "memset" => {
                vm.memory.write(a(0), &vec![a(1) as u8; a(2) as usize])?;
                some(a(0))
            }
            "strcpy" | "strcat" => {
                let mut bytes = vm.memory.read_cstr(a(1), 1 << 20)?;
                bytes.push(0);
                let at = if name == "strcat" { a(0) + vm.memory.read_cstr(a(0), 1 << 20)?.len() as u64 } else { a(0) };
                vm.memory.write(at, &bytes)?;
                some(a(0))
            }
            "strncpy" => {
                let mut bytes = vm.memory.read_cstr(a(1), a(2) as usize)?;
                bytes.resize(a(2) as usize, 0);
                vm.memory.write(a(0), &bytes)?;
                some(a(0))
            }
            "strncat" => {
                let mut bytes = vm.memory.read_cstr(a(1), a(2) as usize)?;
                bytes.push(0);
                let at = a(0) + vm.memory.read_cstr(a(0), 1 << 20)?.len() as u64;
                vm.memory.write(at, &bytes)?;
                some(a(0))
            }
            "abort" => Err(Stop::Exit(3)),
            // A 32-bit program's exit function and filter are addresses in its own image: handed to
            // the host's C runtime they would be called at the host's exit, an access violation.
            // The exit functions go in the interpreter's own table; the filter is never the host's.
            "_crt_atexit" | "atexit" if self.bits == 32 => {
                match self.onexit.iter_mut().find(|(t, _)| *t == 0) {
                    Some(entry) => entry.1.push(a(0)),
                    None => self.onexit.push((0, vec![a(0)])),
                }
                some(0)
            }
            "SetUnhandledExceptionFilter" if self.bits == 32 => some(0),
            "__std_terminate" | "terminate" => Err(Stop::fault("std::terminate was called")),
            "_CxxThrowException" => Err(Stop::fault("a C++ throw the lift did not port")),
            "RaiseException" => Err(Stop::fault(format!("RaiseException({:#x})", a(0)))),
            "WriteFile" | "WriteConsoleA" if self.is_std_output(a(0)) => {
                let bytes = vm.memory.read_vec(a(1), a(2))?;
                self.emit(a(0) == self.std_handles[2], &bytes);
                if a(3) != 0 {
                    vm.memory.write_u32(a(3), a(2) as u32)?;
                }
                some(1)
            }
            // A Linux or Android program's writes to its standard output and error.
            "write" | "_write" if matches!(a(0) as u32, 1 | 2) => {
                let bytes = vm.memory.read_vec(a(1), a(2))?;
                self.emit(a(0) as u32 == 2, &bytes);
                some(a(2))
            }
            "WriteConsoleW" if self.is_std_output(a(0)) => {
                let units = vm.memory.read_vec(a(1), a(2) * 2)?;
                let text: Vec<u16> = units.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                self.emit(a(0) == self.std_handles[2], String::from_utf16_lossy(&text).as_bytes());
                if a(3) != 0 {
                    vm.memory.write_u32(a(3), a(2) as u32)?;
                }
                some(1)
            }
            "CreateThread" => {
                let code = self.run_thread(vm, a(2), a(3))?;
                self.threads.push(Some(code));
                let handle = THREAD_HANDLES + 4 * self.threads.len() as u64;
                if a(5) != 0 {
                    vm.memory.write_u32(a(5), self.next_thread_id - 1)?;
                }
                some(handle)
            }
            "WaitForSingleObject" | "WaitForSingleObjectEx" if self.thread_index(a(0)).is_some() => some(0),
            "GetExitCodeThread" if self.thread_index(a(0)).is_some() => {
                let code = self.thread_index(a(0)).and_then(|i| self.threads[i]).unwrap_or(0);
                vm.memory.write_u32(a(1), code)?;
                some(1)
            }
            "CloseHandle" if self.thread_index(a(0)).is_some() => some(1),
            "GetProcessHeap" | "HeapCreate" => some(HEAP_HANDLE),
            "HeapDestroy" | "HeapValidate" | "HeapFree" | "HeapLock" | "HeapUnlock" | "LocalUnlock" | "GlobalUnlock" | "FreeLibrary" => {
                some(1)
            }
            "HeapAlloc" => some(self.allocate(vm, a(2), 16)?),
            "HeapReAlloc" => some(self.reallocate(vm, a(2), a(3))?),
            "HeapSize" | "_msize" | "LocalSize" | "GlobalSize" => {
                let p = if name == "HeapSize" { a(2) } else { a(0) };
                some(if p != 0 && self.in_heap(p) { vm.memory.read_u64(p - 16)? } else { 0 })
            }
            "malloc" | "LocalAlloc" | "GlobalAlloc" | "??2@YAPEAX_K@Z" | "??2@YAPAXI@Z" | "??_U@YAPEAX_K@Z" | "??_U@YAPAXI@Z" => {
                let size = if name == "malloc" || name.starts_with("??") { a(0) } else { a(1) };
                some(self.allocate(vm, size, 16)?)
            }
            "calloc" => some(self.allocate(vm, a(0).saturating_mul(a(1)), 16)?),
            "_aligned_malloc" => some(self.allocate(vm, a(0), a(1))?),
            "realloc" | "_recalloc" => some(self.reallocate(vm, a(0), if name == "realloc" { a(1) } else { a(1).saturating_mul(a(2)) })?),
            "LocalReAlloc" | "GlobalReAlloc" => some(self.reallocate(vm, a(0), a(1))?),
            "free" | "_aligned_free" | "LocalFree" | "GlobalFree" | "??3@YAXPEAX@Z" | "??3@YAXPAX@Z" | "??_V@YAXPEAX@Z"
            | "??_V@YAXPAX@Z" | "??3@YAXPEAX_K@Z" | "??3@YAXPAXI@Z" | "??_V@YAXPEAX_K@Z" | "??_V@YAXPAXI@Z" => some(0),
            "LocalLock" | "GlobalLock" => some(a(0)),
            "VirtualAlloc" if a(0) == 0 => some(self.allocate(vm, a(1), 4096)?),
            "VirtualFree" if self.in_heap(a(0)) => some(1),
            "GetModuleHandleA" | "GetModuleHandleW" if a(0) == 0 => some(self.image.image_base()),
            "GetModuleHandleA" | "GetModuleHandleW" | "LoadLibraryA" | "LoadLibraryW" | "LoadLibraryExA" | "LoadLibraryExW"
                if self.answers.is_some() =>
            {
                self.replayed(vm).map(Some)
            }
            "GetModuleHandleA" | "GetModuleHandleW" | "LoadLibraryA" | "LoadLibraryW" | "LoadLibraryExA" | "LoadLibraryExW" => {
                self.hosted += 1;
                let wide = name.ends_with('W');
                let text = if wide {
                    String::from_utf16_lossy(&vm.memory.read_wstr(a(0), 1 << 12)?)
                } else {
                    String::from_utf8_lossy(&vm.memory.read_cstr(a(0), 1 << 12)?).into_owned()
                };
                let Some(module) = os::library(&text) else { return some(0) };
                self.libraries.entry(module).or_insert(text);
                some(self.module_handle(module))
            }
            "GetModuleHandleExA" | "GetModuleHandleExW" if a(1) == 0 || a(0) & 4 != 0 => {
                if a(2) != 0 {
                    vm.memory.write_word(a(2), u64::from(self.bits / 8), self.image.image_base())?;
                }
                some(1)
            }
            // The function the stand-in found comes back bound as the host's does, so a call
            // through it is expected and answered like a direct one.
            "GetProcAddress" if self.answers.is_some() => {
                let found = self.replayed(vm)?.as_u64();
                if found == 0 {
                    return some(0);
                }
                if a(1) < 0x10000 {
                    return Err(Stop::fault(format!(
                        "GetProcAddress of the ordinal {} with no host: its binding names a host module the session does not hold",
                        a(1)
                    )));
                }
                let proc = String::from_utf8_lossy(&vm.memory.read_cstr(a(1), 1 << 12)?).into_owned();
                let bound = vm.bind_external(&proc)?;
                if bound != found {
                    return Err(Stop::fault(format!(
                        "GetProcAddress of {proc} answered {found:#x}, and {proc} is bound at {bound:#x} here"
                    )));
                }
                some(bound)
            }
            "GetProcAddress" => {
                self.hosted += 1;
                let module = a(0);
                let Some(host) = self.host_module(module) else { return some(0) };
                let proc = if a(1) < 0x10000 {
                    format!("#{}", a(1))
                } else {
                    String::from_utf8_lossy(&vm.memory.read_cstr(a(1), 1 << 12)?).into_owned()
                };
                let Some(f) = os::procedure(host, &proc) else { return some(0) };
                // The function comes back as an address of the interpreter's that names it, so a
                // call through it is intercepted like a direct one (and fits a dword).
                let symbol = if proc.starts_with('#') { format!("{proc}@{host:x}") } else { proc.clone() };
                self.procedures.insert(symbol.clone(), f);
                let address = vm.bind_external(&symbol)?;
                if let Some(dll) = self.libraries.get(&host) {
                    self.resolved.push(super::Resolved { address, dll: dll.clone(), name: proc });
                }
                some(address)
            }
            "GetCommandLineA" => some(self.command_line(vm, false)?),
            "GetCommandLineW" => some(self.command_line(vm, true)?),
            "GetModuleFileNameA" | "GetModuleFileNameW" if a(0) == 0 || a(0) == self.image.image_base() => {
                let path = self.args.first().cloned().unwrap_or_default();
                let bytes: Vec<u8> = if name.ends_with('W') {
                    path.encode_utf16().chain(std::iter::once(0)).flat_map(u16::to_le_bytes).collect()
                } else {
                    path.bytes().chain(std::iter::once(0)).collect()
                };
                let unit = if name.ends_with('W') { 2 } else { 1 };
                let room = (a(2) * unit) as usize;
                let n = bytes.len().min(room);
                vm.memory.write(a(1), &bytes[..n])?;
                some((n / unit as usize).saturating_sub(1) as u64)
            }
            "FindResourceA" | "FindResourceW" | "FindResourceExA" | "FindResourceExW" if self.is_image_module(a(0)) => {
                let (name_arg, type_arg) = if name.contains("Ex") { (a(2), a(1)) } else { (a(1), a(2)) };
                let wide = name.ends_with('W');
                let kind = Self::resource_id(vm, type_arg, wide)?;
                let wanted = Self::resource_id(vm, name_arg, wide)?;
                let found =
                    self.image.resources.iter().position(|r| resource_ids_match(&r.kind, &kind) && resource_ids_match(&r.name, &wanted));
                some(found.map_or(0, |i| RESOURCE_HANDLES + 16 * (i as u64 + 1)))
            }
            "LoadResource" if self.resource_index(a(1)).is_some() => {
                let index = self.resource_index(a(1)).unwrap_or(0);
                some(self.resource_copy(vm, index)?)
            }
            "SizeofResource" if self.resource_index(a(1)).is_some() => {
                some(self.resource_index(a(1)).map_or(0, |i| u64::from(self.image.resources[i].size)))
            }
            "LockResource" if self.resources.values().any(|&p| p == a(0)) => some(a(0)),
            "FreeResource" => some(0),
            "LoadStringA" | "LoadStringW" if self.is_image_module(a(0)) => {
                let wide = name.ends_with('W');
                let text =
                    ra_asm_pe::resources::string_table(self.image).into_iter().find(|(id, _)| u64::from(*id) == a(1)).map(|(_, s)| s);
                let Some(text) = text else { return some(0) };
                let units: Vec<u16> = text.encode_utf16().collect();
                let room = a(3) as usize;
                if room == 0 {
                    // The pointer form: the caller reads the string in place, which only the
                    // image's own table can give.
                    return some(0);
                }
                let n = units.len().min(room - 1);
                let bytes: Vec<u8> = if wide {
                    units[..n].iter().chain(std::iter::once(&0)).flat_map(|u| u.to_le_bytes()).collect()
                } else {
                    units[..n].iter().map(|&u| if u < 0x80 { u as u8 } else { b'?' }).chain(std::iter::once(0)).collect()
                };
                vm.memory.write(a(2), &bytes)?;
                some(n as u64)
            }
            "__acrt_iob_func" => some(STREAMS + a(0)),
            "printf" | "_printf_l" => {
                let text = self.format_slots(vm, a(0), &args[1..])?;
                self.emit(false, &text);
                some(text.len() as u64)
            }
            "vprintf" => {
                let text = self.format_list(vm, a(0), a(1), 8)?;
                self.emit(false, &text);
                some(text.len() as u64)
            }
            "puts" | "_cputs" => {
                let mut text = vm.memory.read_cstr(a(0), 1 << 24)?;
                if name == "puts" {
                    text.push(b'\n');
                }
                self.emit(false, &text);
                some(0)
            }
            "putchar" | "_putch" => {
                self.emit(false, &[a(0) as u8]);
                some(a(0) & 0xff)
            }
            "fputs" if self.std_stream(a(1)).is_some() => {
                let text = vm.memory.read_cstr(a(0), 1 << 24)?;
                self.write_stream(vm, a(1), &text)?;
                some(0)
            }
            "fputc" | "putc" if self.std_stream(a(1)).is_some() => {
                self.write_stream(vm, a(1), &[a(0) as u8])?;
                some(a(0) & 0xff)
            }
            "fwrite" if self.std_stream(a(3)).is_some() => {
                let bytes = vm.memory.read_vec(a(0), a(1).saturating_mul(a(2)))?;
                self.write_stream(vm, a(3), &bytes)?;
                some(a(2))
            }
            "fprintf" if self.std_stream(a(0)).is_some() => {
                let text = self.format_slots(vm, a(1), &args[2..])?;
                self.write_stream(vm, a(0), &text)?;
                some(text.len() as u64)
            }
            "vfprintf" if self.std_stream(a(0)).is_some() => {
                let text = self.format_list(vm, a(1), a(2), 8)?;
                self.write_stream(vm, a(0), &text)?;
                some(text.len() as u64)
            }
            "fflush" if a(0) == 0 || self.std_stream(a(0)).is_some() => some(0),
            "_write" if a(0) == 1 || a(0) == 2 => {
                let bytes = vm.memory.read_vec(a(1), a(2))?;
                self.emit(a(0) == 2, &bytes);
                some(a(2))
            }
            "__stdio_common_vfprintf" | "__stdio_common_vfprintf_s" if self.std_stream(a(1)).is_some() => {
                let text = self.format_list(vm, a(2), a(4), 8)?;
                self.write_stream(vm, a(1), &text)?;
                some(text.len() as u64)
            }
            "__stdio_common_vsprintf" | "__stdio_common_vsprintf_s" => {
                let text = self.format_list(vm, a(3), a(5), 8)?;
                Self::write_formatted(vm, a(1), a(2), &text).map(Some)
            }
            "__stdio_common_vsnprintf_s" => {
                let text = self.format_list(vm, a(4), a(6), 8)?;
                Self::write_formatted(vm, a(1), a(2).min(a(3).saturating_add(1)), &text).map(Some)
            }
            "sprintf" | "sprintf_s" | "_snprintf" | "snprintf" | "_snprintf_s" => {
                let (count, format, rest) = match name {
                    "sprintf" => (u64::MAX, a(1), 2),
                    "_snprintf_s" => (a(1).min(a(2).saturating_add(1)), a(3), 4),
                    _ => (a(1), a(2), 3),
                };
                let text = self.format_slots(vm, format, &args[rest.min(args.len())..])?;
                Self::write_formatted(vm, a(0), count, &text).map(Some)
            }
            "vsprintf" | "vsprintf_s" | "vsnprintf" | "_vsnprintf" | "_vsnprintf_s" => {
                let (count, format, list) = match name {
                    "vsprintf" => (u64::MAX, a(1), a(2)),
                    "_vsnprintf_s" => (a(1).min(a(2).saturating_add(1)), a(3), a(4)),
                    _ => (a(1), a(2), a(3)),
                };
                let text = self.format_list(vm, format, list, 8)?;
                Self::write_formatted(vm, a(0), count, &text).map(Some)
            }
            // The Xbox kernel: a program of the original Xbox has no host DLL to reach, so the
            // interpreter answers the exports a homebrew calls: the debug print (the program's
            // output), the pool and the contiguous memory (the heap), the memory routines, and
            // the return to the dashboard (the exit).
            "DbgPrint" => {
                let text = self.format_slots(vm, a(0), &args[1..])?;
                self.emit(false, &text);
                some(0)
            }
            "RtlSprintf" => {
                let text = self.format_slots(vm, a(1), &args[2..])?;
                Self::write_formatted(vm, a(0), u64::from(u32::MAX), &text).map(Some)
            }
            "RtlSnprintf" => {
                let text = self.format_slots(vm, a(2), &args[3..])?;
                Self::write_formatted(vm, a(0), a(1), &text).map(Some)
            }
            "HalReturnToFirmware" => Err(Stop::Exit(0)),
            "ExAllocatePool"
            | "ExAllocatePoolWithTag"
            | "MmAllocateContiguousMemory"
            | "MmAllocateContiguousMemoryEx"
            | "MmAllocateSystemMemory" => some(self.allocate(vm, a(0), 4096)?),
            "ExFreePool" | "MmFreeContiguousMemory" => some(0),
            "MmFreeSystemMemory" => some(a(1)),
            "RtlZeroMemory" | "RtlFillMemory" => {
                let fill = if name == "RtlFillMemory" { a(2) as u8 } else { 0 };
                vm.memory.write(a(0), &vec![fill; a(1) as usize])?;
                some(0)
            }
            "RtlMoveMemory" => {
                let bytes = vm.memory.read_vec(a(1), a(2))?;
                vm.memory.write(a(0), &bytes)?;
                some(0)
            }
            "RtlCompareMemory" => {
                let (x, y) = (vm.memory.read_vec(a(0), a(2))?, vm.memory.read_vec(a(1), a(2))?);
                some(x.iter().zip(&y).take_while(|(p, q)| p == q).count() as u64)
            }
            _ => Ok(None),
        }
    }

    fn is_std_output(&self, handle: u64) -> bool {
        handle != 0 && (handle == self.std_handles[1] || handle == self.std_handles[2])
    }

    /// Which standard stream a `FILE` is: 1 for output, 2 for error; the runtime's own or the
    /// host's.
    fn std_stream(&self, file: u64) -> Option<u8> {
        if file == STREAMS + 1 || (file != 0 && file == self.std_files[0]) {
            Some(1)
        } else if file == STREAMS + 2 || (file != 0 && file == self.std_files[1]) {
            Some(2)
        } else {
            None
        }
    }

    fn write_stream(&mut self, _vm: &mut Vm<'_>, file: u64, bytes: &[u8]) -> Result<Value, Stop> {
        match self.std_stream(file) {
            Some(which) => {
                self.emit(which == 2, bytes);
                int(bytes.len() as u64)
            }
            None => Err(Stop::fault(format!("a write to the stream {file:#x}, which is not a standard one"))),
        }
    }

    /// Writes formatted text into the program's buffer of `count` bytes, NUL-terminated, and
    /// returns the text's length (what the C functions return when it fits).
    fn write_formatted(vm: &mut Vm<'_>, buffer: u64, count: u64, text: &[u8]) -> Result<Value, Stop> {
        if buffer != 0 && count != 0 {
            let n = text.len().min((count - 1) as usize);
            vm.memory.write(buffer, &text[..n])?;
            vm.memory.write_u8(buffer + n as u64, 0)?;
            if (text.len() as u64) >= count {
                return int(u64::MAX);
            }
        }
        int(text.len() as u64)
    }

    /// The program's output: captured, and written through as it comes when asked.
    fn emit(&mut self, error: bool, bytes: &[u8]) {
        use std::io::Write;
        if error {
            let _ = std::io::stderr().write_all(bytes);
            return;
        }
        self.output.extend_from_slice(bytes);
        if self.live {
            let mut out = std::io::stdout().lock();
            let _ = out.write_all(bytes);
            let _ = out.flush();
        }
    }

    fn thread_index(&self, handle: u64) -> Option<usize> {
        let i = handle.checked_sub(THREAD_HANDLES)? / 4;
        (i >= 1 && i as usize <= self.threads.len()).then(|| i as usize - 1)
    }

    /// The handle a program holds for a host module: the host's own in 64 bits, a number of the
    /// runtime's in 32, where the host's does not fit.
    fn module_handle(&mut self, host: usize) -> u64 {
        if self.bits == 64 {
            return host as u64;
        }
        let i = self.modules.iter().position(|&m| m == host).unwrap_or_else(|| {
            self.modules.push(host);
            self.modules.len() - 1
        });
        MODULE_HANDLES + 0x1000 * i as u64
    }

    fn host_module(&self, handle: u64) -> Option<usize> {
        if handle == self.image.image_base() {
            return None;
        }
        if self.bits == 64 {
            return Some(handle as usize);
        }
        self.modules.get((handle.checked_sub(MODULE_HANDLES)? / 0x1000) as usize).copied()
    }

    /// The host function for `symbol`: resolved before, an import of the image by its DLL, or a
    /// function of the host's C runtime. An ELF image's C library (`libc.so.6`, bionic's `libc.so`)
    /// is no library of a Windows host: its functions are the host's C runtime's of the same name.
    fn procedure(&mut self, symbol: &str) -> Result<usize, Stop> {
        if let Some(&f) = self.procedures.get(symbol) {
            return Ok(f);
        }
        let runtime = |name: &str| RUNTIME_DLLS.iter().find_map(|dll| os::library(dll).and_then(|m| os::procedure(m, name)));
        let found = match self.imports.get(symbol) {
            Some((dll, name)) => {
                os::library(dll).and_then(|m| os::procedure(m, name)).or_else(|| self.image.elf.as_ref().and_then(|_| runtime(name)))
            }
            None => runtime(symbol),
        };
        match found {
            Some(f) => {
                self.procedures.insert(symbol.to_owned(), f);
                Ok(f)
            }
            None => Err(Stop::fault(format!("no binding for {symbol}: neither a function of the interpreter nor one of the host's DLLs"))),
        }
    }

    /// A call into the host: by the import's prototype when the table has one with floating-point
    /// arguments or result, else with the eight-integer adapter. With a stand-in for the host, the
    /// stand-in's answer instead.
    fn native_call(&mut self, vm: &mut Vm<'_>, symbol: &str, ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        if self.answers.is_some() {
            let answer = self.replayed(vm)?;
            // The standard handles are those the stand-in hands out, so a write to one is output.
            if symbol == "GetStdHandle"
                && let Some(which) = (arg(args, 0) as u32).wrapping_neg().checked_sub(10).filter(|&which| which < 3)
            {
                self.std_handles[which as usize] = answer.as_u64();
            }
            return Ok(answer);
        }
        // A host DLL writes through the pointers it is handed past the memory's log (a file's
        // `WriteFile` count, a `ReadFile` buffer): while the run keeps its boundary, what the
        // pointed-at memory holds is read before the call and again after, and what changed is
        // the call's writes, so a replay leaves the same bytes.
        let watched = if self.boundary.is_some() { watch(vm, args) } else { Vec::new() };
        let result = self.host_call(symbol, ty, args);
        if let Some(boundary) = &mut self.boundary {
            boundary.saw(changed(vm, &watched));
        }
        result
    }

    /// The call into the host's DLL itself, for [`Self::native_call`].
    fn host_call(&mut self, symbol: &str, ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        self.hosted += 1;
        let f = self.procedure(symbol)?;
        let typed = prototypes::typed(symbol);
        let floating = |k: &Kind| matches!(k, Kind::Float | Kind::Double);
        if let Some(t) = typed.as_ref().filter(|t| t.params.iter().any(floating) || t.returns.as_ref().is_some_and(floating)) {
            let letter = |k: &Kind| match k {
                Kind::Int | Kind::Wide => 'i',
                Kind::Float => 'f',
                Kind::Double => 'd',
            };
            let shape = format!("{}>{}", t.params.iter().map(letter).collect::<String>(), t.returns.as_ref().map_or('i', letter));
            let ints: Vec<u64> = t.params.iter().zip(args).filter(|(k, _)| !floating(k)).map(|(_, a)| a.as_u64()).collect();
            let floats: Vec<f64> = t.params.iter().zip(args).filter(|(k, _)| floating(k)).map(|(_, a)| a.as_f64()).collect();
            return match unsafe { os::call_shaped(f, &shape, &ints, &floats) } {
                Some(os::Floated::Int(v)) => int(v),
                Some(os::Floated::Float(v)) => Ok(Value::F32(v)),
                Some(os::Floated::Double(v)) => Ok(Value::F64(v)),
                None => Err(Stop::fault(format!("no way to call {symbol} with the shape {shape}"))),
            };
        }
        let mut ints = [0u64; 8];
        for (slot, a) in ints.iter_mut().zip(args) {
            *slot = a.as_u64();
        }
        let result = unsafe { os::call8(f, &ints) };
        let _ = ty;
        int(result)
    }

    // endregion

    // region Formatting

    /// `printf` through the integer slots of a call: the format at `format`, the arguments after.
    fn format_slots(&mut self, vm: &mut Vm<'_>, format: u64, slots: &[Value]) -> Result<Vec<u8>, Stop> {
        let spec = vm.memory.read_cstr(format, 1 << 20)?;
        let values: Vec<u64> = slots.iter().map(Value::as_u64).collect();
        let mut args = SlotArgs { values, at: 0, vm };
        Ok(printf::format(&spec, self.bits, &mut args))
    }

    /// `vprintf` through a `va_list` in the program's memory, with slots of `slot` bytes.
    fn format_list(&mut self, vm: &mut Vm<'_>, format: u64, list: u64, slot: u64) -> Result<Vec<u8>, Stop> {
        let spec = vm.memory.read_cstr(format, 1 << 20)?;
        let mut args = ListArgs { at: list, slot, vm };
        Ok(printf::format(&spec, self.bits, &mut args))
    }

    // endregion

    /// A call the program made, answered here: the stand-in for the host, when the run has one,
    /// hears of a call across the boundary first, and answers the host's part of it.
    fn answer_call(&mut self, vm: &mut Vm<'_>, name: &str, ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        let event = match &self.answers {
            Some(answers) if crosses(name) => Some(answers.expect(name, args)?),
            _ => None,
        };
        let outer = std::mem::replace(&mut self.event, event);
        let result = if name.starts_with("ra_") { self.runtime_call(vm, name, args) } else { self.import_call(vm, name, ty, args) };
        self.event = outer;
        result
    }
}

impl Host for Runtime<'_> {
    fn call(&mut self, vm: &mut Vm<'_>, name: &str, ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        let kept = crosses(name) && self.boundary.is_some();
        if kept && let Some(boundary) = &mut self.boundary {
            boundary.begin(vm, name, args);
        }
        let result = self.answer_call(vm, name, ty, args);
        if kept && let Some(boundary) = &mut self.boundary {
            boundary.end(vm, &result);
        }
        result
    }

    fn poll(&mut self, vm: &mut Vm<'_>) -> Result<(), Stop> {
        self.console_poll(vm)
    }

    fn boundary(&mut self, vm: &mut Vm<'_>, from: Option<u64>, to: u64, start: bool, state: Option<u64>) -> Result<(), Stop> {
        self.psx_boundary(vm, from, to, start, state)
    }
}

/// The arguments of a call as its integer slots.
struct SlotArgs<'v, 'm> {
    values: Vec<u64>,
    at: usize,
    vm: &'v mut Vm<'m>,
}

impl printf::Args for SlotArgs<'_, '_> {
    fn int(&mut self, _wide: bool) -> u64 {
        let v = self.values.get(self.at).copied().unwrap_or(0);
        self.at += 1;
        v
    }

    fn double(&mut self) -> f64 {
        f64::from_bits(self.int(true))
    }

    fn string(&mut self, address: u64) -> Vec<u8> {
        self.vm.memory.read_cstr(address, 1 << 24).unwrap_or_default()
    }

    fn wstring(&mut self, address: u64) -> Vec<u16> {
        self.vm.memory.read_wstr(address, 1 << 24).unwrap_or_default()
    }
}

/// The arguments of a call as a `va_list`: slots of four bytes (x86, a 64-bit value in two) or
/// eight (x64).
struct ListArgs<'v, 'm> {
    at: u64,
    slot: u64,
    vm: &'v mut Vm<'m>,
}

impl printf::Args for ListArgs<'_, '_> {
    fn int(&mut self, wide: bool) -> u64 {
        let width = if wide { 8 } else { self.slot };
        let v = self.vm.memory.read_word(self.at, width).unwrap_or(0);
        self.at += width;
        v
    }

    fn double(&mut self) -> f64 {
        f64::from_bits(self.int(true))
    }

    fn string(&mut self, address: u64) -> Vec<u8> {
        self.vm.memory.read_cstr(address, 1 << 24).unwrap_or_default()
    }

    fn wstring(&mut self, address: u64) -> Vec<u16> {
        self.vm.memory.read_wstr(address, 1 << 24).unwrap_or_default()
    }
}

// region The floating-point environment

// The C runtime's control word: the exception masks, the rounding mode and the precision control.
const MCW_EM: u32 = 0x8001f;
const MCW_RC: u32 = 0x300;
const MCW_PC: u32 = 0x30000;

/// The exception bits of the C runtime's word (`_EM_INEXACT` 1, `_EM_UNDERFLOW` 2, `_EM_OVERFLOW`
/// 4, `_EM_ZERODIVIDE` 8, `_EM_INVALID` 0x10, `_EM_DENORMAL` 0x80000) from the x87's order
/// (invalid, denormal, zero divide, overflow, underflow, precision in bits 0 to 5).
fn crt_flags(x87: u32) -> u32 {
    ((x87 >> 5) & 1) | ((x87 >> 3) & 2) | ((x87 >> 1) & 4) | ((x87 << 1) & 8) | ((x87 << 4) & 0x10) | if x87 & 2 != 0 { 0x80000 } else { 0 }
}

fn x87_flags(crt: u32) -> u32 {
    ((crt >> 4) & 1) | ((crt >> 18) & 2) | ((crt >> 1) & 4) | ((crt << 1) & 8) | ((crt << 3) & 0x10) | ((crt << 5) & 0x20)
}

/// The C runtime's control word from the x87's: the masks, the rounding mode (bits 10 and 11 to
/// bits 8 and 9) and the precision (24 bits is `_PC_24` 0x20000, 53 `_PC_53` 0x10000, 64 `_PC_64` 0).
fn crt_control(cw: u32) -> u32 {
    let pc = match (cw >> 8) & 3 {
        0 => 0x20000,
        2 => 0x10000,
        _ => 0,
    };
    crt_flags(cw & 0x3f) | ((cw >> 2) & MCW_RC) | pc
}

fn x87_control(crt: u32, cw: u16) -> u16 {
    let pc = match crt & MCW_PC {
        0x20000 => 0,
        0x10000 => 2,
        _ => 3,
    };
    (u32::from(cw) & 0xf0c0 | x87_flags(crt & MCW_EM) | ((crt & MCW_RC) << 2) | (pc << 8)) as u16
}

// endregion

// region The processor

#[cfg(target_arch = "x86_64")]
fn cpuid(leaf: u32, subleaf: u32) -> [u32; 4] {
    let r = std::arch::x86_64::__cpuid_count(leaf, subleaf);
    [r.eax, r.ebx, r.ecx, r.edx]
}

#[cfg(not(target_arch = "x86_64"))]
fn cpuid(_leaf: u32, _subleaf: u32) -> [u32; 4] {
    [0; 4]
}

/// The enabled state components: the host's answer, or x87, SSE and AVX elsewhere.
#[cfg(target_arch = "x86_64")]
fn xgetbv(which: u32) -> u64 {
    let (low, high): (u32, u32);
    unsafe { std::arch::asm!("xgetbv", in("ecx") which, out("eax") low, out("edx") high, options(nomem, nostack, preserves_flags)) };
    (u64::from(high) << 32) | u64::from(low)
}

#[cfg(not(target_arch = "x86_64"))]
fn xgetbv(_which: u32) -> u64 {
    7
}

#[cfg(target_arch = "x86_64")]
fn rdtsc() -> u64 {
    unsafe { std::arch::x86_64::_rdtsc() }
}

#[cfg(not(target_arch = "x86_64"))]
fn rdtsc() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos() as u64)
}

// endregion

// region Parsing

/// `strtol`'s reading of `text` in `base` (0 for the prefix to say): the value and the bytes used.
fn parse_integer(text: &[u8], base: u32) -> (i128, usize) {
    let mut i = 0;
    while text.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    let negative = match text.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let mut base = base;
    if (base == 0 || base == 16)
        && text.get(i) == Some(&b'0')
        && matches!(text.get(i + 1), Some(b'x' | b'X'))
        && text.get(i + 2).is_some_and(u8::is_ascii_hexdigit)
    {
        i += 2;
        base = 16;
    } else if base == 0 {
        base = if text.get(i) == Some(&b'0') { 8 } else { 10 };
    }
    let start = i;
    let mut value: i128 = 0;
    while let Some(d) = text.get(i).and_then(|c| (*c as char).to_digit(base)) {
        value = value.saturating_mul(i128::from(base)).saturating_add(i128::from(d));
        i += 1;
    }
    if i == start {
        return (0, 0);
    }
    (if negative { -value } else { value }, i)
}

/// `strtod`'s reading of `text`: the value and the bytes used.
fn parse_double(text: &[u8]) -> (f64, usize) {
    let mut i = 0;
    while text.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    let start = i;
    if matches!(text.get(i), Some(b'+' | b'-')) {
        i += 1;
    }
    while text.get(i).is_some_and(u8::is_ascii_digit) {
        i += 1;
    }
    if text.get(i) == Some(&b'.') {
        i += 1;
        while text.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
    }
    if matches!(text.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(text.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        if text.get(j).is_some_and(u8::is_ascii_digit) {
            while text.get(j).is_some_and(u8::is_ascii_digit) {
                j += 1;
            }
            i = j;
        }
    }
    let used = &text[start..i];
    match std::str::from_utf8(used).ok().and_then(|s| s.parse::<f64>().ok()) {
        Some(v) => (v, i),
        None => (0.0, 0),
    }
}

// endregion
