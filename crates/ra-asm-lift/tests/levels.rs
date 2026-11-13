//! The invariants of the raise levels (`raise::check`): what the lift writes passes its level's
//! checker, and a function broken in one way, written as text, fails with that violation and the
//! block it is in.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ra_asm_ir::{BlockId, Callee, FnType, Function, FunctionBuilder, Module, Operand, Terminator, Type, ValueId};
use ra_asm_lift::raise::Level;
use ra_asm_lift::raise::check::{Rule, Violation, check, check_in};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin").join(name)
}

/// A fixture lifted and raised to `level`: over the mapped image, and standalone at the ABI
/// level, as the oracle builds each.
fn raised(binary: &str, level: Level) -> ra_asm_lift::Lifted {
    let image = ra_asm_pe::load(&fixture(binary)).unwrap();
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: level == Level::Abi, ..Default::default() };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let options = lifted.raise_options(level);
    let _signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    assert!(!lifted.module.functions.is_empty(), "{binary}: nothing was lifted");
    lifted
}

/// The stack accesses the level flags over every function of a raised fixture, each of which
/// passes the level.
fn flagged(binary: &str, level: Level) -> usize {
    let lifted = raised(binary, level);
    let mut flagged = 0;
    for f in &lifted.module.functions {
        match check_in(&lifted.module, f, level) {
            Ok(count) => flagged += count,
            Err(violation) => panic!("{binary}: {}: {violation}", f.name),
        }
    }
    flagged
}

fn ty(word: &str) -> Type {
    match word {
        "void" => Type::Void,
        "ptr" => Type::Ptr,
        "%State" => Type::Named("State".to_owned()),
        other => Type::Int(other.strip_prefix('i').and_then(|bits| bits.parse().ok()).unwrap_or_else(|| panic!("no type `{other}`"))),
    }
}

fn operand(word: &str, values: &HashMap<String, ValueId>) -> Operand {
    match word.strip_prefix('%') {
        Some(name) => Operand::Local(*values.get(name).unwrap_or_else(|| panic!("no value `%{name}`"))),
        None if word == "undef" => Operand::Undef,
        None => Operand::Int(word.parse().unwrap_or_else(|_| panic!("no operand `{word}`"))),
    }
}

/// The typed operands of a run of words, `ty value` pairs.
fn typed(words: &[&str], values: &HashMap<String, ValueId>) -> Vec<(Type, Operand)> {
    words.chunks(2).map(|pair| (ty(pair[0]), operand(pair[1], values))).collect()
}

/// A function from its text, in the printer's spelling: one `define`, labelled blocks, and
/// `alloca`, `getelementptr`, `load`, `store`, `call`, `ret`, `br label` and `unreachable` with
/// local, integer and `undef` operands. A block the text does not end stays as the builder makes
/// one.
fn function(text: &str) -> Function {
    let spaced = text.replace([',', '(', ')'], " ");
    let lines: Vec<Vec<&str>> = spaced.lines().map(|l| l.split_whitespace().collect::<Vec<_>>()).filter(|l| !l.is_empty()).collect();
    let header = &lines[0];
    assert_eq!(header[0], "define", "the text starts with a function");
    let params: Vec<(Type, &str)> = header[3..header.len() - 1].chunks(2).map(|p| (ty(p[0]), &p[1][1..])).collect();
    let names: Vec<&str> = params.iter().map(|(_, name)| *name).collect();
    let fn_ty = FnType::new(ty(header[1]), params.iter().map(|(t, _)| t.clone()).collect());
    let mut b = FunctionBuilder::new(&header[2][1..], fn_ty, &names);
    let mut values: HashMap<String, ValueId> = names.iter().enumerate().map(|(n, name)| ((*name).to_owned(), b.param(n))).collect();
    let body = &lines[1..lines.len() - 1];
    let mut blocks: HashMap<&str, BlockId> = HashMap::new();
    for line in body {
        if let Some(label) = line[0].strip_suffix(':') {
            blocks.insert(label, b.block(label));
        }
    }
    let mut block = None;
    for line in body {
        if let Some(label) = line[0].strip_suffix(':') {
            block = Some(blocks[label]);
            continue;
        }
        let at = block.expect("an instruction stands in a block");
        let (result, rest) = if line.get(1) == Some(&"=") { (&line[0][1..], &line[2..]) } else { ("", &line[..]) };
        let defined = match rest[0] {
            "alloca" => {
                let slot = match rest[1].strip_prefix('[') {
                    Some(count) => Type::array(count.parse().expect("an array's count"), ty(rest[3].trim_end_matches(']'))),
                    None => ty(rest[1]),
                };
                Some(b.alloca(at, slot, result))
            }
            "getelementptr" => Some(b.gep(at, ty(rest[1]), operand(rest[3], &values), typed(&rest[4..], &values), result)),
            "load" => Some(b.load(at, ty(rest[1]), operand(rest[3], &values), result)),
            "store" => {
                b.store(at, ty(rest[1]), operand(rest[2], &values), operand(rest[4], &values));
                None
            }
            "call" => {
                let args = typed(&rest[3..], &values);
                let fn_ty = FnType::new(ty(rest[1]), args.iter().map(|(t, _)| t.clone()).collect());
                let callee = match rest[2].strip_prefix('@') {
                    Some(name) => Callee::Direct(name.to_owned()),
                    None => Callee::Indirect(operand(rest[2], &values)),
                };
                b.call(at, fn_ty, callee, args, result)
            }
            "ret" => {
                let value = (rest[1] != "void").then(|| (ty(rest[1]), operand(rest[2], &values)));
                b.terminate(at, Terminator::Ret(value));
                None
            }
            "br" => {
                b.terminate(at, Terminator::Br(blocks[&rest[2][1..]]));
                None
            }
            "unreachable" => {
                b.terminate(at, Terminator::Unreachable);
                None
            }
            other => panic!("no instruction `{other}`"),
        };
        if let Some(value) = defined {
            values.insert(result.to_owned(), value);
        }
    }
    b.finish()
}

/// The violation of a function the machine level refuses.
fn refused(text: &str) -> Violation {
    refused_at(text, Level::Machine)
}

/// The violation of a function `level` refuses.
fn refused_at(text: &str, level: Level) -> Violation {
    check(&function(text), level).expect_err("the function breaks its level")
}

/// A module of the functions of `texts`.
fn module(texts: &[&str]) -> Module {
    let mut module = Module::x86_64_windows("levels");
    module.functions.extend(texts.iter().map(|text| function(text)));
    module
}

#[test]
fn machine_lifted_fixtures_pass() {
    for binary in ["hello64.exe", "hello32.exe"] {
        assert_eq!(flagged(binary, Level::Machine), 0, "{binary}");
    }
}

#[test]
fn machine_text_function_passes() {
    let f = function(
        "define void @F_00401000(ptr %state) {
        b_401000:
          %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
          %rax = load i64, ptr %p.rax
          %p.zf = getelementptr %State, ptr %state, i32 0, i32 2, i32 3
          store i8 1, ptr %p.zf
          %p.rip = getelementptr %State, ptr %state, i32 0, i32 1
          store i64 4198400, ptr %p.rip
          call void @F_00401020(ptr %state)
          call void @ra_call(ptr %state, i64 %rax)
          br label %b_401010
        b_401010:
          %p.xmm15 = getelementptr %State, ptr %state, i32 0, i32 3, i32 15
          store i128 0, ptr %p.xmm15
          ret void
        b_401018:
          call void @ra_trap(i64 4198424, i64 1)
          unreachable
        }",
    );
    assert_eq!(f.blocks.len(), 3);
    assert_eq!(check(&f, Level::Machine), Ok(0));
}

#[test]
fn machine_block_without_terminator() {
    let violation = refused(
        "define void @F_00401000(ptr %state) {
        b_401000:
          br label %b_401008
        b_401008:
          %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
          store i64 1, ptr %p.rax
        }",
    );
    assert_eq!((violation.rule, violation.block.as_str()), (Rule::Terminator, "b_401008"), "{violation}");
}

#[test]
fn machine_register_outside_layout() {
    let violation = refused(
        "define void @F_00401000(ptr %state) {
        b_401000:
          %p.r16 = getelementptr %State, ptr %state, i32 0, i32 0, i32 16
          store i64 1, ptr %p.r16
          ret void
        }",
    );
    assert_eq!((violation.rule, violation.block.as_str()), (Rule::Register, "b_401000"), "{violation}");
    assert!(violation.what.contains("%p.r16"), "{violation}");
}

#[test]
fn machine_state_read_outside_a_field() {
    let violation = refused(
        "define void @F_00401000(ptr %state) {
        b_401000:
          %rax = load i64, ptr %state
          ret void
        }",
    );
    assert_eq!((violation.rule, violation.block.as_str()), (Rule::Register, "b_401000"), "{violation}");
}

#[test]
fn machine_indirect_call() {
    let violation = refused(
        "define void @F_00401000(ptr %state) {
        b_401000:
          %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
          %rax = load i64, ptr %p.rax
          br label %b_401008
        b_401008:
          call void %rax(ptr %state)
          ret void
        }",
    );
    assert_eq!((violation.rule, violation.block.as_str()), (Rule::Call, "b_401008"), "{violation}");
}

#[test]
fn machine_direct_call_without_the_state() {
    let violation = refused(
        "define void @F_00401000(ptr %state) {
        b_401000:
          call void @F_00401020(i64 1)
          ret void
        }",
    );
    assert_eq!((violation.rule, violation.block.as_str()), (Rule::Call, "b_401000"), "{violation}");
}

#[test]
fn machine_undef_value() {
    let violation = refused(
        "define void @F_00401000(ptr %state) {
        b_401000:
          %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
          store i64 undef, ptr %p.rcx
          ret void
        }",
    );
    assert_eq!((violation.rule, violation.block.as_str()), (Rule::Undef, "b_401000"), "{violation}");
}

#[test]
fn raised_promoted_fixtures_pass() {
    for binary in ["hello64.exe", "hello32.exe"] {
        assert_eq!(flagged(binary, Level::Promoted), 0, "{binary}");
    }
}

#[test]
fn raised_promoted_text_function_passes() {
    let f = function(
        "define void @F_00401000(ptr %state) {
        b_401000:
          %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
          %rcx = load i64, ptr %p.rcx
          %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
          store i64 %rcx, ptr %p.rax
          call void @F_00401020(ptr %state)
          %p.rax.1 = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
          %rax = load i64, ptr %p.rax.1
          %p.xmm0 = getelementptr %State, ptr %state, i32 0, i32 3, i32 0
          store i128 0, ptr %p.xmm0
          %p.rdx = getelementptr %State, ptr %state, i32 0, i32 0, i32 2
          store i64 %rax, ptr %p.rdx
          ret void
        }",
    );
    assert_eq!(check(&f, Level::Promoted), Ok(0));
}

#[test]
fn raised_promoted_store_away_from_a_call() {
    let violation = refused_at(
        "define void @F_00401000(ptr %state) {
        b_401000:
          %p.rax = getelementptr %State, ptr %state, i32 0, i32 0, i32 0
          store i64 1, ptr %p.rax
          br label %b_401008
        b_401008:
          ret void
        }",
        Level::Promoted,
    );
    assert_eq!((violation.rule, violation.block.as_str()), (Rule::Sync, "b_401000"), "{violation}");
    assert!(violation.what.contains("`rax` is stored away"), "{violation}");
}

#[test]
fn raised_promoted_register_loaded_twice() {
    let violation = refused_at(
        "define void @F_00401000(ptr %state) {
        b_401000:
          %p.rcx = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
          %rcx = load i64, ptr %p.rcx
          br label %b_401008
        b_401008:
          %p.rcx.1 = getelementptr %State, ptr %state, i32 0, i32 0, i32 1
          %rcx.1 = load i64, ptr %p.rcx.1
          %rcx.2 = load i64, ptr %p.rcx.1
          ret void
        }",
        Level::Promoted,
    );
    assert_eq!((violation.rule, violation.block.as_str()), (Rule::Sync, "b_401008"), "{violation}");
    assert!(violation.what.contains("`rcx` is loaded"), "{violation}");
}

#[test]
fn raised_promoted_flag_stored() {
    let violation = refused_at(
        "define void @F_00401000(ptr %state) {
        b_401000:
          %p.zf = getelementptr %State, ptr %state, i32 0, i32 2, i32 3
          store i8 1, ptr %p.zf
          call void @F_00401020(ptr %state)
          ret void
        }",
        Level::Promoted,
    );
    assert_eq!((violation.rule, violation.block.as_str()), (Rule::Sync, "b_401000"), "{violation}");
    assert!(violation.what.contains("the flag `zf`"), "{violation}");
}

/// A callee of two integers, for the calls of the ABI functions below.
const CALLEE: &str = "define i64 @F_00401020(i64 %arg0, i64 %arg1) {
    entry:
      ret i64 %arg0
    }";

#[test]
fn raised_abi_fixtures_pass() {
    for binary in ["hello64.exe", "hello32.exe"] {
        // The entry point hands `WriteFile` the addresses of `buf` and `written`: both stay in
        // its frame, and their accesses are flagged.
        assert!(flagged(binary, Level::Abi) > 0, "{binary}: no stack access was flagged");
    }
}

#[test]
fn raised_abi_text_function_counts_its_frame_accesses() {
    let module = module(&[
        "define i64 @F_00401000(i64 %arg0) {
        entry:
          %frame = alloca [64 x i8]
          %local_8 = alloca i64
          %frame.top = getelementptr i8, ptr %frame, i64 40
          %slot = getelementptr i8, ptr %frame.top, i64 -8
          store i64 %arg0, ptr %slot
          %v = load i64, ptr %slot
          store i64 %v, ptr %local_8
          %ret = call i64 @F_00401020(i64 %v, i64 2)
          ret i64 %ret
        }",
        CALLEE,
    ]);
    assert_eq!(check_in(&module, &module.functions[0], Level::Abi), Ok(2));
    assert_eq!(check_in(&module, &module.functions[1], Level::Abi), Ok(0));
}

#[test]
fn raised_abi_state_parameter() {
    let violation = refused_at(
        "define void @F_00401000(ptr %state) {
        entry:
          ret void
        }",
        Level::Abi,
    );
    assert_eq!((violation.rule, violation.block.as_str()), (Rule::State, "entry"), "{violation}");
}

#[test]
fn raised_abi_call_against_the_signature() {
    let module = module(&[
        "define i64 @F_00401000(i64 %arg0) {
        entry:
          br label %b_401008
        b_401008:
          %ret = call i64 @F_00401020(i64 %arg0)
          ret i64 %ret
        }",
        CALLEE,
    ]);
    let violation = check_in(&module, &module.functions[0], Level::Abi).expect_err("the call passes one argument of two");
    assert_eq!((violation.rule, violation.block.as_str()), (Rule::Signature, "b_401008"), "{violation}");
    assert!(violation.what.contains("@F_00401020"), "{violation}");
}

#[test]
fn raised_abi_access_outside_the_frame() {
    let violation = refused_at(
        "define void @F_00401000(i64 %arg0) {
        entry:
          %frame = alloca [64 x i8]
          %slot = getelementptr i8, ptr %frame, i64 60
          store i64 %arg0, ptr %slot
          ret void
        }",
        Level::Abi,
    );
    assert_eq!((violation.rule, violation.block.as_str()), (Rule::Stack, "entry"), "{violation}");
    assert!(violation.what.contains("offset 60"), "{violation}");
}
