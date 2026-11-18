//! `ras` over the Linux fixture: `info`, `sections`, `symbols`, `imports`, `exports` and `functions`
//! print an ELF file as they print a PE.

use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/hello_linux64")
}

fn ras(args: &[&str]) -> String {
    ras_on(&fixture(), args)
}

fn ras_on(path: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_ras")).args(args).arg(path).output().unwrap();
    assert!(output.status.success(), "ras {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn ras_names_the_mips_control_mips() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/mips/control");
    let info = ras_on(&path, &["info"]);
    for line in [
        "machine        mips",
        "format         ELF32",
        "type           executable\n",
        "abi            o32, mips2, $gp 0x38850",
        "entry point    0x00020200",
    ] {
        assert!(info.contains(line), "no `{line}` in:\n{info}");
    }
    let json: serde_json::Value = serde_json::from_str(&ras_on(&path, &["--json", "info"])).unwrap();
    assert_eq!((json["machine"].as_str(), json["format"].as_str()), (Some("mips"), Some("elf32")));
    assert_eq!(json["elf"]["big_endian"], false);
}

#[test]
fn ras_names_the_platform_of_the_ps2_hello_ps2() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/ps2/hello.elf");
    let info = ras_on(&path, &["info"]);
    for line in [
        "machine        mips",
        "format         ELF32",
        "platform       ps2",
        "abi            n32, mips3, r5900, $gp 0x1290f0",
        "image base     0x00100000",
        "entry point    0x00100b48",
    ] {
        assert!(info.contains(line), "no `{line}` in:\n{info}");
    }
    let json: serde_json::Value = serde_json::from_str(&ras_on(&path, &["--json", "info"])).unwrap();
    assert_eq!((json["platform"].as_str(), json["format"].as_str()), (Some("ps2"), Some("elf32")));
}

/// The table of the ps2sdk hello lists each kernel stub's `syscall` as a call on the EE kernel's
/// function by its name.
#[test]
fn ras_lists_the_ee_kernel_calls_of_the_ps2_hello() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/ps2/hello.elf");
    let json: serde_json::Value = serde_json::from_str(&ras_on(&path, &["--json", "functions"])).unwrap();
    let rows = json["functions"].as_array().unwrap();
    let syscalls = |symbol: &str| {
        let row = rows.iter().find(|r| r["symbol"] == symbol || r["name"] == symbol).unwrap_or_else(|| panic!("no {symbol} in the table"));
        row["syscalls"]
            .as_array()
            .unwrap_or_else(|| panic!("no syscalls in {row}"))
            .iter()
            .map(|s| s.as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    for (stub, kernel) in
        [("KExit", "KExit"), ("FlushCache", "FlushCache"), ("CreateThread", "CreateThread"), ("sceSifSetDma", "SifSetDma")]
    {
        assert_eq!(syscalls(stub), [kernel], "{stub}");
    }
    assert_eq!(syscalls("main"), Vec::<String>::new(), "main makes no system call of its own");
}

/// The source files of a port's directory, by name, with their texts: `.c` in `dir`, `.rs` under
/// `dir/src`.
fn sources(dir: &Path, extension: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == extension))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read_to_string(&p).unwrap()))
        .collect();
    out.sort();
    out
}

/// The body of the C function `name` in `text`, from its definition (a line at the left margin
/// that names it and is no prototype) to the closing brace at the left margin.
fn c_body<'t>(text: &'t str, name: &str) -> Option<&'t str> {
    let named = format!(" {name}(");
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if !line.starts_with(char::is_whitespace) && line.contains(&named) && line.contains('{') {
            let end = text[at..].find("\n}").map_or(text.len(), |e| at + e);
            return Some(&text[at..end]);
        }
        at += line.len();
    }
    None
}

/// The ps2sdk hello ported over the platform interface: the serial port's registers read and
/// written as calls on the platform (`sio_putc` polls the status register, then writes the
/// transmit FIFO, in that order), the kernel's functions as calls on it too, and beside the port
/// the stub runtime that logs each call, in C (`platform.h`, `platform.c`) and in Rust (at the end
/// of the runtime module, over its `Platform` trait).
#[test]
fn ras_ports_the_ps2_hello_over_the_platform_interface() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/ps2/hello.elf");
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("ps2-platform");
    let (c_dir, rust_dir) = (root.join("c"), root.join("rust"));
    for (lang, dir) in [("c", &c_dir), ("rust", &rust_dir)] {
        let _ = std::fs::remove_dir_all(dir);
        let out = dir.to_string_lossy().into_owned();
        ras_on(&path, &["emit", "--lang", lang, "--out", &out, "--quiet"]);
    }

    let header = std::fs::read_to_string(c_dir.join("platform.h")).expect("no platform.h beside the C port");
    let stub = std::fs::read_to_string(c_dir.join("platform.c")).expect("no platform.c beside the C port");
    for line in ["void ra_sio_write8(uint64_t a0, uint8_t a1);", "uint32_t ra_sio_read32(uint64_t a0);", " ra_ee_FlushCache(uint64_t a0);"]
    {
        assert!(header.contains(line), "no `{line}` in platform.h:\n{header}");
    }
    for text in [
        "const ra_platform ra_platform_stub = ",
        "void ra_sio_write8(uint64_t a0, uint8_t a1) {\n    ra_platform_bound->write(\"sio\", a0, 8, (uint64_t)a1);",
        "ra_platform_bound->call(\"FlushCache\", 1, ",
    ] {
        assert!(stub.contains(text), "no `{text}` in platform.c:\n{stub}");
    }
    let c: String = sources(&c_dir, "c").into_iter().filter(|(n, _)| n != "platform.c").map(|(_, t)| t).collect();
    let putc = c_body(&c, "sio_putc").unwrap_or_else(|| panic!("no sio_putc in the C port"));
    let (read, write) = (putc.find("ra_sio_read32("), putc.find("ra_sio_write8("));
    assert!(read.is_some() && write.is_some() && read < write, "sio_putc does not poll then write the serial port:\n{putc}");
    assert!(!putc.contains("RA_U8(268497280ull)"), "the transmit FIFO is still memory:\n{putc}");
    assert!(c.contains("ra_ee_FlushCache("), "the C port does not call the kernel's FlushCache on the platform");

    let runtime = std::fs::read_to_string(rust_dir.join("src/rhipper.rs")).unwrap();
    for text in [
        "pub trait Platform",
        "pub fn ra_sio_write8(a0: u64, a1: u8) {\n    ra_platform().write(\"sio\", a0, 8, a1 as u64);",
        "pub fn ra_sio_read32(a0: u64) -> u32 {",
        "ra_platform().call(\"FlushCache\", &[a0 as u64])",
    ] {
        assert!(runtime.contains(text), "no `{text}` in the Rust runtime module");
    }
    let rust: String = sources(&rust_dir.join("src"), "rs").into_iter().filter(|(n, _)| n != "rhipper.rs").map(|(_, t)| t).collect();
    for call in ["rhipper::ra_sio_read32(", "rhipper::ra_sio_write8(", "rhipper::ra_ee_FlushCache("] {
        assert!(rust.contains(call), "no `{call}` in the Rust port");
    }

    // The stub runtime is C a compiler takes, over the repository's header as a port's build reads it.
    let runtime_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime");
    match Command::new("clang")
        .arg("-fsyntax-only")
        .arg("-I")
        .arg(&c_dir)
        .arg("-I")
        .arg(&runtime_dir)
        .arg(c_dir.join("platform.c"))
        .output()
    {
        Ok(out) => assert!(out.status.success(), "platform.c does not compile:\n{}", String::from_utf8_lossy(&out.stderr)),
        Err(_) => eprintln!("skipped the compile of platform.c: no clang on the PATH"),
    }
}

/// The EE caller of the IOP module fixture: each `SifCallRpc` names the function of `hello.irx`
/// beside it that its number reaches through the dispatcher the module registers, and the module's
/// own import stubs read by their names.
#[test]
fn ras_resolves_the_ps2_rpc_to_the_iop_module_function() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/ps2");
    let json: serde_json::Value = serde_json::from_str(&ras_on(&dir.join("rpc.elf"), &["--json", "functions"])).unwrap();
    let rpc = json["rpc"].as_array().unwrap_or_else(|| panic!("no rpc in {json:#}"));
    // The SDK's own calls go to the servers of the IOP's ROM modules, which no file here registers.
    let resolved: Vec<_> = rpc
        .iter()
        .filter(|c| c["module"].is_string())
        .map(|c| (c["server"].as_str(), c["function"].as_u64(), c["module"].as_str(), c["target"].as_str(), c["name"].as_str()))
        .collect();
    assert_eq!(
        resolved,
        [
            (Some("0x48454c4f"), Some(0), Some("hello.irx"), Some("0x00100040"), Some("F_00100040")),
            (Some("0x48454c4f"), Some(1), Some("hello.irx"), Some("0x0010013c"), Some("F_0010013c")),
        ],
        "{rpc:#?}"
    );

    let module: serde_json::Value = serde_json::from_str(&ras_on(&dir.join("hello.irx"), &["--json", "functions"])).unwrap();
    let names: Vec<_> = module["functions"].as_array().unwrap().iter().filter_map(|f| f["name"].as_str()).collect();
    for name in ["thunk_sceSifRegisterRpc", "thunk_RegisterLibraryEntries", "thunk_printf", "hello_0", "hello_4"] {
        assert!(names.contains(&name), "no {name} in {names:?}");
    }
}

#[test]
fn ras_prints_the_psp_module_at_its_load_address() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/psp/hello.prx");
    let info = ras_on(&path, &["info"]);
    for line in [
        "machine        mips",
        "type           psp module",
        "module         hello, user, version 1.0, loaded at 0x08804000",
        "IoFileMgrForUser (9)",
        "LoadExecForUser (1)",
        "image base     0x08804000",
        "entry point    0x088040ec",
        "43 imports",
    ] {
        assert!(info.contains(line), "no `{line}` in:\n{info}");
    }
    let json: serde_json::Value = serde_json::from_str(&ras_on(&path, &["--json", "info"])).unwrap();
    assert_eq!(json["elf"]["prx"]["name"], "hello");
    assert_eq!(json["elf"]["prx"]["load"], "0x08804000");
    assert_eq!(json["elf"]["prx"]["relocations"], 3421);

    // Every import by the name its NID stands for, none by the NID itself.
    let imports = ras_on(&path, &["imports"]);
    for name in ["sceIoWrite", "sceKernelExitGame"] {
        assert!(imports.contains(&format!("  {name} ")), "no {name} in:\n{imports}");
    }
    assert!(!imports.lines().any(|l| l.contains("ForUser_")), "an import named by its NID in:\n{imports}");
}

#[test]
fn ras_prints_the_linux_hello() {
    let info = ras(&["info"]);
    for line in ["machine        x86_64", "format         ELF64", "interpreter    /lib64/ld-linux-x86-64.so.2", "needed         libc.so.6"]
    {
        assert!(info.contains(line), "no `{line}` in:\n{info}");
    }
    let json: serde_json::Value = serde_json::from_str(&ras(&["--json", "info"])).unwrap();
    assert_eq!(json["format"], "elf64");
    assert_eq!(json["elf"]["type"], "executable");
    assert_eq!(json["imports"], 2);

    let sections = ras(&["sections"]);
    for name in [".text", ".plt", ".got.plt", ".eh_frame"] {
        assert!(sections.lines().any(|l| l.starts_with(name)), "no {name} in:\n{sections}");
    }

    let symbols = ras(&["symbols"]);
    for name in ["square", "_start"] {
        assert!(symbols.lines().any(|l| l.ends_with(&format!("  {name}"))), "no {name} in:\n{symbols}");
    }

    let imports = ras(&["imports"]);
    assert!(imports.starts_with("libc.so.6\n"), "{imports}");
    assert!(imports.contains("  write (hint 0)") && imports.contains("  exit (hint 0)"), "{imports}");
    assert_eq!(ras(&["exports"]).trim(), "no exports");

    let extents = ras(&["functions", "--pdata"]);
    assert!(extents.contains("2 .eh_frame functions"), "{extents}");
    // The functions by their identifiers: `_start` reads `start`, a PLT stub `thunk_<import>`.
    let functions = ras(&["functions"]);
    for name in ["square", "start", "thunk_write", "thunk_exit"] {
        assert!(functions.lines().any(|l| l.contains(&format!("  {name}"))), "no {name} in:\n{functions}");
    }
}
