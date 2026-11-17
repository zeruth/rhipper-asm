//! `ras disasm --at`: several functions' listings in one call, each under a line naming it, in the
//! order given, so no reader loops over addresses.

use std::path::{Path, PathBuf};
use std::process::Command;

fn control64() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/control64.exe")
}

/// Runs `ras disasm <control64> <args>`.
fn disasm(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_ras")).arg("disasm").arg(control64()).args(args).output().unwrap();
    assert!(output.status.success(), "ras disasm {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn at_lists_each_function_in_the_order_given_and_names_a_non_start() {
    // Repeated and comma-separated alike; the third address lies inside print_int.
    let out = disasm(&["--at", "0x1400013f0,F_140001330", "--at", "0x140001334", "--at", "140001660"]);
    let headers: Vec<&str> = out.lines().filter(|l| l.starts_with("== ")).collect();
    assert_eq!(
        headers,
        [
            "== 0x1400013f0 fib ==",
            "== 0x140001330 print_int ==",
            "== 0x140001334 starts no function, inside print_int at 0x140001330 ==",
            "== 0x140001660 op_add ==",
        ],
        "{out}"
    );

    // Each listing follows its line and holds that function's instructions alone.
    let sections: Vec<&str> = out.split("== ").skip(1).collect();
    for (section, (lo, hi)) in
        sections.iter().zip([(0x1400013f0u64, 0x140001430u64), (0x140001330, 0x1400013c0), (0, 0), (0x140001660, 0x140001670)])
    {
        let addresses: Vec<u64> = section
            .lines()
            .skip(1)
            .map(|l| u64::from_str_radix(l.split_whitespace().next().unwrap().trim_start_matches("0x"), 16).unwrap())
            .collect();
        if lo == 0 {
            assert!(addresses.is_empty(), "a listing for an address that starts no function:\n{section}");
            continue;
        }
        assert_eq!(addresses.first(), Some(&lo), "{section}");
        assert!(addresses.iter().all(|&a| (lo..hi).contains(&a)), "an instruction outside [{lo:#x}, {hi:#x}):\n{section}");
    }

    // The listing of a function is the one a single address prints for it.
    let fib = disasm(&["0x1400013f0"]);
    assert!(fib.starts_with(sections[0].lines().nth(1).unwrap()), "{fib}\n---\n{}", sections[0]);
}

#[test]
fn at_as_json_gives_a_row_per_address() {
    let output = Command::new(env!("CARGO_BIN_EXE_ras"))
        .args(["--json", "disasm"])
        .arg(control64())
        .args(["--at", "0x140001660,0x140001661"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let rows: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows[0]["address"], "0x140001660");
    assert_eq!(rows[0]["name"], "op_add");
    assert!(rows[0]["instrs"].as_array().is_some_and(|i| !i.is_empty()), "{rows}");
    assert_eq!(rows[1]["address"], "0x140001661");
    assert!(rows[1]["name"].is_null() && rows[1]["instrs"].is_null(), "{rows}");
    assert_eq!(rows[1]["inside"], "op_add");
}
