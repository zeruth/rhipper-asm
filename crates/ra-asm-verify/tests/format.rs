//! The vector file: what the writer prints, what the reader takes, and that one undoes the other.

use ra_asm_verify::{Call, Left, Vector, read, write};

/// A vector with something in every section.
fn sample() -> Vector {
    Vector {
        function: "F_140001000".to_owned(),
        address: 0x1_4000_1000,
        entry: vec![("rcx".to_owned(), 5)],
        memory_in: vec![(0x1_4000_a000, vec![1, 2, 3])],
        calls: vec![Call {
            callee: "kernel32!GetTickCount".to_owned(),
            args: vec![1],
            results: vec![("rax".to_owned(), 0x2a)],
            effects: vec![(0x1_4000_b000, vec![0xff])],
        }],
        exit: vec![("rax".to_owned(), 0x2f)],
        memory_out: vec![(0x1_4000_a000, vec![4])],
        left: Left::Return,
    }
}

const SAMPLE: &str = "\
ra-asm-verify vectors 1

vector F_140001000 at 0x140001000
entry
  rcx = 0x5
memory in
  0x14000a000: 01 02 03
call kernel32!GetTickCount
  arg 0x1
  result rax = 0x2a
  effect 0x14000b000: ff
exit
  rax = 0x2f
memory out
  0x14000a000: 04
left return
";

#[test]
fn the_writer_prints_one_item_a_line() {
    assert_eq!(write(&[sample()]).unwrap(), SAMPLE);
}

#[test]
fn the_reader_takes_what_the_writer_prints() {
    assert_eq!(read(SAMPLE).unwrap(), [sample()]);
}

#[test]
fn a_file_of_several_vectors_round_trips() {
    let wide = Vector {
        function: "?run@Game@@QEAAXXZ".to_owned(),
        address: 0x40_1000,
        entry: vec![("xmm0".to_owned(), u128::MAX), ("esp".to_owned(), 0x19_ff00)],
        calls: vec![
            Call { callee: "F_401200".to_owned(), args: vec![u64::MAX, 0], ..Call::default() },
            Call { callee: "operator=".to_owned(), results: vec![("eax".to_owned(), 1), ("edx".to_owned(), 0)], ..Call::default() },
        ],
        left: Left::Exit(3),
        ..Vector::default()
    };
    let vectors = [sample(), wide, Vector { function: "empty".to_owned(), ..Vector::default() }];
    let text = write(&vectors).unwrap();
    assert_eq!(read(&text).unwrap(), vectors);
    assert_eq!(write(&read(&text).unwrap()).unwrap(), text);
}

#[test]
fn a_file_without_vectors_is_its_header() {
    let text = write(&[]).unwrap();
    assert_eq!(text, "ra-asm-verify vectors 1\n");
    assert!(read(&text).unwrap().is_empty());
}

#[test]
fn a_long_run_goes_on_over_lines_of_bytes() {
    let bytes: Vec<u8> = (0..40).collect();
    let vector = Vector {
        function: "F_1000".to_owned(),
        address: 0x1000,
        memory_in: vec![(0x2000, bytes.clone()), (0x3000, Vec::new()), (0x2028, vec![0xaa])],
        calls: vec![Call { callee: "memcpy".to_owned(), effects: vec![(0x4000, bytes.clone()), (0x5000, vec![1])], ..Call::default() }],
        memory_out: vec![(0x4000, bytes)],
        ..Vector::default()
    };
    let text = write(std::slice::from_ref(&vector)).unwrap();
    assert!(text.contains("  0x2000: 00 01 02 03 04 05 06 07 08 09 0a 0b 0c 0d 0e 0f\n    10 11 "), "{text}");
    assert!(text.contains("\n    20 21 22 23 24 25 26 27\n  0x3000:\n  0x2028: aa\n"), "{text}");
    assert!(text.lines().all(|line| line.len() <= 80), "{text}");
    assert_eq!(read(&text).unwrap(), [vector]);
}

#[test]
fn every_way_of_leaving_round_trips() {
    let ways = [
        Left::Return,
        Left::Exit(0),
        Left::Exit(u32::MAX),
        Left::Trap { address: 0x1_4000_1234, reason: "unsupported instruction: xsave  [rcx]".to_owned() },
        Left::Trap { address: 0x1_4000_1234, reason: String::new() },
        Left::Exception { code: 0xc000_0005, address: 0x1_4000_2000 },
    ];
    for left in ways {
        let vector = Vector { function: "F_140001000".to_owned(), address: 0x1_4000_1000, left, ..Vector::default() };
        let text = write(std::slice::from_ref(&vector)).unwrap();
        assert_eq!(read(&text).unwrap(), [vector], "{text}");
    }
    let text =
        write(&[Vector { function: "f".to_owned(), left: Left::Exception { code: 0xc000_0005, address: 0x10 }, ..Vector::default() }])
            .unwrap();
    assert!(text.ends_with("left exception 0xc0000005 at 0x10\n"), "{text}");
}

#[test]
fn comments_blank_lines_and_indentation_say_nothing() {
    let text = "\
# recorded by hand
ra-asm-verify vectors 1
vector F_1000 at 0x1000

    entry
rcx   =   0x5
  # the stack slot
memory in
0x2000:   01
        02
exit
memory out
left return";
    let expected = Vector {
        function: "F_1000".to_owned(),
        address: 0x1000,
        entry: vec![("rcx".to_owned(), 5)],
        memory_in: vec![(0x2000, vec![1, 2])],
        ..Vector::default()
    };
    assert_eq!(read(text).unwrap(), [expected]);
}

#[test]
fn a_fault_names_its_line() {
    let fault = |text: &str| read(text).unwrap_err();

    assert_eq!(fault("").line, Some(1));
    assert_eq!(fault("vectors 2\n").line, Some(1));

    // A section missing: `exit` stands where `memory in` or a register is expected.
    let error = fault("ra-asm-verify vectors 1\nvector f at 0x1\nentry\nexit\nmemory out\nleft return\n");
    assert_eq!(error.line, Some(4), "{error}");
    assert!(error.to_string().starts_with("line 4: "), "{error}");

    let head = "ra-asm-verify vectors 1\nvector f at 0x1\nentry\n";
    assert_eq!(fault(&format!("{head}rcx = 5\n")).line, Some(4));
    assert_eq!(fault(&format!("{head}memory in\n0x10: 1\n")).line, Some(5));
    assert_eq!(fault(&format!("{head}memory in\n01 02\n")).line, Some(5));
    assert_eq!(fault(&format!("{head}memory in\ncall g\narg 0x1\n01\n")).line, Some(7));
    assert_eq!(fault(&format!("{head}memory in\nexit\nmemory out\nleft sideways\n")).line, Some(7));
    assert_eq!(fault(&format!("{head}memory in\nexit\nmemory out\nleft exit 0x100000000\n")).line, Some(7));
    // The text ends before the vector does.
    assert_eq!(fault(&format!("{head}memory in\nexit\nmemory out\n")).line, Some(6));
    assert_eq!(fault("ra-asm-verify vectors 1\nvector f g at 0x1\n").line, Some(2));
}

#[test]
fn the_writer_refuses_what_would_not_read_back() {
    let refused = |vector: Vector| {
        let error = write(&[vector]).unwrap_err();
        assert_eq!(error.line, None, "{error}");
    };
    refused(Vector::default());
    refused(Vector { function: "two words".to_owned(), ..Vector::default() });
    refused(Vector { function: "f".to_owned(), entry: vec![("r ax".to_owned(), 0)], ..Vector::default() });
    refused(Vector { function: "f".to_owned(), exit: vec![("a=b".to_owned(), 0)], ..Vector::default() });
    refused(Vector { function: "f".to_owned(), calls: vec![Call::default()], ..Vector::default() });
    refused(Vector { function: "f".to_owned(), left: Left::Trap { address: 0, reason: "two\nlines".to_owned() }, ..Vector::default() });
    refused(Vector { function: "f".to_owned(), left: Left::Trap { address: 0, reason: " padded".to_owned() }, ..Vector::default() });
}
