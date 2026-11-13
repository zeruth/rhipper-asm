//! The structured tree against the graph it came from: one small function per statement kind,
//! structured, then run by the interpreter as its graph and as its tree. The two give the same
//! result, and a tree that does not follow its graph is a fault that names the function.

use ra_asm_emit::structure::{COPY_LIMIT, Graph, Stmt, structure};
use ra_asm_emit::tree::run_tree;
use ra_asm_ir::interp::{Host, Memory, Stop, Value, Vm};
use ra_asm_ir::{BinOp, BlockId, FnType, Function, FunctionBuilder, ICond, Inst, Module, Operand, Terminator, Type, ValueId};

/// A host no function of these tests calls.
struct NoHost;

impl Host for NoHost {
    fn call(&mut self, _vm: &mut Vm<'_>, name: &str, _ty: &FnType, _args: &[Value]) -> Result<Value, Stop> {
        Err(Stop::fault(format!("no host function {name}")))
    }
}

/// `i32 name(i32 x)`.
fn builder(name: &str) -> FunctionBuilder {
    FunctionBuilder::new(name, FnType::new(Type::I32, vec![Type::I32]), &["x"])
}

fn ret(value: impl Into<Operand>) -> Terminator {
    Terminator::Ret(Some((Type::I32, value.into())))
}

fn phi(b: &mut FunctionBuilder, block: BlockId, incoming: Vec<(Operand, BlockId)>, hint: &str) -> ValueId {
    b.value(block, Inst::Phi { ty: Type::I32, incoming }, hint)
}

/// Gives the phi `of` in `block` the value it takes from `from`: the value of a back edge is
/// defined after the phi that reads it.
fn feed(f: &mut Function, block: BlockId, of: ValueId, value: ValueId, from: BlockId) {
    for i in &mut f.blocks[block.0 as usize].instrs {
        if i.result == Some(of)
            && let Inst::Phi { incoming, .. } = &mut i.inst
        {
            incoming.push((value.into(), from));
        }
    }
}

/// Whether any statement of `stmts`, at any depth, is `this`.
fn any(stmts: &[Stmt], this: fn(&Stmt) -> bool) -> bool {
    stmts.iter().any(|s| {
        this(s)
            || match s {
                Stmt::If { then, els, .. } => any(then, this) || any(els, this),
                Stmt::Loop { body, .. } => any(body, this),
                Stmt::Switch { cases, .. } => cases.iter().any(|c| any(&c.body, this)),
                _ => false,
            }
    })
}

fn module_of(f: Function) -> Module {
    let mut module = Module::default();
    module.functions.push(f);
    module
}

/// Structures `f`, whose tree has a statement that is `this`, and runs it on each argument of
/// `runs` as a graph and as a tree: both give the result beside the argument.
fn same(f: Function, this: fn(&Stmt) -> bool, runs: &[(u128, u128)]) {
    let module = module_of(f);
    let f = &module.functions[0];
    let tree = structure(f, &Graph::of(f));
    assert!(any(&tree.body, this), "{:#?}", tree.body);
    for &(x, expected) in runs {
        let mut vm = Vm::new(&module, Memory::new());
        let graph = vm.call_named(&mut NoHost, &f.name, vec![Value::Int(x)]).expect("the graph runs");
        assert_eq!(graph, Value::Int(expected), "the graph of @{}({x})", f.name);
        let ran = run_tree(&mut vm, &mut NoHost, &f.name, &tree, vec![Value::Int(x)]);
        assert_eq!(ran, Ok(graph), "the tree of @{}({x}): {:#?}", f.name, tree.body);
    }
}

/// `(x + 1) * 2`, over two blocks.
fn straight() -> Function {
    let mut b = builder("straight");
    let entry = b.block("entry");
    let next = b.block("next");
    let x = b.param(0);
    let more = b.bin(entry, BinOp::Add, Type::I32, x, 1i64, "more");
    b.terminate(entry, Terminator::Br(next));
    let twice = b.bin(next, BinOp::Mul, Type::I32, more, 2i64, "twice");
    b.terminate(next, ret(twice));
    b.finish()
}

/// `if (x == 0) return 7; return x + 1;`.
fn early() -> Function {
    let mut b = builder("early");
    let entry = b.block("entry");
    let none = b.block("none");
    let rest = b.block("rest");
    let x = b.param(0);
    let zero = b.icmp(entry, ICond::Eq, Type::I32, x, 0i64, "zero");
    b.terminate(entry, Terminator::CondBr { cond: zero.into(), then: none, els: rest });
    b.terminate(none, ret(7i64));
    let more = b.bin(rest, BinOp::Add, Type::I32, x, 1i64, "more");
    b.terminate(rest, ret(more));
    b.finish()
}

/// `x < 10 ? x + 100 : x * 2`, the arms meeting in a phi.
fn diamond() -> Function {
    let mut b = builder("diamond");
    let entry = b.block("entry");
    let then = b.block("then");
    let els = b.block("els");
    let join = b.block("join");
    let x = b.param(0);
    let small = b.icmp(entry, ICond::Slt, Type::I32, x, 10i64, "small");
    b.terminate(entry, Terminator::CondBr { cond: small.into(), then, els });
    let high = b.bin(then, BinOp::Add, Type::I32, x, 100i64, "high");
    b.terminate(then, Terminator::Br(join));
    let twice = b.bin(els, BinOp::Mul, Type::I32, x, 2i64, "twice");
    b.terminate(els, Terminator::Br(join));
    let r = phi(&mut b, join, vec![(high.into(), then), (twice.into(), els)], "r");
    b.terminate(join, ret(r));
    b.finish()
}

/// `for (i = 0, s = 0; i < x; i++) s += i; return s;`, the test at the top.
fn sum() -> Function {
    let mut b = builder("sum");
    let entry = b.block("entry");
    let header = b.block("header");
    let body = b.block("body");
    let exit = b.block("exit");
    let x = b.param(0);
    b.terminate(entry, Terminator::Br(header));
    let i = phi(&mut b, header, vec![(Operand::Int(0), entry)], "i");
    let s = phi(&mut b, header, vec![(Operand::Int(0), entry)], "s");
    let more = b.icmp(header, ICond::Slt, Type::I32, i, x, "more");
    b.terminate(header, Terminator::CondBr { cond: more.into(), then: body, els: exit });
    let added = b.bin(body, BinOp::Add, Type::I32, s, i, "added");
    let next = b.bin(body, BinOp::Add, Type::I32, i, 1i64, "next");
    b.terminate(body, Terminator::Br(header));
    b.terminate(exit, ret(s));
    let mut f = b.finish();
    feed(&mut f, header, i, next, body);
    feed(&mut f, header, s, added, body);
    f
}

/// `i = 0; do i++; while (i < x); return i;`, the test at the bottom.
fn count() -> Function {
    let mut b = builder("count");
    let entry = b.block("entry");
    let body = b.block("body");
    let exit = b.block("exit");
    let x = b.param(0);
    b.terminate(entry, Terminator::Br(body));
    let i = phi(&mut b, body, vec![(Operand::Int(0), entry)], "i");
    let next = b.bin(body, BinOp::Add, Type::I32, i, 1i64, "next");
    let more = b.icmp(body, ICond::Slt, Type::I32, next, x, "more");
    b.terminate(body, Terminator::CondBr { cond: more.into(), then: body, els: exit });
    b.terminate(exit, ret(next));
    let mut f = b.finish();
    feed(&mut f, body, i, next, body);
    f
}

/// `for (i = 0, s = 0; i < x; i++) { if (i == 3) continue; s += i; } return s;`.
fn skip() -> Function {
    let mut b = builder("skip");
    let entry = b.block("entry");
    let header = b.block("header");
    let body = b.block("body");
    let add = b.block("add");
    let latch = b.block("latch");
    let exit = b.block("exit");
    let x = b.param(0);
    b.terminate(entry, Terminator::Br(header));
    let i = phi(&mut b, header, vec![(Operand::Int(0), entry)], "i");
    let s = phi(&mut b, header, vec![(Operand::Int(0), entry)], "s");
    let more = b.icmp(header, ICond::Slt, Type::I32, i, x, "more");
    b.terminate(header, Terminator::CondBr { cond: more.into(), then: body, els: exit });
    let third = b.icmp(body, ICond::Eq, Type::I32, i, 3i64, "third");
    b.terminate(body, Terminator::CondBr { cond: third.into(), then: latch, els: add });
    let added = b.bin(add, BinOp::Add, Type::I32, s, i, "added");
    b.terminate(add, Terminator::Br(latch));
    let kept = phi(&mut b, latch, vec![(s.into(), body), (added.into(), add)], "kept");
    let next = b.bin(latch, BinOp::Add, Type::I32, i, 1i64, "next");
    b.terminate(latch, Terminator::Br(header));
    b.terminate(exit, ret(s));
    let mut f = b.finish();
    feed(&mut f, header, i, next, latch);
    feed(&mut f, header, s, kept, latch);
    f
}

/// A `switch` on `x`: case 1 runs into case 2's block, which returns 1 coming from it and 2 on
/// its own; case 3 returns 3, case -1 returns 4 and the default 9.
fn choose() -> Function {
    let mut b = builder("choose");
    let entry = b.block("entry");
    let c1 = b.block("c1");
    let c2 = b.block("c2");
    let c3 = b.block("c3");
    let minus = b.block("minus");
    let other = b.block("other");
    let join = b.block("join");
    let x = b.param(0);
    b.terminate(
        entry,
        Terminator::Switch { ty: Type::I32, value: x.into(), default: other, cases: vec![(1, c1), (2, c2), (3, c3), (-1, minus)] },
    );
    b.terminate(c1, Terminator::Br(c2));
    let q = phi(&mut b, c2, vec![(Operand::Int(1), c1), (Operand::Int(2), entry)], "q");
    b.terminate(c2, Terminator::Br(join));
    b.terminate(c3, Terminator::Br(join));
    b.terminate(minus, Terminator::Br(join));
    b.terminate(other, Terminator::Br(join));
    let r = phi(&mut b, join, vec![(q.into(), c2), (Operand::Int(3), c3), (Operand::Int(4), minus), (Operand::Int(9), other)], "r");
    b.terminate(join, ret(r));
    b.finish()
}

/// `if (0 < x && x < 10) r = 5; else r = 0; return r;`, where the block of the second test hands
/// the 5 to a phi of the arm it falls through to.
fn both() -> Function {
    let mut b = builder("both");
    let entry = b.block("entry");
    let second = b.block("second");
    let then = b.block("then");
    let els = b.block("els");
    let join = b.block("join");
    let x = b.param(0);
    let positive = b.icmp(entry, ICond::Slt, Type::I32, 0i64, x, "positive");
    b.terminate(entry, Terminator::CondBr { cond: positive.into(), then: second, els });
    let small = b.icmp(second, ICond::Slt, Type::I32, x, 10i64, "small");
    b.terminate(second, Terminator::CondBr { cond: small.into(), then, els });
    let five = phi(&mut b, then, vec![(Operand::Int(5), second)], "five");
    b.terminate(then, Terminator::Br(join));
    b.terminate(els, Terminator::Br(join));
    let r = phi(&mut b, join, vec![(five.into(), then), (Operand::Int(0), els)], "r");
    b.terminate(join, ret(r));
    b.finish()
}

/// An early exit when `x` is zero, then a loop that counts to `x` and leaves through two blocks
/// (the count reached, a 7 found) to one return block, which the early exit shares: 100, 200 or
/// 300 by the way in, plus what the `tail` instructions of the return block add.
fn two_exits(tail: usize) -> Function {
    let mut b = builder("two_exits");
    let entry = b.block("entry");
    let header = b.block("header");
    let body = b.block("body");
    let latch = b.block("latch");
    let done = b.block("done");
    let found = b.block("found");
    let last = b.block("ret");
    let x = b.param(0);
    let none = b.icmp(entry, ICond::Eq, Type::I32, x, 0i64, "none");
    b.terminate(entry, Terminator::CondBr { cond: none.into(), then: last, els: header });
    let i = phi(&mut b, header, vec![(Operand::Int(0), entry)], "i");
    let more = b.icmp(header, ICond::Slt, Type::I32, i, x, "more");
    b.terminate(header, Terminator::CondBr { cond: more.into(), then: body, els: done });
    let hit = b.icmp(body, ICond::Eq, Type::I32, i, 7i64, "hit");
    b.terminate(body, Terminator::CondBr { cond: hit.into(), then: found, els: latch });
    let next = b.bin(latch, BinOp::Add, Type::I32, i, 1i64, "next");
    b.terminate(latch, Terminator::Br(header));
    b.terminate(done, Terminator::Br(last));
    b.terminate(found, Terminator::Br(last));
    let mut r = phi(&mut b, last, vec![(Operand::Int(100), entry), (Operand::Int(200), done), (Operand::Int(300), found)], "r");
    for k in 1..tail {
        r = b.bin(last, BinOp::Add, Type::I32, r, i64::try_from(k).expect("a small count"), "r");
    }
    b.terminate(last, ret(r));
    let mut f = b.finish();
    feed(&mut f, header, i, next, latch);
    f
}

/// What the return block of [`two_exits`] adds.
fn tail_adds(tail: usize) -> u128 {
    (1..tail).sum::<usize>() as u128
}

#[test]
fn blocks_run_in_the_order_the_tree_writes_them() {
    same(straight(), |s| matches!(s, Stmt::Block(_)), &[(3, 8), (0, 2)]);
}

#[test]
fn a_return_gives_the_value_of_its_block() {
    same(early(), |s| matches!(s, Stmt::Return(_)), &[(0, 7), (5, 6)]);
}

#[test]
fn an_if_runs_the_arm_its_branch_takes() {
    same(diamond(), |s| matches!(s, Stmt::If { .. }), &[(3, 103), (20, 40)]);
}

#[test]
fn a_loop_goes_round_until_it_is_left() {
    same(sum(), |s| matches!(s, Stmt::Loop { .. }), &[(0, 0), (1, 0), (5, 10)]);
}

#[test]
fn a_break_leaves_the_loop_to_its_exit() {
    same(count(), |s| matches!(s, Stmt::Break), &[(0, 1), (4, 4)]);
}

#[test]
fn a_continue_goes_back_to_the_header() {
    same(skip(), |s| matches!(s, Stmt::Continue), &[(0, 0), (3, 3), (6, 12)]);
}

#[test]
fn a_switch_runs_its_case_and_the_case_it_falls_into() {
    same(choose(), |s| matches!(s, Stmt::Switch { .. }), &[(1, 1), (2, 2), (3, 3), (0xffff_ffff, 4), (8, 9)]);
}

#[test]
fn moves_hand_over_the_values_of_a_folded_condition() {
    same(both(), |s| matches!(s, Stmt::Moves(_)), &[(5, 5), (0, 0), (20, 0)]);
}

#[test]
fn a_copy_runs_its_block_a_second_time() {
    let plus = tail_adds(2);
    same(two_exits(2), |s| matches!(s, Stmt::Copy(_)), &[(0, 100 + plus), (3, 200 + plus), (10, 300 + plus)]);
}

#[test]
fn a_goto_goes_on_from_the_label_inside_a_loop() {
    let plus = tail_adds(COPY_LIMIT + 1);
    same(two_exits(COPY_LIMIT + 1), |s| matches!(s, Stmt::Goto(_)), &[(0, 100 + plus), (3, 200 + plus), (10, 300 + plus)]);
}

#[test]
fn a_tree_that_leaves_its_graph_is_a_fault_that_names_the_function() {
    let module = module_of(diamond());
    let f = &module.functions[0];
    let mut tree = structure(f, &Graph::of(f));
    let mut swapped = false;
    for s in &mut tree.body {
        if let Stmt::If { then, els, .. } = s {
            std::mem::swap(then, els);
            swapped = true;
        }
    }
    assert!(swapped, "{:#?}", tree.body);
    let mut vm = Vm::new(&module, Memory::new());
    let stop = run_tree(&mut vm, &mut NoHost, "diamond", &tree, vec![Value::Int(3)]).expect_err("the arms are the wrong way round");
    assert!(matches!(&stop, Stop::Fault(message) if message.contains("@diamond") && message.contains("%els")), "{stop:?}");
    // The graph still runs: the fault is the tree's.
    assert_eq!(vm.call_named(&mut NoHost, "diamond", vec![Value::Int(3)]), Ok(Value::Int(103)));
}
