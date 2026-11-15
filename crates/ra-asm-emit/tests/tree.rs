//! The structured tree against the graph it came from: one small function per statement kind,
//! structured, then run by the interpreter as its graph and as its tree. The two give the same
//! result, and a tree that does not follow its graph is a fault that names the function.
//!
//! Over the fixtures, every function of each program of `testdata/bin` is structured at the ABI
//! level, and the committed vectors of `testdata/vectors/fn` that replay through its graph replay
//! again through its tree, against the same record. A tree that differs where its graph does not
//! is a structuring bug, held in `testdata/vectors/fn/TREE_PENDING.txt` until it is fixed.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use ra_asm_emit::structure::{COPY_LIMIT, Graph, Stmt, Structured, structure};
use ra_asm_emit::tree::run_tree;
use ra_asm_ir::interp::{Host, Memory, Stop, Value, Vm};
use ra_asm_ir::{BinOp, BlockId, FnType, Function, FunctionBuilder, ICond, Inst, Module, Operand, Terminator, Type, ValueId};
use ra_asm_lift::raise::abi::Signature;
use ra_asm_lift::{LiftOptions, Lifted, RaiseLevel, X87Unit};
use ra_asm_pe::Image;
use ra_asm_verify::{Replayer, Runner};

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
                Stmt::Loop { body, .. } | Stmt::Tail { body, .. } => any(body, this),
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

/// A return block too long to copy is the tail of a region the loop leaves from inside.
#[test]
fn a_leave_goes_on_after_its_region_from_inside_a_loop() {
    let plus = tail_adds(COPY_LIMIT + 1);
    same(two_exits(COPY_LIMIT + 1), |s| matches!(s, Stmt::Tail { .. }), &[(0, 100 + plus), (3, 200 + plus), (10, 300 + plus)]);
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

// region The fixtures

const VECTORS_AT_LEAST: usize = 400;
const REPLAY_WITHIN: Duration = Duration::from_secs(60);

/// The functions of the fixtures whose trees differ from their graphs, one line each as the
/// fixtures test prints it.
const PENDING: &str = "testdata/vectors/fn/TREE_PENDING.txt";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// What the trees of one program did with its vectors.
#[derive(Default)]
struct Trees {
    /// The vectors replayed through a tree: those whose graph replays.
    vectors: usize,
    functions: usize,
    /// One line for each function whose tree differed: its first difference.
    differences: Vec<String>,
}

#[test]
fn every_function_of_the_fixtures_replays_its_vectors_through_its_tree() {
    let programs = programs();
    assert!(!programs.is_empty(), "no vectors are committed under testdata/vectors/fn");
    let start = Instant::now();
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(&str, Trees)>> = Mutex::new(Vec::new());
    let workers = std::thread::available_parallelism().map_or(1, usize::from).min(4);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|n| {
                // The interpreter recurses with the program's calls, past a test thread's stack.
                std::thread::Builder::new()
                    .name(format!("trees-{n}"))
                    .stack_size(256 << 20)
                    .spawn_scoped(scope, || {
                        while let Some(name) = programs.get(next.fetch_add(1, Ordering::Relaxed)) {
                            let trees = trees_of(name);
                            results.lock().unwrap().push((name.as_str(), trees));
                        }
                    })
                    .expect("a worker starts")
            })
            .collect();
        for handle in handles {
            handle.join().expect("a worker ends");
        }
    });
    let elapsed = start.elapsed();

    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|(name, _)| *name);
    let vectors: usize = results.iter().map(|(_, trees)| trees.vectors).sum();
    let functions: usize = results.iter().map(|(_, trees)| trees.functions).sum();
    let differences: Vec<&str> = results.iter().flat_map(|(_, trees)| trees.differences.iter().map(String::as_str)).collect();
    println!(
        "replayed {vectors} vectors of {functions} functions of {} programs through their trees in {:.1} s",
        results.len(),
        elapsed.as_secs_f64()
    );

    let pending = pending(&root().join(PENDING));
    let new: Vec<&str> = differences.iter().copied().filter(|line| !pending.contains_key(key(line))).collect();
    let keys: HashSet<&str> = differences.iter().copied().map(key).collect();
    let fixed: Vec<&str> = pending.iter().filter(|(held, _)| !keys.contains(held.as_str())).map(|(_, line)| line.as_str()).collect();
    println!("{} functions' trees differ as {PENDING} holds", differences.len() - new.len());
    assert!(new.is_empty(), "{} functions' trees differ from their graphs, which {PENDING} does not hold:\n{}", new.len(), new.join("\n"));
    assert!(
        fixed.is_empty(),
        "{} functions of {PENDING} no longer differ through their trees: take them out of it\n{}",
        fixed.len(),
        fixed.join("\n")
    );
    assert!(vectors >= VECTORS_AT_LEAST, "{vectors} vectors replayed through trees, not {VECTORS_AT_LEAST}");
    assert!(elapsed < REPLAY_WITHIN, "the replay took {:.1} s, past {} s", elapsed.as_secs_f64(), REPLAY_WITHIN.as_secs());
}

/// The programs whose vectors are committed, by the name of their `.fv` file, in order.
fn programs() -> Vec<String> {
    let dir = root().join("testdata/vectors/fn");
    let entries = std::fs::read_dir(&dir).unwrap_or_else(|error| panic!("reading {}: {error}", dir.display()));
    let mut names: Vec<String> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? != "fv" {
                return None;
            }
            Some(path.file_stem()?.to_str()?.to_owned())
        })
        .collect();
    names.sort();
    names
}

/// The functions `file` holds as pending, each line by its [`key`]; none when there is no file.
fn pending(file: &Path) -> BTreeMap<String, String> {
    let Ok(text) = std::fs::read_to_string(file) else { return BTreeMap::new() };
    let lines = text.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#'));
    lines.map(|line| (key(line).to_owned(), line.to_owned())).collect()
}

/// What a line of a difference is of: the program and the function, its first two words.
fn key(line: &str) -> &str {
    line.match_indices(' ').nth(1).map_or(line, |(at, _)| &line[..at])
}

/// Structures every function of `name` at the ABI level and replays each committed vector whose
/// graph replays through the tree of its function, up to each function's first difference.
fn trees_of(name: &str) -> Trees {
    let file = root().join("testdata/vectors/fn").join(format!("{name}.fv"));
    let text = std::fs::read_to_string(&file).unwrap_or_else(|error| panic!("reading {}: {error}", file.display()));
    let vectors = ra_asm_verify::read(&text).unwrap_or_else(|error| panic!("reading the vectors of {}: {error}", file.display()));
    let image = image(name);
    let (abi, signatures) = signed(&image);
    let trees: HashMap<&str, Structured> =
        abi.module.functions.iter().filter(|f| !f.blocks.is_empty()).map(|f| (f.name.as_str(), structure(f, &Graph::of(f)))).collect();
    let replayer = Replayer::with_signatures(&image, &abi, &signatures).unwrap_or_else(|error| panic!("{name}: {error:#}"));
    let runner: &Runner<'_> = &|vm, host, body, function, args| match trees.get(function) {
        Some(tree) => run_tree(vm, host, body, tree, args),
        None => Err(Stop::fault(format!("@{function} has no tree"))),
    };

    let mut replayed = Trees::default();
    let mut seen: HashMap<u64, usize> = HashMap::new();
    let mut ran: HashSet<u64> = HashSet::new();
    let mut differ: HashSet<u64> = HashSet::new();
    for vector in &vectors {
        let n = seen.entry(vector.address).or_default();
        *n += 1;
        // A vector the graph does not replay is the fixtures test of ra-asm-verify's to hold.
        if differ.contains(&vector.address) || !replayer.replay(vector).is_ok_and(|differences| differences.is_empty()) {
            continue;
        }
        replayed.vectors += 1;
        ran.insert(vector.address);
        let what = match replayer.replayed_by(vector, runner) {
            Ok(tree) => tree.differences.first().map(ToString::to_string),
            Err(error) => Some(format!("does not load: {error:#}")),
        };
        if let Some(what) = what {
            differ.insert(vector.address);
            replayed.differences.push(format!("{name} {} vector {}: {what}", vector.function, *n));
        }
    }
    replayed.functions = ran.len();
    replayed
}

fn image(name: &str) -> Image {
    let path = root().join("testdata/bin").join(format!("{name}.exe"));
    ra_asm_pe::load(&path).unwrap_or_else(|error| panic!("{}: {error:#}", path.display()))
}

/// `image` lifted standing alone and raised to the ABI level, with the signatures of its
/// functions under the names the run knows.
fn signed(image: &Image) -> (Lifted, HashMap<String, Signature>) {
    let analysis = ra_asm_analysis::analyze(image);
    let mut lifted = ra_asm_lift::lift_with(image, &analysis, LiftOptions { standalone: true, x87: X87Unit::Double });
    let options = lifted.raise_options(RaiseLevel::Abi);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    (lifted, signatures)
}

// endregion
