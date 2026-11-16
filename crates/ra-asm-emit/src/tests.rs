//! The files of a translation formatted and written one at a time, as each is ready, against the
//! whole output formatted and then written.

use crate::{EmittedFile, EmittedFunction, format_and_write_files, format_files, write_files};

/// A split output: a header, and two files of functions, the first larger than a formatter's
/// piece so it goes to the formatters in parts.
fn output() -> (Vec<EmittedFile>, Vec<EmittedFunction>) {
    let mut functions = Vec::new();
    let mut files = vec![EmittedFile { name: "program.h".into(), text: "#pragma once\nint    g;\n".into(), functions: Vec::new() }];
    for (index, count) in [(0, 30_000), (1, 3)] {
        let mut text = String::from("#include \"program.h\"\n\n");
        let (mut lines, mut line) = (Vec::new(), 3);
        for n in 0..count {
            let address: u64 = 0x1000 + (index << 20) + n;
            let ir_name = format!("F_{address:x}");
            let body = format!("// {ir_name} at {address:#x}\nint {ir_name}(void) {{ return {n}; }}\n");
            // The definition line, below the comment; each function and the blank after it take three.
            lines.push((ir_name.clone(), line + 1));
            line += 3;
            text.push_str(&body);
            text.push('\n');
            functions.push(EmittedFunction {
                ir_name,
                address: Some(address),
                header_lines: 1,
                traps: 0,
                copies: 0,
                grade: crate::Grade::default(),
                hand: false,
                text: body,
            });
        }
        files.push(EmittedFile { name: format!("program_{index:03}.c"), text, functions: lines });
    }
    (files, functions)
}

#[test]
fn files_written_as_each_is_formatted_are_those_formatted_whole() {
    let root = std::env::temp_dir().join(format!("ra-asm-emit-format-and-write-{}", std::process::id()));
    let (whole, each) = (root.join("whole"), root.join("each"));
    // A part of a larger earlier output, which neither write keeps.
    for dir in [&whole, &each] {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("program_002.c"), "int stale;\n").unwrap();
    }
    let (mut formatted, functions) = output();
    format_files(&mut formatted, &functions);
    write_files(&whole, &formatted).unwrap();
    let (mut streamed, functions) = output();
    let bytes = format_and_write_files(&each, &mut streamed, &functions).unwrap();

    assert_eq!(bytes, formatted.iter().map(|f| f.text.len()).sum::<usize>());
    for (f, s) in formatted.iter().zip(&streamed) {
        assert_eq!(f.name, s.name);
        assert_eq!(f.functions, s.functions, "{}", f.name);
        assert!(s.text.is_empty(), "{} kept its text", s.name);
        assert!(std::fs::read(each.join(&f.name)).unwrap() == f.text.as_bytes(), "{} differs", f.name);
        assert!(std::fs::read(whole.join(&f.name)).unwrap() == f.text.as_bytes(), "{} differs", f.name);
    }
    assert!(!each.join("program_002.c").exists() && !whole.join("program_002.c").exists());
    let _ = std::fs::remove_dir_all(&root);
}
