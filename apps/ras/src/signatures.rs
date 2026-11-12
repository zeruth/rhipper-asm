//! `ras signatures`: what each function takes and returns, as the lift infers it; with `--from`,
//! the library recognition: which functions the signatures of the given libraries name, and the
//! collisions among them. [`apply_files`] is the same recognition for every other command, from
//! `--signatures` or the project's `signatures` key.
//!
//! The store keeps signature files outside the repository, one per library and machine, as
//! `<store>/<toolset>/<machine>/<library>.sig` (`msvc-14.44/x86/libcmt.lib.sig`): the toolset is
//! the key of the fingerprint ([`ra_asm_pe::Toolset::runtime_key`]), so the files of every
//! version stand side by side, and the file's header carries the version of the format. The
//! user's store is `RHIPPER_SIGNATURES`, else `~/.rhipper/signatures` (an empty
//! `RHIPPER_SIGNATURES` turns it off); a project's own is `rhipper/signatures` beside its
//! `rhipper.json`, and hides the user's files of the same library. `ras signatures --build`
//! writes a store, from libraries or from the installed MSVC runtime, as the releases publish
//! it for the toolsets a person does not have; `--list` says what a store holds and which files
//! an image's fingerprint picks from it.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail, ensure};
use ra_asm_analysis::signatures::{Applied, SignatureSet};
use ra_asm_lift::raise::abi::{Kind, infer};
use ra_asm_pe::Image;
use serde_json::json;

use crate::report::hex;

/// Whether a path is a library or an object the signatures are made from, rather than a
/// signature file.
fn is_library(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "lib" | "a" | "obj" | "o"))
}

/// The signatures of `files`: each a signature file, or a library made into one.
pub(crate) fn read_files(files: &[PathBuf]) -> Result<SignatureSet> {
    let mut set = SignatureSet::default();
    for file in files {
        let read = if is_library(file) { SignatureSet::from_library(file) } else { SignatureSet::read(file) };
        set.extend(read.with_context(|| format!("signatures from {}", file.display()))?);
    }
    Ok(set)
}

/// Applies the signatures of `files` to `image`, so the analysis names what they match.
pub(crate) fn apply_files(image: &mut Image, files: &[PathBuf]) -> Result<Applied> {
    let set = read_files(files)?;
    Ok(ra_asm_analysis::signatures::apply(image, &set))
}

/// The environment variable that names the user's store; set and empty, there is none.
pub(crate) const STORE_ENV: &str = "RHIPPER_SIGNATURES";

/// A project's own store, under its root.
pub(crate) const PROJECT_STORE: &str = "rhipper/signatures";

/// The user's store: `RHIPPER_SIGNATURES`, else `.rhipper/signatures` in the home directory.
pub(crate) fn user_store() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(STORE_ENV) {
        return (!dir.is_empty()).then(|| PathBuf::from(dir));
    }
    let home = if cfg!(windows) { std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) } else { std::env::var_os("HOME") };
    home.filter(|h| !h.is_empty()).map(|h| PathBuf::from(h).join(".rhipper/signatures"))
}

/// The file of `library`'s signatures for `machine` under the toolset `key` in `store`.
pub(crate) fn store_file(store: &Path, key: &str, machine: &str, library: &str) -> PathBuf {
    store.join(key).join(machine).join(format!("{}.sig", library.to_lowercase()))
}

/// The library a signature file or a library file stands for, by its name: `libcmt.lib` for
/// `libcmt.lib.sig` and for `libcmt.lib` itself.
pub(crate) fn library_of(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_string_lossy().to_lowercase();
    Some(name.strip_suffix(".sig").map_or_else(|| name.clone(), str::to_owned))
}

/// Writes the signatures of `libraries` into `store` under the toolset `key`, a file per library
/// and machine, in place of what the store held there; returns each file with its count.
pub(crate) fn build_store(store: &Path, key: &str, libraries: &[PathBuf]) -> Result<Vec<(PathBuf, usize)>> {
    ensure!(
        !key.is_empty() && key != "." && key != ".." && !key.contains(['/', '\\', ':']),
        "`{key}` is not a toolset key: one name, such as msvc-14.44"
    );
    let mut written = Vec::new();
    for library in libraries {
        let set = SignatureSet::from_library(library).with_context(|| format!("signatures from {}", library.display()))?;
        let Some(name) = library_of(library) else { bail!("{} names no library", library.display()) };
        let mut by_machine: BTreeMap<String, SignatureSet> = BTreeMap::new();
        for signature in set.signatures {
            by_machine.entry(signature.machine.clone()).or_default().signatures.push(signature);
        }
        for (machine, set) in by_machine {
            let file = store_file(store, key, &machine, &name);
            if let Some(dir) = file.parent() {
                std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
            }
            set.write(&file)?;
            written.push((file, set.signatures.len()));
        }
    }
    Ok(written)
}

/// The toolset keys of `store` that serve an image of the toolset `key`, best first: the key
/// itself, then, for MSVC's runtime of the 14.x toolsets, which every one from Visual Studio 2015
/// on shares, the store's other 14.x versions, newest first.
fn keys_for(store: &Path, key: &str) -> Vec<String> {
    let mut keys = vec![key.to_owned()];
    if key == "msvc" || key.starts_with("msvc-14.") {
        let mut others: Vec<String> = std::fs::read_dir(store)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|k| k.starts_with("msvc-14.") && k != key)
            .collect();
        // The minor version is written in two digits, so the text orders as the versions do.
        others.sort_unstable_by(|a, b| b.cmp(a));
        keys.extend(others);
    }
    keys
}

/// The signature files of a directory of the store, in name order.
fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("sig")))
        .collect();
    files.sort();
    files
}

/// The files `stores` give an image of the toolset `key` and `machine`: in each store, those of
/// the first of [`keys_for`] it holds files of; a library an earlier store gave hides a later
/// store's.
pub(crate) fn pick(stores: &[PathBuf], key: &str, machine: &str) -> Vec<PathBuf> {
    let mut libraries = BTreeSet::new();
    let mut files = Vec::new();
    for store in stores {
        let Some(found) = keys_for(store, key).iter().map(|k| files_in(&store.join(k).join(machine))).find(|f| !f.is_empty()) else {
            continue;
        };
        files.extend(found.into_iter().filter(|f| library_of(f).is_some_and(|l| libraries.insert(l))));
    }
    files
}

/// The files `stores` give `image`, by its fingerprint; none for an image without a known C
/// runtime, whose toolset keys nothing.
pub(crate) fn picked(image: &Image, stores: &[PathBuf]) -> Vec<PathBuf> {
    let Some(key) = ra_asm_pe::Toolset::of(image).runtime_key() else { return Vec::new() };
    pick(stores, &key, ra_asm_analysis::signatures::machine_name(image))
}

/// Every signature file of `stores`, whatever toolset it serves: what a step that applies the
/// stores depends on.
pub(crate) fn store_files(stores: &[PathBuf]) -> Vec<PathBuf> {
    let dirs = |dir: &Path| -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        dirs
    };
    stores.iter().flat_map(|s| dirs(s)).flat_map(|k| dirs(&k)).flat_map(|m| files_in(&m)).collect()
}

/// The toolset key of the installed MSVC runtime, from the version directory of its
/// `libcmt.lib` (`VC/Tools/MSVC/14.44.35207/lib/x86`): `msvc-14.44`.
fn installed_key(libcmt: &Path) -> Option<String> {
    let version = libcmt.parent()?.parent()?.parent()?.file_name()?.to_string_lossy().into_owned();
    let mut parts = version.split('.');
    let major: u8 = parts.next()?.parse().ok()?;
    let minor: u8 = parts.next()?.parse().ok()?;
    Some(format!("msvc-{major}.{minor:02}"))
}

/// The installed MSVC static runtime of every machine it is installed for, with its toolset key.
fn installed_runtime() -> Option<(String, Vec<PathBuf>)> {
    let mut key = None;
    let mut libraries = Vec::new();
    for arch in ["x86", "x64", "arm64"] {
        let Some(paths) = ra_asm_analysis::signatures::installed_msvc_runtime(arch) else { continue };
        if key.is_none() {
            key = paths.first().and_then(|p| installed_key(p));
        }
        libraries.extend(paths);
    }
    Some((key?, libraries))
}

/// `ras signatures --build <libraries> [--installed] [--toolset <key>] [--store <dir>]`: writes
/// the signatures of the libraries into the store under the toolset key, the key of `image`'s
/// fingerprint when `--toolset` does not give one; `--installed` adds the installed MSVC static
/// runtime of every machine, under its own version's key unless `--toolset` gives one.
pub(crate) fn build(
    store: Option<&Path>,
    libraries: &[PathBuf],
    installed: bool,
    toolset: Option<&str>,
    image: Option<&Image>,
    json: bool,
) -> Result<()> {
    let Some(store) = store.map(Path::to_path_buf).or_else(user_store) else {
        bail!("no store to build: pass --store, or set {STORE_ENV} or the home directory")
    };
    let fingerprint = image.and_then(|i| ra_asm_pe::Toolset::of(i).runtime_key());
    let mut written = Vec::new();
    if !libraries.is_empty() {
        let Some(key) = toolset.map(str::to_owned).or(fingerprint) else {
            bail!("no toolset key for the libraries: pass --toolset, or an image whose C runtime the fingerprint knows")
        };
        written.extend(build_store(&store, &key, libraries)?);
    }
    if installed {
        let Some((key, runtime)) = installed_runtime() else { bail!("no MSVC toolset with libcmt.lib is installed (VCToolsInstallDir or vswhere)") };
        written.extend(build_store(&store, toolset.unwrap_or(&key), &runtime)?);
    }
    let mut out = std::io::stdout().lock();
    if json {
        let files: Vec<_> = written.iter().map(|(f, n)| json!({ "file": f, "signatures": n })).collect();
        serde_json::to_writer_pretty(&mut out, &json!({ "store": store, "written": files }))?;
        writeln!(out)?;
        return Ok(());
    }
    for (file, count) in &written {
        writeln!(out, "{}  {count} signatures", file.display())?;
    }
    writeln!(out, "{} files written to {}", written.len(), store.display())?;
    Ok(())
}

/// `ras signatures --list [<pe> | --project <dir>] [--store <dir>]`: the files of the stores by
/// toolset and machine, and for an image the key of its fingerprint, the files it picks, and the
/// key the installed MSVC runtime would be built under, which the workbench offers when the
/// stores hold nothing for the image.
pub(crate) fn list(stores: &[PathBuf], image: Option<&Image>, json: bool) -> Result<()> {
    let available: Vec<(PathBuf, String, String, PathBuf)> = store_files(stores)
        .into_iter()
        .filter_map(|f| {
            let machine_dir = f.parent()?;
            let key_dir = machine_dir.parent()?;
            let machine = machine_dir.file_name()?.to_string_lossy().into_owned();
            let key = key_dir.file_name()?.to_string_lossy().into_owned();
            Some((key_dir.parent()?.to_path_buf(), key, machine, f))
        })
        .collect();
    let key = image.and_then(|i| ra_asm_pe::Toolset::of(i).runtime_key());
    let machine = image.map(ra_asm_analysis::signatures::machine_name);
    let loaded = image.map(|i| picked(i, stores)).unwrap_or_default();
    let installed = image.is_some().then(installed_runtime).flatten().map(|(k, _)| k);
    let mut out = std::io::stdout().lock();
    if json {
        let available: Vec<_> = available
            .iter()
            .map(|(store, key, machine, file)| {
                json!({ "store": store, "toolset": key, "machine": machine, "library": library_of(file), "file": file })
            })
            .collect();
        serde_json::to_writer_pretty(
            &mut out,
            &json!({
                "stores": stores, "toolset": key, "machine": machine, "loaded": loaded, "installed": installed,
                "available": available,
            }),
        )?;
        writeln!(out)?;
        return Ok(());
    }
    for store in stores {
        writeln!(out, "store {}", store.display())?;
    }
    for (_, key, machine, file) in &available {
        writeln!(out, "  {key}  {machine}  {}", file.display())?;
    }
    if image.is_some() {
        writeln!(out, "toolset {}  machine {}", key.as_deref().unwrap_or("(no known C runtime)"), machine.unwrap_or("?"))?;
        for file in &loaded {
            writeln!(out, "  loaded {}", file.display())?;
        }
        if loaded.is_empty() {
            let offer = installed.map(|k| format!(": `ras signatures --build --installed` writes {k}")).unwrap_or_default();
            writeln!(out, "  no store file fits the image{offer}")?;
        }
    }
    Ok(())
}

/// `ras signatures <pe> --from <files> [--write <file>]`: the matches and the collisions of the
/// signatures over the image; `--write` keeps the signatures made from the libraries.
pub(crate) fn recognize(image: &mut Image, from: &[PathBuf], write: Option<&Path>, json: bool) -> Result<()> {
    let set = read_files(from)?;
    if let Some(path) = write {
        set.write(path)?;
    }
    let applied = ra_asm_analysis::signatures::apply(image, &set);
    let mut out = std::io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(
            &mut out,
            &json!({
                "signatures": set.signatures.len(), "functions": applied.functions,
                "matches": applied.matches, "collisions": applied.collisions().count(), "callees": applied.callees,
            }),
        )?;
        writeln!(out)?;
        return Ok(());
    }
    for m in &applied.matches {
        write!(out, "{}  {}  {}:{}", hex(m.va), m.name, m.library, m.object)?;
        if !m.alternatives.is_empty() {
            write!(out, "  collision: {}", m.alternatives.join(" | "))?;
        }
        writeln!(out)?;
    }
    for c in &applied.callees {
        writeln!(
            out,
            "{}  {}  named by a match's reference{}",
            hex(c.va),
            c.name,
            c.library.as_deref().map(|l| format!(" ({l})")).unwrap_or_default()
        )?;
    }
    writeln!(
        out,
        "{} signatures, {} functions, {} matched, {} collisions, {} named by reference",
        set.signatures.len(),
        applied.functions,
        applied.matches.len(),
        applied.collisions().count(),
        applied.callees.len()
    )?;
    Ok(())
}

fn kind_name(kind: Kind, bits: u32) -> &'static str {
    match kind {
        Kind::Int if bits == 32 => "i32",
        Kind::Int | Kind::Wide => "i64",
        Kind::Float => "float",
        Kind::Double => "double",
    }
}

pub(crate) fn signatures(image: &Image, json: bool) -> Result<()> {
    let analysis = ra_asm_analysis::analyze(image);
    let lifted = ra_asm_lift::lift(image, &analysis);
    let bits = lifted.bits;
    let kind_name = |k: Kind| kind_name(k, bits);
    let signatures = infer(&lifted.module, &lifted.roots, ra_asm_lift::raise::Target { bits, isa: lifted.isa }, &lifted.vtables);
    let mut out = std::io::stdout().lock();
    if json {
        let rows: Vec<_> = analysis
            .functions
            .values()
            .filter_map(|f| {
                let s = signatures.get(&ra_asm_lift::function_name(f.entry))?;
                Some(json!({
                    "addr": hex(f.entry), "name": f.name,
                    "params": s.params.iter().map(|k| kind_name(*k)).collect::<Vec<_>>(),
                    "stack_params": s.stack_params.iter().map(|k| kind_name(*k)).collect::<Vec<_>>(),
                    "returns": s.returns.map(kind_name), "noreturn": s.noreturn, "frame_below": s.frame_below, "dynamic_stack": s.dynamic_stack,
                    "pops": s.pops,
                }))
            })
            .collect();
        serde_json::to_writer_pretty(&mut out, &rows)?;
        writeln!(out)?;
        return Ok(());
    }
    for f in analysis.functions.values() {
        let Some(s) = signatures.get(&ra_asm_lift::function_name(f.entry)) else { continue };
        let params: Vec<&str> = s.params.iter().chain(&s.stack_params).map(|k| kind_name(*k)).collect();
        let ret = if s.noreturn { "noreturn" } else { s.returns.map_or("void", kind_name) };
        let pops = if s.pops > 0 { format!("  pops {}", s.pops) } else { String::new() };
        writeln!(
            out,
            "{}  {ret} {}({}){pops}{}",
            hex(f.entry),
            f.name,
            params.join(", "),
            if s.dynamic_stack { "  dynamic stack" } else { "" }
        )?;
        if s.frame_below > 0 {
            writeln!(out, "{:18}  frame {} bytes", "", s.frame_below)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    /// The fixture library: two functions long enough to stand on their own bytes.
    const LIBRARY: &str = "
int scale(int x, int k) { int s = 0; for (int i = 0; i < k; i++) s += x * (i + 1); return s ^ 0x5a5a; }
unsigned checksum(const unsigned char *p, int n) { unsigned h = 2166136261u; while (n--) h = (h ^ *p++) * 16777619u; return h; }
";

    /// The program that links it, without a C runtime, so nothing else names its functions.
    const PROGRAM: &str = "
int scale(int x, int k);
unsigned checksum(const unsigned char *p, int n);
int start(void) { static const unsigned char text[] = \"store\"; return scale(3, 4) + (int)checksum(text, 5); }
";

    /// Compiles the fixture library into `lib.obj` and links the program over it into
    /// `program.exe`, in `dir`; `None` when clang is not on the PATH.
    fn fixture(dir: &Path) -> Option<(PathBuf, PathBuf)> {
        std::fs::write(dir.join("lib.c"), LIBRARY).unwrap();
        std::fs::write(dir.join("program.c"), PROGRAM).unwrap();
        let (library, program) = (dir.join("lib.obj"), dir.join("program.exe"));
        let target = "--target=x86_64-pc-windows-msvc";
        let compiled = Command::new("clang").args(["-c", "-O1", target, "-o"]).arg(&library).arg(dir.join("lib.c")).output().ok()?;
        assert!(compiled.status.success(), "clang -c lib.c:\n{}", String::from_utf8_lossy(&compiled.stderr));
        let linked = Command::new("clang")
            .args(["-O1", target, "-fuse-ld=lld", "-nostdlib", "-Wl,/entry:start", "-Wl,/subsystem:console", "-Wl,/nodefaultlib", "-o"])
            .arg(&program)
            .arg(dir.join("program.c"))
            .arg(&library)
            .output()
            .unwrap();
        assert!(linked.status.success(), "clang program.c lib.obj:\n{}", String::from_utf8_lossy(&linked.stderr));
        Some((library, program))
    }

    /// A store built from the fixture library, written, picked by its toolset key and machine
    /// and read back, matches what the library itself matches.
    #[test]
    fn a_store_built_from_a_library_matches_as_the_library_does() {
        let dir = ra_asm_testing::Scratch::new("ras-signature-tests", "store");
        let Some((library, program)) = fixture(&dir) else {
            eprintln!("skipped: clang is not on the PATH");
            return;
        };
        let store = dir.join("store");
        let written = build_store(&store, "fixture-1.00", std::slice::from_ref(&library)).unwrap();
        let file = store_file(&store, "fixture-1.00", "x64", "lib.obj");
        assert_eq!(written.iter().map(|(f, _)| f.clone()).collect::<Vec<_>>(), vec![file.clone()]);
        assert!(written[0].1 >= 2, "{} signatures from lib.obj", written[0].1);

        let picked = pick(std::slice::from_ref(&store), "fixture-1.00", "x64");
        assert_eq!(picked, vec![file.clone()]);
        assert!(pick(std::slice::from_ref(&store), "fixture-1.00", "x86").is_empty(), "another machine picks nothing");
        assert!(pick(std::slice::from_ref(&store), "other-1.00", "x64").is_empty(), "another toolset picks nothing");
        assert_eq!(store_files(std::slice::from_ref(&store)), vec![file]);

        let image = || Image::parse(std::fs::read(&program).unwrap()).unwrap();
        let matches = |applied: &Applied| -> Vec<(u64, String, String, String, Vec<String>)> {
            applied.matches.iter().map(|m| (m.va, m.name.clone(), m.library.clone(), m.object.clone(), m.alternatives.clone())).collect()
        };
        let direct = apply_files(&mut image(), std::slice::from_ref(&library)).unwrap();
        let stored = apply_files(&mut image(), &picked).unwrap();
        for name in ["scale", "checksum"] {
            assert!(direct.matches.iter().any(|m| m.name == name), "{name} is not matched: {:?}", matches(&direct));
        }
        assert_eq!(matches(&direct), matches(&stored));
    }

    /// A project's store hides the user's file of the same library, and the MSVC 14.x runtime of
    /// another version serves when the store has not the image's own, newest first.
    #[test]
    fn the_stores_pick_by_toolset_and_library() {
        let dir = ra_asm_testing::Scratch::new("ras-signature-tests", "pick");
        let (project, user) = (dir.join("project"), dir.join("user"));
        let touch = |store: &Path, key: &str, library: &str| {
            let file = store_file(store, key, "x86", library);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, "# ra signatures 1\n").unwrap();
            file
        };
        let own = touch(&project, "msvc-14.44", "libcmt.lib");
        touch(&user, "msvc-14.44", "libcmt.lib");
        let ucrt = touch(&user, "msvc-14.44", "libucrt.lib");
        let newer = touch(&user, "msvc-14.50", "libvcruntime.lib");
        touch(&user, "msvc-14.38", "libvcruntime.lib");
        let stores = [project, user];
        assert_eq!(pick(&stores, "msvc-14.44", "x86"), vec![own.clone(), ucrt]);
        assert_eq!(pick(&stores, "msvc-14.51", "x86"), vec![own, newer]);
        assert!(pick(&stores, "msvc-16.00", "x86").is_empty(), "Visual Studio 2010's runtime is another");
        assert_eq!(library_of(Path::new("a/libcmt.lib.sig")).as_deref(), Some("libcmt.lib"));
        assert_eq!(installed_key(Path::new("VC/Tools/MSVC/14.44.35207/lib/x86/libcmt.lib")).as_deref(), Some("msvc-14.44"));
    }
}
