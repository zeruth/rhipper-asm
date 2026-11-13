//! The parser against the printer: a module is printed, read back and compared with what was
//! printed, and a text the parser does not read is refused at its line and token.

use ra_asm_ir::parse::parse;
use ra_asm_ir::{CallConv, Const, Declaration, FnType, Global, Linkage, Module, Type};

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

    /// The line and the token of the refusal of `text`.
    fn refused(text: &str) -> (usize, String) {
        let error = parse(text).expect_err(text);
        assert!(error.to_string().starts_with(&format!("line {}: `{}`: ", error.line, error.token)), "{error}");
        (error.line, error.token)
    }

    #[test]
    fn a_character_outside_the_text() {
        assert_eq!(refused("source_filename = \"m\"\n\n@g = global i32 ?\n"), (3, "?".to_owned()));
    }

    #[test]
    fn a_function_with_a_body() {
        assert_eq!(refused("declare void @g()\ndefine void @f() {\n  ret void\n}\n"), (2, "define".to_owned()));
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
