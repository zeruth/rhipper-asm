//! A console program's lift as a program of this machine: the machine-level module made ready for
//! a compiler, and the C source that ties what the compiler makes of it to the run.
//!
//! The compiled functions are the module's own, `void f(ptr state)` each, and run in the same
//! run the interpreter makes ([`ra_asm_ir::interp::native`]): the console's machine, its
//! exceptions and the processor for the code no function has are the run's, whichever of the
//! functions are compiled. What a compiled function cannot do itself it calls for:
//!
//! - an access to the program's memory, `ra_ld<bits>(address)` and `ra_st<bits>(address, value)`:
//!   the console's address space has its RAM through three segments and its devices' registers
//!   among them, which no pointer of this machine reaches;
//! - the look at the machine where a block of the program starts, `ra_tick(state, address,
//!   instructions)`, which counts the block's instructions for the console's clock and is
//!   where an interrupt is taken, and `ra_step(instructions)` for the instructions of a block
//!   the lift split off another;
//! - the runtime's functions the lift names (`ra_call`, `ra_psx_exception`, the GTE's
//!   commands), each through one entry of the run, by its number.
//!
//! [`nativize`] rewrites the module so, and [`source`] writes the C beside it: the calls above,
//! the table of the functions by their addresses, the ROM's bytes and the `main` that hands
//! them all to the run.

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write;

use ra_asm_ir::{CallConv, Callee, Cast, Declaration, FnType, Function, Inst, Instruction, Module, Operand, Type, ValueId};

use crate::Lifted;

/// The look at the machine where a block starts: `void ra_tick(ptr state, i64 address, i64 n)`.
pub const TICK: &str = "ra_tick";
/// The instructions of a block that is no start of one: `void ra_step(i64 n)`.
pub const STEP: &str = "ra_step";
/// The globals of the module the run reads and writes too: the compiled program's own copies
/// are the ones the run uses.
pub const SHARED_GLOBALS: [&str; 3] = [crate::state::N64_COP0, crate::state::PSX_GP, crate::state::PSX_GTE];

/// The name of the load or the store of `bits` bits.
fn accessor(store: bool, bits: u16) -> String {
    format!("ra_{}{bits}", if store { "st" } else { "ld" })
}

/// The width of a type an accessor moves.
fn bits_of(ty: &Type) -> Option<u16> {
    match ty {
        Type::Int(bits @ (8 | 16 | 32 | 64)) => Some(*bits),
        _ => None,
    }
}

/// Rewrites the machine-level `module` for a compiler: every access to the program's memory a
/// call of its accessor, and a look at the machine where each block of the program starts.
/// The functions keep their names, their types and what they compute.
pub fn nativize(module: &mut Module) {
    for bits in [8u16, 16, 32, 64] {
        let ty = Type::Int(bits);
        module.declare(Declaration { name: accessor(false, bits), ty: FnType::new(ty.clone(), vec![Type::I64]), conv: CallConv::C, attrs: vec![] });
        module.declare(Declaration { name: accessor(true, bits), ty: FnType::new(Type::Void, vec![Type::I64, ty]), conv: CallConv::C, attrs: vec![] });
    }
    module.declare(Declaration { name: TICK.into(), ty: tick_type(), conv: CallConv::C, attrs: vec![] });
    module.declare(Declaration { name: STEP.into(), ty: step_type(), conv: CallConv::C, attrs: vec![] });
    for function in &mut module.functions {
        accesses(function);
        ticks(function);
    }
}

fn tick_type() -> FnType {
    FnType::new(Type::Void, vec![Type::Ptr, Type::I64, Type::I64])
}

fn step_type() -> FnType {
    FnType::new(Type::Void, vec![Type::I64])
}

/// Makes each load and store through an address of the program (a pointer made of an
/// integer) a call of its accessor. An access to the state or to a global of the module goes
/// through a pointer of this machine and stays as it is.
fn accesses(function: &mut Function) {
    let addresses: HashMap<ValueId, (Type, Operand)> = function
        .blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .filter_map(|i| match (&i.inst, i.result) {
            (Inst::Cast { cast: Cast::IntToPtr, from, value, .. }, Some(result)) => Some((result, (from.clone(), value.clone()))),
            _ => None,
        })
        .collect();
    if addresses.is_empty() {
        return;
    }
    for block in 0..function.blocks.len() {
        let mut at = 0;
        while at < function.blocks[block].instrs.len() {
            let instruction = &function.blocks[block].instrs[at];
            let (store, ty, pointer) = match &instruction.inst {
                Inst::Load { ty, ptr, .. } => (None, ty.clone(), ptr.local()),
                Inst::Store { ty, value, ptr, .. } => (Some(value.clone()), ty.clone(), ptr.local()),
                _ => (None, Type::Void, None),
            };
            let (Some(bits), Some((from, address))) = (bits_of(&ty), pointer.and_then(|p| addresses.get(&p)).cloned()) else {
                at += 1;
                continue;
            };
            let site = instruction.address;
            // The address at 64 bits, as the accessor takes it: a narrower one zero-extended.
            let address = if from == Type::I64 {
                address
            } else {
                let wide = function.new_value("address", Type::I64);
                let inst = Inst::Cast { cast: Cast::ZExt, from, value: address, to: Type::I64 };
                function.blocks[block].instrs.insert(at, Instruction { result: Some(wide), inst, address: site });
                at += 1;
                Operand::Local(wide)
            };
            let (fn_ty, args) = match store {
                Some(value) => (FnType::new(Type::Void, vec![Type::I64, ty.clone()]), vec![(Type::I64, address), (ty, value)]),
                None => (FnType::new(ty, vec![Type::I64]), vec![(Type::I64, address)]),
            };
            let callee = Callee::Direct(accessor(args.len() == 2, bits));
            function.blocks[block].instrs[at].inst = Inst::Call { fn_ty, callee, args, tail: false };
            at += 1;
        }
    }
}

/// Puts the look at the machine at the start of each block of the program, after its phis,
/// with the count of the program's instructions the block holds; a block the lift split off
/// another counts its own.
fn ticks(function: &mut Function) {
    if function.ty.params != [Type::Ptr] {
        return;
    }
    let Some(&state) = function.params.first() else { return };
    for block in &mut function.blocks {
        let instructions = block.instrs.iter().filter_map(|i| i.address).collect::<BTreeSet<u64>>().len() as i64;
        if instructions == 0 && block.address.is_none() {
            continue;
        }
        let first = block.instrs.iter().position(|i| !matches!(i.inst, Inst::Phi { .. })).unwrap_or(block.instrs.len());
        let inst = match block.address {
            Some(address) => Inst::Call {
                fn_ty: tick_type(),
                callee: Callee::Direct(TICK.into()),
                args: vec![(Type::Ptr, Operand::Local(state)), (Type::I64, (address as i64).into()), (Type::I64, instructions.into())],
                tail: false,
            },
            None => Inst::Call {
                fn_ty: step_type(),
                callee: Callee::Direct(STEP.into()),
                args: vec![(Type::I64, instructions.into())],
                tail: false,
            },
        };
        block.instrs.insert(first, Instruction { result: None, inst, address: block.address });
    }
}

/// The C source beside the compiled module of `lifted`: the program's calls on the run, the
/// table of its functions by their addresses, the image's bytes (`image.bin`, embedded) and the
/// `main` that gives them to the run. `module` is the module [`nativize`] made.
pub fn source(lifted: &Lifted) -> String {
    let module = &lifted.module;
    let mut out = String::new();
    let _ = writeln!(out, "// Generated by ras port. What ties the compiled program to its run: build with");
    let _ = writeln!(out, "//   clang -O2 -std=c23 program.ll port.c ra_asm_port.dll.lib -o <name>.exe");
    out.push_str(PRELUDE);
    // The runtime's functions the module names, each through the run's one entry by its number.
    let defined: BTreeSet<&str> = module.functions.iter().map(|f| f.name.as_str()).collect();
    let own = |name: &str| name == TICK || name == STEP || name.starts_with("ra_ld") || name.starts_with("ra_st");
    let imports: Vec<&Declaration> =
        module.declarations.iter().filter(|d| !d.name.starts_with("llvm.") && !own(&d.name) && !defined.contains(d.name.as_str())).collect();
    for (index, declaration) in imports.iter().enumerate() {
        let _ = writeln!(
            out,
            "uint64_t {}(uint64_t a0, uint64_t a1, uint64_t a2, uint64_t a3, uint64_t a4, uint64_t a5) {{ return ra_host.call({index}, a0, a1, a2, a3, a4, a5); }}",
            declaration.name
        );
    }
    let _ = writeln!(out, "static const char *const ra_imports[] = {{");
    for declaration in &imports {
        let _ = writeln!(out, "    \"{}\",", declaration.name);
    }
    let _ = writeln!(out, "    0\n}};");
    // The functions, by the addresses the program has them at.
    let by_name: HashMap<&str, u64> = lifted.functions.iter().map(|(address, name)| (name.as_str(), *address)).collect();
    let functions: Vec<(u64, &str)> = module.functions.iter().filter_map(|f| by_name.get(f.name.as_str()).map(|address| (*address, f.name.as_str()))).collect();
    // A function's name in the module may be none of C's: each is declared by its address,
    // under the symbol the module gives it.
    for (address, name) in &functions {
        let _ = writeln!(out, "void ra_f_{address:x}(void *state) __asm__(\"{name}\");");
    }
    let _ = writeln!(out, "static const ra_function ra_functions[] = {{");
    for (address, _) in &functions {
        let _ = writeln!(out, "    {{ {address:#x}ull, ra_f_{address:x} }},");
    }
    let _ = writeln!(out, "    {{ 0, 0 }}\n}};");
    // The globals the run shares with the program.
    let shared: Vec<&str> = SHARED_GLOBALS.into_iter().filter(|name| module.globals.iter().any(|g| g.name == *name)).collect();
    for name in &shared {
        let _ = writeln!(out, "extern unsigned char {name}[];");
    }
    let _ = writeln!(out, "static const ra_global ra_globals[] = {{");
    for name in &shared {
        let _ = writeln!(out, "    {{ \"{name}\", {name} }},");
    }
    let _ = writeln!(out, "    {{ 0, 0 }}\n}};");
    out.push_str(MAIN);
    out
}

/// The types of the tables, the run's entries and the accessors.
const PRELUDE: &str = r#"#include <stdint.h>

typedef struct ra_function { uint64_t address; void (*code)(void *state); } ra_function;
typedef struct ra_global { const char *name; void *at; } ra_global;
// The run's entries, which the run fills in before it calls the program.
typedef struct ra_host {
    uint64_t (*load)(uint64_t address, uint64_t bytes);
    void (*store)(uint64_t address, uint64_t bytes, uint64_t value);
    void (*tick)(void *state, uint64_t address, uint64_t instructions);
    uint64_t (*call)(uint64_t import, uint64_t a0, uint64_t a1, uint64_t a2, uint64_t a3, uint64_t a4, uint64_t a5);
    // The instructions the program ran since the run last counted them, and how many of them
    // the run lets go by before it looks at its machine again.
    uint64_t steps;
    uint64_t budget;
    // The run has something for the program where its next block starts: an interrupt.
    uint64_t boundary;
    // The loads and the stores the program made in its RAM itself, since the run last counted.
    uint64_t loads;
    uint64_t stores;
    // The console's RAM, which the program reads and writes itself: where it is on this
    // machine, the segments of the address space that reach it (a bit each, by the address's
    // top three bits), what of an address is its place there, the places the RAM answers at
    // and what of a place is the byte's offset, the RAM repeating over them. No RAM while the
    // limit is zero.
    uint64_t ram;
    uint64_t segments;
    uint64_t fold;
    uint64_t ram_limit;
    uint64_t ram_wrap;
    // The ranges of the RAM a store must go through the run for, each an offset and a length:
    // code the program copied there, which the run has compiled functions for while it stays.
    uint64_t watched[4][2];
} ra_host_t;
typedef struct ra_port {
    const unsigned char *image;
    uint64_t image_size;
    const ra_function *functions;
    const char *const *imports;
    const ra_global *globals;
    ra_host_t *host;
} ra_port;
__declspec(dllimport) int ra_port_main(const ra_port *port, int argc, char **argv);

static ra_host_t ra_host;

// Where the `bytes` at `address` are when they are RAM the program reads itself: an aligned
// access that lies in the RAM through a segment that reaches it. Null for any other, which the
// run answers.
static inline unsigned char *ra_ram(uint64_t address, uint64_t bytes) {
    uint32_t a = (uint32_t)address;
    uint64_t place = a & ra_host.fold;
    if (!((ra_host.segments >> (a >> 29)) & 1) || place >= ra_host.ram_limit || (a & (bytes - 1))) return 0;
    return (unsigned char *)(uintptr_t)(ra_host.ram + (place & ra_host.ram_wrap));
}

// The same for a store, which the run also takes when it lands in a range it watches.
static inline unsigned char *ra_ram_store(uint64_t address, uint64_t bytes) {
    unsigned char *at = ra_ram(address, bytes);
    if (!at) return 0;
    uint64_t offset = (uint64_t)(uintptr_t)at - ra_host.ram;
    for (int n = 0; n < 4; n++)
        if (offset - ra_host.watched[n][0] < ra_host.watched[n][1]) return 0;
    return at;
}

#define RA_LOAD(bits, bytes) \
    uint##bits##_t ra_ld##bits(uint64_t address) { \
        unsigned char *at = ra_ram(address, bytes); \
        if (!at) return (uint##bits##_t)ra_host.load(address, bytes); \
        uint##bits##_t value; \
        __builtin_memcpy(&value, at, bytes); \
        ra_host.loads++; \
        return value; \
    }
#define RA_STORE(bits, bytes) \
    void ra_st##bits(uint64_t address, uint##bits##_t value) { \
        unsigned char *at = ra_ram_store(address, bytes); \
        if (!at) { ra_host.store(address, bytes, value); return; } \
        __builtin_memcpy(at, &value, bytes); \
        ra_host.stores++; \
    }
RA_LOAD(8, 1) RA_LOAD(16, 2) RA_LOAD(32, 4) RA_LOAD(64, 8)
RA_STORE(8, 1) RA_STORE(16, 2) RA_STORE(32, 4) RA_STORE(64, 8)

// Where a block of the program starts: its instructions count, and the run looks at its machine
// when that many have gone by or it has something for the program.
void ra_tick(void *state, uint64_t address, uint64_t instructions) {
    ra_host.steps += instructions;
    if (ra_host.steps >= ra_host.budget || ra_host.boundary) ra_host.tick(state, address, 0);
}
void ra_step(uint64_t instructions) { ra_host.steps += instructions; }

"#;

/// The image's bytes and the entry.
const MAIN: &str = r#"
static const unsigned char ra_image[] = {
#embed "image.bin"
};

int main(int argc, char **argv) {
    const ra_port port = { ra_image, sizeof ra_image, ra_functions, ra_imports, ra_globals, &ra_host };
    return ra_port_main(&port, argc, argv);
}
"#;
