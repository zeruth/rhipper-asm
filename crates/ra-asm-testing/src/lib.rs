//! The scratch directory of a test: `<temp>/<suite>/<build>-<pid>-<name>`, made by
//! [`Scratch::new`] and removed, with its suite directory once that is empty, when the guard
//! drops, whether the test passed or failed. `<build>` names the checkout and the directory the test
//! binaries were built in, so the runs of several checkouts at once, even into one target
//! directory, tell their directories apart. A person who
//! wants to read a test's output sets `RA_KEEP_SCRATCH` to any value: every scratch directory then
//! stays and names its path on stderr.

use std::ffi::OsStr;
use std::ops::Deref;
use std::path::{Path, PathBuf};

/// The environment variable that keeps every scratch directory.
pub const KEEP: &str = "RA_KEEP_SCRATCH";

/// A directory under the system temp that lives as long as the guard.
#[derive(Debug)]
pub struct Scratch {
    path: PathBuf,
}

impl Scratch {
    /// Makes `<temp>/<suite>/<build>-<pid>-<name>`, empty: a directory left by a kept or killed
    /// run of the same process id goes first.
    pub fn new(suite: &str, name: &str) -> Self {
        let path = suite_dir(suite).join(format!("{}-{}-{name}", build(), std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        // Another test of the process may remove the suite directory between its creation and
        // the directory's own: the second attempt finds it gone and makes both again.
        if std::fs::create_dir_all(&path).is_err() {
            std::fs::create_dir_all(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        }
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The scratch directories of `suite` that the test binaries of this build made in other
    /// processes: what a run of the tests must not leave behind. The runs of other checkouts,
    /// and the processes still running, which may own their directories yet, are not counted.
    pub fn foreign(suite: &str) -> Vec<PathBuf> {
        let build = format!("{}-", build());
        let Ok(entries) = std::fs::read_dir(suite_dir(suite)) else {
            return Vec::new();
        };
        entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let Some(name) = p.file_name().map(OsStr::to_string_lossy) else {
                    return false;
                };
                let Some(pid) = name.strip_prefix(&build).and_then(|rest| rest.split('-').next()).and_then(|pid| pid.parse::<u32>().ok())
                else {
                    return false;
                };
                pid != std::process::id() && !running(pid)
            })
            .collect()
    }
}

/// Whether the process `pid` is running: a test binary of another run on the same build, whose
/// directories are its own to remove.
fn running(pid: u32) -> bool {
    if cfg!(windows) {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).contains(&format!("\"{pid}\"")))
    } else {
        Path::new(&format!("/proc/{pid}")).exists()
    }
}

fn suite_dir(suite: &str) -> PathBuf {
    std::env::temp_dir().join(suite)
}

/// A short name of the checkout and of the directory this test binary lives in, which every test
/// binary of one build shares: checkouts that build into one target directory stay apart.
fn build() -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    env!("CARGO_MANIFEST_DIR").hash(&mut hasher);
    std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)).hash(&mut hasher);
    format!("{:08x}", hasher.finish() & 0xffff_ffff)
}

impl Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<OsStr> for Scratch {
    fn as_ref(&self) -> &OsStr {
        self.path.as_os_str()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if std::env::var_os(KEEP).is_some() {
            eprintln!("{KEEP} is set: {} is kept", self.path.display());
            return;
        }
        // A program the test just ran may still be held open for a moment on Windows.
        let mut removed = std::fs::remove_dir_all(&self.path);
        for _ in 0..20 {
            if removed.is_ok() || !self.path.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            removed = std::fs::remove_dir_all(&self.path);
        }
        if let Err(e) = removed
            && self.path.exists()
        {
            eprintln!("{}: not removed: {e}", self.path.display());
        } else if std::thread::panicking() {
            eprintln!("{} is removed; set {KEEP} to keep a test's output", self.path.display());
        }
        // The suite directory goes with its last scratch; a sibling of another test keeps it.
        if let Some(suite) = self.path.parent() {
            let _ = std::fs::remove_dir(suite);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Scratch;

    #[test]
    fn a_scratch_goes_with_its_guard() {
        let suite = "ra-asm-testing-tests";
        let kept = Scratch::new(suite, "kept");
        let path = {
            let scratch = Scratch::new(suite, "gone");
            std::fs::write(scratch.join("a.txt"), "a").unwrap();
            std::fs::create_dir(scratch.join("sub")).unwrap();
            scratch.path().to_path_buf()
        };
        assert!(!path.exists(), "{} outlived its guard", path.display());
        assert!(kept.is_dir() && path.parent().unwrap().is_dir(), "the suite directory went with a sibling still in it");
        assert!(Scratch::foreign(suite).is_empty(), "{:?}", Scratch::foreign(suite));
        let suite_dir = kept.parent().unwrap().to_path_buf();
        drop(kept);
        assert!(!suite_dir.exists(), "{} outlived its last scratch", suite_dir.display());
    }
}
