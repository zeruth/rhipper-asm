//! hello32 packed by upx, run at the machine level until its stub jumps into what it unpacked,
//! and the run's memory there written as a PE file: the file parses, it imports what hello32
//! imports through hello32's own slots, its code is hello32's, and Windows loads and runs it as it
//! runs hello32.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ra_asm_pe::Image;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn hello32() -> Image {
    ra_asm_pe::load(&root().join("testdata/bin/hello32.exe")).unwrap()
}

/// The file written from the run of hello32-upx, made once for the tests; none when this host has
/// no Windows DLLs to answer the stub's imports, which says so on stderr.
fn unpacked() -> Option<&'static [u8]> {
    static FILE: OnceLock<Option<Vec<u8>>> = OnceLock::new();
    FILE.get_or_init(|| {
        let path = root().join("testdata/bin/hello32-upx.exe");
        let image = ra_asm_pe::load(&path).unwrap_or_else(|e| panic!("{}: {e:#}", path.display()));
        let analysis = ra_asm_analysis::analyze(&image);
        let lift = ra_asm_lift::LiftOptions { standalone: false, x87: ra_asm_lift::X87Unit::Double };
        let mut lifted = ra_asm_lift::lift_with(&image, &analysis, lift);
        let raise = lifted.raise_options(ra_asm_lift::RaiseLevel::Machine);
        let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, raise);
        lifted.apply_names(&analysis, Some(&mut signatures));
        let options = ra_asm_host::Options { unpack: true, limit: 500_000_000, ..ra_asm_host::Options::default() };
        let outcome = ra_asm_host::run(&image, &lifted, &signatures, &options).unwrap();
        if let Some(stop) = &outcome.stop
            && stop.starts_with("no binding for ")
            && !cfg!(windows)
        {
            eprintln!("SKIPPED: hello32-upx.exe: {stop}; this host has no Windows DLLs: the interpreted run is skipped");
            return None;
        }
        Some(ra_asm_host::unpack::image(&image, &outcome).unwrap_or_else(|e| panic!("hello32-upx.exe ({} steps): {e:#}", outcome.steps)))
    })
    .as_deref()
}

fn parsed() -> Option<Image> {
    unpacked().map(|file| Image::parse(file.to_vec()).unwrap_or_else(|e| panic!("the unpacked file does not parse: {e:#}")))
}

#[test]
fn the_unpacked_file_parses_with_hello32s_entry() {
    let Some(image) = parsed() else { return };
    assert!(image.warnings.is_empty(), "{:?}", image.warnings);
    assert_eq!(image.image_base(), hello32().image_base());
    assert_eq!(image.entry_point(), hello32().entry_point());
}

#[test]
fn the_unpacked_file_imports_hello32s_functions_through_its_slots() {
    let Some(image) = parsed() else { return };
    let imports = |image: &Image| {
        let mut all: Vec<(String, Option<String>, u64)> =
            image.imports.iter().map(|i| (i.dll.to_ascii_lowercase(), i.name.clone(), i.iat_va)).collect();
        all.sort();
        all
    };
    assert_eq!(imports(&image), imports(&hello32()));
}

#[test]
fn the_unpacked_code_section_is_hello32s() {
    let Some(image) = parsed() else { return };
    let twin = hello32();
    let text = twin.sections.iter().find(|s| s.is_code()).expect("hello32 has a code section");
    let va = twin.va(text.virtual_address);
    let code = image.section_of(va).expect("the unpacked file maps hello32's code");
    assert!(code.is_code() && code.is_executable(), "the section of the code, {}, is no code: {:#x}", code.name, code.characteristics);
    let len = text.mapped_size() as usize;
    assert!(image.read(va, len).unwrap() == twin.read(va, len).unwrap(), "the bytes of hello32's {} at {va:#x}", text.name);
}

/// The loader reads the file: it runs as hello32 runs, with its output and its exit.
#[test]
#[cfg(windows)]
fn the_unpacked_file_runs_as_hello32() {
    let Some(file) = unpacked() else { return };
    let scratch = ra_asm_testing::Scratch::new("ra-asm-host-unpack", "hello32");
    let exe = scratch.join("hello32-unpacked.exe");
    std::fs::write(&exe, file).unwrap();
    let native = std::process::Command::new(root().join("testdata/bin/hello32.exe")).output().unwrap();
    let run = std::process::Command::new(&exe).output().unwrap_or_else(|e| panic!("{}: {e}", exe.display()));
    assert_eq!(
        (String::from_utf8_lossy(&run.stdout), run.status.code()),
        (String::from_utf8_lossy(&native.stdout), native.status.code()),
        "the unpacked file's run beside hello32's (stderr: {})",
        String::from_utf8_lossy(&run.stderr)
    );
}
