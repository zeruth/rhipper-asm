//! Library recognition: signatures made from the libraries themselves, and the functions of an
//! image they name. A signature is a function of an object as the library holds it: its bytes with
//! the relocated ones masked out, and the symbols its relocations name, in order. A function of
//! the image matches when every unmasked byte agrees over the signature's whole length; when
//! several signatures agree on the bytes (the C runtime's one-line stubs do), the names the image
//! calls at the relocated places tell them apart, and what they cannot tell apart is a collision:
//! the first name is taken and the others reported. A match names the function and, when the
//! library is a C runtime's, marks it as the host's through the same symbols and library ranges a
//! PDB fills ([`apply`]), so the lift calls it by name and leaves its body.
//!
//! The format is the repository's own text: a header, `machine` and `library` lines that hold
//! for the lines after them, and one signature per line as `object`, `name`, the pattern in hex
//! with `..` for a masked byte, and the references as `offset:symbol`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use ra_asm_pe::{Image, LibraryRange, Machine, Symbol, Toolset, object};

use crate::{Kind, analyze};

/// One function of a library, as the matcher sees it.
#[derive(Debug, Clone)]
pub struct Signature {
    /// `x86`, `x64`, `arm64`…: the machine of the object.
    pub machine: String,
    /// The library's file name in lower case (`libcmt.lib`), or the object's own for a lone object.
    pub library: String,
    /// The archive member or the file (`printf.obj`).
    pub object: String,
    /// The linkage name as the object has it (`_printf` on x86).
    pub name: String,
    /// Whether the object exports the name: a static function's name is the object's own, and
    /// only an external one is what a host links.
    pub external: bool,
    pub pattern: Vec<u8>,
    /// Whether each byte of `pattern` is compared: false under a relocation.
    pub mask: Vec<bool>,
    /// The symbols the relocations name, by offset into the pattern.
    pub refs: Vec<(u32, String)>,
}

impl Signature {
    pub fn len(&self) -> usize {
        self.pattern.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pattern.is_empty()
    }

    /// How many bytes the signature compares.
    pub fn unmasked(&self) -> usize {
        self.mask.iter().filter(|&&m| m).count()
    }

    /// Whether `bytes` (at least as long as the pattern) agree with the pattern where it is unmasked.
    pub fn matches(&self, bytes: &[u8]) -> bool {
        bytes.len() >= self.pattern.len() && self.pattern.iter().zip(&self.mask).zip(bytes).all(|((&p, &m), &b)| !m || p == b)
    }
}

/// The fewest compared bytes a match stands on its own: with fewer, and no reference the image
/// confirms, a function (`xor eax, eax; ret`) is as likely the program's own, and the match holds
/// only between neighbours that matched on their own, where the linker laid the library's objects
/// side by side.
const MIN_UNMASKED: usize = 8;

/// A set of signatures, from one library or many.
#[derive(Debug, Clone, Default)]
pub struct SignatureSet {
    pub signatures: Vec<Signature>,
}

const HEADER: &str = "# ra signatures 1";

impl SignatureSet {
    /// The signatures of every function of a library's objects: one per distinct function, with
    /// the functions that share bytes and references kept apart by name (a collision the matcher
    /// reports). `library` names the set's source (`libcmt.lib`).
    pub fn generate(library: &object::Library, name: &str) -> Self {
        // The signatures already made, by their bytes, mask and references.
        type Key = (Vec<u8>, Vec<bool>, Vec<(u32, String)>);
        let mut set = Self::default();
        let mut seen: HashMap<Key, Vec<usize>> = HashMap::new();
        for object in &library.objects {
            let machine = object.machine_name();
            for f in &object.functions {
                let section = &object.sections[f.section];
                let start = f.offset as usize;
                let end = (start + f.size as usize).min(section.bytes.len());
                if start >= end {
                    continue;
                }
                // The alignment fill after the function (`int 3`, `nop`) is the object's, not the
                // function's: compared, it would let a six-byte accessor stand on its own.
                let mut end = end;
                while end > start + 1 && matches!(section.bytes[end - 1], 0xCC | 0x90) {
                    end -= 1;
                }
                let mut pattern = section.bytes[start..end].to_vec();
                let mut mask = vec![true; pattern.len()];
                let mut refs = Vec::new();
                for r in &section.relocations {
                    let (ro, rs) = (r.offset as usize, usize::from(r.size));
                    if ro + rs <= start || ro >= end {
                        continue;
                    }
                    // A masked byte is zero: the addend under a relocation is the object's, and
                    // the text format has no place for it.
                    for i in ro.saturating_sub(start)..(ro + rs - start).min(pattern.len()) {
                        mask[i] = false;
                        pattern[i] = 0;
                    }
                    if let Some(symbol) = &r.symbol
                        && ro >= start
                    {
                        refs.push(((ro - start) as u32, symbol.clone()));
                    }
                }
                let key = (pattern.clone(), mask.clone(), refs.clone());
                let same = seen.entry(key).or_default();
                if same.iter().any(|&i| set.signatures[i].name == f.name) {
                    continue;
                }
                same.push(set.signatures.len());
                set.signatures.push(Signature {
                    machine: machine.clone(),
                    library: name.to_lowercase(),
                    object: object.name.clone(),
                    name: f.name.clone(),
                    external: f.external,
                    pattern,
                    mask,
                    refs,
                });
            }
        }
        set
    }

    /// [`generate`](Self::generate) over the library file at `path`, named by its file name.
    pub fn from_library(path: &Path) -> Result<Self> {
        let library = object::read(path)?;
        ensure!(!library.objects.is_empty(), "{} holds no object this reader knows", path.display());
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(Self::generate(&library, &name))
    }

    /// Adds another set's signatures.
    pub fn extend(&mut self, other: Self) {
        self.signatures.extend(other.signatures);
    }

    /// The set in the text format.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(HEADER);
        out.push('\n');
        let (mut machine, mut library) = ("", "");
        let mut order: Vec<&Signature> = self.signatures.iter().collect();
        order.sort_by(|a, b| (&a.machine, &a.library, &a.object, &a.name).cmp(&(&b.machine, &b.library, &b.object, &b.name)));
        for s in order {
            if s.machine != machine {
                machine = &s.machine;
                let _ = writeln!(out, "machine {machine}");
            }
            if s.library != library {
                library = &s.library;
                let _ = writeln!(out, "library {library}");
            }
            let _ = write!(out, "{}\t{}\t{}\t", s.object, if s.external { "ext" } else { "static" }, s.name);
            for (&p, &m) in s.pattern.iter().zip(&s.mask) {
                if m {
                    let _ = write!(out, "{p:02x}");
                } else {
                    out.push_str("..");
                }
            }
            out.push('\t');
            for (i, (offset, name)) in s.refs.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                let _ = write!(out, "{offset}:{name}");
            }
            out.push('\n');
        }
        out
    }

    /// A set from the text format; every refusal names the line.
    pub fn parse(text: &str) -> Result<Self> {
        let mut lines = text.lines().enumerate();
        ensure!(lines.next().is_some_and(|(_, l)| l.trim_end() == HEADER), "not a signature file: the first line is not `{HEADER}`");
        let mut set = Self::default();
        let (mut machine, mut library) = (String::new(), String::new());
        for (i, line) in lines {
            let line = line.trim_end();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(m) = line.strip_prefix("machine ") {
                m.trim().clone_into(&mut machine);
                continue;
            }
            if let Some(l) = line.strip_prefix("library ") {
                l.trim().clone_into(&mut library);
                continue;
            }
            let fields: Vec<&str> = line.split('\t').collect();
            ensure!(
                fields.len() >= 4,
                "line {}: a signature has an object, a linkage, a name, a pattern and references, tab-separated",
                i + 1
            );
            ensure!(!machine.is_empty(), "line {}: no `machine` line before the first signature", i + 1);
            let external = match fields[1] {
                "ext" => true,
                "static" => false,
                other => bail!("line {}: the linkage is `ext` or `static`, not `{other}`", i + 1),
            };
            let hex = fields[3].as_bytes();
            ensure!(hex.len().is_multiple_of(2), "line {}: the pattern has an odd number of digits", i + 1);
            let mut pattern = Vec::with_capacity(hex.len() / 2);
            let mut mask = Vec::with_capacity(hex.len() / 2);
            for pair in hex.chunks(2) {
                if pair == b".." {
                    pattern.push(0);
                    mask.push(false);
                } else {
                    let text = std::str::from_utf8(pair).ok().and_then(|p| u8::from_str_radix(p, 16).ok());
                    let Some(byte) = text else { bail!("line {}: `{}` is not a hex byte", i + 1, String::from_utf8_lossy(pair)) };
                    pattern.push(byte);
                    mask.push(true);
                }
            }
            let mut refs = Vec::new();
            for r in fields.get(4).map(|f| f.split(' ')).into_iter().flatten().filter(|r| !r.is_empty()) {
                let Some((offset, name)) = r.split_once(':') else { bail!("line {}: reference `{r}` is not `offset:symbol`", i + 1) };
                let offset: u32 = offset.parse().with_context(|| format!("line {}: reference `{r}`", i + 1))?;
                refs.push((offset, name.to_owned()));
            }
            set.signatures.push(Signature {
                machine: machine.clone(),
                library: library.clone(),
                object: fields[0].to_owned(),
                name: fields[2].to_owned(),
                external,
                pattern,
                mask,
                refs,
            });
        }
        Ok(set)
    }

    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        Self::parse(&text).with_context(|| path.display().to_string())
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        std::fs::write(path, self.to_text()).with_context(|| format!("cannot write {}", path.display()))
    }
}

/// The static C runtime of the installed MSVC toolset for `arch` (`x86`, `x64`, `arm64`):
/// `libcmt.lib` and `libvcruntime.lib` of the newest toolset (`VCToolsInstallDir` in a developer
/// shell, else the one `vswhere` finds) and `libucrt.lib` of the newest Windows SDK
/// (`WindowsSdkDir` and `UCRTVersion`, else the highest version under `Windows Kits/10/Lib`).
/// `None` without a toolset; the ucrt is left out when there is no SDK.
pub fn installed_msvc_runtime(arch: &str) -> Option<Vec<PathBuf>> {
    let programs = || std::env::var_os("ProgramFiles(x86)").or_else(|| std::env::var_os("ProgramFiles")).map(PathBuf::from);
    let newest = |dir: PathBuf| -> Option<PathBuf> {
        let mut versions: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.filter_map(|e| Some(e.ok()?.path())).collect();
        versions.sort();
        versions.pop()
    };
    let tools = if let Some(dir) = std::env::var_os("VCToolsInstallDir") {
        PathBuf::from(dir)
    } else {
        let vswhere = programs()?.join("Microsoft Visual Studio/Installer/vswhere.exe");
        let output = Command::new(vswhere)
            .args([
                "-latest",
                "-products",
                "*",
                "-requires",
                "Microsoft.VisualStudio.Component.VC.Tools.x86.x64",
                "-property",
                "installationPath",
            ])
            .output()
            .ok()?;
        newest(PathBuf::from(String::from_utf8_lossy(&output.stdout).trim()).join("VC/Tools/MSVC"))?
    };
    let lib = tools.join("lib").join(arch);
    if !lib.join("libcmt.lib").is_file() {
        return None;
    }
    let mut paths = vec![lib.join("libcmt.lib"), lib.join("libvcruntime.lib")];
    let sdk = match (std::env::var_os("WindowsSdkDir"), std::env::var_os("UCRTVersion")) {
        (Some(sdk), Some(version)) => Some(PathBuf::from(sdk).join("Lib").join(version)),
        _ => programs().and_then(|p| newest(p.join("Windows Kits/10/Lib"))),
    };
    if let Some(ucrt) = sdk.map(|s| s.join("ucrt").join(arch).join("libucrt.lib"))
        && ucrt.is_file()
    {
        paths.push(ucrt);
    }
    Some(paths.into_iter().filter(|p| p.is_file()).collect())
}

/// The C runtime libraries whose signatures fit `image`, by its toolset: the installed MSVC static
/// runtime ([`installed_msvc_runtime`]) when the image starts through MSVC's runtime of the 14.x
/// toolsets, which share one runtime from Visual Studio 2015 on; `None` for an older MSVC (Visual
/// Studio 2010's `msvcr100` is another runtime), a MinGW one, or an image without a C runtime, whose
/// functions the installed libraries would only misname.
pub fn runtime_for(image: &Image) -> Option<Vec<PathBuf>> {
    let toolset = Toolset::of(image);
    let arch = match image.headers.coff.machine {
        Machine::I386 => "x86",
        Machine::Amd64 => "x64",
        Machine::Arm64 => "arm64",
        Machine::Other(_) => return None,
    };
    if takes_installed_msvc_runtime(&toolset) { installed_msvc_runtime(arch) } else { None }
}

/// Whether the installed MSVC runtime is the one `toolset` linked: an MSVC runtime of a 14.x
/// toolset, or of a linker that does not say its version (lld over the MSVC runtime).
pub fn takes_installed_msvc_runtime(toolset: &Toolset) -> bool {
    toolset.runtime_key().is_some_and(|key| key == "msvc" || key.starts_with("msvc-14."))
}

/// A function of the image a signature named.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Match {
    pub va: u64,
    pub size: u32,
    pub name: String,
    pub library: String,
    pub object: String,
    /// The other names of the same bytes and references, when the image could not tell them
    /// apart: a collision.
    pub alternatives: Vec<String>,
}

/// A function no signature matched that a match's reference names.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Callee {
    pub va: u64,
    /// The extent of the function's code, as the analysis found it.
    pub size: u32,
    pub name: String,
    /// The library of the match that names it, unless the name is the program's to define
    /// (`main`).
    pub library: Option<String>,
}

/// What [`apply`] did.
#[derive(Debug, Default)]
pub struct Applied {
    /// In address order.
    pub matches: Vec<Match>,
    /// The functions no signature matched that the references of the matches name (the program's
    /// `main`, which the runtime's startup calls; the runtime's `abort` in a shape the library
    /// no longer holds), in address order.
    pub callees: Vec<Callee>,
    /// The functions of the image the signatures were tried on.
    pub functions: usize,
}

impl Applied {
    pub fn collisions(&self) -> impl Iterator<Item = &Match> {
        self.matches.iter().filter(|m| !m.alternatives.is_empty())
    }
}

/// The machine name of an image, as the signatures carry it.
pub fn machine_name(image: &Image) -> &'static str {
    match image.machine() {
        Machine::I386 => "x86",
        Machine::Amd64 => "x64",
        Machine::Arm64 => "arm64",
        Machine::Other(_) => "other",
    }
}

/// Matches `set` against every function of `image` and writes the matches, and the callees they
/// name, into its symbols and library ranges, beside what a PDB or a map already put there (an
/// existing symbol keeps its name). The analysis then names and classifies the matched functions
/// as it does a PDB's.
pub fn apply(image: &mut Image, set: &SignatureSet) -> Applied {
    let applied = matches(image, set);
    let named: BTreeSet<u64> = image.symbols.iter().map(|s| s.va).collect();
    // Every named place with whether a library owns it. The linker laid a library's objects side
    // by side: between two of its functions lies its code too (a tail the analysis split off, a
    // static helper too small to stand on its own), so a match's range runs to the next named
    // place when a library owns that one as well, and stops at its own bytes before the program's
    // `main` or an unnamed function, which may be the program's.
    let mut points: BTreeMap<u64, bool> = image.symbols.iter().map(|s| (s.va, s.library.is_some())).collect();
    points.extend(applied.matches.iter().map(|m| (m.va, true)));
    points.extend(applied.callees.iter().map(|c| (c.va, c.library.is_some())));
    let mut added = Vec::new();
    for m in &applied.matches {
        if !named.contains(&m.va) {
            added.push(Symbol { va: m.va, name: m.name.clone(), library: Some(m.library.clone()) });
        }
        let section_end = image.section_of(m.va).map(|s| image.va(s.end_rva()));
        let size = match points.range(m.va + 1..).next() {
            Some((&next, true)) if section_end.is_some_and(|end| next < end) => (next - m.va).min(u64::from(u32::MAX)) as u32,
            _ => m.size,
        };
        image.libraries.push(LibraryRange { va: m.va, size: size.max(m.size), library: m.library.clone() });
    }
    for c in &applied.callees {
        if !named.contains(&c.va) {
            added.push(Symbol { va: c.va, name: c.name.clone(), library: c.library.clone() });
        }
        if let Some(library) = &c.library {
            image.libraries.push(LibraryRange { va: c.va, size: c.size, library: library.clone() });
        }
    }
    image.symbols.extend(added);
    image.symbols.sort_by_key(|s| s.va);
    image.libraries.sort_by_key(|r| r.va);
    applied
}

/// The matches of `set` over the functions of `image`, without writing them into it.
pub fn matches(image: &Image, set: &SignatureSet) -> Applied {
    let analysis = analyze(image);
    let machine = machine_name(image);
    let by_first: HashMap<u8, Vec<usize>> =
        set.signatures.iter().enumerate().filter(|(_, s)| s.machine == machine && !s.is_empty()).fold(HashMap::new(), |mut acc, (i, s)| {
            if s.mask[0] {
                acc.entry(s.pattern[0]).or_default().push(i);
            }
            acc
        });
    let wild: Vec<usize> =
        set.signatures.iter().enumerate().filter(|(_, s)| s.machine == machine && !s.is_empty() && !s.mask[0]).map(|(i, _)| i).collect();
    let import_slots: HashMap<u64, String> = image.imports.iter().map(|i| (i.iat_va, i.display_name())).collect();
    let thunks: HashMap<u64, String> = analysis
        .functions
        .values()
        .filter_map(|f| if let Kind::ImportThunk(n) = &f.kind { Some((f.entry, n.clone())) } else { None })
        .collect();

    // The candidates of each function: every signature whose unmasked bytes agree and that
    // reaches to the end of the function's code, so that a short function of the library
    // (`_expand`) does not pass for the longer one it is a prefix of (`malloc`). The functions
    // keep their address order for the neighbour rule.
    let mut candidates: Vec<(u64, &[u8], Vec<usize>)> = Vec::new();
    let mut entries: Vec<u64> = Vec::new();
    let mut extents: HashMap<u64, u32> = HashMap::new();
    let mut by_object: HashMap<(&str, &str), Vec<usize>> = HashMap::new();
    for (i, s) in set.signatures.iter().enumerate().filter(|(_, s)| s.machine == machine && !s.is_empty()) {
        by_object.entry((&s.library, &s.object)).or_default().push(i);
    }
    // A signature shorter than the function's code still covers it when the object's next
    // function follows it in the image too (the analysis joins `_strchr`'s dispatch to the
    // variants after it, which the library holds as functions of their own).
    let continues = |s: &Signature, bytes: &[u8]| {
        let mut at = s.len();
        while at < bytes.len() && bytes[at] == 0xCC {
            at += 1;
        }
        by_object.get(&(s.library.as_str(), s.object.as_str())).is_some_and(|same| {
            same.iter().any(|&j| {
                let t = &set.signatures[j];
                t.name != s.name && t.matches(&bytes[at..])
            })
        })
    };
    for f in analysis.functions.values().filter(|f| !f.is_thunk()) {
        entries.push(f.entry);
        extents.insert(f.entry, (f.end().saturating_sub(f.entry)).min(u64::from(u32::MAX)) as u32);
        let Ok(bytes) = image.read_to_section_end(f.entry) else { continue };
        let mut found: Vec<usize> = by_first.get(&bytes[0]).into_iter().flatten().chain(&wild).copied().collect();
        // The function's code without the fill the analysis kept at its end (the `int 3` after a
        // call that does not return), which the signature trimmed on its side.
        let mut code_end = f.end();
        while code_end > f.entry + 1 && bytes.get((code_end - f.entry - 1) as usize).is_some_and(|b| matches!(b, 0xCC | 0x90)) {
            code_end -= 1;
        }
        found.retain(|&i| {
            let s = &set.signatures[i];
            s.matches(bytes) && (f.entry + s.len() as u64 >= code_end || continues(s, bytes))
        });
        if found.is_empty() {
            continue;
        }
        candidates.push((f.entry, bytes, found));
    }

    // The references settle the rest: a call at a relocated place names a function matched in the
    // pass before, a thunk or an import slot. A match stands on its own with enough bytes or a
    // confirmed reference; the smaller ones hold in a run between functions that stand (or the
    // edge of the section), where the linker laid the library's objects side by side. Among the
    // names of the same bytes, the one whose references lead into its own object wins: the
    // startups of `main`, `wmain` and `WinMain` are one byte sequence each, told apart only by the
    // `__scrt_common_main_seh` each calls. A few passes, since a name comes from a match.
    let section_start = |va: u64| image.section_of(va).map(|s| s.virtual_address);
    let mut names: HashMap<u64, (Vec<String>, String)> = HashMap::new();
    let mut matched: Vec<Match> = Vec::new();
    let mut strong_set: BTreeSet<u64> = BTreeSet::new();
    for _ in 0..8 {
        let mut strong: Vec<Match> = Vec::new();
        let mut weak: Vec<Match> = Vec::new();
        for (va, bytes, found) in &candidates {
            let verdicts: Vec<Verdict> = found
                .iter()
                .map(|&i| {
                    let s = &set.signatures[i];
                    let mut v = Verdict { signature: i, agree: 0, disagree: 0, coherent: 0 };
                    for (offset, name) in &s.refs {
                        match resolve_ref(image, &names, &thunks, &import_slots, *va, bytes, s, *offset) {
                            Some(target) if target.names.iter().any(|n| bare(n) == bare(name)) => {
                                v.agree += 1;
                                if target.object.as_deref() == Some(&s.object) {
                                    v.coherent += 1;
                                }
                            }
                            Some(_) => v.disagree += 1,
                            None => {}
                        }
                    }
                    v
                })
                .collect();
            let stands = |v: &Verdict| v.disagree == 0 && (set.signatures[v.signature].unmasked() >= MIN_UNMASKED || v.agree > 0);
            let clean = verdicts.iter().any(|v| v.disagree == 0);
            let is_strong = verdicts.iter().any(stands);
            let mut kept: Vec<Verdict> =
                verdicts.iter().filter(|v| if is_strong { stands(v) } else { !clean || v.disagree == 0 }).copied().collect();
            // The most confirmed references first, then the ones into the same object, then the
            // most compared bytes, then the names a host links, then the name.
            kept.sort_by(|a, b| {
                let (sa, sb) = (&set.signatures[a.signature], &set.signatures[b.signature]);
                (b.agree, b.coherent, sb.unmasked(), sb.external)
                    .cmp(&(a.agree, a.coherent, sa.unmasked(), sa.external))
                    .then(sa.name.cmp(&sb.name))
            });
            let best = (kept[0].agree, kept[0].coherent);
            let first = &set.signatures[kept[0].signature];
            let mut alternatives: Vec<String> = kept[1..]
                .iter()
                .filter(|v| (v.agree, v.coherent) == best)
                .map(|v| set.signatures[v.signature].name.clone())
                .filter(|n| *n != first.name)
                .collect();
            alternatives.dedup();
            let m = Match {
                va: *va,
                size: first.len() as u32,
                name: first.name.clone(),
                library: first.library.clone(),
                object: first.object.clone(),
                alternatives,
            };
            if is_strong { strong.push(m) } else { weak.push(m) }
        }
        // The neighbour rule: a run of weak matches, with nothing unmatched between them, is
        // accepted when a standing match (or the section's edge) closes each end of the run.
        let mut accepted: BTreeMap<u64, Match> = strong.into_iter().map(|m| (m.va, m)).collect();
        let weak: BTreeMap<u64, Match> = weak.into_iter().map(|m| (m.va, m)).collect();
        let mut i = 0;
        while i < entries.len() {
            if !weak.contains_key(&entries[i]) {
                i += 1;
                continue;
            }
            let mut j = i;
            while j < entries.len() && weak.contains_key(&entries[j]) {
                j += 1;
            }
            let section = section_start(entries[i]);
            let before = i == 0 || section_start(entries[i - 1]) != section || accepted.contains_key(&entries[i - 1]);
            let after = j == entries.len() || section_start(entries[j]) != section || accepted.contains_key(&entries[j]);
            if before && after {
                for va in &entries[i..j] {
                    accepted.insert(*va, weak[va].clone());
                }
            }
            i = j;
        }
        strong_set = accepted.keys().copied().collect();
        let next: Vec<Match> = accepted.into_values().collect();
        let settled = next.len() == matched.len()
            && next.iter().zip(&matched).all(|(a, b)| a.va == b.va && a.name == b.name && a.alternatives == b.alternatives);
        matched = next;
        names = matched
            .iter()
            .map(|m| (m.va, (std::iter::once(&m.name).chain(&m.alternatives).cloned().collect(), m.object.clone())))
            .collect();
        if settled {
            break;
        }
    }

    // What the settled matches call that nothing names: the function behind a direct branch at a
    // relocated place takes the reference's name (`_main` from the startup's
    // `__scrt_common_main_seh`), when the match is no collision and the place is a function's
    // entry. It is the caller's library's function too, in a shape the library no longer holds
    // (`abort` of another build of the runtime), unless the name is one a program defines for the
    // runtime to call.
    let mut callees: BTreeMap<u64, Option<Callee>> = BTreeMap::new();
    for m in matched.iter().filter(|m| m.alternatives.is_empty()) {
        let Some((_, bytes, found)) = candidates.iter().find(|c| c.0 == m.va) else { continue };
        let Some(&i) = found.iter().find(|&&i| set.signatures[i].name == m.name && set.signatures[i].object == m.object) else { continue };
        let s = &set.signatures[i];
        for (offset, name) in &s.refs {
            let Some(target) = branch_target(bytes, s, *offset, m.va) else { continue };
            if strong_set.contains(&target) || thunks.contains_key(&target) || entries.binary_search(&target).is_err() {
                continue;
            }
            let library = (!is_program_entry(name)).then(|| m.library.clone());
            match callees.get(&target) {
                None => {
                    callees.insert(
                        target,
                        Some(Callee { va: target, size: extents.get(&target).copied().unwrap_or(1), name: name.clone(), library }),
                    );
                }
                Some(Some(other)) if other.name != *name => {
                    callees.insert(target, None);
                }
                Some(_) => {}
            }
        }
    }
    Applied { matches: matched, callees: callees.into_values().flatten().collect(), functions: entries.len() }
}

/// Whether a linkage name is one of the functions a program defines for its runtime's startup to
/// call: the runtime calls it, and it is the program's own.
fn is_program_entry(name: &str) -> bool {
    matches!(bare(name), "main" | "wmain" | "WinMain" | "wWinMain" | "DllMain" | "matherr")
}

/// What one signature's references say of one function of the image.
#[derive(Clone, Copy)]
struct Verdict {
    signature: usize,
    /// The references the image confirms.
    agree: usize,
    /// The references the image names otherwise.
    disagree: usize,
    /// The confirmed references into a match of the signature's own object.
    coherent: usize,
}

/// What the image names at a relocated place: a matched function (its name and the alternatives
/// of a collision, with its object), a thunk or an import slot.
struct Target {
    names: Vec<String>,
    object: Option<String>,
}

/// The target of a `call`/`jmp`/`jcc` whose rel32 is at `offset` of the function at `va`;
/// `None` when the place holds no branch.
fn branch_target(bytes: &[u8], signature: &Signature, offset: u32, va: u64) -> Option<u64> {
    let o = offset as usize;
    if o + 4 > bytes.len() || o + 4 > signature.len() {
        return None;
    }
    let is_branch = (o >= 1 && matches!(bytes[o - 1], 0xE8 | 0xE9)) || (o >= 2 && bytes[o - 2] == 0x0F && bytes[o - 1] & 0xF0 == 0x80);
    let word = u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    is_branch.then(|| (va + o as u64 + 4).wrapping_add(i64::from(word as i32) as u64))
}

/// What the image names at the relocated place `offset` of the function at `va`: a
/// `call`/`jmp`/`jcc` to a matched function or a thunk, a slot of an import, or an absolute (or
/// RIP-relative) address of a matched function. `None` when the place names nothing the image
/// knows yet.
#[allow(clippy::too_many_arguments)]
fn resolve_ref(
    image: &Image,
    names: &HashMap<u64, (Vec<String>, String)>,
    thunks: &HashMap<u64, String>,
    slots: &HashMap<u64, String>,
    va: u64,
    bytes: &[u8],
    signature: &Signature,
    offset: u32,
) -> Option<Target> {
    let o = offset as usize;
    if o + 4 > bytes.len() || o + 4 > signature.len() {
        return None;
    }
    let matched = |at: u64| names.get(&at).map(|(names, object)| Target { names: names.clone(), object: Some(object.clone()) });
    let known = |at: u64| thunks.get(&at).or_else(|| slots.get(&at)).map(|name| Target { names: vec![name.clone()], object: None });
    if let Some(target) = branch_target(bytes, signature, offset, va) {
        return matched(target).or_else(|| known(target));
    }
    let word = u32::from_le_bytes([bytes[o], bytes[o + 1], bytes[o + 2], bytes[o + 3]]);
    let at = if image.is_64() { (va + o as u64 + 4).wrapping_add(i64::from(word as i32) as u64) } else { u64::from(word) };
    known(at).or_else(|| matched(at))
}

/// A name with its linkage decoration off, for the comparison of a reference with what the image
/// names: `__imp__GetLastError@4`, `_GetLastError@4` and `GetLastError` are one.
fn bare(name: &str) -> &str {
    let name = name.strip_prefix("__imp_").unwrap_or(name);
    if name.starts_with('?') {
        return name;
    }
    let name = name.strip_prefix(['_', '@']).unwrap_or(name);
    name.split('@').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::{Signature, SignatureSet, bare};

    #[test]
    fn the_text_format_round_trips() {
        let set = SignatureSet {
            signatures: vec![
                Signature {
                    machine: "x86".into(),
                    library: "libcmt.lib".into(),
                    object: "exe_main.obj".into(),
                    name: "_mainCRTStartup".into(),
                    external: true,
                    pattern: vec![0xE8, 0, 0, 0, 0, 0xE9, 0, 0, 0, 0],
                    mask: vec![true, false, false, false, false, true, false, false, false, false],
                    refs: vec![(1, "___security_init_cookie".into()), (6, "___scrt_common_main".into())],
                },
                Signature {
                    machine: "x86".into(),
                    library: "libcmt.lib".into(),
                    object: "matherr.obj".into(),
                    name: "__matherr".into(),
                    external: false,
                    pattern: vec![0x33, 0xC0, 0xC3],
                    mask: vec![true; 3],
                    refs: vec![],
                },
            ],
        };
        let text = set.to_text();
        assert!(
            text.contains("exe_main.obj\text\t_mainCRTStartup\te8........e9........\t1:___security_init_cookie 6:___scrt_common_main\n"),
            "{text}"
        );
        assert!(text.contains("matherr.obj\tstatic\t__matherr\t33c0c3\t\n"), "{text}");
        let back = SignatureSet::parse(&text).unwrap();
        assert_eq!(back.signatures.len(), 2);
        let s = &back.signatures[0];
        assert_eq!(
            (s.machine.as_str(), s.library.as_str(), s.object.as_str(), s.name.as_str()),
            ("x86", "libcmt.lib", "exe_main.obj", "_mainCRTStartup")
        );
        assert!(s.external && !back.signatures[1].external);
        assert_eq!(s.pattern, set.signatures[0].pattern);
        assert_eq!(s.mask, set.signatures[0].mask);
        assert_eq!(s.refs, set.signatures[0].refs);
        assert_eq!(back.signatures[1].refs, vec![]);
        assert!(s.matches(&[0xE8, 1, 2, 3, 4, 0xE9, 5, 6, 7, 8, 9]));
        assert!(!s.matches(&[0xE8, 1, 2, 3, 4, 0xE8, 5, 6, 7, 8]));
        assert!(!s.matches(&[0xE8, 1, 2, 3]));
        assert!(SignatureSet::parse("hello\n").is_err());
        assert!(SignatureSet::parse("# ra signatures 1\nmachine x86\nx.obj\text\t_f\tzz\t\n").is_err());
        assert!(SignatureSet::parse("# ra signatures 1\nmachine x86\nx.obj\tweak\t_f\t33c0\t\n").is_err());
    }

    #[test]
    fn a_reference_compares_without_its_decoration() {
        assert_eq!(bare("__imp__GetLastError@4"), "GetLastError");
        assert_eq!(bare("_GetLastError@4"), "GetLastError");
        assert_eq!(bare("@__security_check_cookie@4"), "__security_check_cookie");
        assert_eq!(bare("printf"), "printf");
        assert_eq!(bare("__imp_printf"), "printf");
        assert_eq!(bare("?area@Rect@@UEBANXZ"), "?area@Rect@@UEBANXZ");
    }
}
