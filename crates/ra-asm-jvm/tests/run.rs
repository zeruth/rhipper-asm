//! The lift against the machine itself: the fixture class of every release, lifted and run over
//! the interpreter with the runtime, prints what `java` printed when the fixture was built.

use std::path::PathBuf;

use ra_asm_jvm::ClassFile;
use ra_asm_jvm::lift::lift;
use ra_asm_jvm::run::run;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/java")
}

/// What a fixture printed, whatever line ends the checkout gave its file.
fn printed(path: &std::path::Path) -> String {
    std::fs::read_to_string(path).unwrap().replace("\r\n", "\n")
}

#[test]
fn the_lifted_fixture_prints_what_java_prints() {
    for release in 8..=25 {
        let directory = fixtures().join(release.to_string());
        let file = ClassFile::parse(&std::fs::read(directory.join("Opcodes.class")).unwrap()).unwrap();
        let module = lift(std::slice::from_ref(&file)).unwrap_or_else(|e| panic!("release {release}: {e}"));
        let outcome = run(&module, "Opcodes", &[], 50_000_000).unwrap_or_else(|stop| panic!("release {release}: {stop:?}"));
        assert_eq!(outcome.thrown, None, "release {release}");
        let expected = printed(&directory.join("Opcodes.out"));
        assert_eq!(String::from_utf8_lossy(&outcome.out), expected, "release {release}");
        assert!(outcome.steps > 1000, "release {release}: {} steps", outcome.steps);
    }
}

/// The module the lift makes is LLVM's: `clang` takes its text, where there is a `clang`.
#[test]
fn clang_accepts_the_lifted_fixture() {
    if !std::process::Command::new("clang").arg("--version").output().is_ok_and(|o| o.status.success()) {
        eprintln!("clang is not on the PATH: the compile test is skipped");
        return;
    }
    let scratch = ra_asm_testing::Scratch::new("ra-asm-jvm-tests", "clang");
    for release in [8, 17, 25] {
        let file = ClassFile::parse(&std::fs::read(fixtures().join(format!("{release}/Opcodes.class"))).unwrap()).unwrap();
        let ll = scratch.join(format!("opcodes{release}.ll"));
        std::fs::write(&ll, lift(std::slice::from_ref(&file)).unwrap().to_ll()).unwrap();
        let check =
            std::process::Command::new("clang").args(["-c", "-x", "ir", "-o"]).arg(scratch.join("opcodes.o")).arg(&ll).output().unwrap();
        assert!(check.status.success(), "release {release}: clang -c:\n{}", String::from_utf8_lossy(&check.stderr));
    }
}

/// What the machine throws itself, and what a program throws and does not catch.
#[test]
fn an_uncaught_exception_ends_the_run_by_its_name() {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(fixtures().join("thrown")).unwrap().map(|entry| entry.unwrap().path()).collect();
    paths.retain(|path| path.extension().is_some_and(|e| e == "class"));
    paths.sort();
    assert_eq!(paths.len(), 3, "the fixture and its two nested classes");
    let classes: Vec<ClassFile> = paths.iter().map(|path| ClassFile::parse(&std::fs::read(path).unwrap()).unwrap()).collect();
    let module = lift(&classes).unwrap();
    let outcome = run(&module, "Thrown", &[], 1_000_000).unwrap();
    let expected = printed(&fixtures().join("thrown/Thrown.out"));
    assert_eq!(String::from_utf8_lossy(&outcome.out), expected);
    assert_eq!(outcome.thrown.as_deref(), Some("java.lang.IllegalStateException: 3"));
}
