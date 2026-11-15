use super::*;

/// A directory keeps the case under its step, by the program and the function; any other path
/// is the file itself.
#[test]
fn a_case_goes_under_its_step_in_a_directory() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let program = Path::new("testdata/bin/control64.exe");
    assert_eq!(case_file(dir, program, "conditions", "fib"), dir.join("conditions").join("control64_fib.ll"));
    let file = dir.join("fib.ll");
    assert_eq!(case_file(&file, program, "conditions", "fib"), file);
}

/// A filter keeps the vectors of the one function, by name or by address, and one that names no
/// function the vectors hold is refused by name.
#[test]
fn the_filter_takes_one_function() {
    let vector = |function: &str, address: u64| Vector { function: function.to_owned(), address, ..Vector::default() };
    let vectors = [vector("fib", 0x1400_1000), vector("main", 0x1400_2000), vector("fib", 0x1400_1000)];
    let all = select(&vectors, None).unwrap();
    assert_eq!(all.len(), 2);
    let by_name = select(&vectors, Some("fib")).unwrap();
    assert_eq!(by_name.keys().copied().collect::<Vec<_>>(), [0x1400_1000]);
    assert_eq!(by_name[&0x1400_1000].vectors.len(), 2);
    let by_address = select(&vectors, Some("0x14002000")).unwrap();
    assert_eq!(by_address[&0x1400_2000].name, "main");
    let refusal = select(&vectors, Some("nothing")).err().unwrap().to_string();
    assert!(refusal.contains("nothing has no vector"), "{refusal}");
}
