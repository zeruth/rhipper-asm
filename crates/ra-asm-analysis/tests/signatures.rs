//! The signatures of the MSVC C runtime over the rhtest `/MT` builds, which carry no symbols of
//! their own in this test: the image is parsed without the map beside it, the signatures made
//! from the installed toolset's `libcmt.lib`, `libucrt.lib` and `libvcruntime.lib` name the
//! runtime's functions, and the map is the judge.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use ra_asm_analysis::signatures::{SignatureSet, apply, installed_msvc_runtime};
use ra_asm_analysis::{Kind, analyze};
use ra_asm_pe::Image;

fn rhtest(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/rhtest").join(name)
}

/// The signatures of the static C runtime of the installed toolset (`libcmt.lib` and
/// `libvcruntime.lib` of MSVC, `libucrt.lib` of the Windows SDK), or the reason there are none.
fn runtime_signatures(arch: &str) -> Result<(SignatureSet, String), String> {
    let Some(paths) = installed_msvc_runtime(arch) else {
        return Err("no MSVC toolset with libcmt.lib found (VCToolsInstallDir or vswhere)".into());
    };
    let mut set = SignatureSet::default();
    for path in &paths {
        set.extend(SignatureSet::from_library(path).unwrap_or_else(|e| panic!("{}: {e:#}", path.display())));
    }
    Ok((set, paths[0].parent().unwrap().display().to_string()))
}

/// The toolset picks the runtime libraries: the installed MSVC runtime for the MSVC 14.51 build
/// and for clang over the MSVC runtime, none for a program without a C runtime.
#[test]
fn the_toolset_picks_the_runtime_signatures() {
    let takes = |path: PathBuf| {
        let image = ra_asm_pe::load(&path).unwrap();
        ra_asm_analysis::signatures::takes_installed_msvc_runtime(&ra_asm_pe::Toolset::of(&image))
    };
    assert!(takes(rhtest("core-O2.exe")), "MSVC 14.51 takes the installed 14.x runtime");
    assert!(takes(rhtest("core-static.exe")), "clang/lld over the MSVC runtime takes it");
    let hello = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/hello64.exe");
    assert!(!takes(hello), "hello64 has no C runtime to name");
}

/// Every symbol of an MSVC or lld map at each address of the code section, with the object that
/// holds it: `address -> [(symbol, object)]`.
fn map_symbols(path: &Path) -> BTreeMap<u64, Vec<(String, String)>> {
    let text = std::fs::read_to_string(path).unwrap();
    let mut out: BTreeMap<u64, Vec<(String, String)>> = BTreeMap::new();
    for line in text.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 || !parts[0].starts_with("0001:") || parts[1].ends_with('H') {
            continue;
        }
        let address = u64::from_str_radix(parts[2], 16).unwrap();
        out.entry(address).or_default().push((parts[1].to_owned(), (*parts.last().unwrap()).to_owned()));
    }
    out
}

fn is_runtime_object(object: &str) -> bool {
    object.split(':').next().is_some_and(|lib| ra_asm_analysis::symbols::is_runtime_library(&format!("{lib}.lib")))
}

/// `core-static.exe` and `int64-static.exe` without their maps: the signatures of the toolset's
/// `libcmt.lib` (and the ucrt and vcruntime beside it) recognize the C runtime linked into them.
/// Every match names what the map names at that address; the startup is recognized, so the port
/// starts at `main`; the program's own functions stay its own; and the runtime functions the map
/// knows are nearly all recognized (what is not is a function the toolset's library no longer
/// holds in that shape).
#[test]
fn the_libcmt_signatures_recognize_the_static_runtime() {
    let (set, from) = match runtime_signatures("x86") {
        Ok(found) => found,
        Err(reason) => {
            eprintln!("skipped: {reason}");
            return;
        }
    };
    assert!(set.signatures.len() > 1000, "{} signatures from {from}", set.signatures.len());
    let mut failures = Vec::new();
    for name in ["core-static", "int64-static"] {
        let bytes = std::fs::read(rhtest(&format!("{name}.exe"))).unwrap();
        let mut image = Image::parse(bytes).unwrap();
        assert!(image.symbols.is_empty());
        let applied = apply(&mut image, &set);
        let map = map_symbols(&rhtest(&format!("{name}.map")));
        let analysis = analyze(&image);

        // Every match names what the map names there; a match inside a runtime object's code
        // that the map does not name (a tail the analysis made a function of) is let be.
        let enclosing = |va: u64| map.range(..=va).next_back().map(|(_, s)| s);
        let mut wrong = Vec::new();
        for m in &applied.matches {
            let Some(there) = map.get(&m.va) else {
                if !enclosing(m.va).is_some_and(|s| s.iter().any(|(_, o)| is_runtime_object(o))) {
                    wrong.push(format!("  {:#x} {} ({}:{}): the map has no symbol there", m.va, m.name, m.library, m.object));
                }
                continue;
            };
            let names: BTreeSet<&str> = there.iter().map(|(s, _)| s.as_str()).collect();
            if !names.contains(m.name.as_str()) && !m.alternatives.iter().any(|a| names.contains(a.as_str())) {
                wrong.push(format!(
                    "  {:#x} {} ({}:{}): the map says {}",
                    m.va,
                    m.name,
                    m.library,
                    m.object,
                    there.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>().join(" | ")
                ));
            }
        }
        // The runtime functions the map knows, among the functions the analysis found: recognized
        // when a match named them (a helper the port keeps, `_alldiv`, stays `Normal` by design).
        let runtime_in_map: Vec<u64> = analysis
            .functions
            .values()
            .filter(|f| map.get(&f.entry).is_some_and(|s| s.iter().any(|(_, o)| is_runtime_object(o))))
            .map(|f| f.entry)
            .collect();
        let is_recognized = |va: &u64| analysis.function(*va).is_some_and(|f| f.symbol.is_some());
        let recognized = runtime_in_map.iter().filter(|va| is_recognized(va)).count();
        let missed: Vec<String> = runtime_in_map
            .iter()
            .filter(|va| !is_recognized(va))
            .map(|va| format!("{va:#x} {}", map[va].iter().map(|(s, o)| format!("{s} ({o})")).collect::<Vec<_>>().join(" | ")))
            .collect();
        eprintln!(
            "{name}: {} functions, {} matched, {} collisions, {recognized}/{} runtime functions of the map recognized, {} wrong\nmissed:\n  {}",
            analysis.functions.len(),
            applied.matches.len(),
            applied.collisions().count(),
            runtime_in_map.len(),
            wrong.len(),
            missed.join("\n  "),
        );
        if !wrong.is_empty() {
            failures.push(format!("{name}: matches the map disagrees with:\n{}", wrong.join("\n")));
        }
        let entry = analysis.function(image.entry_point().unwrap()).unwrap();
        if !matches!(&entry.kind, Kind::Runtime(host) if host == "mainCRTStartup") {
            failures.push(format!("{name}: the entry is {:?}, not the runtime's mainCRTStartup", entry.kind));
        }
        let main_va = map.iter().find(|(_, s)| s.iter().any(|(n, _)| n == "_main")).map(|(va, _)| *va).expect("_main in the map");
        let main = analysis.function(main_va).expect("main is a function");
        if !matches!(main.kind, Kind::Normal) || main.name != "main_" {
            failures.push(format!("{name}: main is {:?} {}", main.kind, main.name));
        }
        if !applied.callees.iter().any(|c| c.va == main_va && c.name == "_main" && c.library.is_none()) {
            failures.push(format!("{name}: the startup's reference did not name main: callees {:?}", applied.callees));
        }
        if analysis.program_entry != Some(main_va) {
            failures.push(format!("{name}: the port starts at {:?}, not main", analysis.program_entry));
        }
        // What the port lifts is the program's own code: nothing the map attributes to a runtime
        // object, but for the helpers the port keeps by design.
        let ported_runtime: Vec<String> = analysis
            .ported()
            .filter(|f| map.get(&f.entry).is_some_and(|s| s.iter().any(|(_, o)| is_runtime_object(o))))
            .filter(|f| {
                !f.symbol
                    .as_deref()
                    .is_some_and(|s| ra_asm_analysis::symbols::is_ported_helper(&ra_asm_analysis::symbols::undecorate(s, false)))
            })
            .map(|f| format!("{:#x} {}", f.entry, f.name))
            .collect();
        if !ported_runtime.is_empty() {
            failures.push(format!("{name}: runtime functions ported: {ported_runtime:?}"));
        }
        eprintln!("{name}: {} ported functions", analysis.ported().count());
        if recognized * 100 < runtime_in_map.len() * 95 {
            failures.push(format!(
                "{name}: {recognized} of {} runtime functions recognized:\n  {}",
                runtime_in_map.len(),
                missed.join("\n  ")
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The set written and read back matches the same functions.
#[test]
fn a_written_set_reads_back_whole() {
    let (set, _) = match runtime_signatures("x86") {
        Ok(found) => found,
        Err(reason) => {
            eprintln!("skipped: {reason}");
            return;
        }
    };
    let text = set.to_text();
    let back = SignatureSet::parse(&text).unwrap();
    assert_eq!(back.signatures.len(), set.signatures.len());
    // Every copy of the name (`_memcpy` is in memcpy.obj and memmove.obj), in a fixed order: the
    // text is sorted, the generated set is in the libraries' order.
    let find = |s: &SignatureSet, name: &str| {
        let mut all: Vec<_> = s
            .signatures
            .iter()
            .filter(|x| x.name == name)
            .map(|x| (x.object.clone(), x.pattern.clone(), x.mask.clone(), x.refs.clone()))
            .collect();
        all.sort();
        all
    };
    for name in ["_mainCRTStartup", "___stdio_common_vfprintf", "_memcpy", "_strlen", "_malloc"] {
        assert!(!find(&set, name).is_empty(), "{name} in the set");
        assert_eq!(find(&set, name), find(&back, name), "{name} reads back");
    }
}
