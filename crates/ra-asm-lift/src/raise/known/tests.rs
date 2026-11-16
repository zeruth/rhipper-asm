use ra_asm_ir::Linkage;

use super::*;

fn global(name: &str, ty: Type, init: Const, constant: bool) -> Global {
    Global { name: name.to_owned(), ty, init: Some(init), constant, linkage: Linkage::Internal, align: 1, address: None }
}

/// `.rdata` as the lift writes it: byte runs between the relocated words.
fn section() -> Global {
    let fields = vec![Type::array(5, Type::I8), Type::I64, Type::array(3, Type::I8), Type::I64, Type::array(7, Type::I8)];
    let items = vec![
        Const::Bytes((1..=5).collect()),
        Const::Address { global: "sec_data".to_owned(), offset: 0 },
        Const::Bytes(vec![0x10, 0x11, 0x12]),
        Const::Int(0x1234),
        Const::Bytes((0x20..0x27).collect()),
    ];
    global("sec_rdata", Type::Packed(fields), Const::Struct(items), true)
}

/// The bytes at `offset` as a walk of the fields finds them.
fn walked(global: &Global, offset: u64, size: u64) -> Option<&[u8]> {
    let (Type::Packed(fields), Some(Const::Struct(items))) = (&global.ty, &global.init) else { return None };
    let mut at = 0u64;
    for (field, item) in fields.iter().zip(items) {
        let field_size = field.size(8)?;
        if offset < at + field_size {
            return match item {
                Const::Bytes(bytes) => run_of(bytes, offset - at, size),
                _ => None,
            };
        }
        at += field_size;
    }
    None
}

#[test]
fn the_index_finds_the_bytes_a_walk_of_the_fields_finds() {
    let globals = vec![section()];
    let constants = Constants::of(&globals);
    assert!(constants.indexes(&globals));
    let (found, ends) = constants.get(&globals, "sec_rdata").expect("the section is indexed");
    for offset in 0u8..40 {
        for size in [1, 2, 4, 8] {
            assert_eq!(
                bytes_of(found, ends, i64::from(offset), size),
                walked(&globals[0], u64::from(offset), size),
                "{size} bytes at {offset}"
            );
        }
    }
    assert_eq!(bytes_of(found, ends, 13, 2), Some(&[0x10, 0x11][..]));
    assert_eq!(bytes_of(found, ends, 5, 1), None, "a relocated word is not bytes");
    assert_eq!(bytes_of(found, ends, -1, 1), None);
}

#[test]
fn the_index_holds_the_first_read_only_global_of_a_name() {
    let globals = vec![
        global("g", Type::array(2, Type::I8), Const::Bytes(vec![1, 2]), false),
        global("g", Type::array(2, Type::I8), Const::Bytes(vec![3, 4]), true),
        global("g", Type::array(2, Type::I8), Const::Bytes(vec![5, 6]), true),
    ];
    let constants = Constants::of(&globals);
    let (found, ends) = constants.get(&globals, "g").expect("a read-only `g`");
    assert_eq!(bytes_of(found, ends, 0, 2), Some(&[3, 4][..]));
    assert!(constants.get(&globals, "h").is_none());
    assert!(!constants.indexes(&globals[1..]));
}
