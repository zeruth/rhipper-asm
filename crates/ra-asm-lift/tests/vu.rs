//! The VU1 micro program of `testdata/ps2/vu1.vsm`, lifted from the object `dvp-as` made of it and
//! run through the interpreter on an input buffer: the vertices it transforms, the packet it
//! kicks, and the registers and flags it leaves, against the values the source computes.

use std::path::Path;

use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{FnType, Module};
use ra_asm_lift::vu;

/// Where the run keeps the unit's state and its data memory.
const STATE: u64 = 0x1_0000;
const MEM: u64 = 0x2_0000;
/// The `TOP` the VIF would have set: the input buffer's first quadword.
const TOP: u64 = 16;

/// The matrix rows (`vf01` to `vf04`), the vertices, and `vf10` before the program writes it.
const MATRIX: [[f32; 4]; 4] = [[2.0, 0.0, 1.0, 0.0], [1.0, 2.0, 0.0, 0.0], [0.0, 0.0, 2.0, 0.0], [1.0, 2.0, 3.0, 2.0]];
const VERTICES: [[f32; 4]; 3] = [[1.0, -3.0, 0.5, 1.0], [-4.0, 2.5, -1.0, 1.0], [10.0, 20.0, 30.0, 1.0]];
const VF10: [f32; 4] = [8.0, 16.0, 32.0, 64.0];
/// The GIF tag the program copies ahead of its vertices, as words.
const TAG: [u32; 4] = [0x8003, 0x3000_0000, 0xe, 0];

/// The answers of the program's runtime: the FPU's arithmetic, and the GIF, which records the
/// address of each packet kicked to it.
#[derive(Default)]
struct Gif {
    kicks: Vec<u64>,
}

impl Host for Gif {
    fn call(&mut self, _: &mut Vm<'_>, name: &str, _: &FnType, args: &[Value]) -> Result<Value, Stop> {
        if name == vu::XGKICK {
            self.kicks.push(args[0].as_u64());
            return Ok(Value::Int(0));
        }
        let bits: Vec<u32> = args.iter().map(|a| a.as_u64() as u32).collect();
        ra_asm_lift::run::ftz_call(name, &bits)
            .map(|r| Value::Int(u128::from(r)))
            .ok_or_else(|| Stop::fault(format!("{name} is no VU runtime")))
    }
}

fn program() -> vu::Program {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/ps2/vu1.o");
    let bytes = std::fs::read(&path).expect("the fixture is committed");
    let object = ra_asm_pe::object::parse_object("vu1.o", &bytes).expect("vu1.o reads").expect("vu1.o is an object");
    let text = object.sections.iter().find(|s| s.name == ".vutext").expect("vu1.o has its micro code");
    vu::lift_program("vu1_transform", &text.bytes, 0, false)
}

fn qword(at: u64) -> u64 {
    MEM + 16 * at
}

fn vf(n: u64, lane: u64) -> u64 {
    STATE + vu::VF + 16 * n + 4 * lane
}

fn vi(n: u64) -> u64 {
    STATE + vu::VI + 2 * n
}

/// A vertex as the loop of the source transforms it: the matrix product, the divide by `w`, the
/// half `vf10` adds and the bound `vf12` (the rotated `vf10` the program found), in 12.4 fixed point.
fn transformed(v: [f32; 4]) -> [u32; 4] {
    let bound = [VF10[1], VF10[2], VF10[3]];
    let mut p = [0f32; 4];
    for (l, lane) in p.iter_mut().enumerate() {
        *lane = MATRIX[0][l] * v[0] + MATRIX[1][l] * v[1] + MATRIX[2][l] * v[2] + MATRIX[3][l] * v[3];
    }
    let q = 1.0 / p[3];
    for l in 0..3 {
        p[l] = (p[l] * q + 0.5).min(bound[l]);
    }
    p.map(|x| (x * 16.0) as i32 as u32)
}

#[test]
fn the_vu1_program_transforms_its_vertices_and_kicks_them() {
    let program = program();
    assert!(program.traps.is_empty(), "the lift trapped at {:#x?}", program.traps);
    let mut module = Module::x86_64_windows("vu1");
    vu::declare(&mut module);
    module.functions.push(program.function);
    let mut memory = Memory::new();
    memory.add_region(Region::owned(STATE, vu::STATE_SIZE, "vu1"));
    memory.add_region(Region::owned(MEM, 0x4000, "vu1 data"));
    let mut vm = Vm::new(&module, memory);
    let write = |vm: &mut Vm<'_>, at: u64, v: u32| vm.memory.write_u32(at, v).expect("mapped");
    write(&mut vm, STATE + vu::TOP, TOP as u32);
    for (l, x) in VF10.iter().enumerate() {
        write(&mut vm, vf(10, l as u64), x.to_bits());
    }
    for (r, row) in MATRIX.iter().enumerate() {
        for (l, x) in row.iter().enumerate() {
            write(&mut vm, qword(TOP + r as u64) + 4 * l as u64, x.to_bits());
        }
    }
    write(&mut vm, qword(TOP + 4), VERTICES.len() as u32);
    for (l, word) in TAG.iter().enumerate() {
        write(&mut vm, qword(TOP + 5) + 4 * l as u64, *word);
    }
    for (n, v) in VERTICES.iter().enumerate() {
        for (l, x) in v.iter().enumerate() {
            write(&mut vm, qword(TOP + 6 + n as u64) + 4 * l as u64, x.to_bits());
        }
    }

    let mut gif = Gif::default();
    vm.call_named(&mut gif, "vu1_transform", vec![Value::Int(u128::from(STATE)), Value::Int(u128::from(MEM))]).expect("the program runs");

    let mut read = |at: u64| vm.memory.read_u32(at).expect("mapped");
    let words = |read: &mut dyn FnMut(u64) -> u32, q: u64| -> [u32; 4] { std::array::from_fn(|l| read(qword(q) + 4 * l as u64)) };
    let out = TOP + 256;
    // The tag, its `x` overwritten by `isw.x vi09`: `vi04` past the vertices, or'd with nothing.
    let tag = words(&mut read, out);
    assert_eq!(tag, [TOP as u32 + 6 + VERTICES.len() as u32, TAG[1], TAG[2], TAG[3]]);
    // The first vertex is overwritten by `sq vf17, 1(vi05)`, which reads `vf17` as `madd` left it:
    // the pair's own `msubi` writes it after the store reads.
    let f = |x: f32| x.to_bits();
    assert_eq!(words(&mut read, out + 1), [f(-82.0), f(43.0), f(296.0), f(0.0)], "sq reads vf17 before the upper writes it");
    for (n, v) in VERTICES.iter().enumerate().skip(1) {
        assert_eq!(words(&mut read, out + 1 + n as u64), transformed(*v), "vertex {n}");
    }
    assert_eq!(gif.kicks, [qword(out)], "the packet kicked");

    // `mr32` read `vf10` before the pair's `addi` wrote it.
    let lanes = |read: &mut dyn FnMut(u64) -> u32, n: u64| -> [u32; 4] { std::array::from_fn(|l| read(vf(n, l as u64))) };
    assert_eq!(lanes(&mut read, 11), [VF10[1], VF10[2], VF10[3], VF10[0]].map(f32::to_bits));
    assert_eq!(lanes(&mut read, 10), [0.5f32; 4].map(f32::to_bits));
    // The last vertex back from fixed point, and the tail's cross product and multiply-adds.
    assert_eq!(lanes(&mut read, 14), transformed(VERTICES[2]).map(|x| (x as i32 as f32).to_bits()));
    assert_eq!(lanes(&mut read, 16), [f(-2.0), f(1.0), f(4.0), 0]);
    assert_eq!(lanes(&mut read, 17), [f(41.0), f(-20.5), f(-144.0), f(0.0)]);
    // esadd: 41 * 41 + 20.5 * 20.5 + 144 * 144.
    assert_eq!(read(vf(18, 0)), f(22_837.25));
    assert_eq!(read(STATE + vu::P), f(22_837.25));
    assert_eq!(read(STATE + vu::Q), f(0.5));

    let mut half = |n: u64| vm.memory.read_u16(vi(n)).expect("mapped");
    assert_eq!(half(1), 1, "fcand: the last vertex is outside");
    assert_eq!(half(3), 0);
    assert_eq!(half(4), TOP as u16 + 6 + VERTICES.len() as u16);
    assert_eq!(half(5), TOP as u16 + 256);
    assert_eq!(half(6), TOP as u16 + 256 + 1 + VERTICES.len() as u16);
    assert_eq!(half(10), 0x80, "fsand: a sign was seen");
    assert_eq!(half(11), 0, "fmand: msubi's x is positive");
    assert_eq!(half(15), 0x158 / 8, "bal at 0x148 links the pair after its delay slot");
    let mut read = |at: u64| vm.memory.read_u32(at).expect("mapped");
    // msubi: y and z negative, w zero.
    assert_eq!(read(STATE + vu::MAC), 0x61);
    assert_eq!(read(STATE + vu::STATUS), 0xc3);
    // The three vertices' judgements against |w| = 2, the oldest highest.
    assert_eq!(read(STATE + vu::CLIP), (0x18 << 12) | (0x26 << 6) | 0x15);
}
