//! The lift's reading of the Windows API prototype table of `ra-asm-protos`: the kind letters as
//! the ABI's kinds, and the hand table of `raise::prototypes` beside the generated one. The table
//! itself, and the test that regenerates it, are that crate's.

use ra_asm_lift::raise::abi::Kind;
use ra_asm_lift::raise::prototypes::{arity, typed, win32};

/// The generated table read as kinds, and right about the functions the hand table knows.
#[test]
fn the_generated_table_reads_as_kinds() {
    let (returns, params) = win32("WriteFile").expect("WriteFile is in the table");
    assert_eq!((returns, params.len()), (Some(Kind::Int), 5));
    assert_eq!(win32("GetTickCount64").map(|(r, p)| (r, p.len())), Some((Some(Kind::Wide), 0)));
    assert_eq!(win32("SetFilePointerEx").map(|(_, p)| p), Some(vec![Kind::Int, Kind::Wide, Kind::Int, Kind::Int]));
    assert_eq!(win32("ExitProcess").map(|(r, p)| (r, p.len())), Some((None, 1)));
    // The table reaches past the hand one, and agrees with it where both know a function.
    assert_eq!(arity("GetDiskFreeSpaceExA"), Some(4));
    assert_eq!(arity("CreateWindowExW"), Some(12));
    assert_eq!(arity("MultiByteToWideChar"), Some(6));
    assert!(typed("ChildWindowFromPoint").is_some_and(|t| t.params == [Kind::Int, Kind::Wide]));
    assert!(win32("wsprintfA").is_none(), "a variadic function is in the table");
    assert!(win32("printf").is_none());
}
