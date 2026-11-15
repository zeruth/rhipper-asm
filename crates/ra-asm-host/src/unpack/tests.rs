use ra_asm_lift::run::Resolved;

use super::{DESCRIPTOR, import_directory, runs};

/// Adjacent slots of one DLL are one table, whatever the case of its name; a gap or another DLL
/// starts the next.
#[test]
fn a_table_is_a_run_of_adjacent_slots_of_one_dll() {
    let slots = [
        (0x2054, "KERNEL32.DLL"),
        (0x2058, "kernel32.dll"),
        (0x205c, "KERNEL32.DLL"),
        (0x2060, "USER32.DLL"),
        (0x2068, "USER32.DLL"),
        (0x206c, "USER32.DLL"),
    ];
    assert_eq!(runs(&slots, 4), [0..3, 3..4, 4..6]);
    assert!(runs(&[], 4).is_empty());
}

/// The directory of one table: its descriptor names the lookup table, the DLL and the slots, the
/// lookup table ends with a zero, an import by name points at its hint/name entry and one by
/// ordinal carries the flag; each slot gets its lookup entry.
#[test]
fn the_directory_points_at_its_names_and_its_slots() {
    let resolved = [
        Resolved { address: 0x10, dll: "KERNEL32.DLL".into(), name: "ExitProcess".into() },
        Resolved { address: 0x20, dll: "KERNEL32.DLL".into(), name: "#17".into() },
    ];
    let slots = [(0x2054, &resolved[0]), (0x2058, &resolved[1])];
    let rva = 0x7000;
    let (data, filled) = import_directory(&slots, &[0..2], rva, 4).unwrap();
    let u32_at = |offset: usize| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
    let lookup = 2 * DESCRIPTOR;
    assert_eq!(u32_at(0), rva + lookup as u32, "OriginalFirstThunk");
    assert_eq!(u32_at(16), 0x2054, "FirstThunk");
    assert!(data[DESCRIPTOR..2 * DESCRIPTOR].iter().all(|&b| b == 0), "the directory ends with an empty descriptor");
    assert_eq!(u32_at(lookup + 4), 0x8000_0011, "the import by ordinal");
    assert_eq!(u32_at(lookup + 8), 0, "the lookup table's end");
    let name = (u32_at(lookup) - rva) as usize;
    assert_eq!(&data[name..name + 2], [0, 0], "the hint");
    assert_eq!(&data[name + 2..name + 14], b"ExitProcess\0");
    let dll = (u32_at(12) - rva) as usize;
    assert_eq!(&data[dll..dll + 13], b"KERNEL32.DLL\0");
    assert_eq!(filled, [(0x2054, u64::from(u32_at(lookup))), (0x2058, 0x8000_0011)]);
}
