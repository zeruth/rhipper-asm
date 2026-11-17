//! A phi's incoming added after the fact: a loop's phi reads the value of its back edge, which the
//! builder defines after it.

use ra_asm_ir::interp::{Host, Memory, Stop, Value, Vm};
use ra_asm_ir::{BinOp, FnType, FunctionBuilder, ICond, Inst, Module, Operand, Terminator, Type};

/// A host no function of these tests calls.
struct NoHost;

impl Host for NoHost {
    fn call(&mut self, _vm: &mut Vm<'_>, name: &str, _ty: &FnType, _args: &[Value]) -> Result<Value, Stop> {
        Err(Stop::fault(format!("no host function {name}")))
    }
}

/// `i = 0; do i++; while (i < n); return i;`, the back edge's `i` added once `next` is defined.
fn count() -> FunctionBuilder {
    let mut b = FunctionBuilder::new("count", FnType::new(Type::I32, vec![Type::I32]), &["n"]);
    let n = b.param(0);
    let (entry, body, exit) = (b.block("entry"), b.block("body"), b.block("exit"));
    b.terminate(entry, Terminator::Br(body));
    let i = b.value(body, Inst::Phi { ty: Type::I32, incoming: vec![(Operand::Int(0), entry)] }, "i");
    let next = b.bin(body, BinOp::Add, Type::I32, i, 1i64, "next");
    let more = b.icmp(body, ICond::Slt, Type::I32, next, n, "more");
    b.terminate(body, Terminator::CondBr { cond: more.into(), then: body, els: exit });
    b.terminate(exit, Terminator::Ret(Some((Type::I32, next.into()))));
    b.add_incoming(i, body, next);
    b
}

#[test]
fn the_incoming_lands_last_on_its_phi() {
    let f = count().finish();
    let body = &f.blocks[1];
    let Inst::Phi { incoming, .. } = &body.instrs[0].inst else { panic!("the first instruction of the loop is its phi") };
    let next = body.instrs[1].result.expect("the add defines a value");
    assert_eq!(incoming, &vec![(Operand::Int(0), f.blocks[0].id), (next.into(), body.id)]);
}

#[test]
fn the_loop_runs_round_its_back_edge() {
    let mut module = Module::default();
    module.functions.push(count().finish());
    for (n, expected) in [(0u128, 1u128), (1, 1), (5, 5)] {
        let mut vm = Vm::new(&module, Memory::new());
        let got = vm.call_named(&mut NoHost, "count", vec![Value::Int(n)]).expect("the loop runs");
        assert_eq!(got, Value::Int(expected), "@count({n})");
    }
}

#[test]
#[should_panic(expected = "the value is a phi of the function")]
fn a_value_that_is_no_phi_is_refused() {
    let mut b = count();
    let n = b.param(0);
    let entry = ra_asm_ir::BlockId(0);
    b.add_incoming(n, entry, 1i64);
}
