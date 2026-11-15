//! The engine over `computed64.exe`, its image's bytes and a stack as the program's memory:
//! `resolve` run from its recorded vector, `h_blend` (a function no analysis finds, which the
//! program reaches by an address it computes) run from its bytes, the cache over unchanged and
//! changed bytes, and the calls of `apply` handed back.

use std::path::Path;

use ra_asm_lift::Isa;
use ra_asm_residual::{Call, Calls, Engine, Machine, Reader, State, Stop};
use ra_asm_verify::Vector;

const RESOLVE: u64 = 0x1_4000_1180;
const APPLY: u64 = 0x1_4000_1160;
/// `h_blend(a, b)`, `a * 3 + b`: `lea eax, [rcx + rcx*2]`, `add eax, edx`, `ret`.
const H_BLEND: u64 = 0x1_4000_108c;
/// The `add eax, edx` of `h_blend`.
const H_BLEND_ADD: u64 = 0x1_4000_108f;

/// The stack of the recorded run, and where its stack pointer stood at `resolve`'s entry.
const STACK: u64 = 0x1200_0000;
const STACK_LEN: usize = 0x10_0000;
const SP: u64 = 0x120f_ff48;
const RETURN_ADDRESS: u64 = 0x1_4000_1170;

const GENERAL: [&str; 16] = ["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"];
const FLAGS: [&str; 7] = ["cf", "pf", "af", "zf", "sf", "df", "of"];

/// The program's memory: runs of bytes at their addresses.
struct Space(Vec<(u64, Vec<u8>)>);

impl Reader for Space {
    fn read(&self, address: u64, buf: &mut [u8]) -> bool {
        self.0.iter().any(|(start, bytes)| {
            let Some(offset) = address.checked_sub(*start) else { return false };
            let Some(held) = bytes.get(offset as usize..offset as usize + buf.len()) else { return false };
            buf.copy_from_slice(held);
            true
        })
    }
}

impl Space {
    fn write(&mut self, address: u64, data: &[u8]) {
        let (start, bytes) =
            self.0.iter_mut().find(|(start, bytes)| address >= *start && address - *start < bytes.len() as u64).expect("a mapped address");
        let offset = (address - *start) as usize;
        bytes[offset..offset + data.len()].copy_from_slice(data);
    }
}

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/computed64.exe")
}

/// The engine of the fixture, and its memory: the image as mapped, and a stack whose top holds
/// the return address `resolve` was called with.
fn setup() -> (Engine, Space) {
    let image = ra_asm_pe::load(&fixture()).expect("the fixture loads");
    let mapped = (image.image_base(), image.mapped().to_vec());
    let mut space = Space(vec![mapped, (STACK, vec![0; STACK_LEN])]);
    space.write(SP, &RETURN_ADDRESS.to_le_bytes());
    (Engine::new(image), space)
}

fn vector(function: &str) -> Vector {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/vectors/fn/computed64.fv");
    let text = std::fs::read_to_string(&path).expect("the vectors of the fixture");
    ra_asm_verify::read(&text).expect("the vectors read").into_iter().find(|v| v.function == function).expect("a vector of the function")
}

fn set(state: &mut State, name: &str, value: u128) {
    if let Some(n) = GENERAL.iter().position(|r| *r == name) {
        state.set_general(n, value as u64);
    } else if let Some(n) = FLAGS.iter().position(|f| *f == name) {
        state.set_flag(n, value as u8);
    } else if name == "rip" {
        state.set_counter(value as u64);
    } else if let Some(n) = name.strip_prefix("xmm").and_then(|n| n.parse::<usize>().ok()) {
        state.set_vector(n, value);
    } else if let Some(n) = name.strip_prefix("ymm").and_then(|n| n.strip_suffix('h')).and_then(|n| n.parse::<usize>().ok()) {
        state.set_vector(16 + n, value);
    } else {
        panic!("no register {name} in an x64 state");
    }
}

fn get(state: &State, name: &str) -> u128 {
    if let Some(n) = GENERAL.iter().position(|r| *r == name) {
        u128::from(state.general(n))
    } else if let Some(n) = FLAGS.iter().position(|f| *f == name) {
        u128::from(state.flag(n))
    } else if name == "rip" {
        u128::from(state.counter())
    } else if let Some(n) = name.strip_prefix("xmm").and_then(|n| n.parse::<usize>().ok()) {
        state.vector(n)
    } else if let Some(n) = name.strip_prefix("ymm").and_then(|n| n.strip_suffix('h')).and_then(|n| n.parse::<usize>().ok()) {
        state.vector(16 + n)
    } else {
        panic!("no register {name} in an x64 state");
    }
}

/// The answers of a vector's calls, in its order: an import's result is the `rax` it recorded.
struct Recorded<'a> {
    vector: &'a Vector,
    next: usize,
}

impl Calls for Recorded<'_> {
    fn call(&mut self, call: Call<'_>, _: &mut Machine<'_, '_>) -> Result<u64, Stop> {
        let recorded = self.vector.calls.get(self.next).ok_or_else(|| Stop::fault(format!("{call:?}, which the vector does not hold")))?;
        self.next += 1;
        match call {
            Call::Import { name, .. } if recorded.callee == format!("imp_{name}") => {
                let rax = recorded.results.iter().find(|(register, _)| register == "rax").map_or(0, |(_, value)| *value);
                Ok(rax as u64)
            }
            other => Err(Stop::fault(format!("{other:?} where the vector holds a call of {}", recorded.callee))),
        }
    }
}

/// A function that makes no call.
struct NoCalls;

impl Calls for NoCalls {
    fn call(&mut self, call: Call<'_>, _: &mut Machine<'_, '_>) -> Result<u64, Stop> {
        Err(Stop::fault(format!("{call:?} from a leaf")))
    }
}

/// The state of a call of `h_blend(a, b)` with the stack of the recorded run.
fn blend(a: u64, b: u64) -> State {
    let mut state = State::new(Isa::X86);
    state.set_general(1, a);
    state.set_general(2, b);
    state.set_general(4, SP);
    state
}

#[test]
fn resolve_returns_what_its_vector_says() {
    let vector = vector("resolve");
    assert_eq!(vector.address, RESOLVE);
    let (mut engine, mut space) = setup();
    for (address, bytes) in &vector.memory_in {
        space.write(*address, bytes);
    }
    let mut state = State::new(Isa::X86);
    for (name, value) in &vector.entry {
        set(&mut state, name, *value);
    }
    let mut calls = Recorded { vector: &vector, next: 0 };
    let outcome = engine.run(&space, &state, RESOLVE, &mut calls).expect("resolve lifts");
    assert_eq!(outcome.stop, None, "resolve returns");
    assert_eq!(calls.next, vector.calls.len(), "every call of the vector is made");
    for (name, recorded) in &vector.exit {
        assert_eq!(get(&outcome.state, name), *recorded, "{name} at the exit");
    }
    assert_eq!(outcome.written, vector.memory_out, "the bytes resolve writes");
    assert!(!outcome.cached);
}

#[test]
fn a_function_no_analysis_finds_runs_from_the_images_bytes() {
    let (mut engine, space) = setup();
    let outcome = engine.run(&space, &blend(40, 2), H_BLEND, &mut NoCalls).expect("h_blend lifts");
    assert_eq!(outcome.stop, None, "h_blend returns");
    assert_eq!(outcome.state.general(0) & 0xffff_ffff, 122, "h_blend(40, 2)");
    assert_eq!(outcome.state.stack_pointer(), SP + 8, "the return pops its address");
}

#[test]
fn the_second_call_hits_the_cache() {
    let (mut engine, space) = setup();
    let first = engine.run(&space, &blend(40, 2), H_BLEND, &mut NoCalls).expect("h_blend lifts");
    let second = engine.run(&space, &blend(5, 6), H_BLEND, &mut NoCalls).expect("h_blend runs again");
    assert!(!first.cached && second.cached);
    assert_eq!((engine.misses(), engine.hits()), (1, 1));
    assert_eq!(second.stop, None);
    assert_eq!(second.state.general(0) & 0xffff_ffff, 21, "h_blend(5, 6)");
}

#[test]
fn changed_bytes_miss_the_cache() {
    let (mut engine, mut space) = setup();
    let before = engine.run(&space, &blend(40, 2), H_BLEND, &mut NoCalls).expect("h_blend lifts");
    assert_eq!(before.state.general(0) & 0xffff_ffff, 122);
    // `add eax, edx` becomes `sub eax, edx`.
    space.write(H_BLEND_ADD, &[0x29]);
    let after = engine.run(&space, &blend(40, 2), H_BLEND, &mut NoCalls).expect("the changed h_blend lifts");
    assert!(!after.cached, "changed bytes are lifted anew");
    assert_eq!(after.stop, None);
    assert_eq!(after.state.general(0) & 0xffff_ffff, 118, "the changed code runs");
    assert_eq!((engine.misses(), engine.hits()), (2, 0));
    // The bytes of the first lift are back: its lift still stands for them.
    space.write(H_BLEND_ADD, &[0x01]);
    let again = engine.run(&space, &blend(40, 2), H_BLEND, &mut NoCalls).expect("h_blend runs again");
    assert!(again.cached);
    assert_eq!(again.state.general(0) & 0xffff_ffff, 122);
}

/// `apply`'s callees, answered as they run: `resolve(0)` gives `h_blend`, and `h_blend` blends.
#[derive(Default)]
struct Callees(Vec<u64>);

impl Calls for Callees {
    fn call(&mut self, call: Call<'_>, machine: &mut Machine<'_, '_>) -> Result<u64, Stop> {
        let Call::Address(address) = call else { return Err(Stop::fault(format!("{call:?} from apply"))) };
        self.0.push(address);
        let mut state = machine.state()?;
        let result = match address {
            RESOLVE => H_BLEND,
            H_BLEND => u64::from((state.general(1) as u32).wrapping_mul(3).wrapping_add(state.general(2) as u32)),
            other => return Err(Stop::fault(format!("a call of {other:#x} from apply"))),
        };
        state.set_general(0, result);
        // The callee's `ret`.
        state.set_general(4, state.general(4) + 8);
        machine.set_state(&state)?;
        Ok(0)
    }
}

#[test]
fn the_calls_of_a_function_are_handed_back() {
    let (mut engine, space) = setup();
    let mut state = State::new(Isa::X86);
    state.set_general(1, 0);
    state.set_general(2, 40);
    state.set_general(8, 2);
    state.set_general(4, SP);
    let mut callees = Callees::default();
    let outcome = engine.run(&space, &state, APPLY, &mut callees).expect("apply lifts");
    assert_eq!(callees.0, [RESOLVE, H_BLEND], "the direct call, then the computed one");
    assert_eq!(outcome.stop, None, "apply returns");
    assert_eq!(outcome.state.general(0) & 0xffff_ffff, 122, "apply(0, 40, 2)");
}
