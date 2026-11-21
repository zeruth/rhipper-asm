//! The emit: the lifted fixtures through the C backend. The source defines every method under
//! its C name and leaves no global unwritten; compiled with the C runtime, where there is a
//! `clang`, it is a program that prints what `java` printed.

use std::path::PathBuf;

use ra_asm_emit::c::{Options, emit_c};
use ra_asm_jvm::ClassFile;
use ra_asm_jvm::lift::{entry, lift};
use ra_asm_jvm::names::{c_name, c_names};

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The classes of a directory of the Java fixtures, in the order of their files' names. After
/// an `@`, the version their class files are set to: `plain@45` is the plain fixture as classes
/// of the first release.
fn classes(directory: &str) -> Vec<ClassFile> {
    let (directory, major) = directory.split_once('@').map_or((directory, None), |(d, major)| (d, major.parse::<u16>().ok()));
    let mut paths: Vec<PathBuf> =
        std::fs::read_dir(workspace().join("testdata/java").join(directory)).unwrap().map(|entry| entry.unwrap().path()).collect();
    paths.retain(|path| path.extension().is_some_and(|e| e == "class"));
    paths.sort();
    paths
        .iter()
        .map(|path| {
            let mut bytes = std::fs::read(path).unwrap();
            if let Some(major) = major {
                bytes[4..6].copy_from_slice(&(if major == 45 { 3u16 } else { 0 }).to_be_bytes());
                bytes[6..8].copy_from_slice(&major.to_be_bytes());
            }
            ClassFile::parse(&bytes).unwrap()
        })
        .collect()
}

fn source(directory: &str) -> String {
    let mut module = lift(&classes(directory)).unwrap();
    c_names(&mut module);
    emit_c(&module, &Options::default())
}

#[test]
fn the_source_defines_every_method_by_its_c_name() {
    for directory in ["8", "11", "17", "21", "25", "thrown", "plain", "features/8", "records/17", "patterns/21", "patterns/25"] {
        let text = source(directory);
        assert!(!text.contains("is not written"), "{directory}: a global the backend left out");
        for file in classes(directory) {
            let class = file.name().unwrap();
            for method in &file.methods {
                if file.code(method).unwrap().is_none() {
                    continue;
                }
                let name = c_name(&format!("{class}.{}:{}", file.utf8(method.name).unwrap(), file.utf8(method.descriptor).unwrap()));
                // The definition: the name, its parameters, and a body.
                let defined = text.lines().any(|line| line.contains(&format!(" {name}(")) && line.trim_end().ends_with('{'))
                    || text.lines().any(|line| line.contains(&format!("{name}(")) && line.contains(") {"));
                assert!(defined, "{directory}: no definition of {name}");
            }
        }
        assert!(text.contains("void jvm_classes(void)"), "{directory}: the classes are not told to the runtime");
    }
}

/// The port: the source with the function the runtime's `main` calls, compiled with the C
/// runtime, prints what `java` printed, and ends as the program ended.
#[test]
fn the_compiled_port_prints_what_java_prints() {
    if !std::process::Command::new("clang").arg("--version").output().is_ok_and(|o| o.status.success()) {
        eprintln!("clang is not on the PATH: the port's test is skipped");
        return;
    }
    let scratch = ra_asm_testing::Scratch::new("ra-asm-jvm-tests", "port");
    for (directory, main, code) in [
        ("8", "Opcodes", 0),
        ("11", "Opcodes", 0),
        ("17", "Opcodes", 0),
        ("25", "Opcodes", 0),
        ("thrown", "Thrown", 1),
        ("plain", "Plain", 0),
        ("plain@45", "Plain", 0),
        ("plain@48", "Plain", 0),
        ("plain@51", "Plain", 0),
        ("library", "Library", 0),
        ("features/8", "Features", 0),
        ("features/25", "Features", 0),
        ("records/17", "Records", 0),
        ("records/25", "Records", 0),
        ("patterns/21", "Patterns", 0),
        ("patterns/25", "Patterns", 0),
    ] {
        let mut module = lift(&classes(directory)).unwrap();
        entry(&mut module, main).unwrap();
        c_names(&mut module);
        let name = directory.replace(['/', '@'], "-");
        let directory = directory.split('@').next().unwrap_or(directory);
        let path = scratch.join(format!("{name}.c"));
        std::fs::write(&path, emit_c(&module, &Options::default())).unwrap();
        let executable = scratch.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        let build = std::process::Command::new("clang")
            .args(["-O1", "-Werror=implicit-function-declaration", "-I"])
            .arg(workspace().join("runtime"))
            .arg(&path)
            .arg(workspace().join("runtime/jvm.c"))
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        if !build.status.success() {
            let text = String::from_utf8_lossy(&build.stderr);
            // Without the C runtime library installed, the link fails and the run is skipped.
            if text.contains("cannot open input file") || text.contains("libcmt") || text.contains("msvcrt") {
                eprintln!("the C runtime is not installed: the run is skipped\n{text}");
                return;
            }
            panic!("{directory}: clang:\n{text}");
        }
        let run = std::process::Command::new(&executable).output().unwrap();
        let expected =
            std::fs::read_to_string(workspace().join(format!("testdata/java/{directory}/{main}.out"))).unwrap().replace("\r\n", "\n");
        assert_eq!(String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n"), expected, "{directory}");
        assert_eq!(run.status.code(), Some(code), "{directory}: {}", String::from_utf8_lossy(&run.stderr));
        if code != 0 {
            let named = String::from_utf8_lossy(&run.stderr);
            assert!(named.contains("Exception in thread \"main\" java.lang.IllegalStateException: 3"), "{directory}: {named}");
        }
    }
}
