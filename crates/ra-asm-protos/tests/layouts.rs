//! The structure layouts that are in: the Windows API's structures at 32 and at 64 bits, read by
//! their tag or typedef name and through the functions whose parameters point to them.

use ra_asm_protos::{Kind, Place, layout, pointees, records};

fn at(offset: u32, size: u32) -> Place {
    Place { offset, size }
}

/// A 32-bit program's `WNDCLASSEXA` is not the host's: its pointers are half as wide.
#[test]
fn wndclassexa_is_48_bytes_at_32_bits_and_80_at_64() {
    let class = layout("WNDCLASSEXA").expect("WNDCLASSEXA is in the table");
    assert_eq!((class.size32, class.align32), (48, 4));
    assert_eq!((class.size64, class.align64), (80, 8));
    assert!(!class.same && !class.overlapping);
    assert_eq!(class.fields.len(), 12);
    let procedure = class.field("lpfnWndProc").unwrap();
    assert_eq!((procedure.kind, procedure.at32, procedure.at64), (Kind::Pointer, at(8, 4), at(8, 8)));
    let extra = class.field("cbClsExtra").unwrap();
    assert_eq!((extra.kind, extra.at32, extra.at64), (Kind::Signed, at(12, 4), at(16, 4)));
    let small = class.field("hIconSm").unwrap();
    assert_eq!((small.at32, small.at64), (at(44, 4), at(72, 8)));
    assert_eq!(layout("WNDCLASSEX").map(|r| r.name), Some("tagWNDCLASSEXA"));
}

/// `MSG`'s fields at both widths, the nested `POINT`'s by their path.
#[test]
fn msg_fields_are_found_at_both_widths() {
    let msg = layout("MSG").expect("MSG is in the table");
    assert_eq!(layout("tagMSG"), Some(msg));
    assert_eq!((msg.size32, msg.size64), (28, 48));
    let expected = [
        ("hwnd", Kind::Pointer, at(0, 4), at(0, 8)),
        ("message", Kind::Unsigned, at(4, 4), at(8, 4)),
        ("wParam", Kind::Unsigned, at(8, 4), at(16, 8)),
        ("lParam", Kind::Signed, at(12, 4), at(24, 8)),
        ("time", Kind::Unsigned, at(16, 4), at(32, 4)),
        ("pt.x", Kind::Signed, at(20, 4), at(36, 4)),
        ("pt.y", Kind::Signed, at(24, 4), at(40, 4)),
    ];
    for (name, kind, at32, at64) in expected {
        let field = msg.field(name).unwrap_or_else(|| panic!("MSG has no {name}"));
        assert_eq!((field.kind, field.count, field.at32, field.at64), (kind, 1, at32, at64), "{name}");
    }
}

/// A structure without a pointer passes as it is, and the table says so.
#[test]
fn a_structure_with_no_pointer_has_equal_layouts_and_is_marked_so() {
    for name in ["RECT", "POINT"] {
        let record = layout(name).unwrap_or_else(|| panic!("{name} is in the table"));
        assert!(record.same, "{name} is not marked the same at both widths");
        assert_eq!((record.size32, record.align32), (record.size64, record.align64), "{name}");
        assert!(record.fields.iter().all(|f| f.kind != Kind::Pointer && f.at32 == f.at64), "{name}");
    }
    assert_eq!(layout("RECT").map(|r| r.size32), Some(16));
    assert!(layout("NO_SUCH_STRUCT").is_none());
}

/// The functions' parameters name the structures they point to.
#[test]
fn the_prototypes_point_to_their_structures() {
    let names = |function: &str| pointees(function).into_iter().map(|(i, r)| (i, r.name)).collect::<Vec<_>>();
    assert_eq!(names("RegisterClassExA"), [(0, "tagWNDCLASSEXA")]);
    assert_eq!(names("GetMessageA"), [(0, "tagMSG")]);
    assert_eq!(names("GetClientRect"), [(1, "tagRECT")]);
    assert!(pointees("GetTickCount").is_empty());
}

/// Every structure of the table holds together: its marks agree with its fields, which sit
/// inside it, and the record a field names is in the table.
#[test]
fn every_structure_agrees_with_its_marks() {
    assert!(records().windows(2).all(|w| w[0].name < w[1].name), "the table is not sorted");
    for record in records() {
        let same = record.size32 == record.size64 && record.align32 == record.align64 && record.fields.iter().all(|f| f.at32 == f.at64);
        assert_eq!(record.same, same, "{}", record.name);
        for field in record.fields {
            assert!(field.at32.offset + field.at32.size <= record.size32, "{}.{} at 32 bits", record.name, field.name);
            assert!(field.at64.offset + field.at64.size <= record.size64, "{}.{} at 64 bits", record.name, field.name);
            assert_eq!(field.kind == Kind::Record, !field.record.is_empty(), "{}.{}", record.name, field.name);
            if field.kind == Kind::Record {
                assert!(layout(field.record).is_some(), "{}.{} names {}", record.name, field.name, field.record);
            }
            if field.kind == Kind::Pointer {
                assert_eq!((field.at32.size, field.at64.size), (4 * field.count, 8 * field.count), "{}.{}", record.name, field.name);
            }
        }
    }
}
