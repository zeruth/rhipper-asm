//! The byte order of a run: an IR load and store follow the module's data layout, as LLVM's do,
//! and so do the host's words of the program; the fixed-width accessors stay in the host's order.

use super::{Host, Memory, Region, Stop, Value, Vm};
use crate::module::Module;
use crate::types::FnType;

/// Where the tests' memory lies: an owned region, at an address no process maps.
const BASE: u64 = 0xffff_8000_0000_1000;

/// Answers nothing: the functions below call no host.
struct NoHost;

impl Host for NoHost {
    fn call(&mut self, _vm: &mut Vm<'_>, name: &str, _ty: &FnType, _args: &[Value]) -> Result<Value, Stop> {
        Err(Stop::fault(format!("no host function {name}")))
    }
}

/// Stores and loads of 8, 32 and 128 bits through a pointer, under the data layout given.
fn module(layout: &str) -> Module {
    let text = format!(
        "target datalayout = \"{layout}\"

define void @store32(ptr %p, i32 %v) {{
entry:
  store i32 %v, ptr %p, align 4
  ret void
}}

define i32 @load32(ptr %p) {{
entry:
  %v = load i32, ptr %p, align 4
  ret i32 %v
}}

define i8 @load8(ptr %p) {{
entry:
  %v = load i8, ptr %p, align 1
  ret i8 %v
}}

define void @store128(ptr %p, i128 %v) {{
entry:
  store i128 %v, ptr %p, align 16
  ret void
}}

define i128 @load128(ptr %p) {{
entry:
  %v = load i128, ptr %p, align 16
  ret i128 %v
}}
"
    );
    crate::parse::parse(&text).expect("the text parses")
}

fn memory() -> Memory {
    let mut memory = Memory::new();
    memory.add_region(Region::owned(BASE, 32, "order"));
    memory
}

fn call(vm: &mut Vm<'_>, name: &str, args: Vec<Value>) -> u128 {
    vm.call_named(&mut NoHost, name, args).expect("the function runs").as_u128()
}

#[test]
fn a_big_endian_word_is_stored_and_loaded_most_significant_byte_first() {
    let module = module("E-m:e-p:32:32-i64:64-n32-S64");
    assert!(module.big_endian());
    let mut vm = Vm::new(&module, memory());
    assert!(vm.memory.big_endian());
    call(&mut vm, "store32", vec![Value::Int(u128::from(BASE)), Value::Int(0x1122_3344)]);
    assert_eq!(vm.memory.read_vec(BASE, 4).unwrap(), [0x11, 0x22, 0x33, 0x44]);
    assert_eq!(call(&mut vm, "load32", vec![Value::Int(u128::from(BASE))]), 0x1122_3344);
    // The program's words follow the module; the machine state's accessors stay in host order.
    assert_eq!(vm.memory.read_word(BASE, 4).unwrap(), 0x1122_3344);
    assert_eq!(vm.memory.read_u32(BASE).unwrap(), 0x4433_2211);
    vm.memory.write_word(BASE + 8, 8, 0x0102_0304_0506_0708).unwrap();
    assert_eq!(vm.memory.read_vec(BASE + 8, 8).unwrap(), [1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(vm.memory.read_word(BASE + 8, 8).unwrap(), 0x0102_0304_0506_0708);
}

#[test]
fn a_byte_read_of_a_big_endian_word_finds_its_high_byte_first() {
    let module = module("E-m:e-p:32:32-i64:64-n32-S64");
    let mut vm = Vm::new(&module, memory());
    call(&mut vm, "store32", vec![Value::Int(u128::from(BASE)), Value::Int(0xaabb_ccdd)]);
    assert_eq!(call(&mut vm, "load8", vec![Value::Int(u128::from(BASE))]), 0xaa);
    assert_eq!(call(&mut vm, "load8", vec![Value::Int(u128::from(BASE + 3))]), 0xdd);
}

#[test]
fn a_big_endian_128_bit_value_round_trips_in_order() {
    let module = module("E-m:e-p:32:32-i64:64-n32-S64");
    let mut vm = Vm::new(&module, memory());
    let value = 0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10;
    call(&mut vm, "store128", vec![Value::Int(u128::from(BASE)), Value::Int(value)]);
    assert_eq!(vm.memory.read_vec(BASE, 16).unwrap(), (1..=16).collect::<Vec<u8>>());
    assert_eq!(call(&mut vm, "load128", vec![Value::Int(u128::from(BASE))]), value);
}

#[test]
fn a_little_endian_module_is_unchanged() {
    let module = module("e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128");
    assert!(!module.big_endian());
    let mut vm = Vm::new(&module, memory());
    assert!(!vm.memory.big_endian());
    call(&mut vm, "store32", vec![Value::Int(u128::from(BASE)), Value::Int(0x1122_3344)]);
    assert_eq!(vm.memory.read_vec(BASE, 4).unwrap(), [0x44, 0x33, 0x22, 0x11]);
    assert_eq!(call(&mut vm, "load8", vec![Value::Int(u128::from(BASE))]), 0x44);
    assert_eq!(vm.memory.read_word(BASE, 4).unwrap(), 0x1122_3344);
    assert_eq!(vm.memory.read_u32(BASE).unwrap(), 0x1122_3344);
    let value = 0x0102_0304_0506_0708_090a_0b0c_0d0e_0f10;
    call(&mut vm, "store128", vec![Value::Int(u128::from(BASE)), Value::Int(value)]);
    assert_eq!(vm.memory.read_vec(BASE, 16).unwrap(), (1..=16).rev().collect::<Vec<u8>>());
    assert_eq!(call(&mut vm, "load128", vec![Value::Int(u128::from(BASE))]), value);
}

#[test]
fn the_byte_order_prints_and_parses_back() {
    let mut module = Module::x86_64_linux("order");
    assert!(!module.big_endian());
    module.set_big_endian(true);
    assert!(module.data_layout.starts_with("E-m:e-"), "{}", module.data_layout);
    let text = module.to_ll();
    assert!(text.contains("target datalayout = \"E-m:e-"), "{text}");
    assert!(crate::parse::parse(&text).expect("the text parses").big_endian());
    module.set_big_endian(false);
    assert_eq!(module.data_layout, Module::x86_64_linux("order").data_layout);
    assert!(!crate::parse::parse(&module.to_ll()).expect("the text parses").big_endian());
}
