//! The tools a test needs from the PATH. A test that prints "not on the PATH" and returns is a
//! pass that checked nothing, so [`tool`] panics with the tool's name when it is missing. A
//! person on a bare machine sets `RA_ALLOW_SKIP=1`: the missing tool then prints a line that
//! starts with `SKIPPED` and the test returns early.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// The environment variable that turns a missing tool into a skip: any value but empty or `0`.
pub const ALLOW_SKIP: &str = "RA_ALLOW_SKIP";

/// The path of `name` on the PATH. `None` only under [`ALLOW_SKIP`], after a `SKIPPED` line on
/// stderr: the caller returns from its test.
///
/// # Panics
///
/// When the tool is missing and [`ALLOW_SKIP`] is not set; the message starts with its name.
#[must_use]
pub fn tool(name: &str) -> Option<PathBuf> {
    let found = std::env::var_os("PATH").and_then(|path| find_in(name, &path));
    if found.is_some() {
        return found;
    }
    assert!(skip_allowed(), "{name} is not on the PATH and the test needs it: install it, or set {ALLOW_SKIP}=1 to skip");
    eprintln!("SKIPPED: {name} is not on the PATH ({ALLOW_SKIP} is set)");
    None
}

/// Whether [`ALLOW_SKIP`] is set in this process.
#[must_use]
pub fn skip_allowed() -> bool {
    std::env::var_os(ALLOW_SKIP).is_some_and(|value| !value.is_empty() && value != "0")
}

/// The first file named `name` in the directories of `path`, a PATH-shaped list. On Windows the
/// name also matches with each extension of `PATHEXT`.
#[must_use]
pub fn find_in(name: &str, path: &OsStr) -> Option<PathBuf> {
    let names = candidates(name);
    std::env::split_paths(path).flat_map(|dir| names.iter().map(move |file| dir.join(file))).find(|file| runnable(file))
}

#[cfg(windows)]
fn candidates(name: &str) -> Vec<String> {
    let extensions = std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_owned());
    let mut names = vec![name.to_owned()];
    names.extend(extensions.split(';').filter(|ext| !ext.is_empty()).map(|ext| format!("{name}{}", ext.to_ascii_lowercase())));
    names
}

#[cfg(not(windows))]
fn candidates(name: &str) -> Vec<String> {
    vec![name.to_owned()]
}

#[cfg(unix)]
fn runnable(file: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(file).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn runnable(file: &Path) -> bool {
    file.is_file()
}
