//! `ras doctor`: the tools a port needs on this machine, each with its version or `missing` and
//! what it is needed for. A C or C++ port needs clang, clang++, lld, cmake and a generator (ninja
//! or Visual Studio); a Rust port needs cargo; a crash needs `llvm-symbolizer` to be named. The
//! pipeline's `build` step runs the same check first, so a missing tool fails with its name instead
//! of `CMake`'s error.

use std::io::Write;
use std::path::Path;
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

/// What answers an import of a Windows program in its port on Linux or macOS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Answer {
    /// The runtime's Win32 layer, or its version of the import for a 32-bit program.
    Layer,
    /// The host's C library under the same name: an import of a C runtime DLL the layer leaves to it.
    Host,
    /// A stub that stops the program if it is reached: the x86 exception handling of the C runtime.
    Stub,
    /// Nothing: the port does not link.
    Missing,
}

/// One import and what answers it; `symbol` is the runtime's version a 32-bit program calls instead.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct HostImport {
    pub dll: String,
    pub name: String,
    pub answer: Answer,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol: Option<&'static str>,
}

/// The imports of a program off Windows, and whether every one is answered.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct HostReport {
    pub ok: bool,
    pub imports: Vec<HostImport>,
}

/// `ras doctor --host <exe>`: the imports of a Windows program against the runtime's Win32 layer,
/// failing with the ones nothing answers off Windows.
pub(crate) fn host(exe: &Path, json: bool) -> Result<()> {
    let image = ra_asm_pe::load(exe)?;
    if image.elf.is_some() || image.macho.is_some() || image.xbe.is_some() {
        bail!("{} is no Windows program: its imports are its own host's or kernel's", exe.display());
    }
    let report = host_report(&image, crate::emit::DRIVER_C);
    let mut out = std::io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(&mut out, &report)?;
        writeln!(out)?;
    } else {
        for i in &report.imports {
            let answer = match i.answer {
                Answer::Layer => "layer",
                Answer::Host => "host",
                Answer::Stub => "stub",
                Answer::Missing => "missing",
            };
            let symbol = i.symbol.map(|s| format!(" ({s})")).unwrap_or_default();
            writeln!(out, "{:<36} {:<44} {answer}{symbol}", i.dll, i.name)?;
        }
    }
    drop(out);
    if !report.ok {
        let missing: Vec<&str> = report.imports.iter().filter(|i| i.answer == Answer::Missing).map(|i| i.name.as_str()).collect();
        bail!("missing off Windows: {}", missing.join(", "));
    }
    Ok(())
}

/// The answer to every import of `image`, by the functions `driver` defines off Windows.
pub(crate) fn host_report(image: &ra_asm_pe::Image, driver: &str) -> HostReport {
    use ra_asm_lift::raise::prototypes;
    let defined = layer_definitions(driver, image.pointer_size());
    let x86 = !image.is_64();
    let imports: Vec<HostImport> = image
        .imports
        .iter()
        .map(|import| {
            let dll = import.dll.clone();
            // The layer binds by name: an import by ordinal has nothing to bind to.
            let Some(name) = import.name.clone() else {
                return HostImport { dll, name: import.display_name(), answer: Answer::Missing, symbol: None };
            };
            let (answer, symbol) = if let Some(shim) = prototypes::x86_shim(&name).filter(|_| x86) {
                (if defined.contains(shim) { Answer::Layer } else { Answer::Missing }, Some(shim))
            } else if x86 && prototypes::host_lacks(&name) {
                (Answer::Stub, None)
            } else if defined.contains(name.as_str()) {
                (Answer::Layer, None)
            } else if prototypes::is_crt_dll(&dll) {
                (Answer::Host, None)
            } else {
                (Answer::Missing, None)
            };
            HostImport { dll, name, answer, symbol }
        })
        .collect();
    HostReport { ok: imports.iter().all(|i| i.answer != Answer::Missing), imports }
}

/// A conditional of the driver as far as the check reads it: code a Windows host alone compiles,
/// code it does not, code of one pointer width, and anything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Frame {
    Windows,
    Posix,
    Width(u8),
    Other,
}

fn frame_of(condition: &str) -> Frame {
    let c: String = condition.split("//").next().unwrap_or("").chars().filter(|c| !c.is_whitespace()).collect();
    if c.contains("||") {
        Frame::Other
    } else if c == "defined(_WIN32)" || c.starts_with("defined(_WIN32)&&") {
        Frame::Windows
    } else if c.starts_with("!defined(_WIN32)") {
        Frame::Posix
    } else if c.starts_with("RA_POINTER_SIZE==4") {
        Frame::Width(4)
    } else if c.starts_with("RA_POINTER_SIZE==8") {
        Frame::Width(8)
    } else {
        Frame::Other
    }
}

/// The functions `driver` defines for a program of `width`-byte pointers off Windows: the
/// definitions at the start of a line (`uint64_t CreateFileA(...) {`, or a `RA_` macro that
/// defines its first argument) outside the code only a Windows host or the other width compiles.
fn layer_definitions(driver: &str, width: u8) -> std::collections::HashSet<&str> {
    let mut frames: Vec<Frame> = Vec::new();
    let mut defined = std::collections::HashSet::new();
    for line in driver.lines() {
        if let Some(directive) = line.trim_start().strip_prefix('#') {
            let directive = directive.trim_start();
            if let Some(name) = directive.strip_prefix("ifdef ") {
                frames.push(frame_of(&format!("defined({})", name.trim())));
            } else if let Some(name) = directive.strip_prefix("ifndef ") {
                frames.push(frame_of(&format!("!defined({})", name.trim())));
            } else if let Some(condition) = directive.strip_prefix("if ") {
                frames.push(frame_of(condition));
            } else if let Some(condition) = directive.strip_prefix("elif ") {
                if let Some(last) = frames.last_mut() {
                    *last = match *last {
                        Frame::Windows => Frame::Posix,
                        Frame::Posix => Frame::Windows,
                        Frame::Width(w) => Frame::Width(12 - w),
                        Frame::Other => frame_of(condition),
                    };
                }
            } else if directive.starts_with("else") {
                if let Some(last) = frames.last_mut() {
                    *last = match *last {
                        Frame::Windows => Frame::Posix,
                        Frame::Posix => Frame::Windows,
                        Frame::Width(w) => Frame::Width(12 - w),
                        Frame::Other => Frame::Other,
                    };
                }
            } else if directive.starts_with("endif") {
                frames.pop();
            }
            continue;
        }
        if frames.iter().any(|f| *f == Frame::Windows || matches!(f, Frame::Width(w) if *w != width)) {
            continue;
        }
        if line.starts_with(char::is_whitespace) || line.starts_with("static") || line.starts_with("extern") || line.starts_with("typedef")
        {
            continue;
        }
        let Some(open) = line.find('(') else { continue };
        let name = if line.starts_with("RA_") && line.trim_end().ends_with(')') {
            line[open + 1..].split(',').next().unwrap_or("").trim()
        } else if line.trim_end().ends_with('{') {
            line[..open].rsplit(|c: char| c.is_whitespace() || c == '*').next().unwrap_or("")
        } else {
            continue;
        };
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            defined.insert(name);
        }
    }
    defined
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
    use super::{Answer, first_version, host_report, layer_definitions};

    #[test]
    fn the_layer_is_what_the_driver_defines_off_windows_for_the_width() {
        let driver = "#ifdef _WIN32\nuint64_t OnlyWindows(void) {\n#else\nuint64_t Posix(uint64_t a) {\n#endif\n\
                      #if RA_POINTER_SIZE == 8\nuint64_t Wide(void) {\n#endif\nRA_STRTO(ra_x86_strtol, strtol, long, int64_t)\n\
                      static uint64_t hidden(void) {\n    uint64_t inner(void) {\n";
        let four = layer_definitions(driver, 4);
        assert!(four.contains("Posix") && four.contains("ra_x86_strtol"), "{four:?}");
        assert!(!four.contains("OnlyWindows") && !four.contains("Wide") && !four.contains("hidden") && !four.contains("inner"), "{four:?}");
        assert!(layer_definitions(driver, 8).contains("Wide"));
    }

    /// Every import of the rhtest `core` program is answered off Windows: its port runs over the layer.
    #[test]
    fn the_layer_answers_every_import_of_core() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/rhtest/core-O2.exe");
        let image = ra_asm_pe::load(&path).unwrap();
        let report = host_report(&image, crate::emit::DRIVER_C);
        let missing: Vec<&str> = report.imports.iter().filter(|i| i.answer == Answer::Missing).map(|i| i.name.as_str()).collect();
        assert!(report.ok, "missing: {missing:?}");
        let answer = |name: &str| report.imports.iter().find(|i| i.name == name).map(|i| i.answer);
        assert_eq!(answer("WriteFile"), Some(Answer::Layer));
        assert_eq!(answer("_initterm"), Some(Answer::Layer));
        assert_eq!(answer("_except_handler4_common"), Some(Answer::Stub));
        assert_eq!(answer("exit"), Some(Answer::Host));
    }

    #[test]
    fn the_version_is_the_first_dotted_number() {
        assert_eq!(first_version("clang version 22.1.7 (https://github.com/llvm/llvm-project a255c1)").as_deref(), Some("22.1.7"));
        assert_eq!(first_version("llvm-symbolizer\nLLVM (http://llvm.org/):\n  LLVM version 22.1.7\n").as_deref(), Some("22.1.7"));
        assert_eq!(first_version("cargo 1.91.0 (ab1234 2026-10-01)").as_deref(), Some("1.91.0"));
        assert_eq!(first_version("1.13.2").as_deref(), Some("1.13.2"));
        assert_eq!(first_version("x64 only"), None);
    }
}
