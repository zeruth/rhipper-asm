//! The version: `<major>.<minor>.<n>`, the workspace's major and minor with `n` counting the
//! commits of the tree the build is of, as Rhipper Agent numbers its own, so a build names the
//! change it carries and two checkouts at one commit name the same. A tree with changes past its
//! commit adds `+` to the number. Without git the count is zero.

use std::path::Path;
use std::process::Command;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(&root)
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    };
    let commits = git(&["rev-list", "--count", "HEAD"]).and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
    let dirty =
        git(&["status", "--porcelain", "--untracked-files=no", "--", "crates", "apps", "runtime"]).is_some_and(|s| !s.trim().is_empty());
    let (major, minor) = (env!("CARGO_PKG_VERSION_MAJOR"), env!("CARGO_PKG_VERSION_MINOR"));
    println!("cargo:rustc-env=RAS_VERSION={major}.{minor}.{commits}{}", if dirty { "+" } else { "" });
    // a new commit, a branch moved or a source changed is a new version
    for path in [".git/HEAD", ".git/refs/heads", ".git/logs/HEAD", "crates", "apps", "runtime"] {
        println!("cargo:rerun-if-changed={}", root.join(path).display());
    }
}
