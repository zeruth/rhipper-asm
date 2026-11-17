//! The parser against the printer: a module is printed, read back and compared with what was
//! printed, and a text the parser does not read is refused at its line and token.

use ra_asm_ir::parse::parse;
use ra_asm_ir::{
    AtomicOp, BinOp, BlockId, CallConv, Callee, Cast, Const, Declaration, FCond, FnType, Function, FunctionBuilder, Global, ICond, Inst,
    Linkage, Module, Operand, Terminator, Type,
};

/// Prints the module, parses the text and holds the result to the module and to its text.
fn round_trip(module: &Module) {
    let text = module.to_ll();
    let parsed = parse(&text).unwrap_or_else(|e| panic!("{e}\n{text}"));
    assert_eq!(parsed.name, module.name, "{text}");
    assert_eq!(parsed.triple, module.triple, "{text}");
    assert_eq!(parsed.data_layout, module.data_layout, "{text}");
    assert_eq!(parsed.types, module.types, "{text}");
    assert_eq!(parsed.globals, module.globals, "{text}");
    assert_eq!(parsed.declarations, module.declarations, "{text}");
    assert_eq!(parsed.functions, module.functions, "{text}");
    assert_eq!(parsed.to_ll(), text);
}

fn global(name: &str, ty: Type, init: Const) -> Global {
    Global { name: name.to_owned(), ty, init: Some(init), constant: false, linkage: Linkage::Internal, align: 0, address: None }
}

fn declaration(name: &str, ret: Type, params: Vec<Type>) -> Declaration {
    Declaration { name: name.to_owned(), ty: FnType::new(ret, params), conv: CallConv::C, attrs: Vec::new() }
}

/// The line and the token of the refusal of `text`.
fn refused(text: &str) -> (usize, String) {
    let error = parse(text).expect_err(text);
    assert!(error.to_string().starts_with(&format!("line {}: `{}`: ", error.line, error.token)), "{error}");
    (error.line, error.token)
}

mod headers {
    use super::*;

    #[test]
    fn name_layout_and_triple() {
        round_trip(&Module::x86_64_windows("demo.exe"));
        round_trip(&Module::aarch64_linux("libdemo.so"));
        // A module without a target prints neither line, and a path keeps its backslashes.
        round_trip(&Module { name: r"C:\games\demo one.exe".to_owned(), ..Module::default() });
    }

    #[test]
    fn named_struct_types() {
        let mut module = Module::x86_64_windows("types");
        module.types = vec![
            ("empty".to_owned(), Vec::new()),
            ("pair".to_owned(), vec![Type::I32, Type::Ptr]),
            ("scalars".to_owned(), vec![Type::I1, Type::I8, Type::I128, Type::Half, Type::Float, Type::Double, Type::Fp80]),
            (
                "nested".to_owned(),
                vec![
                    Type::Named("pair".to_owned()),
                    Type::array(4, Type::vector(4, Type::Float)),
                    Type::Struct(vec![Type::I16, Type::Struct(Vec::new())]),
                    Type::Packed(vec![Type::I8, Type::I64]),
                ],
            ),
            (
                "callbacks".to_owned(),
                vec![
                    Type::Func(Box::new(FnType::new(Type::Void, Vec::new()))),
                    Type::Func(Box::new(FnType { ret: Type::I32, params: vec![Type::Ptr], varargs: true })),
                    Type::Func(Box::new(FnType { ret: Type::Ptr, params: Vec::new(), varargs: true })),
                ],
            ),
        ];
        round_trip(&module);
    }

    #[test]
    fn scalar_globals() {
        let mut module = Module::x86_64_windows("scalars");
        module.globals = vec![
            global("counter", Type::I32, Const::Int(7)),
            global("negative", Type::I64, Const::Int(-42)),
            global("widest", Type::I128, Const::Int(i128::MIN)),
            Global { constant: true, align: 16, ..global("table.size", Type::I16, Const::Int(512)) },
            Global { linkage: Linkage::External, address: Some(0x1_4000_3000), ..global("exported", Type::I8, Const::Int(1)) },
            Global { align: 4, address: Some(0x40_2000), ..global("placed", Type::I32, Const::Zero) },
            Global { init: None, linkage: Linkage::External, ..global("elsewhere", Type::Ptr, Const::Null) },
            Global { init: None, linkage: Linkage::External, align: 8, address: Some(0x10), ..global("aligned", Type::I64, Const::Null) },
            global("nothing", Type::Ptr, Const::Null),
        ];
        round_trip(&module);
    }

    #[test]
    fn float_globals() {
        let mut module = Module::x86_64_windows("floats");
        module.globals = vec![
            global("d", Type::Double, Const::Float(1.5)),
            global("tenth", Type::Double, Const::Float(0.1)),
            global("huge", Type::Double, Const::Float(f64::INFINITY)),
            global("f", Type::Float, Const::Float(0.25)),
            global("f.negative", Type::Float, Const::Float(-3.0)),
            global("h", Type::Half, Const::Float(1.0)),
            global("h.negative", Type::Half, Const::Float(-2.5)),
            global("h.small", Type::Half, Const::Float(2f64.powi(-20))),
            global("x", Type::Fp80, Const::Float(3.0)),
        ];
        round_trip(&module);
    }

    #[test]
    fn byte_strings() {
        let every_byte: Vec<u8> = (0..=255).collect();
        let mut module = Module::x86_64_windows("bytes");
        module.globals = vec![
            global("hello", Type::array(6, Type::I8), Const::Bytes(b"hello\0".to_vec())),
            global("quoted", Type::array(14, Type::I8), Const::Bytes(br#"say "hi" \ ; c"#.to_vec())),
            global("every", Type::array(256, Type::I8), Const::Bytes(every_byte)),
            global("none", Type::array(0, Type::I8), Const::Bytes(Vec::new())),
        ];
        round_trip(&module);
    }

    #[test]
    fn addresses_of_globals() {
        let address = |offset| Const::Address { global: "image".to_owned(), offset };
        let mut module = Module::x86_64_windows("addresses");
        module.globals = vec![
            global("image", Type::array(64, Type::I8), Const::Zero),
            global("whole", Type::Ptr, Const::Global("image".to_owned())),
            global("inside", Type::Ptr, address(12)),
            global("before", Type::Ptr, address(-4)),
            global("rva", Type::I32, address(0)),
            global("rva.inside", Type::I32, address(40)),
            global("va", Type::I64, address(8)),
        ];
        round_trip(&module);
    }

    #[test]
    fn aggregate_initializers() {
        let entry = Type::Struct(vec![Type::I32, Type::Ptr]);
        let header = Type::Packed(vec![Type::I8, Type::I32, Type::array(2, Type::I16)]);
        let mut module = Module::x86_64_windows("aggregates");
        module.globals = vec![
            global("target", Type::I32, Const::Int(0)),
            global("numbers", Type::array(3, Type::I32), Const::Array(vec![Const::Int(1), Const::Int(-2), Const::Int(3)])),
            global("empty", Type::array(0, Type::I64), Const::Array(Vec::new())),
            global(
                "grid",
                Type::array(2, Type::array(2, Type::Double)),
                Const::Array(vec![Const::Array(vec![Const::Float(1.0), Const::Float(2.0)]), Const::Zero]),
            ),
            global("entry", entry.clone(), Const::Struct(vec![Const::Int(5), Const::Global("target".to_owned())])),
            global(
                "entries",
                Type::array(3, entry),
                Const::Array(vec![
                    Const::Struct(vec![Const::Int(1), Const::Null]),
                    Const::Zero,
                    Const::Struct(vec![Const::Int(3), Const::Address { global: "numbers".to_owned(), offset: 4 }]),
                ]),
            ),
            global("header", header, Const::Struct(vec![Const::Int(1), Const::Int(2), Const::Array(vec![Const::Int(3), Const::Int(4)])])),
            global("unit", Type::Struct(Vec::new()), Const::Struct(Vec::new())),
            global(
                "vtable",
                Type::Struct(vec![Type::array(2, Type::Ptr), Type::array(4, Type::I8)]),
                Const::Struct(vec![Const::Array(vec![Const::Global("target".to_owned()), Const::Null]), Const::Bytes(b"ab\"\0".to_vec())]),
            ),
        ];
        round_trip(&module);
    }

    #[test]
    fn quoted_names() {
        let mut module = Module::x86_64_windows("names");
        module.globals = vec![
            global("??_7Widget@@6B@", Type::Ptr, Const::Null),
            global("two words", Type::I32, Const::Int(1)),
            global("a \"quote\" and a \\ slash", Type::I32, Const::Int(2)),
            global("9lives", Type::I32, Const::Int(3)),
            global("données", Type::I32, Const::Int(4)),
            global("bare.$name-1", Type::Ptr, Const::Global("??_7Widget@@6B@".to_owned())),
            global("far", Type::I64, Const::Address { global: "two words".to_owned(), offset: 2 }),
        ];
        module.declarations = vec![
            declaration("??2@YAPEAX_K@Z", Type::Ptr, vec![Type::I64]),
            declaration("operator delete(void *)", Type::Void, vec![Type::Ptr]),
        ];
        round_trip(&module);
    }

    #[test]
    fn declarations_and_attributes() {
        let attrs = |list: &[&str]| list.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>();
        let mut module = Module::x86_64_windows("declarations");
        module.types = vec![("pair".to_owned(), vec![Type::I32, Type::I32])];
        module.declarations = vec![
            declaration("ra_tick", Type::Void, Vec::new()),
            declaration("memcpy", Type::Ptr, vec![Type::Ptr, Type::Ptr, Type::I64]),
            Declaration {
                ty: FnType { ret: Type::I32, params: vec![Type::Ptr], varargs: true },
                ..declaration("printf", Type::Void, Vec::new())
            },
            Declaration {
                ty: FnType { ret: Type::I32, params: Vec::new(), varargs: true },
                ..declaration("old_style", Type::Void, Vec::new())
            },
            Declaration { attrs: attrs(&["noreturn"]), ..declaration("ra_trap", Type::Void, vec![Type::I64]) },
            Declaration {
                attrs: attrs(&["nounwind", "memory(none)", "\"frame-pointer\"=\"all\"", "#0"]),
                ..declaration("sqrt", Type::Double, vec![Type::Double])
            },
            Declaration { conv: CallConv::Win64, ..declaration("CreateFileW", Type::Ptr, vec![Type::Ptr, Type::I32]) },
            Declaration { conv: CallConv::X86Stdcall, attrs: attrs(&["nounwind"]), ..declaration("Sleep", Type::Void, vec![Type::I32]) },
            Declaration { conv: CallConv::X86Fastcall, ..declaration("fast", Type::I32, vec![Type::I32, Type::I32]) },
            Declaration { conv: CallConv::X86Thiscall, ..declaration("method", Type::Named("pair".to_owned()), vec![Type::Ptr]) },
            declaration("wide", Type::vector(4, Type::Float), vec![Type::vector(2, Type::Double), Type::array(2, Type::I8)]),
            declaration("fields", Type::Struct(vec![Type::I64, Type::I64]), vec![Type::Packed(vec![Type::I8, Type::Ptr])]),
        ];
        round_trip(&module);
    }

    #[test]
    fn a_whole_header() {
        let mut module = Module::x86_64_linux("whole");
        module.types =
            vec![("node".to_owned(), vec![Type::Ptr, Type::I32]), ("list".to_owned(), vec![Type::Named("node".to_owned()), Type::I64])];
        module.globals = vec![
            Global { address: Some(0x60_1000), ..global("head", Type::Struct(vec![Type::Ptr, Type::I32]), Const::Zero) },
            Global {
                constant: true,
                align: 1,
                address: Some(0x40_0800),
                ..global("format", Type::array(4, Type::I8), Const::Bytes(b"%d\n\0".to_vec()))
            },
            Global { init: None, linkage: Linkage::External, ..global("stdout", Type::Ptr, Const::Null) },
            global("cursor", Type::Ptr, Const::Address { global: "format".to_owned(), offset: 2 }),
        ];
        module.declarations = vec![
            Declaration {
                ty: FnType { ret: Type::I32, params: vec![Type::Ptr], varargs: true },
                ..declaration("printf", Type::Void, Vec::new())
            },
            Declaration { attrs: vec!["noreturn".to_owned()], ..declaration("exit", Type::Void, vec![Type::I32]) },
        ];
        round_trip(&module);
    }
}

mod refusals {
    use super::*;

    #[test]
    fn a_character_outside_the_text() {
        assert_eq!(refused("source_filename = \"m\"\n\n@g = global i32 ?\n"), (3, "?".to_owned()));
    }

    #[test]
    fn a_function_without_a_label() {
        assert_eq!(refused("declare void @g()\ndefine void @f() {\n  ret void\n}\n"), (3, "ret".to_owned()));
    }

    #[test]
    fn an_element_of_another_type() {
        assert_eq!(refused("@g = global [2 x i32] [i32 1, i64 2]\n"), (1, "i64".to_owned()));
        assert_eq!(refused("@a = global i8 0\n@g = global i32 ptrtoint (ptr @a to i64)\n"), (2, "i64".to_owned()));
    }

    #[test]
    fn a_float_of_the_wrong_format() {
        assert_eq!(refused("@g = global half 0x3FF0000000000000\n"), (1, "0x3FF0000000000000".to_owned()));
        assert_eq!(refused("@g = global i32 0x3FF0000000000000\n"), (1, "0x3FF0000000000000".to_owned()));
    }

    #[test]
    fn a_string_that_does_not_close() {
        assert_eq!(refused("\n@g = global [1 x i8] c\"a\n"), (2, "\"".to_owned()));
    }

    #[test]
    fn an_unknown_type_and_a_missing_one() {
        assert_eq!(refused("%t = type { i32, quad }\n"), (1, "quad".to_owned()));
        assert_eq!(refused("@g = global\n"), (1, "end of file".to_owned()));
        assert_eq!(refused("@g = global i32 1, align 4 5\n"), (1, "5".to_owned()));
    }
}

mod bodies {
    use super::*;

    fn module_of(functions: Vec<Function>) -> Module {
        let mut module = Module::x86_64_windows("bodies");
        module.functions = functions;
        module
    }

    fn void(params: Vec<Type>) -> FnType {
        FnType::new(Type::Void, params)
    }

    /// A function of one block, `entry`, that `body` fills; the block returns nothing unless
    /// `body` ends it.
    fn single(name: &str, ty: FnType, params: &[&str], body: impl FnOnce(&mut FunctionBuilder, BlockId)) -> Function {
        let mut b = FunctionBuilder::new(name, ty, params);
        let entry = b.block("entry");
        body(&mut b, entry);
        if !b.has_terminator(entry) {
            b.terminate(entry, Terminator::Ret(None));
        }
        b.finish()
    }

    fn trip(function: Function) {
        round_trip(&module_of(vec![function]));
    }

    fn ret(ty: Type, value: impl Into<Operand>) -> Terminator {
        Terminator::Ret(Some((ty, value.into())))
    }

    #[test]
    fn a_return_and_nothing_else() {
        trip(single("f", void(Vec::new()), &[], |_, _| {}));
    }

    #[test]
    fn a_function_without_blocks() {
        trip(FunctionBuilder::new("empty", void(Vec::new()), &[]).finish());
    }

    #[test]
    fn parameters_returned() {
        trip(single("add", FnType::new(Type::I32, vec![Type::I32, Type::I32]), &["a", "b"], |b, e| {
            let (x, y) = (b.param(0), b.param(1));
            let sum = b.bin(e, BinOp::Add, Type::I32, x, y, "sum");
            b.terminate(e, ret(Type::I32, sum));
        }));
    }

    #[test]
    fn varargs_parameters() {
        let printf = FnType { ret: Type::I32, params: vec![Type::Ptr], varargs: true };
        trip(single("printf", printf, &["format"], |b, e| b.terminate(e, ret(Type::I32, 0i32))));
        let old = FnType { ret: Type::Void, params: Vec::new(), varargs: true };
        trip(single("old", old, &[], |_, _| {}));
    }

    #[test]
    fn linkage_convention_attributes_and_addresses() {
        let mut b = FunctionBuilder::new("F_401000", void(vec![Type::Ptr]), &["this"]);
        b.set_linkage(Linkage::Internal).set_conv(CallConv::X86Thiscall).add_attr("nounwind").add_attr("\"frame-pointer\"=\"all\"");
        b.set_address(0x40_1000);
        b.address = Some(0x40_1000);
        let entry = b.block("entry");
        let this = b.param(0);
        b.store(entry, Type::I32, 1i32, this);
        b.address = Some(0x40_1006);
        let exit = b.block("exit");
        b.terminate(entry, Terminator::Br(exit));
        b.address = None;
        b.load(exit, Type::I32, this, "v");
        b.terminate(exit, Terminator::Ret(None));
        let mut sleep = FunctionBuilder::new("Sleep", void(vec![Type::I32]), &["ms"]);
        sleep.set_conv(CallConv::X86Stdcall).add_attr("noreturn");
        let e = sleep.block("entry");
        sleep.terminate(e, Terminator::Ret(None));
        let mut create = FunctionBuilder::new("CreateFileW", FnType::new(Type::Ptr, vec![Type::Ptr]), &["name"]);
        create.set_conv(CallConv::Win64).set_linkage(Linkage::Internal);
        let e = create.block("entry");
        create.terminate(e, ret(Type::Ptr, Operand::Null));
        round_trip(&module_of(vec![b.finish(), sleep.finish(), create.finish()]));
    }

    #[test]
    fn alloca_load_and_store() {
        trip(single("locals", FnType::new(Type::I64, Vec::new()), &[], |b, e| {
            let slot = b.alloca(e, Type::I64, "slot");
            b.store(e, Type::I64, 5i64, slot);
            let bytes = b.alloca(e, Type::array(4, Type::I8), "bytes");
            b.push(e, Inst::Store { ty: Type::I8, value: 1i32.into(), ptr: bytes.into(), align: 1, volatile: true }, "");
            b.value(e, Inst::Load { ty: Type::I8, ptr: bytes.into(), align: 1, volatile: true }, "flag");
            let v = b.load(e, Type::I64, slot, "v");
            b.terminate(e, ret(Type::I64, v));
        }));
    }

    #[test]
    fn integer_operations() {
        trip(single("arith", FnType::new(Type::I64, vec![Type::I64]), &["a"], |b, e| {
            let ops = [
                BinOp::Add,
                BinOp::Sub,
                BinOp::Mul,
                BinOp::UDiv,
                BinOp::SDiv,
                BinOp::URem,
                BinOp::SRem,
                BinOp::Shl,
                BinOp::LShr,
                BinOp::AShr,
                BinOp::And,
                BinOp::Or,
                BinOp::Xor,
            ];
            let mut v: Operand = b.param(0).into();
            for op in ops {
                v = b.bin(e, op, Type::I64, v, 3i64, op.name()).into();
            }
            b.terminate(e, ret(Type::I64, v));
        }));
    }

    #[test]
    fn float_operations() {
        trip(single("farith", FnType::new(Type::Double, vec![Type::Double]), &["a"], |b, e| {
            let mut v: Operand = b.param(0).into();
            for op in [BinOp::FAdd, BinOp::FSub, BinOp::FMul, BinOp::FDiv, BinOp::FRem] {
                v = b.bin(e, op, Type::Double, v, Operand::Float(2.5), op.name()).into();
            }
            b.terminate(e, ret(Type::Double, v));
        }));
    }

    #[test]
    fn integer_comparisons() {
        trip(single("compare", void(vec![Type::I32, Type::I32]), &["a", "b"], |b, e| {
            let (x, y) = (b.param(0), b.param(1));
            let conds =
                [ICond::Eq, ICond::Ne, ICond::Ugt, ICond::Uge, ICond::Ult, ICond::Ule, ICond::Sgt, ICond::Sge, ICond::Slt, ICond::Sle];
            for cond in conds {
                b.icmp(e, cond, Type::I32, x, y, cond.name());
            }
        }));
    }

    #[test]
    fn float_comparisons() {
        trip(single("fcompare", void(vec![Type::Float]), &["a"], |b, e| {
            let a = b.param(0);
            let conds = [
                FCond::Oeq,
                FCond::Ogt,
                FCond::Oge,
                FCond::Olt,
                FCond::Ole,
                FCond::One,
                FCond::Ord,
                FCond::Ueq,
                FCond::Ugt,
                FCond::Uge,
                FCond::Ult,
                FCond::Ule,
                FCond::Une,
                FCond::Uno,
            ];
            for cond in conds {
                b.value(e, Inst::FCmp { cond, ty: Type::Float, lhs: a.into(), rhs: Operand::Float(0.5) }, cond.name());
            }
        }));
    }

    #[test]
    fn select_between_values() {
        trip(single("abs", FnType::new(Type::I32, vec![Type::I32]), &["a"], |b, e| {
            let a = b.param(0);
            let negative = b.icmp(e, ICond::Slt, Type::I32, a, 0i32, "negative");
            let neg = b.bin(e, BinOp::Sub, Type::I32, 0i32, a, "neg");
            let abs = b.select(e, negative, Type::I32, neg, a, "abs");
            b.select(e, Operand::TRUE, Type::Ptr, Operand::Null, Operand::Undef, "p");
            b.select(e, Operand::FALSE, Type::Double, Operand::Float(1.0), Operand::Float(-1.0), "d");
            b.terminate(e, ret(Type::I32, abs));
        }));
    }

    #[test]
    fn every_cast() {
        trip(single("casts", void(vec![Type::I64]), &["a"], |b, e| {
            let a: Operand = b.param(0).into();
            let casts = [
                (Cast::Trunc, Type::I64, a.clone(), Type::I32),
                (Cast::ZExt, Type::I32, 1i32.into(), Type::I64),
                (Cast::SExt, Type::I8, (-1i32).into(), Type::I32),
                (Cast::FPTrunc, Type::Double, Operand::Float(1.5), Type::Float),
                (Cast::FPExt, Type::Float, Operand::Float(0.25), Type::Double),
                (Cast::FPToUI, Type::Double, Operand::Float(3.0), Type::I32),
                (Cast::FPToSI, Type::Float, Operand::Float(-2.0), Type::I64),
                (Cast::UIToFP, Type::I32, 7i32.into(), Type::Double),
                (Cast::SIToFP, Type::I64, (-7i64).into(), Type::Float),
                (Cast::PtrToInt, Type::Ptr, Operand::Null, Type::I64),
                (Cast::IntToPtr, Type::I64, a, Type::Ptr),
                (Cast::Bitcast, Type::I32, 1i32.into(), Type::Float),
            ];
            for (cast, from, value, to) in casts {
                b.cast(e, cast, from, value, to, cast.name());
            }
        }));
    }

    #[test]
    fn element_pointers() {
        trip(single("gep", FnType::new(Type::Ptr, vec![Type::Ptr, Type::I64]), &["p", "i"], |b, e| {
            let (p, i) = (b.param(0), b.param(1));
            let byte = b.gep(e, Type::I8, p, vec![(Type::I64, 16i64.into())], "byte");
            b.gep(e, Type::I32, p, Vec::new(), "bare");
            let pair = Type::Struct(vec![Type::I32, Type::Ptr]);
            let indices = vec![(Type::I64, 0i64.into()), (Type::I32, 1i32.into())];
            let field = b.value(e, Inst::Gep { base_ty: pair, ptr: p.into(), indices, inbounds: true }, "field");
            b.gep(e, Type::array(4, Type::I32), byte, vec![(Type::I64, 0i64.into()), (Type::I64, i.into())], "cell");
            b.terminate(e, ret(Type::Ptr, field));
        }));
    }

    #[test]
    fn vector_elements() {
        let v4 = Type::vector(4, Type::I32);
        trip(single("lanes", FnType::new(v4.clone(), vec![v4.clone()]), &["v"], move |b, e| {
            let v = b.param(0);
            let lane = b.value(e, Inst::ExtractElement { ty: v4.clone(), vector: v.into(), index: 2i32.into() }, "lane");
            let floats = Type::vector(4, Type::Float);
            b.value(e, Inst::InsertElement { ty: floats, vector: Operand::Undef, element: Operand::Float(1.0), index: 0i32.into() }, "f");
            let g = b.value(e, Inst::InsertElement { ty: v4.clone(), vector: v.into(), element: lane.into(), index: 3i32.into() }, "g");
            b.terminate(e, ret(v4, g));
        }));
    }

    #[test]
    fn vector_shuffles() {
        let v4 = Type::vector(4, Type::I32);
        trip(single("shuffle", FnType::new(Type::vector(2, Type::I32), vec![v4.clone(), v4.clone()]), &["a", "b"], move |b, e| {
            let (x, y) = (b.param(0), b.param(1));
            b.value(e, Inst::ShuffleVector { ty: v4.clone(), a: x.into(), b: y.into(), mask: vec![0, 4, -1, 7] }, "mixed");
            b.value(e, Inst::ShuffleVector { ty: v4.clone(), a: x.into(), b: Operand::Undef, mask: vec![0, 0, 1, 1, 2, 2, 3, 3] }, "wide");
            let low = b.value(e, Inst::ShuffleVector { ty: v4, a: x.into(), b: y.into(), mask: vec![1, 2] }, "low");
            b.terminate(e, ret(Type::vector(2, Type::I32), low));
        }));
    }

    #[test]
    fn direct_calls() {
        let g = FnType::new(Type::I32, vec![Type::I32, Type::Ptr]);
        let f = single("f", FnType::new(Type::I32, Vec::new()), &[], |b, e| {
            b.call(e, void(Vec::new()), Callee::Direct("ra_tick".to_owned()), Vec::new(), "");
            let args = vec![(Type::I32, 1i32.into()), (Type::Ptr, Operand::Global("data".to_owned()))];
            let r = b.call(e, g.clone(), Callee::Direct("g".to_owned()), args, "r").unwrap();
            b.terminate(e, ret(Type::I32, r));
        });
        let mut module = module_of(vec![f]);
        module.globals = vec![global("data", Type::I32, Const::Int(0))];
        module.declarations =
            vec![declaration("ra_tick", Type::Void, Vec::new()), Declaration { ty: g, ..declaration("g", Type::Void, Vec::new()) }];
        round_trip(&module);
    }

    #[test]
    fn varargs_calls() {
        let printf = FnType { ret: Type::I32, params: vec![Type::Ptr], varargs: true };
        let old = FnType { ret: Type::Void, params: Vec::new(), varargs: true };
        let f = single("f", void(Vec::new()), &[], |b, e| {
            let args =
                vec![(Type::Ptr, Operand::Global("format".to_owned())), (Type::I32, 1i32.into()), (Type::Double, Operand::Float(2.5))];
            b.call(e, printf.clone(), Callee::Direct("printf".to_owned()), args, "n");
            b.call(e, old.clone(), Callee::Direct("old".to_owned()), Vec::new(), "");
        });
        let mut module = module_of(vec![f]);
        module.globals = vec![global("format", Type::array(3, Type::I8), Const::Bytes(b"%d\0".to_vec()))];
        module.declarations = vec![
            Declaration { ty: printf, ..declaration("printf", Type::Void, Vec::new()) },
            Declaration { ty: old, ..declaration("old", Type::Void, Vec::new()) },
        ];
        round_trip(&module);
    }

    #[test]
    fn indirect_and_tail_calls() {
        trip(single("through", FnType::new(Type::I32, vec![Type::Ptr]), &["fp"], |b, e| {
            let fp = b.param(0);
            let callee = Callee::Indirect(fp.into());
            let r = b.call(e, FnType::new(Type::I32, vec![Type::I32]), callee.clone(), vec![(Type::I32, 4i32.into())], "r").unwrap();
            let exit = Inst::Call {
                fn_ty: void(vec![Type::I32]),
                callee: Callee::Direct("exit".to_owned()),
                args: vec![(Type::I32, r.into())],
                tail: true,
            };
            b.push(e, exit, "");
            let t = b.value(
                e,
                Inst::Call { fn_ty: FnType::new(Type::I32, vec![Type::I32]), callee, args: vec![(Type::I32, r.into())], tail: true },
                "t",
            );
            b.terminate(e, ret(Type::I32, t));
        }));
    }

    #[test]
    fn freeze() {
        trip(single("frozen", FnType::new(Type::I32, vec![Type::I32]), &["a"], |b, e| {
            let a = b.param(0);
            let f = b.value(e, Inst::Freeze { ty: Type::I32, value: a.into() }, "f");
            b.value(e, Inst::Freeze { ty: Type::vector(2, Type::Double), value: Operand::Undef }, "v");
            b.terminate(e, ret(Type::I32, f));
        }));
    }

    #[test]
    fn atomics() {
        trip(single("locked", FnType::new(Type::I32, vec![Type::Ptr, Type::I32]), &["p", "v"], |b, e| {
            let (p, v) = (b.param(0), b.param(1));
            b.value(e, Inst::AtomicRmw { op: AtomicOp::Xchg, ty: Type::I32, ptr: p.into(), value: v.into() }, "old");
            b.value(e, Inst::AtomicRmw { op: AtomicOp::Sub, ty: Type::I8, ptr: p.into(), value: 1i32.into() }, "byte");
            let was = b.value(e, Inst::CmpXchg { ty: Type::I32, ptr: p.into(), expected: 0i32.into(), new: v.into() }, "was");
            b.terminate(e, ret(Type::I32, was));
        }));
    }

    #[test]
    fn phis_across_a_loop() {
        let mut b = FunctionBuilder::new("count", FnType::new(Type::I32, vec![Type::I32]), &["n"]);
        let n = b.param(0);
        let (entry, lp, exit) = (b.block("entry"), b.block("loop"), b.block("exit"));
        b.terminate(entry, Terminator::Br(lp));
        let i = b.value(lp, Inst::Phi { ty: Type::I32, incoming: vec![(0i32.into(), entry)] }, "i");
        let x = b.value(lp, Inst::Phi { ty: Type::Double, incoming: vec![(Operand::Float(1.0), entry)] }, "x");
        let next = b.bin(lp, BinOp::Add, Type::I32, i, 1i32, "next");
        let twice = b.bin(lp, BinOp::FAdd, Type::Double, x, x, "twice");
        let done = b.icmp(lp, ICond::Eq, Type::I32, next, n, "done");
        b.terminate(lp, Terminator::CondBr { cond: done.into(), then: exit, els: lp });
        b.terminate(exit, ret(Type::I32, i));
        b.add_incoming(i, lp, next);
        b.add_incoming(x, lp, twice);
        trip(b.finish());
    }

    #[test]
    fn conditional_branches() {
        let mut b = FunctionBuilder::new("sign", FnType::new(Type::I32, vec![Type::I32]), &["a"]);
        let a = b.param(0);
        let (entry, then, els) = (b.block("entry"), b.block("then"), b.block("else"));
        let negative = b.icmp(entry, ICond::Slt, Type::I32, a, 0i32, "negative");
        b.terminate(entry, Terminator::CondBr { cond: negative.into(), then, els });
        b.terminate(then, ret(Type::I32, -1i32));
        let join = b.block("join");
        b.terminate(els, Terminator::Br(join));
        b.terminate(join, ret(Type::I32, 1i32));
        trip(b.finish());
    }

    #[test]
    fn switches() {
        let mut b = FunctionBuilder::new("pick", FnType::new(Type::I32, vec![Type::I32, Type::I64]), &["a", "w"]);
        let (a, w) = (b.param(0), b.param(1));
        let (entry, one, two, default, none) = (b.block("entry"), b.block("one"), b.block("two"), b.block("default"), b.block("none"));
        b.terminate(
            entry,
            Terminator::Switch { ty: Type::I32, value: a.into(), default, cases: vec![(1, one), (-2, two), (i128::from(i32::MAX), one)] },
        );
        b.terminate(one, ret(Type::I32, 1i32));
        b.terminate(two, ret(Type::I32, 2i32));
        b.terminate(default, Terminator::Switch { ty: Type::I64, value: w.into(), default: none, cases: Vec::new() });
        b.terminate(none, ret(Type::I32, 0i32));
        trip(b.finish());
    }

    #[test]
    fn unreachable_blocks() {
        let mut b = FunctionBuilder::new("stops", void(Vec::new()), &[]);
        let (entry, dead) = (b.block("entry"), b.block("dead"));
        b.terminate(entry, Terminator::Br(dead));
        b.call(dead, void(vec![Type::I64]), Callee::Direct("ra_trap".to_owned()), vec![(Type::I64, 0x40_1000i64.into())], "");
        b.terminate(dead, Terminator::Unreachable);
        trip(b.finish());
    }

    #[test]
    fn integer_operands() {
        trip(single("ints", void(vec![Type::I128, Type::I8, Type::I1]), &["w", "x", "c"], |b, e| {
            let (w, x, c) = (b.param(0), b.param(1), b.param(2));
            b.bin(e, BinOp::Add, Type::I128, w, i128::MAX, "max");
            b.bin(e, BinOp::Sub, Type::I128, w, i128::MIN, "min");
            b.bin(e, BinOp::Add, Type::I8, x, -128i32, "low");
            b.bin(e, BinOp::Mul, Type::I16, 32767i32, -1i32, "wide");
            b.bin(e, BinOp::And, Type::I64, -1i64, u64::MAX / 2, "mask");
            b.bin(e, BinOp::Xor, Type::I1, c, Operand::TRUE, "not");
            b.bin(e, BinOp::Or, Type::I1, c, Operand::FALSE, "same");
        }));
    }

    #[test]
    fn float_operands() {
        trip(single("floats", void(vec![Type::Half, Type::Float, Type::Double, Type::Fp80]), &["h", "f", "d", "x"], |b, e| {
            let (h, f, d, x) = (b.param(0), b.param(1), b.param(2), b.param(3));
            b.bin(e, BinOp::FAdd, Type::Half, h, Operand::Float(1.0), "h1");
            b.bin(e, BinOp::FMul, Type::Half, h, Operand::Float(-2.5), "h2");
            b.bin(e, BinOp::FMul, Type::Float, f, Operand::Float(-0.25), "f1");
            b.bin(e, BinOp::FDiv, Type::Double, d, Operand::Float(0.1), "d1");
            b.bin(e, BinOp::FAdd, Type::Double, d, Operand::Float(f64::INFINITY), "d2");
            b.bin(e, BinOp::FSub, Type::Double, d, Operand::Float(-0.0), "d3");
            b.bin(e, BinOp::FSub, Type::Fp80, x, Operand::Float(3.0), "x1");
        }));
    }

    #[test]
    fn global_operands() {
        let address = |offset| Operand::Address { global: "image".to_owned(), offset };
        let f = single("globals", void(Vec::new()), &[], |b, e| {
            let v = b.load(e, Type::I32, Operand::Global("table".to_owned()), "v");
            b.store(e, Type::I32, v, address(8));
            b.bin(e, BinOp::Add, Type::I32, address(0), 4i32, "rva");
            b.bin(e, BinOp::Add, Type::I64, address(12), 0i64, "va");
            b.gep(e, Type::I8, address(32), vec![(Type::I64, 1i64.into())], "p");
            b.cast(e, Cast::PtrToInt, Type::Ptr, address(-4), Type::I32, "before");
            b.store(e, Type::Ptr, Operand::Global("image".to_owned()), Operand::Global("table".to_owned()));
        });
        let mut module = module_of(vec![f]);
        module.globals = vec![global("image", Type::array(64, Type::I8), Const::Zero), global("table", Type::I32, Const::Int(0))];
        round_trip(&module);
    }

    #[test]
    fn null_undef_and_zero() {
        let v4 = Type::vector(4, Type::I32);
        trip(single("empties", FnType::new(Type::Ptr, vec![Type::Ptr, v4.clone()]), &["p", "v"], move |b, e| {
            let (p, v) = (b.param(0), b.param(1));
            let is_null = b.icmp(e, ICond::Eq, Type::Ptr, p, Operand::Null, "is_null");
            b.select(e, is_null, Type::I32, Operand::Undef, 1i32, "s");
            b.bin(e, BinOp::Add, v4.clone(), v, Operand::Zero, "same");
            b.store(e, v4, Operand::Zero, p);
            b.store(e, Type::Ptr, Operand::Null, p);
            b.store(e, Type::Struct(vec![Type::I64, Type::Ptr]), Operand::Undef, p);
            b.terminate(e, ret(Type::Ptr, Operand::Undef));
        }));
    }

    #[test]
    fn vector_splats() {
        let v4 = Type::vector(4, Type::I32);
        let d2 = Type::vector(2, Type::Double);
        trip(single("splats", FnType::new(Type::vector(4, Type::I1), vec![v4.clone(), d2.clone()]), &["v", "d"], move |b, e| {
            let (v, d) = (b.param(0), b.param(1));
            b.bin(e, BinOp::Add, v4.clone(), v, 7i32, "sevens");
            b.bin(e, BinOp::Mul, v4.clone(), v, -1i32, "negated");
            b.bin(e, BinOp::FMul, d2.clone(), d, Operand::Float(1.5), "scaled");
            b.value(e, Inst::FCmp { cond: FCond::Olt, ty: d2, lhs: d.into(), rhs: Operand::Float(0.0) }, "below");
            let small = b.icmp(e, ICond::Slt, v4.clone(), v, 3i32, "small");
            b.terminate(e, ret(Type::vector(4, Type::I1), small));
        }));
    }

    #[test]
    fn quoted_names() {
        let mut f = single("operator new(unsigned long long)", FnType::new(Type::Ptr, vec![Type::I64]), &["size"], |b, e| {
            let size = b.param(0);
            let x = b.bin(e, BinOp::Add, Type::I64, size, 1i64, "x");
            let p = b.cast(e, Cast::IntToPtr, Type::I64, x, Type::Ptr, "p");
            b.terminate(e, ret(Type::Ptr, p));
        });
        f.value_names[0] = "the size".to_owned();
        f.value_names[1] = "9x".to_owned();
        f.value_names[2] = "say \"hi\" \\ now".to_owned();
        f.blocks[0].name = "1st block".to_owned();
        trip(f);
    }

    #[test]
    fn named_types() {
        let pair = Type::Named("pair".to_owned());
        let f = single("pairs", void(vec![Type::Ptr]), &["p"], |b, e| {
            let p = b.param(0);
            let slot = b.alloca(e, pair.clone(), "slot");
            b.store(e, pair.clone(), Operand::Undef, slot);
            let v = b.load(e, pair.clone(), p, "v");
            b.store(e, pair.clone(), v, slot);
            b.gep(e, pair.clone(), slot, vec![(Type::I64, 0i64.into()), (Type::I32, 1i32.into())], "second");
            b.call(e, FnType::new(pair.clone(), Vec::new()), Callee::Direct("make".to_owned()), Vec::new(), "made");
        });
        let mut module = module_of(vec![f]);
        module.types = vec![("pair".to_owned(), vec![Type::I32, Type::Ptr])];
        module.declarations = vec![declaration("make", pair, Vec::new())];
        round_trip(&module);
    }

    #[test]
    fn suffixed_names() {
        let mut b = FunctionBuilder::new("f", FnType::new(Type::I32, vec![Type::I32]), &["x"]);
        let x = b.param(0);
        let (entry, lp, again) = (b.block("entry"), b.block("loop"), b.block("loop"));
        let x1 = b.bin(entry, BinOp::Add, Type::I32, x, 1i32, "x");
        let x2 = b.bin(entry, BinOp::Add, Type::I32, x1, 1i32, "x");
        b.terminate(entry, Terminator::Br(lp));
        b.terminate(lp, Terminator::Br(again));
        b.terminate(again, ret(Type::I32, x2));
        assert_eq!(b.function().value_names, ["x", "x.1", "x.2"]);
        assert_eq!(b.function().blocks[2].name, "loop.1");
        trip(b.finish());
    }

    #[test]
    fn a_whole_module() {
        let helper = single("helper", FnType::new(Type::I32, vec![Type::I32]), &["a"], |b, e| {
            let a = b.param(0);
            let doubled = b.bin(e, BinOp::Shl, Type::I32, a, 1i32, "doubled");
            b.terminate(e, ret(Type::I32, doubled));
        });
        let mut b = FunctionBuilder::new("main", FnType::new(Type::I32, Vec::new()), &[]);
        b.set_address(0x40_1000);
        let (entry, exit) = (b.block("entry"), b.block("exit"));
        let count = b.load(entry, Type::I32, Operand::Global("count".to_owned()), "count");
        let r = b
            .call(entry, FnType::new(Type::I32, vec![Type::I32]), Callee::Direct("helper".to_owned()), vec![(Type::I32, count.into())], "r")
            .unwrap();
        b.store(entry, Type::I32, r, Operand::Global("count".to_owned()));
        b.terminate(entry, Terminator::Br(exit));
        b.call(exit, void(vec![Type::I32]), Callee::Direct("exit".to_owned()), vec![(Type::I32, r.into())], "");
        b.terminate(exit, Terminator::Unreachable);
        let mut module = module_of(vec![helper, b.finish()]);
        module.globals = vec![Global { address: Some(0x40_3000), ..global("count", Type::I32, Const::Int(1)) }];
        module.declarations = vec![Declaration { attrs: vec!["noreturn".to_owned()], ..declaration("exit", Type::Void, vec![Type::I32]) }];
        round_trip(&module);
    }

    #[test]
    fn refuses_an_unknown_value() {
        assert_eq!(refused("define i32 @f() {\nentry:\n  ret i32 %x\n}\n"), (3, "%x".to_owned()));
    }

    #[test]
    fn refuses_an_unknown_block() {
        assert_eq!(refused("define void @f() {\nentry:\n  br label %nowhere\n}\n"), (3, "%nowhere".to_owned()));
    }

    #[test]
    fn refuses_an_unknown_instruction() {
        assert_eq!(refused("define void @f() {\nentry:\n  %x = frob i32 1\n  ret void\n}\n"), (3, "frob".to_owned()));
    }

    #[test]
    fn refuses_a_body_that_does_not_close() {
        assert_eq!(refused("define void @f() {\nentry:\n  ret void\n"), (3, "end of file".to_owned()));
    }

    #[test]
    fn refuses_a_value_defined_twice() {
        assert_eq!(refused("define void @f(i32 %x) {\nentry:\n  %x = add i32 1, 2\n  ret void\n}\n"), (3, "%x".to_owned()));
    }

    #[test]
    fn refuses_a_result_of_a_terminator_or_a_store() {
        assert_eq!(refused("define void @f() {\nentry:\n  %x = ret void\n}\n"), (3, "ret".to_owned()));
        assert_eq!(refused("define void @f() {\nentry:\n  %x = store i32 1, ptr null, align 4\n  ret void\n}\n"), (3, "store".to_owned()));
    }

    #[test]
    fn refuses_uneven_lanes() {
        let text = "define void @f(<2 x i32> %v) {\nentry:\n  %x = add <2 x i32> %v, <i32 1, i32 2>\n  ret void\n}\n";
        assert_eq!(refused(text), (3, "<".to_owned()));
    }

    #[test]
    fn refuses_a_mask_of_another_count() {
        let text = "define void @f(<2 x i32> %v) {\nentry:\n  %x = shufflevector <2 x i32> %v, <2 x i32> %v, <3 x i32> <i32 0, i32 1>\n  ret void\n}\n";
        assert_eq!(refused(text), (3, "<".to_owned()));
    }
}
