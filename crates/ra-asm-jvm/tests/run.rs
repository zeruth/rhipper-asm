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

/// What a release brought, compiled for the release that brought it and for the newest: enums,
/// boxes and the for-each loop, with a class initialized when it is first used; records and
/// their methods the library's bootstrap answers; a switch over the type of its operand.
#[test]
fn what_each_release_brought_runs() {
    for (directory, main) in [
        ("features/8", "Features"),
        ("features/25", "Features"),
        ("records/17", "Records"),
        ("records/25", "Records"),
        ("patterns/21", "Patterns"),
        ("patterns/25", "Patterns"),
        // The library a program leans on every day: strings, numbers, characters, a list, a map.
        ("library", "Library"),
    ] {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(fixtures().join(directory)).unwrap().map(|entry| entry.unwrap().path()).collect();
        paths.retain(|path| path.extension().is_some_and(|e| e == "class"));
        paths.sort();
        let classes: Vec<ClassFile> = paths.iter().map(|path| ClassFile::parse(&std::fs::read(path).unwrap()).unwrap()).collect();
        let module = lift(&classes).unwrap_or_else(|e| panic!("{directory}: {e}"));
        let outcome = run(&module, main, &[], 10_000_000).unwrap_or_else(|stop| panic!("{directory}: {stop:?}"));
        assert_eq!(outcome.thrown, None, "{directory}");
        assert_eq!(String::from_utf8_lossy(&outcome.out), printed(&fixtures().join(directory).join(format!("{main}.out"))), "{directory}");
    }
}

/// Every release from the first: the plain fixture uses nothing the first class files had not,
/// so with its classes' version set to a release's they are classes of that release, and each
/// release's lift of them prints what `java` printed.
#[test]
fn every_release_runs_the_plain_fixture() {
    let mut paths: Vec<PathBuf> = std::fs::read_dir(fixtures().join("plain")).unwrap().map(|entry| entry.unwrap().path()).collect();
    paths.retain(|path| path.extension().is_some_and(|e| e == "class"));
    paths.sort();
    assert_eq!(paths.len(), 5, "the fixture and its four nested classes");
    let bytes: Vec<Vec<u8>> = paths.iter().map(|path| std::fs::read(path).unwrap()).collect();
    let expected = printed(&fixtures().join("plain/Plain.out"));
    for major in ra_asm_jvm::version::FIRST..=ra_asm_jvm::version::LAST {
        let classes: Vec<ClassFile> = bytes
            .iter()
            .map(|class| {
                // The version: the minor at four, the major at six. The first release's class files
                // are 45.3, where a method's code has the sizes it has had since.
                let mut class = class.clone();
                class[4..6].copy_from_slice(&(if major == 45 { 3u16 } else { 0 }).to_be_bytes());
                class[6..8].copy_from_slice(&major.to_be_bytes());
                ClassFile::parse(&class).unwrap_or_else(|e| panic!("class files {major}: {e}"))
            })
            .collect();
        let module = lift(&classes).unwrap_or_else(|e| panic!("class files {major}: {e}"));
        let outcome = run(&module, "Plain", &[], 10_000_000).unwrap_or_else(|stop| panic!("class files {major}: {stop:?}"));
        assert_eq!(outcome.thrown, None, "class files {major}");
        assert_eq!(String::from_utf8_lossy(&outcome.out), expected, "class files {major}");
    }
}
