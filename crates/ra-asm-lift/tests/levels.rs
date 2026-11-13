//! The invariants of the raise levels (`raise::check`): what the lift writes passes its level's
//! checker, and a function broken in one way, written as text, fails with that violation and the
//! block it is in.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ra_asm_ir::{BlockId, Callee, FnType, Function, FunctionBuilder, Operand, Terminator, Type, ValueId};
use ra_asm_lift::raise::Level;
use ra_asm_lift::raise::check::{Rule, Violation, check};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin").join(name)
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

/// A machine-level function from its text, in the printer's spelling: one `define`, labelled
/// blocks, and `getelementptr`, `load`, `store`, `call`, `ret void`, `br label` and `unreachable`
/// with local, integer and `undef` operands. A block the text does not end stays as the builder
/// makes one.
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
                b.terminate(at, Terminator::Ret(None));
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
    check(&function(text), Level::Machine).expect_err("the function breaks the machine level")
}

#[test]
fn machine_lifted_fixtures_pass() {
    for binary in ["hello64.exe", "hello32.exe"] {
        let image = ra_asm_pe::load(&fixture(binary)).unwrap();
        let analysis = ra_asm_analysis::analyze(&image);
        let mut lifted = ra_asm_lift::lift(&image, &analysis);
        let options = lifted.raise_options(Level::Machine);
        let _signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
        assert!(!lifted.module.functions.is_empty(), "{binary}: nothing was lifted");
        for f in &lifted.module.functions {
            if let Err(violation) = check(f, Level::Machine) {
                panic!("{binary}: {}: {violation}", f.name);
            }
        }
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
    assert_eq!(check(&f, Level::Machine), Ok(()));
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
