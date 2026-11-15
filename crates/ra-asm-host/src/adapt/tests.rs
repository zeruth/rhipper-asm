use ra_asm_protos::{Field, Kind, Place, Record, layout};

use super::{adapter, generate};

const fn field(name: &'static str, kind: Kind, count: u32, at32: (u32, u32), at64: (u32, u32), record: &'static str) -> Field {
    Field { name, kind, count, at32: Place { offset: at32.0, size: at32.1 }, at64: Place { offset: at64.0, size: at64.1 }, record }
}

/// A structure of a pointer, a signed `LONG_PTR` and two `POINT`s, to exercise the recursion the
/// generated table's records do not have.
static NESTED: Record = Record {
    name: "NESTED",
    overlapping: false,
    same: false,
    size32: 24,
    align32: 4,
    size64: 32,
    align64: 8,
    fields: &[
        field("data", Kind::Pointer, 1, (0, 4), (0, 8), ""),
        field("extra", Kind::Signed, 1, (4, 4), (8, 8), ""),
        field("points", Kind::Record, 2, (8, 16), (16, 16), "tagPOINT"),
    ],
};

static UNION: Record = Record {
    name: "UNION",
    overlapping: true,
    same: false,
    size32: 4,
    align32: 4,
    size64: 8,
    align64: 8,
    fields: &[field("a", Kind::Pointer, 1, (0, 4), (0, 8), ""), field("b", Kind::Unsigned, 1, (0, 4), (0, 4), "")],
};

fn words(values: &[u32]) -> Vec<u8> {
    values.iter().flat_map(|value| value.to_le_bytes()).collect()
}

#[test]
fn wndclassexa_widens_its_pointers_and_comes_back() {
    let adapter = adapter("WNDCLASSEXA").expect("the host adapts WNDCLASSEXA");
    let guest = words(&[48, 3, 0x0040_1000, 0, 8, 0x0040_0000, 0x10, 0x20, 0x8000_0006, 0, 0x0040_5000, 0x30]);
    let host = adapter.to_host(&guest).expect("WNDCLASSEXA widens");
    assert_eq!(host.len(), 80);
    assert_eq!(host[8..16], 0x0040_1000_u64.to_le_bytes(), "lpfnWndProc is zero-extended");
    assert_eq!(host[16..24], words(&[0, 8])[..], "cbClsExtra and cbWndExtra keep their width");
    assert_eq!(host[48..56], 0x8000_0006_u64.to_le_bytes(), "a pointer with its top bit set is zero-extended");
    assert_eq!(adapter.to_guest(&host).as_deref(), Ok(&guest[..]), "it narrows back to the same bytes");
}

#[test]
fn a_host_pointer_above_4_gib_does_not_narrow() {
    let adapter = adapter("WNDCLASSEXA").expect("the host adapts WNDCLASSEXA");
    let mut host = vec![0; 80];
    host[64..72].copy_from_slice(&0x1_0000_2000_u64.to_le_bytes());
    let error = adapter.to_guest(&host).expect_err("a 64-bit pointer does not fit the program's field");
    assert!(error.contains("lpszClassName") && error.contains("0x100002000"), "{error}");
}

#[test]
fn a_structure_of_the_wrong_size_is_refused() {
    let adapter = adapter("WNDCLASSEXA").expect("the host adapts WNDCLASSEXA");
    assert!(adapter.to_host(&[0; 47]).is_err());
    assert!(adapter.to_guest(&[0; 48]).is_err());
}

#[test]
fn the_host_adapts_only_the_structures_of_its_calls() {
    assert!(adapter("MSG").is_err(), "MSG has a layout but no call of the host converts it yet");
    assert!(adapter("NO_SUCH_STRUCT").is_err());
    assert!(generate(layout("MSG").expect("the layout table has MSG")).is_ok(), "MSG's layouts generate an adapter");
}

#[test]
fn a_nested_array_adapts_element_by_element() {
    let adapter = generate(&NESTED).expect("NESTED's layouts generate an adapter");
    let guest = words(&[0x0040_2000, (-5_i32).cast_unsigned(), 1, 2, 3, 4]);
    let host = adapter.to_host(&guest).expect("NESTED widens");
    assert_eq!(host[0..8], 0x0040_2000_u64.to_le_bytes());
    assert_eq!(host[8..16], (-5_i64).to_le_bytes(), "a signed integer is sign-extended");
    assert_eq!(host[16..32], words(&[1, 2, 3, 4])[..], "both POINTs are copied");
    assert_eq!(adapter.to_guest(&host).as_deref(), Ok(&guest[..]));

    let mut wrong = host.clone();
    wrong[8..16].copy_from_slice(&(-5_i64 - (1 << 40)).to_le_bytes());
    assert!(adapter.to_guest(&wrong).is_err(), "a signed value beyond 32 bits does not narrow");
    wrong[8..16].copy_from_slice(&0xFFFF_FFFF_u64.to_le_bytes());
    assert!(adapter.to_guest(&wrong).is_err(), "a positive value whose low half reads negative does not narrow");
}

#[test]
fn a_union_has_no_adapter() {
    assert!(generate(&UNION).is_err());
}
