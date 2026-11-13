//! A missing tool fails by name: `tool` finds what is on the PATH, panics with the name of what
//! is not, and skips in capitals only under `RA_ALLOW_SKIP`. The variable is read per process,
//! so the two missing-tool tests run this binary again with the environment they need.

use std::process::{Command, Output};

use ra_asm_testing::tools::{ALLOW_SKIP, find_in};
use ra_asm_testing::{Scratch, tool};

/// A name no machine has on its PATH.
const MISSING: &str = "ra-no-such-tool";

/// Set in the child run only: the probe test below does nothing without it.
const PROBE: &str = "RA_TOOLS_PROBE";

/// Runs the `probe` test alone in a child of this binary, with `RA_ALLOW_SKIP` set or removed.
fn run_probe(allow_skip: bool) -> Output {
    let mut command = Command::new(std::env::current_exe().expect("the test binary's path"));
    command.args(["--exact", "probe", "--nocapture", "--test-threads=1"]).env(PROBE, "1");
    if allow_skip {
        command.env(ALLOW_SKIP, "1");
    } else {
        command.env_remove(ALLOW_SKIP);
    }
    command.output().expect("the test binary runs again")
}

/// The child's half of the missing-tool tests: asks for the missing tool and says what it got.
#[test]
fn probe() {
    if std::env::var_os(PROBE).is_none() {
        return;
    }
    let found = tool(MISSING);
    eprintln!("probe returned {found:?}");
}

#[test]
fn a_tool_that_exists_is_found() {
    let scratch = Scratch::new("ra-asm-testing-tools", "found");
    let file = scratch.join(if cfg!(windows) { "ra-fake-tool.exe" } else { "ra-fake-tool" });
    std::fs::write(&file, b"").expect("the fake tool is written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).expect("the fake tool is made executable");
    }
    assert_eq!(find_in("ra-fake-tool", scratch.as_os_str()), Some(file));
    assert_eq!(find_in(MISSING, scratch.as_os_str()), None);

    // The test runs under cargo, so cargo is on the PATH of every machine that gets here.
    let cargo = tool("cargo").expect("cargo is on the PATH");
    assert!(cargo.is_file(), "{} is not a file", cargo.display());
}

#[test]
fn a_tool_that_does_not_exist_panics_with_its_name() {
    let output = run_probe(false);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "the probe passed without the tool: {stderr}");
    assert!(stderr.contains(&format!("{MISSING} is not on the PATH")), "the panic does not name the tool: {stderr}");
    assert!(!stderr.contains("SKIPPED"), "the probe skipped without the variable: {stderr}");
}

#[test]
fn the_variable_turns_the_panic_into_a_skip() {
    let output = run_probe(true);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "the probe failed under {ALLOW_SKIP}: {stderr}");
    assert!(stderr.contains(&format!("SKIPPED: {MISSING}")), "the skip is not printed in capitals: {stderr}");
    assert!(stderr.contains("probe returned None"), "the skip did not return None: {stderr}");
}
