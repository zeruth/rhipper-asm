//! `ras doctor`: the tools a port needs on this machine, each with its version or `missing` and
//! what it is needed for. A C or C++ port needs clang, clang++, lld, cmake and a generator (ninja
//! or Visual Studio); a Rust port needs cargo; a crash needs `llvm-symbolizer` to be named. The
//! pipeline's `build` step runs the same check first, so a missing tool fails with its name instead
//! of `CMake`'s error.

use std::io::Write;
use std::process::Command;

use anyhow::{Result, bail};
use serde::Serialize;

/// The port the tools are checked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Port {
    /// A C or C++ port built by `CMake`.
    C,
    /// A Rust port built by cargo.
    Rust,
}

impl Port {
    pub(crate) fn of(lang: &str) -> Port {
        if lang.eq_ignore_ascii_case("rust") || lang.eq_ignore_ascii_case("rs") { Port::Rust } else { Port::C }
    }
}

/// One tool: present with a version, or missing.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Check {
    pub name: &'static str,
    /// The command that answered `--version`, or `vswhere` for Visual Studio.
    pub command: &'static str,
    pub version: Option<String>,
    #[serde(rename = "for")]
    pub needed_for: &'static str,
    /// Whether the port cannot be built without it.
    pub required: bool,
}

/// The report of a check: every tool, and whether every required one is present.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct Report {
    pub ok: bool,
    pub tools: Vec<Check>,
}

impl Report {
    /// The missing required tools, each with what it is needed for; empty when the port can build.
    fn missing(&self) -> Vec<String> {
        self.tools.iter().filter(|t| t.required && t.version.is_none()).map(|t| format!("{} ({})", t.name, t.needed_for)).collect()
    }

    /// The message of a failed check: `missing: cmake (the build of a C or C++ port), ...`.
    pub(crate) fn failure(&self) -> Option<String> {
        let missing = self.missing();
        (!missing.is_empty()).then(|| format!("missing: {}; run `ras doctor`", missing.join(", ")))
    }
}

/// Checks every tool for a port of `port`.
pub(crate) fn check(port: Port) -> Report {
    let c = port == Port::C;
    let ninja = version_of("ninja");
    let studio = visual_studio();
    let tools = vec![
        Check {
            name: "clang",
            command: "clang",
            version: version_of("clang"),
            needed_for: "the C compiler of a C or C++ port",
            required: c,
        },
        Check {
            name: "clang++",
            command: "clang++",
            version: version_of("clang++"),
            needed_for: "the C++ compiler of a C++ port",
            required: c,
        },
        Check {
            name: "lld",
            command: "lld-link",
            version: version_of("lld-link"),
            needed_for: "the linker of a C or C++ port",
            required: c,
        },
        Check { name: "cmake", command: "cmake", version: version_of("cmake"), needed_for: "the build of a C or C++ port", required: c },
        // One generator is enough: each is required only while the other is missing.
        Check {
            name: "ninja",
            command: "ninja",
            version: ninja.clone(),
            needed_for: "the build of a C or C++ port, or Visual Studio",
            required: c && studio.is_none(),
        },
        Check {
            name: "visual studio",
            command: "vswhere",
            version: studio,
            needed_for: "the build of a C or C++ port, or ninja",
            required: c && ninja.is_none(),
        },
        Check { name: "cargo", command: "cargo", version: version_of("cargo"), needed_for: "the build of a Rust port", required: !c },
        Check {
            name: "llvm-symbolizer",
            command: "llvm-symbolizer",
            version: version_of("llvm-symbolizer"),
            needed_for: "naming the function of a crash",
            required: false,
        },
    ];
    let report = Report { ok: true, tools };
    Report { ok: report.missing().is_empty(), ..report }
}

/// `ras doctor`: prints the check and fails when a required tool is missing.
pub(crate) fn doctor(port: Port, json: bool) -> Result<()> {
    let report = check(port);
    let mut out = std::io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(&mut out, &report)?;
        writeln!(out)?;
    } else {
        for t in &report.tools {
            let version = t.version.as_deref().unwrap_or("missing");
            let need = if t.required { "required: " } else { "" };
            writeln!(out, "{:<16} {:<14} {need}{}", t.name, version, t.needed_for)?;
        }
    }
    drop(out);
    if let Some(failure) = report.failure() {
        bail!("{failure}");
    }
    Ok(())
}

/// The version `tool --version` prints: its first `digits.digits[.digits]`; `None` when the tool
/// does not run.
fn version_of(tool: &str) -> Option<String> {
    let output = Command::new(tool).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    Some(first_version(&text).unwrap_or_else(|| "present".to_owned()))
}

/// The first `digits.digits[.digits...]` of a text.
fn first_version(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()) {
            let start = i;
            let mut dots = 0;
            while i < bytes.len()
                && (bytes[i].is_ascii_digit() || (bytes[i] == b'.' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit()))
            {
                dots += usize::from(bytes[i] == b'.');
                i += 1;
            }
            if dots >= 1 {
                return Some(text[start..i].to_owned());
            }
        } else {
            i += 1;
        }
    }
    None
}

/// The version of the newest Visual Studio with the C++ tools, through `vswhere`.
fn visual_studio() -> Option<String> {
    let installer = std::env::var_os("ProgramFiles(x86)").or_else(|| std::env::var_os("ProgramFiles"))?;
    let vswhere = std::path::Path::new(&installer).join("Microsoft Visual Studio/Installer/vswhere.exe");
    if !vswhere.is_file() {
        return None;
    }
    let output = Command::new(vswhere)
        .args([
            "-latest",
            "-products",
            "*",
            "-requires",
            "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
            "-property",
            "installationVersion",
        ])
        .output()
        .ok()?;
    let version = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (output.status.success() && !version.is_empty()).then_some(version)
}

#[cfg(test)]
mod tests {
    use super::first_version;

    #[test]
    fn the_version_is_the_first_dotted_number() {
        assert_eq!(first_version("clang version 22.1.7 (https://github.com/llvm/llvm-project a255c1)").as_deref(), Some("22.1.7"));
        assert_eq!(first_version("llvm-symbolizer\nLLVM (http://llvm.org/):\n  LLVM version 22.1.7\n").as_deref(), Some("22.1.7"));
        assert_eq!(first_version("cargo 1.91.0 (ab1234 2026-10-01)").as_deref(), Some("1.91.0"));
        assert_eq!(first_version("1.13.2").as_deref(), Some("1.13.2"));
        assert_eq!(first_version("x64 only"), None);
    }
}
