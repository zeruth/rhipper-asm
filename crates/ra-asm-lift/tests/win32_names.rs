//! The Windows API's named constants, read under the lift's path: `raise::prototypes` hands on
//! the binding and the names of `ra-asm-protos`, where the emitters look for them. The table
//! itself, and the test that regenerates it, are that crate's.

use ra_asm_lift::raise::prototypes::{constant_name, named};

#[test]
fn the_named_constants_read_under_the_lift_path() {
    let (result, params) = named("CreateFileA").expect("CreateFileA is bound");
    assert_eq!(result, Some("HANDLE_VALUE"));
    assert_eq!(params[..3], [None, Some("ACCESS_MASK"), Some("FILE_SHARE_MODE")]);
    assert!(named("printf").is_none());
    assert_eq!(constant_name("ACCESS_MASK", 0xc000_0000, 32), Some(vec![("GENERIC_READ", 0x8000_0000), ("GENERIC_WRITE", 0x4000_0000)]));
    assert_eq!(constant_name("WIN32_ERROR", 2, 32).map(|p| p[0].0), Some("ERROR_FILE_NOT_FOUND"));
}
