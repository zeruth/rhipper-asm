//! The corpus under `testdata/cases`: every case file, and the notes of the differences no case
//! can hold.

use crate::{Case, case_files, cases_dir, run};

/// Every case file under `testdata/cases` passes: adding a file adds a test.
#[test]
fn every_case_passes() {
    let files = case_files(&cases_dir());
    let mut names = Vec::new();
    let mut failures = Vec::new();
    for file in &files {
        match Case::load(file) {
            Ok(case) => {
                if let Err(failure) = run(&case) {
                    failures.push(failure);
                }
                names.push(case.name);
            }
            Err(failure) => failures.push(failure),
        }
    }
    assert!(failures.is_empty(), "{} of {} cases fail:\n{}", failures.len(), files.len(), failures.join("\n"));
    for seed in ["simplify/folds", "simplify/identities", "simplify/memory"] {
        assert!(names.iter().any(|name| name == seed), "the seed case {seed} did not run: {names:?}");
    }
    // The passes a change most often touches each keep the six shapes they exist for.
    for pass in ["conditions", "simplify", "dce", "widths", "promote", "abi", "stack", "frame"] {
        let ran = names.iter().filter(|name| name.strip_prefix(pass).is_some_and(|rest| rest.starts_with('/'))).count();
        assert!(ran >= 6, "{ran} cases of {pass} ran, not six: {names:?}");
    }
    assert!(names.len() >= 48, "{} cases ran, not 48: {names:?}", names.len());
}

/// A difference bisect finds at `promote` has no case, since a case cannot load the machine
/// state; its note under `pending` names the program, the function and the step instead.
#[test]
fn pending_notes_name_the_step() {
    let notes = [
        ("cpp_multi", "testdata/rhtest/cpp-clang.exe", "measure at 0x401110"),
        ("engine_stream_len", "testdata/rhtest/engine-clang.exe", "float_reader at 0x4012d0"),
    ];
    for (note, program, function) in notes {
        let path = cases_dir().join("pending").join(note).with_extension("txt");
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let field = |name: &str| text.lines().find_map(|line| line.strip_prefix(name)?.strip_prefix(':')).map(str::trim);
        assert_eq!(field("program"), Some(program), "{note}: the program");
        assert_eq!(field("function"), Some(function), "{note}: the function");
        assert_eq!(field("step"), Some("promote"), "{note}: the step");
        assert!(field("difference").is_some_and(|d| !d.is_empty()), "{note}: the difference");
    }
}
