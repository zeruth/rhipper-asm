//! The structure layouts that are in: the Windows API's structures at 32 and at 64 bits, read by
//! their tag or typedef name and through the functions whose parameters point to them.

use ra_asm_protos::{Kind, Place, callback, converted_fields, holds_size, layout, pointees, records, size_parameter};

fn at(offset: u32, size: u32) -> Place {
    Place { offset, size }
}

/// The structures rhtest's sys and win hand the host, at both widths as clang lays them out from
/// the SDK headers: their sizes, the fields that move, the union of `OVERLAPPED` and of
/// `SYSTEM_INFO` crossed by the members of one layout, the window procedure the host calls back
/// and the functions that point to each.
#[test]
fn sys_and_win_layouts() {
    let sizes = [
        ("OVERLAPPED", "_OVERLAPPED", (20, 4), (32, 8)),
        ("MEMORY_BASIC_INFORMATION", "_MEMORY_BASIC_INFORMATION", (28, 4), (48, 8)),
        ("SYSTEM_INFO", "_SYSTEM_INFO", (36, 4), (48, 8)),
        ("WNDCLASSA", "tagWNDCLASSA", (40, 4), (72, 8)),
        ("WSADATA", "WSAData", (400, 4), (408, 8)),
        ("CRITICAL_SECTION", "_RTL_CRITICAL_SECTION", (24, 4), (40, 8)),
    ];
    for (name, tag, at32, at64) in sizes {
        let record = layout(name).unwrap_or_else(|| panic!("{name} is in the table"));
        assert_eq!(record.name, tag, "{name}");
        assert_eq!(((record.size32, record.align32), (record.size64, record.align64)), (at32, at64), "{name}");
        assert!(!record.same, "{name} changes with the width");
        assert!(converted_fields(record).is_ok(), "{name} converts");
    }
    assert_eq!(layout("WNDCLASS"), layout("WNDCLASSA"));
    assert_eq!(layout("RTL_CRITICAL_SECTION"), layout("CRITICAL_SECTION"));

    let fields = |name: &str| {
        let record = layout(name).unwrap();
        record.fields.iter().map(|f| (f.name, f.kind, f.at32, f.at64)).collect::<Vec<_>>()
    };
    let overlapped = layout("OVERLAPPED").unwrap();
    assert!(overlapped.overlapping);
    assert_eq!(
        fields("OVERLAPPED"),
        [
            ("Internal", Kind::Unsigned, at(0, 4), at(0, 8)),
            ("InternalHigh", Kind::Unsigned, at(4, 4), at(8, 8)),
            ("Offset", Kind::Unsigned, at(8, 4), at(16, 4)),
            ("OffsetHigh", Kind::Unsigned, at(12, 4), at(20, 4)),
            ("Pointer", Kind::Pointer, at(8, 4), at(16, 8)),
            ("hEvent", Kind::Pointer, at(16, 4), at(24, 8)),
        ]
    );
    let crossed = |name: &str| converted_fields(layout(name).unwrap()).unwrap().iter().map(|f| f.name).collect::<Vec<_>>();
    assert_eq!(crossed("OVERLAPPED"), ["Internal", "InternalHigh", "Offset", "OffsetHigh", "hEvent"], "the union crosses by its offset");
    let system = layout("SYSTEM_INFO").unwrap();
    assert!(system.overlapping);
    assert_eq!(crossed("SYSTEM_INFO")[..2], ["dwOemId", "dwPageSize"], "the union crosses by its dword");
    let mask = system.field("dwActiveProcessorMask").unwrap();
    assert_eq!((mask.kind, mask.at32, mask.at64), (Kind::Unsigned, at(16, 4), at(24, 8)));

    let partition = layout("MEMORY_BASIC_INFORMATION").unwrap().field("PartitionId").unwrap();
    assert_eq!((partition.at32, partition.at64), (at(0, 0), at(20, 2)), "PartitionId is the host's alone");
    assert!(!crossed("MEMORY_BASIC_INFORMATION").contains(&"PartitionId"));
    let region = layout("MEMORY_BASIC_INFORMATION").unwrap().field("RegionSize").unwrap();
    assert_eq!((region.at32, region.at64), (at(12, 4), at(24, 8)));

    let wsa = layout("WSADATA").unwrap();
    let description = wsa.field("szDescription").unwrap();
    assert_eq!((description.kind, description.count, description.at32, description.at64), (Kind::Signed, 257, at(4, 257), at(16, 257)));
    let vendor = wsa.field("lpVendorInfo").unwrap();
    assert_eq!((vendor.at32, vendor.at64), (at(396, 4), at(8, 8)), "the 64-bit layout puts the short fields first");

    let section = layout("CRITICAL_SECTION").unwrap();
    let spin = section.field("SpinCount").unwrap();
    assert_eq!((spin.kind, spin.at32, spin.at64), (Kind::Unsigned, at(20, 4), at(32, 8)));
    let lock = section.field("LockCount").unwrap();
    assert_eq!((lock.kind, lock.at32, lock.at64), (Kind::Signed, at(4, 4), at(8, 4)));

    let class = layout("WNDCLASSA").unwrap();
    let procedure = class.field("lpfnWndProc").unwrap();
    assert_eq!((procedure.kind, procedure.at32, procedure.at64), (Kind::Pointer, at(4, 4), at(8, 8)));
    let wndproc = callback("tagWNDCLASSA", "lpfnWndProc").expect("the host calls the class's procedure back");
    assert_eq!(wndproc.name, "WNDPROC");
    assert_eq!(wndproc.result, (Kind::Signed, 4));
    assert_eq!(wndproc.params.len(), 4);
    assert_eq!(callback("tagWNDCLASSEXA", "lpfnWndProc"), Some(wndproc));
    assert!(callback("tagWNDCLASSA", "hInstance").is_none());
    assert!(holds_size("tagWNDCLASSEXA", "cbSize") && !holds_size("tagWNDCLASSA", "style"));
    assert_eq!(size_parameter("VirtualQuery"), Some((2, 1, true)));

    let names = |function: &str| pointees(function).into_iter().map(|(i, r)| (i, r.name)).collect::<Vec<_>>();
    assert_eq!(names("ReadFile"), [(4, "_OVERLAPPED")]);
    assert_eq!(names("WriteFile"), [(4, "_OVERLAPPED")]);
    assert_eq!(names("GetOverlappedResult"), [(1, "_OVERLAPPED")]);
    assert_eq!(names("VirtualQuery"), [(1, "_MEMORY_BASIC_INFORMATION")]);
    assert_eq!(names("GetSystemInfo"), [(0, "_SYSTEM_INFO")]);
    assert_eq!(names("RegisterClassA"), [(0, "tagWNDCLASSA")]);
    assert_eq!(names("GetClassInfoA"), [(2, "tagWNDCLASSA")]);
    assert_eq!(names("WSAStartup"), [(1, "WSAData")]);
    for function in ["InitializeCriticalSection", "EnterCriticalSection", "LeaveCriticalSection", "DeleteCriticalSection"] {
        assert_eq!(names(function), [(0, "_RTL_CRITICAL_SECTION")], "{function}");
    }
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
