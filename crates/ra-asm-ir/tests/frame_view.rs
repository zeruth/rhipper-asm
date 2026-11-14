//! The frame from outside the crate: a consumer that runs a function block by block reads the
//! values the frame holds, by id or by name, and only that; nothing of the frame is written here.

use ra_asm_ir::interp::{Exit, Host, Memory, Stop, Value, Vm};
use ra_asm_ir::{BinOp, BlockId, FnType, Function, FunctionBuilder, Module, Terminator, Type, ValueId};

/// A host no block here calls into.
struct NoHost;

impl Host for NoHost {
    fn call(&mut self, _vm: &mut Vm<'_>, name: &str, _ty: &FnType, _args: &[Value]) -> Result<Value, Stop> {
        Err(Stop::fault(format!("no host function {name}")))
    }
}

/// `i32 leaf(i32 x)`: `twice` is `x` times two in `entry`; `done` returns `twice` plus one as `sum`.
fn leaf() -> Function {
    let mut b = FunctionBuilder::new("leaf", FnType::new(Type::I32, vec![Type::I32]), &["x"]);
    let entry = b.block("entry");
    let done = b.block("done");
    let x = b.param(0);
    let twice = b.bin(entry, BinOp::Mul, Type::I32, x, 2i64, "twice");
    b.terminate(entry, Terminator::Br(done));
    let sum = b.bin(done, BinOp::Add, Type::I32, twice, 1i64, "sum");
    b.terminate(done, Terminator::Ret(Some((Type::I32, sum.into()))));
    b.finish()
}

/// The id of the block `name` of `f`.
fn id(f: &Function, name: &str) -> BlockId {
    f.blocks.iter().find(|b| b.name == name).expect("the function has the block").id
}

#[test]
fn a_frame_shows_its_parameters_from_the_start_and_its_locals_once_they_ran() {
    let module = Module { functions: vec![leaf()], ..Module::default() };
    let f = &module.functions[0];
    let mut vm = Vm::new(&module, Memory::new());
    let mut frame = vm.frame("leaf", vec![Value::Int(21)]).expect("the function exists");
    assert_eq!(frame.function().name, "leaf");

    // The parameter is there before any block ran; the locals are not.
    assert_eq!(frame.value(f.params[0]), Some(&Value::Int(21)));
    assert_eq!(frame.value_named("x"), Some(&Value::Int(21)));
    assert_eq!(frame.value_named("twice"), None);
    assert_eq!(frame.value_named("sum"), None);

    let exit = vm.run_block(&mut NoHost, &mut frame, id(f, "entry")).expect("the block runs");
    assert_eq!(exit, Exit::Jump(id(f, "done")));
    assert_eq!(frame.value_named("twice"), Some(&Value::Int(42)));
    assert_eq!(frame.value_named("sum"), None);

    let exit = vm.run_block(&mut NoHost, &mut frame, id(f, "done")).expect("the block runs");
    assert_eq!(exit, Exit::Return(Value::Int(43)));
    assert_eq!(frame.value_named("sum"), Some(&Value::Int(43)));
    // By id and by name agree.
    let sum = f.value_names.iter().position(|n| n == "sum").expect("the function names sum");
    assert_eq!(frame.value(ValueId(sum as u32)), frame.value_named("sum"));
}

#[test]
fn a_value_the_function_has_not_is_none() {
    let module = Module { functions: vec![leaf()], ..Module::default() };
    let f = &module.functions[0];
    let mut vm = Vm::new(&module, Memory::new());
    let mut frame = vm.frame("leaf", vec![Value::Int(1)]).expect("the function exists");
    vm.run_block(&mut NoHost, &mut frame, id(f, "entry")).expect("the block runs");
    assert_eq!(frame.value(ValueId(f.value_names.len() as u32)), None);
    assert_eq!(frame.value_named("missing"), None);
    vm.release(&frame);
}
