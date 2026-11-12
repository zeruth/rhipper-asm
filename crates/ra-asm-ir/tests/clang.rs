//! The printer against clang: a hand-built module must compile, and run as its source would.
#![allow(clippy::many_single_char_names)]

use std::path::PathBuf;
use std::process::Command;

use ra_asm_ir::{
    BinOp, CallConv, Callee, Cast, Const, Declaration, FnType, FunctionBuilder, Global, ICond, Inst, Linkage, Module, Operand, Terminator,
    Type,
};

/// A module with a loop, a call, a global, a select, a switch and a phi: the subset in one piece.
fn sample() -> Module {
    let mut m = Module::x86_64_windows("sample");
    m.globals.push(Global {
        name: "counter".into(),
        ty: Type::I32,
        init: Some(Const::Int(7)),
        constant: false,
        linkage: Linkage::Internal,
        align: 4,
        address: None,
    });
    m.globals.push(Global {
        name: "greeting".into(),
        ty: Type::array(6, Type::I8),
        init: Some(Const::Bytes(b"hi\n\0\0\0".to_vec())),
        constant: true,
        linkage: Linkage::Internal,
        align: 1,
        address: None,
    });
    m.declare(Declaration { name: "ra_twice".into(), ty: FnType::new(Type::I32, vec![Type::I32]), conv: CallConv::C, attrs: vec![] });

    // int sum_to(int n): sums 1..n with a loop, adds the global, calls ra_twice on an odd total.
    let mut b = FunctionBuilder::new("sum_to", FnType::new(Type::I32, vec![Type::I32]), &["n"]);
    let n = b.param(0);
    let entry = b.block("entry");
    let header = b.block("loop");
    let body = b.block("body");
    let done = b.block("done");
    let odd = b.block("odd");
    let join = b.block("join");
    b.terminate(entry, Terminator::Br(header));
    let i = b.value(header, Inst::Phi { ty: Type::I32, incoming: vec![] }, "i");
    let acc = b.value(header, Inst::Phi { ty: Type::I32, incoming: vec![] }, "acc");
    let more = b.icmp(header, ICond::Sle, Type::I32, i, n, "more");
    b.terminate(header, Terminator::CondBr { cond: more.into(), then: body, els: done });
    let acc2 = b.bin(body, BinOp::Add, Type::I32, acc, i, "acc.next");
    let i2 = b.bin(body, BinOp::Add, Type::I32, i, 1i64, "i.next");
    b.terminate(body, Terminator::Br(header));
    // Fill the phis now that the incoming values exist.
    let f = b.function();
    assert_eq!(f.value_name(i), "i");
    let g = b.load(done, Type::I32, Operand::Global("counter".into()), "g");
    let total = b.bin(done, BinOp::Add, Type::I32, acc, g, "total");
    let low = b.bin(done, BinOp::And, Type::I32, total, 1i64, "low");
    let is_odd = b.icmp(done, ICond::Ne, Type::I32, low, 0i64, "is_odd");
    b.terminate(done, Terminator::CondBr { cond: is_odd.into(), then: odd, els: join });
    let doubled = b
        .call(odd, FnType::new(Type::I32, vec![Type::I32]), Callee::Direct("ra_twice".into()), vec![(Type::I32, total.into())], "doubled")
        .unwrap();
    b.terminate(odd, Terminator::Br(join));
    let result = b.value(join, Inst::Phi { ty: Type::I32, incoming: vec![(total.into(), done), (doubled.into(), odd)] }, "result");
    let wide = b.cast(join, Cast::SExt, Type::I32, result, Type::I64, "wide");
    let narrow = b.cast(join, Cast::Trunc, Type::I64, wide, Type::I32, "narrow");
    b.terminate(join, Terminator::Ret(Some((Type::I32, narrow.into()))));
    let mut f = b.finish();
    // The phis of the header: i starts at 1, acc at 0.
    let header_block = f.block_mut(header).unwrap();
    header_block.instrs[0].inst = Inst::Phi { ty: Type::I32, incoming: vec![(Operand::Int(1), entry), (i2.into(), body)] };
    header_block.instrs[1].inst = Inst::Phi { ty: Type::I32, incoming: vec![(Operand::Int(0), entry), (acc2.into(), body)] };
    m.functions.push(f);

    // int pick(int k): a switch with a select, through an alloca, as the lift writes them.
    let mut b = FunctionBuilder::new("pick", FnType::new(Type::I32, vec![Type::I32]), &["k"]);
    let k = b.param(0);
    let entry = b.block("entry");
    let one = b.block("one");
    let two = b.block("two");
    let other = b.block("other");
    let slot = b.alloca(entry, Type::I32, "slot");
    b.store(entry, Type::I32, 0i64, slot);
    b.terminate(entry, Terminator::Switch { ty: Type::I32, value: k.into(), default: other, cases: vec![(1, one), (2, two)] });
    b.store(one, Type::I32, 10i64, slot);
    b.terminate(one, Terminator::Br(other));
    b.store(two, Type::I32, 20i64, slot);
    b.terminate(two, Terminator::Br(other));
    let v = b.load(other, Type::I32, slot, "v");
    let neg = b.icmp(other, ICond::Slt, Type::I32, k, 0i64, "neg");
    let picked = b.select(other, neg, Type::I32, -1i64, v, "picked");
    b.terminate(other, Terminator::Ret(Some((Type::I32, picked.into()))));
    m.functions.push(b.finish());
    m
}

fn clang() -> Option<PathBuf> {
    let output = Command::new("clang").arg("--version").output().ok()?;
    output.status.success().then(|| PathBuf::from("clang"))
}

/// A test's own directory under the system temp, removed when the guard drops; `RA_KEEP_SCRATCH`
/// keeps it for a person to read.
fn scratch(name: &str) -> ra_asm_testing::Scratch {
    ra_asm_testing::Scratch::new("ra-asm-ir-tests", name)
}

#[test]
fn the_sample_prints_the_expected_shapes() {
    let text = sample().to_ll();
    assert!(text.contains("target triple = \"x86_64-pc-windows-msvc\""));
    assert!(text.contains("@counter = internal global i32 7, align 4"));
    assert!(text.contains("@greeting = internal constant [6 x i8] c\"hi\\0A\\00\\00\\00\", align 1"));
    assert!(text.contains("declare i32 @ra_twice(i32)"));
    assert!(text.contains("define i32 @sum_to(i32 %n) {"));
    assert!(text.contains("  %i = phi i32 [ 1, %entry ], [ %i.next, %body ]"));
    assert!(text.contains("  %more = icmp sle i32 %i, %n"));
    assert!(text.contains("  br i1 %more, label %body, label %done"));
    assert!(text.contains("  %doubled = call i32 @ra_twice(i32 %total)"));
    assert!(text.contains("  switch i32 %k, label %other [ i32 1, label %one i32 2, label %two ]"));
    assert!(text.contains("  %picked = select i1 %neg, i32 -1, i32 %v"));
}

#[test]
fn clang_accepts_and_runs_the_sample() {
    let Some(clang) = clang() else {
        eprintln!("clang is not on the PATH: the compile test is skipped");
        return;
    };
    let dir = scratch("sample");
    let ll = dir.join("sample.ll");
    std::fs::write(&ll, sample().to_ll()).unwrap();
    // The gate: the module parses and verifies.
    let check = Command::new(&clang).args(["-c", "-x", "ir", "-o"]).arg(dir.join("sample.o")).arg(&ll).output().unwrap();
    assert!(check.status.success(), "clang -c:\n{}", String::from_utf8_lossy(&check.stderr));
    // The module names the Windows target: it links with a driver and runs on a Windows host.
    if !cfg!(windows) {
        eprintln!("the sample's module targets Windows: the run is skipped on this host");
        return;
    }
    // The run: a C driver calls both functions and prints what they return.
    let driver = dir.join("driver.c");
    std::fs::write(
        &driver,
        "#include <stdio.h>\nint sum_to(int);\nint pick(int);\nint ra_twice(int v) { return v * 2; }\n\
         int main(void) { printf(\"%d %d %d %d %d\\n\", sum_to(4), sum_to(5), pick(1), pick(2), pick(-3)); return 0; }\n",
    )
    .unwrap();
    let exe = dir.join("sample.exe");
    let build = Command::new(&clang).arg("-O1").arg(&ll).arg(&driver).arg("-o").arg(&exe).output().unwrap();
    if !build.status.success() {
        let text = String::from_utf8_lossy(&build.stderr);
        // Without the MSVC C runtime installed, the link fails and the run is skipped.
        if text.contains("cannot open input file") || text.contains("libcmt") || text.contains("msvcrt") {
            eprintln!("the C runtime is not installed: the run is skipped\n{text}");
            return;
        }
        panic!("clang link:\n{text}");
    }
    let run = Command::new(&exe).output().unwrap();
    assert!(run.status.success());
    // sum_to(4) = 10 + 7 = 17, odd -> 34; sum_to(5) = 15 + 7 = 22, even -> 22.
    assert_eq!(String::from_utf8_lossy(&run.stdout).trim(), "34 22 10 20 -1");
}
