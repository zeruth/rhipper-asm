//! The toolset of an image, read from the image alone: the linker from the rich header (its
//! products and builds) or the optional header's linker version, the compiler from the rich
//! header's compiler entries and the strings a compiler leaves, the C runtime from the startup's
//! shape at the entry point and the runtime DLL it imports, and the fill between functions and
//! the section names as the evidence beside them. The fingerprint names the toolset as a person
//! does (`MSVC 14.51`, `clang/lld`) and keys what depends on it: the C++ ABI, the signature
//! libraries and the prototype table of the C runtime.

use serde::Serialize;

use crate::Image;
use crate::headers::{RichHeader, RichTool};

/// The compiler or linker family of a toolset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Msvc,
    Clang,
    Gcc,
    Rustc,
    Unknown,
}

/// The linker of an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Linker {
    /// Microsoft's `link`, with the rich header's build of it when the header is there.
    Msvc {
        build: Option<u16>,
    },
    /// LLVM's `lld-link`: no rich header, linker version 14.0.
    Lld,
    /// The GNU linker of binutils, whose version is the optional header's.
    GnuLd,
    Unknown,
}

/// The C++ ABI the image's classes, exceptions and mangled names follow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CxxAbi {
    Msvc,
    Itanium,
}

/// The C runtime an image starts through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Crt {
    /// `MSVC` or `MinGW`.
    pub vendor: &'static str,
    /// The runtime DLL the image imports, or `None` when the runtime is linked in.
    pub dll: Option<String>,
}

/// The fingerprint of an image's toolset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Toolset {
    /// The toolset as a person names it: `MSVC 14.51`, `MSVC 16.00`, `clang/lld`, `GCC/ld 2.40`.
    pub name: String,
    pub compiler: Family,
    pub linker: Linker,
    /// The optional header's linker version.
    pub linker_version: (u8, u8),
    /// The MSVC toolset's version when Microsoft's linker made the image: `14.<minor>` from Visual
    /// Studio 2015 on, the compiler's own (`16.00` for Visual Studio 2010) before.
    pub msvc: Option<(u8, u8)>,
    pub crt: Option<Crt>,
    /// The byte that fills the gaps between functions: `0xcc` for MSVC and lld, `0x90` for GNU ld.
    pub fill: Option<u8>,
    /// What the fingerprint read, one line each.
    pub evidence: Vec<String>,
}

impl Toolset {
    /// The fingerprint of `image`.
    pub fn of(image: &Image) -> Self {
        let mut evidence = Vec::new();
        let (major, minor) = image.headers.optional.linker_version;
        let rich = image.headers.rich.as_ref();
        let file = image.file();
        let gcc = contains(file, b"GCC: (");
        let rustc = contains(file, b"/rustc/");
        let mingw = contains(file, b"Mingw-w64 runtime failure");
        if gcc {
            evidence.push("the string `GCC: (` of a GCC object".to_owned());
        }
        if rustc {
            evidence.push("the `/rustc/` paths of the Rust standard library".to_owned());
        }

        let linker = match (rich, rich.and_then(RichHeader::linker)) {
            (_, Some(l)) => {
                evidence.push(format!("the rich header's linker: {} build {}", l.product_name().unwrap_or("link"), l.build));
                Linker::Msvc { build: Some(l.build) }
            }
            (Some(_), None) => {
                evidence.push("a rich header without a linker entry the table names".to_owned());
                Linker::Msvc { build: None }
            }
            (None, None) if major == 14 && minor == 0 => {
                evidence.push("no rich header and linker version 14.0, as lld writes".to_owned());
                Linker::Lld
            }
            (None, None) if (6..=14).contains(&major) => {
                evidence.push(format!("no rich header, linker version {major}.{minor:02} of Microsoft's link"));
                Linker::Msvc { build: None }
            }
            (None, None) if major == 2 => {
                evidence.push(format!("linker version {major}.{minor}, as GNU ld writes"));
                Linker::GnuLd
            }
            (None, None) => Linker::Unknown,
        };
        if image.sections.iter().any(|s| s.name.trim_end_matches('\0') == ".CRT") {
            evidence.push("a `.CRT` section GNU ld keeps apart".to_owned());
        }

        let compiler = if rustc {
            Family::Rustc
        } else if gcc || linker == Linker::GnuLd {
            Family::Gcc
        } else {
            match linker {
                Linker::Msvc { build: Some(build) } => {
                    let cl = rich.is_some_and(|r| r.entries.iter().any(|e| e.tool() == RichTool::Compiler && e.build == build));
                    if cl {
                        evidence.push(format!("compiler entries at the linker's build {build}: the program's objects are cl's"));
                        Family::Msvc
                    } else {
                        evidence
                            .push(format!("no compiler entry at the linker's build {build}: the program's objects carry none, as clang's"));
                        Family::Clang
                    }
                }
                Linker::Msvc { build: None } => Family::Msvc,
                Linker::Lld => Family::Clang,
                Linker::GnuLd | Linker::Unknown => Family::Unknown,
            }
        };

        let msvc = matches!(linker, Linker::Msvc { .. }).then(|| if major >= 14 { (14, minor) } else { (major + 6, minor) });
        let crt = crt_of(image, mingw, &mut evidence);
        let fill = fill_of(image);
        if let Some(fill) = fill {
            evidence.push(format!("the gaps between functions filled with {fill:#04x}"));
        }

        let version = msvc.map(|(major, minor)| format!("{major}.{minor:02}"));
        let name = match (compiler, linker, &version) {
            (Family::Msvc, Linker::Msvc { .. }, Some(v)) => format!("MSVC {v}"),
            (Family::Clang, Linker::Msvc { .. }, Some(v)) => format!("clang/MSVC {v}"),
            (Family::Rustc, Linker::Msvc { .. }, Some(v)) => format!("rustc/MSVC {v}"),
            (Family::Clang, Linker::Lld, _) => "clang/lld".to_owned(),
            (Family::Rustc, Linker::Lld, _) => "rustc/lld".to_owned(),
            (Family::Gcc, Linker::GnuLd, _) => format!("GCC/ld {major}.{minor}"),
            (Family::Rustc, Linker::GnuLd, _) => format!("rustc/ld {major}.{minor}"),
            _ => format!("unknown (linker {major}.{minor})"),
        };
        Self { name, compiler, linker, linker_version: (major, minor), msvc, crt, fill, evidence }
    }

    /// The C++ ABI: the Itanium ABI of GCC and of a MinGW runtime, Microsoft's otherwise, which
    /// clang follows on Windows too.
    pub fn cxx_abi(&self) -> CxxAbi {
        let mingw = self.crt.as_ref().is_some_and(|c| c.vendor == "MinGW");
        if self.compiler == Family::Gcc || mingw { CxxAbi::Itanium } else { CxxAbi::Msvc }
    }

    /// The key of the C runtime's signature libraries and prototype table: `msvc-14.51`,
    /// `msvc-16.00`, `mingw`; `None` for an image without a known C runtime.
    pub fn runtime_key(&self) -> Option<String> {
        let crt = self.crt.as_ref()?;
        match (crt.vendor, self.msvc) {
            ("MSVC", Some((major, minor))) => Some(format!("msvc-{major}.{minor:02}")),
            ("MSVC", None) => Some("msvc".to_owned()),
            _ => Some(crt.vendor.to_lowercase()),
        }
    }
}

/// Whether `needle` occurs in `haystack`.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// The C runtime: a runtime DLL among the imports, else the shape of MSVC's `mainCRTStartup`
/// at the entry point (a call of the cookie's initialization and a jump to the common main,
/// in the frame of the x64 ABI on x64), the same from Visual Studio 2010 on.
fn crt_of(image: &Image, mingw: bool, evidence: &mut Vec<String>) -> Option<Crt> {
    let dll = image.imports.iter().map(|i| i.dll.to_ascii_lowercase()).find(|d| {
        d.starts_with("vcruntime")
            || d.starts_with("ucrtbase")
            || d.starts_with("api-ms-win-crt-")
            || (d.starts_with("msvcr") && d != "msvcrt.dll")
    });
    if mingw {
        evidence.push("the string `Mingw-w64 runtime failure` of the MinGW runtime".to_owned());
        let dll = image.imports.iter().map(|i| i.dll.to_ascii_lowercase()).find(|d| d == "msvcrt.dll" || d.starts_with("api-ms-win-crt-"));
        return Some(Crt { vendor: "MinGW", dll });
    }
    let startup = image.entry_point().and_then(|entry| image.read(entry, 18).ok()).is_some_and(|b| {
        if image.is_64() {
            b[..5] == [0x48, 0x83, 0xec, 0x28, 0xe8] && b[9..14] == [0x48, 0x83, 0xc4, 0x28, 0xe9]
        } else {
            b[0] == 0xe8 && b[5] == 0xe9
        }
    });
    if startup {
        evidence.push("the entry point is MSVC's `mainCRTStartup`: a call, then a jump to the common main".to_owned());
    }
    if let Some(dll) = &dll {
        evidence.push(format!("the C runtime DLL {dll} among the imports"));
    }
    (startup || dll.is_some()).then_some(Crt { vendor: "MSVC", dll })
}

/// The byte of the runs between functions in the code sections: the one of `0xcc` and `0x90`
/// that fills more runs of two or more after a `ret`, `None` when neither shows.
fn fill_of(image: &Image) -> Option<u8> {
    let mut runs = [0usize; 2];
    for section in image.sections.iter().filter(|s| s.is_code() || s.is_executable()) {
        let start = section.virtual_address as usize;
        let end = (start + section.raw_size.min(section.mapped_size()) as usize).min(image.mapped().len());
        let Some(bytes) = image.mapped().get(start..end) else { continue };
        for run in bytes.windows(3).filter(|w| w[0] == 0xc3 && w[1] == w[2]) {
            match run[1] {
                0xcc => runs[0] += 1,
                0x90 => runs[1] += 1,
                _ => {}
            }
        }
    }
    match runs {
        [0, 0] => None,
        [cc, nop] if cc >= nop => Some(0xcc),
        _ => Some(0x90),
    }
}
