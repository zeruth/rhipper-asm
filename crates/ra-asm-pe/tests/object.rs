//! The object reader over objects clang builds here: a COFF x86 and x64 object, an ELF x86-64
//! object, and an archive of them. The tests need `clang` and `llvm-ar` and skip without them.

use std::path::PathBuf;
use std::process::Command;

use ra_asm_pe::object;

const SOURCE: &str = r"
extern int helper(int);
extern int table[4];
__attribute__((noinline)) static int local(int x) { return x * 3; }
int entry(int a) { return helper(a + 1) + local(a) + table[a & 3]; }
int other(int b) { return helper(b) * 2; }
";

fn scratch() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("object");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("obj.c"), SOURCE).unwrap();
    dir
}

/// Compiles the source for `target` into `name`; `None` when clang is not here.
fn compile(target: &str, name: &str) -> Option<PathBuf> {
    let dir = scratch();
    let out = dir.join(name);
    let status =
        Command::new("clang").args(["-c", "-O1", &format!("--target={target}"), "-o"]).arg(&out).arg(dir.join("obj.c")).status().ok()?;
    assert!(status.success(), "clang failed for {target}");
    Some(out)
}

fn function<'a>(o: &'a object::Object, name: &str) -> &'a object::ObjectFunction {
    o.functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("{name} among {:?}", o.functions.iter().map(|f| &f.name).collect::<Vec<_>>()))
}

/// The relocations inside a function, as `(offset in the function, size, symbol)`.
fn relocs_of(o: &object::Object, f: &object::ObjectFunction) -> Vec<(u32, u8, Option<String>)> {
    o.sections[f.section]
        .relocations
        .iter()
        .filter(|r| r.offset >= f.offset && r.offset < f.offset + f.size)
        .map(|r| (r.offset - f.offset, r.size, r.symbol.clone()))
        .collect()
}

fn check(o: &object::Object, decorated: impl Fn(&str) -> String, table_named: bool) {
    assert_eq!(o.functions.len(), 3, "{:?}", o.functions);
    let entry = function(o, &decorated("entry"));
    assert!(entry.external && entry.size > 8);
    assert!(!function(o, &decorated("local")).external);
    let relocs = relocs_of(o, entry);
    let names: Vec<&str> = relocs.iter().filter_map(|r| r.2.as_deref()).collect();
    assert!(names.contains(&decorated("helper").as_str()), "{relocs:?}");
    if table_named {
        assert!(names.contains(&decorated("table").as_str()), "{relocs:?}");
    }
    assert!(relocs.iter().all(|r| r.1 == 4 || r.1 == 8), "{relocs:?}");
    let other = function(o, &decorated("other"));
    assert!(relocs_of(o, other).iter().any(|r| r.2.as_deref() == Some(decorated("helper").as_str())));
    // A static function's bytes stay inside its own extent: no symbol of another starts in them.
    for f in &o.functions {
        for g in &o.functions {
            if f.name != g.name && f.section == g.section {
                assert!(g.offset < f.offset || g.offset >= f.offset + f.size, "{} overlaps {}", g.name, f.name);
            }
        }
    }
}

#[test]
fn reads_the_coff_objects_of_both_widths() {
    let Some(x86) = compile("i686-pc-windows-msvc", "obj32.obj") else {
        eprintln!("skipped: no clang");
        return;
    };
    let library = object::read(&x86).unwrap();
    assert_eq!(library.objects.len(), 1);
    let o = &library.objects[0];
    assert_eq!((o.machine_name().as_str(), o.elf), ("x86", false));
    check(o, |n| format!("_{n}"), true);

    let x64 = compile("x86_64-pc-windows-msvc", "obj64.obj").unwrap();
    let library = object::read(&x64).unwrap();
    let o = &library.objects[0];
    assert_eq!(o.machine_name(), "x64");
    check(o, str::to_owned, true);
}

#[test]
fn reads_an_elf_object() {
    let Some(elf) = compile("x86_64-unknown-linux-gnu", "obj.o") else {
        eprintln!("skipped: no clang");
        return;
    };
    let library = object::read(&elf).unwrap();
    let o = &library.objects[0];
    assert_eq!((o.machine_name().as_str(), o.elf), ("x64", true));
    check(o, str::to_owned, true);
}

#[test]
fn reads_an_archive_member_by_member() {
    let Some(x64) = compile("x86_64-pc-windows-msvc", "member64.obj") else {
        eprintln!("skipped: no clang");
        return;
    };
    let dir = scratch();
    let archive = dir.join("members.lib");
    let _ = std::fs::remove_file(&archive);
    let status = Command::new("llvm-ar").arg("rc").arg(&archive).arg(&x64).arg(&x64).status();
    let Ok(status) = status else {
        eprintln!("skipped: no llvm-ar");
        return;
    };
    assert!(status.success());
    let library = object::read(&archive).unwrap();
    assert!(library.skipped.is_empty(), "{:?}", library.skipped);
    assert_eq!(library.objects.len(), 2);
    assert!(library.objects.iter().all(|o| o.name == "member64.obj"), "{:?}", library.objects.iter().map(|o| &o.name).collect::<Vec<_>>());
    check(&library.objects[0], str::to_owned, true);
    let error = object::read(&dir.join("obj.c")).expect_err("a C file is no object");
    assert!(format!("{error:#}").contains("neither"), "{error:#}");
}
