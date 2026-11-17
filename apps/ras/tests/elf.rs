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
