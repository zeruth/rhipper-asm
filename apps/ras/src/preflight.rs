//! `ras preflight`: what a port of an image will get wrong when it runs, read off the lift and
//! the raise before anything is built. Each list is a kind of failure a first run met one at a
//! time: an instruction the lift traps on, an import called with no prototype, a structure the
//! program and the host lay out differently, a function asked for by name whose stack bytes the
//! tables do not know, an address made of a value nothing set (a parameter the inference
//! dropped), an import that has no name.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ra_asm_ir::{BinOp, Function, Inst, Operand, ValueId};
use serde_json::{Value, json};

/// The most lines a list prints: the rest are counted.
const MOST: usize = 25;

/// The image a path names: the file itself, or the reference binary of the project whose
/// `rhipper.json` the directory holds.
fn image_of(path: &Path) -> Result<PathBuf> {
    if !path.is_dir() {
        return Ok(path.to_owned());
    }
    let file = path.join("rhipper.json");
    let text = std::fs::read_to_string(&file).with_context(|| format!("no project at {}", path.display()))?;
    let project: Value = serde_json::from_str(&text).with_context(|| format!("{} is not JSON", file.display()))?;
    let exe = project["exe"].as_str().with_context(|| format!("{} names no `exe`", file.display()))?;
    let exe = Path::new(exe);
    Ok(if exe.is_absolute() { exe.to_owned() } else { path.join(exe) })
}

/// The loads and stores of `f` whose address is made of a value nothing set: `undef` through
/// casts and constant offsets. After the raise that is a register the function read as it came
/// in and no caller was seen to set, so the inference took it for no parameter: the function
/// reads its object at an address of no meaning.
///
/// Each access comes with what the value was to be, by the name the raise gave the first value
/// made of it: [`Unset::Argument`] for a register a caller passes arguments in, [`Unset::Stack`]
/// for the stack pointer, [`Unset::Kept`] for a register a callee keeps, which a piece of
/// another function reads (a catch block the analysis took for a function of its own runs on
/// its function's frame pointer).
fn unset_addresses(f: &Function) -> Vec<(u64, Unset)> {
    let mut made: std::collections::HashMap<ValueId, Unset> = std::collections::HashMap::new();
    let unset = |o: &Operand, made: &std::collections::HashMap<ValueId, Unset>, own: Unset| match o {
        Operand::Undef => Some(own),
        Operand::Local(v) => made.get(v).copied(),
        _ => None,
    };
    loop {
        let mut changed = false;
        for i in f.blocks.iter().flat_map(|b| &b.instrs) {
            let Some(result) = i.result else { continue };
            if made.contains_key(&result) {
                continue;
            }
            let own = Unset::of(f.value_name(result));
            let derived = match &i.inst {
                Inst::Cast { value, .. } => unset(value, &made, own),
                Inst::Bin { op: BinOp::Add | BinOp::Sub, lhs, rhs: Operand::Int(_), .. } => unset(lhs, &made, own),
                Inst::Bin { op: BinOp::Add, lhs: Operand::Int(_), rhs, .. } => unset(rhs, &made, own),
                Inst::Gep { ptr, .. } => unset(ptr, &made, own),
                _ => None,
            };
            if let Some(kind) = derived {
                made.insert(result, kind);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let entry = f.address.unwrap_or(0);
    let mut out = Vec::new();
    for b in &f.blocks {
        for i in &b.instrs {
            let (Inst::Load { ptr, .. } | Inst::Store { ptr, .. }) = &i.inst else { continue };
            if let Some(kind) = unset(ptr, &made, Unset::Stack) {
                out.push((i.address.or(b.address).unwrap_or(entry), kind));
            }
        }
    }
    out
}

/// What a value nothing set was to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Unset {
    /// A register a caller passes an argument in: `this` in `ecx`, the second in `edx`.
    Argument,
    /// The stack pointer, above which a call through a pointer reads its words.
    Stack,
    /// A register a callee keeps: the frame pointer or a saved register of another function.
    Kept,
}

impl Unset {
    /// By the name the raise gives the first value read of a register: the register's.
    fn of(name: &str) -> Self {
        let register = name.split('.').next().unwrap_or(name);
        match register {
            "ecx" | "edx" | "rcx" | "rdx" | "r8" | "r9" | "eax" | "rax" | "cl" | "dl" | "cx" | "dx" => Self::Argument,
            r if r.starts_with("sarg") || r.starts_with("esp") || r.starts_with("rsp") || r == "sp" => Self::Stack,
            _ => Self::Kept,
        }
    }

    fn heading(self) -> (&'static str, &'static str) {
        match self {
            Self::Argument => (
                "functions that read their object or an argument through a register nothing set",
                "an argument register no caller was seen to set, so the inference took it for no parameter: the function reads at an address of no meaning",
            ),
            Self::Stack => (
                "functions that read stack words through a stack pointer nothing set",
                "a call or a jump through a pointer in a function that keeps nothing on the stack: its words are read at addresses 4, 8 and up",
            ),
            Self::Kept => (
                "pieces of a function taken for functions of their own",
                "the frame pointer or a saved register read as it came in: a catch block or a shared tail that runs on another function's frame",
            ),
        }
    }
}

/// The identifiers and the DLL names among the strings of `image`: runs of name characters
/// between zero bytes.
fn names_of(image: &ra_asm_pe::Image) -> (BTreeSet<String>, BTreeSet<String>) {
    let (mut identifiers, mut dlls) = (BTreeSet::new(), BTreeSet::new());
    let bytes = image.mapped();
    let mut start = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-') {
            continue;
        }
        if b == 0
            && i - start >= 4
            && (start == 0 || bytes[start - 1] == 0)
            && let Ok(name) = std::str::from_utf8(&bytes[start..i])
        {
            if name.to_ascii_lowercase().ends_with(".dll") {
                dlls.insert(name.to_owned());
            } else if name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_') && name.as_bytes()[0].is_ascii_alphabetic() {
                identifiers.insert(name.to_owned());
            }
        }
        start = i + 1;
    }
    (identifiers, dlls)
}

/// The functions a 32-bit program may ask `GetProcAddress` for whose stack bytes the tables do
/// not know: an identifier among the image's strings that a DLL the image names (as a string or
/// as an import's) exports on this host, with no prototype. A call through the pointer it is
/// handed pops nothing in the port, and a `stdcall` one leaves the caller's stack off by its
/// arguments. Empty on a host with no DLLs to ask.
fn asked_by_name(image: &ra_asm_pe::Image, lifted: &ra_asm_lift::Lifted) -> Vec<(String, String)> {
    use ra_asm_lift::raise::prototypes;
    if lifted.bits != 32 || !lifted.imports.contains("ra_x86_get_proc_address") {
        return Vec::new();
    }
    let (identifiers, mut dlls) = names_of(image);
    dlls.extend(image.imports.iter().map(|i| i.dll.clone()));
    let imported: BTreeSet<String> = image.imports.iter().map(ra_asm_pe::Import::display_name).collect();
    let mut out = Vec::new();
    for dll in &dlls {
        // A C runtime's functions are `cdecl`: their caller pops, whatever the count.
        if prototypes::is_crt_dll(dll) {
            continue;
        }
        for name in &identifiers {
            if imported.contains(name) || prototypes::is_variadic(name) || prototypes::proc_pops(name).is_some() {
                continue;
            }
            if ra_asm_lift::run::host_exports(dll, name) == Some(true) {
                out.push((name.clone(), dll.clone()));
            }
        }
    }
    out.sort();
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// The OpenGL functions a 32-bit program may ask its context for by name that the OpenGL table
/// does not know: a string of the image that reads as one (`gl` or `wgl` and a capital). The
/// runtime hands such a function out as it is, and a call through it pops nothing and hands the
/// host no float.
fn asked_of_opengl(image: &ra_asm_pe::Image, lifted: &ra_asm_lift::Lifted) -> Vec<(String, String)> {
    use ra_asm_lift::raise::prototypes;
    if lifted.bits != 32 || !lifted.imports.iter().any(|i| prototypes::asks_opengl(i)) {
        return Vec::new();
    }
    let imported: BTreeSet<String> = image.imports.iter().map(ra_asm_pe::Import::display_name).collect();
    let reads_as_one = |name: &str| {
        let rest = name.strip_prefix("wgl").or_else(|| name.strip_prefix("gl"));
        rest.is_some_and(|r| r.as_bytes().first().is_some_and(u8::is_ascii_uppercase))
    };
    names_of(image)
        .0
        .into_iter()
        .filter(|name| reads_as_one(name) && !imported.contains(name) && prototypes::opengl(name).is_none())
        .map(|name| (name, "the OpenGL context".to_owned()))
        .collect()
}

/// A list under its heading: the count, then at most [`MOST`] lines.
fn list(out: &mut String, heading: &str, why: &str, lines: &[String]) {
    let _ = writeln!(out, "{heading} ({})", lines.len());
    if lines.is_empty() {
        let _ = writeln!(out, "  none");
        return;
    }
    let _ = writeln!(out, "  {why}");
    for line in lines.iter().take(MOST) {
        let _ = writeln!(out, "  {line}");
    }
    if lines.len() > MOST {
        let _ = writeln!(out, "  ... {} more", lines.len() - MOST);
    }
}

/// `ras preflight <image or project>`: the lists, as text or as JSON.
pub(crate) fn preflight(path: &Path, signatures: &[PathBuf], as_json: bool) -> Result<()> {
    let pe = image_of(path)?;
    let image = crate::load(&pe, signatures)?;
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: true, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let coverage = ra_asm_lift::coverage(&analysis, &lifted);
    let boundary = crate::smoke::boundary(&lifted, &coverage);
    let mut asked = asked_by_name(&image, &lifted);
    asked.extend(asked_of_opengl(&image, &lifted));
    let unnamed: Vec<String> = lifted.imports.iter().filter(|i| i.starts_with("ra_ordinal_")).cloned().collect();
    let unread_frames = lifted.warnings.iter().filter(|w| w.contains("frame that was not read")).count();
    let _ = crate::lift::raise(&mut lifted, &analysis, ra_asm_lift::RaiseLevel::Abi);

    // The raised functions, the frozen ones thawed one at a time.
    let mut unset: BTreeMap<(Unset, String), Vec<u64>> = BTreeMap::new();
    let mut note = |f: &Function| {
        for (address, kind) in unset_addresses(f) {
            unset.entry((kind, f.name.clone())).or_default().push(address);
        }
    };
    lifted.module.functions.iter().for_each(&mut note);
    for frozen in &lifted.module.frozen {
        note(&ra_asm_ir::freeze::thaw(frozen));
    }

    // A structure the generated layouts convert around each import that takes it has its adapter.
    let adapted = |e: &Value| {
        // The runtime converts the creation record where the window procedure is handed it.
        if e["name"].as_str() == Some("CREATESTRUCTA") {
            return true;
        }
        let through: Vec<&str> = e["through"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        !through.is_empty() && through.iter().all(|import| ra_asm_lift::raise::prototypes::adapts_structures(import))
    };
    let entries = |key: &str| -> Vec<Value> {
        boundary[key].as_array().into_iter().flatten().filter(|e| key != "structures" || !adapted(e)).cloned().collect()
    };
    if as_json {
        let report = json!({
            "image": pe.display().to_string(),
            "functions": lifted.functions.len(),
            "traps": coverage.uncovered.iter().map(|u| json!({"mnemonic": u.mnemonic, "count": u.count, "functions": u.functions, "example": format!("{:#x}", u.example)})).collect::<Vec<_>>(),
            "imports": entries("imports"),
            "structures": entries("structures"),
            "interfaces": entries("interfaces"),
            "asked_by_name": asked.iter().map(|(name, dll)| json!({"name": name, "dll": dll})).collect::<Vec<_>>(),
            "unset_addresses": unset.iter().map(|((kind, name), sites)| json!({"function": name, "kind": format!("{kind:?}").to_ascii_lowercase(), "sites": sites.len(), "example": format!("{:#x}", sites[0])})).collect::<Vec<_>>(),
            "unnamed_imports": unnamed,
            "unresolved_jumps": coverage.unresolved_jumps.len(),
            "unread_frames": unread_frames,
        });
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    let row = |e: &Value| -> String {
        let through: Vec<&str> = e["through"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let via = if through.is_empty() { String::new() } else { format!("  through {}", through.join(", ")) };
        format!("{:<34} {:>5} call(s) in {:>4} function(s){via}", e["name"].as_str().unwrap_or("?"), e["sites"], e["functions"])
    };
    let mut out = String::new();
    let _ = writeln!(out, "{}: {} functions lifted", pe.display(), lifted.functions.len());
    for kind in [Unset::Argument, Unset::Stack, Unset::Kept] {
        let mut rows: Vec<(&String, &Vec<u64>)> =
            unset.iter().filter(|((k, _), _)| *k == kind).map(|((_, name), sites)| (name, sites)).collect();
        // A piece nothing names is a catch block or an unwind action, which the lift also runs
        // inline in its function: the copy that stands alone is never entered. Counted, not listed.
        let unnamed = if kind == Unset::Kept {
            let before = rows.len();
            rows.retain(|(name, _)| !name.starts_with("F_"));
            before - rows.len()
        } else {
            0
        };
        rows.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(b.0)));
        let (heading, why) = kind.heading();
        let lines: Vec<String> =
            rows.iter().map(|(name, sites)| format!("{name:<48} {:>4} access(es), the first at {:#x}", sites.len(), sites[0])).collect();
        list(&mut out, heading, why, &lines);
        if unnamed > 0 {
            let _ =
                writeln!(out, "  and {unnamed} unnamed piece(s): catch blocks and unwind actions the lift runs inline in their functions");
        }
    }
    list(
        &mut out,
        "functions asked for by name with no prototype",
        "a call through the pointer GetProcAddress hands back pops nothing: a stdcall one leaves the stack off by its arguments",
        &asked.iter().map(|(name, dll)| format!("{name:<34} of {dll}")).collect::<Vec<_>>(),
    );
    list(
        &mut out,
        "structure parameters without an adapter row",
        "the program lays the structure out for 4-byte pointers and the host for 8",
        &entries("structures").iter().map(row).collect::<Vec<_>>(),
    );
    list(
        &mut out,
        "COM interfaces without a row",
        "the program calls the object through an x86 vtable the host's object does not have",
        &entries("interfaces").iter().map(row).collect::<Vec<_>>(),
    );
    list(
        &mut out,
        "imports without a prototype",
        "called with the universal arguments and nothing popped: wrong for a stdcall function, and for a float argument",
        &entries("imports").iter().map(row).collect::<Vec<_>>(),
    );
    list(&mut out, "imports with no name", "an import by ordinal the tables do not name: the port stops when it is called", &unnamed);
    let _ = writeln!(out, "instructions the lift traps on ({} traps)", coverage.traps());
    if coverage.uncovered.is_empty() {
        let _ = writeln!(out, "  none");
    }
    for u in coverage.uncovered.iter().take(MOST) {
        let _ = writeln!(out, "  {:<14} {:>5} in {:>4} function(s), the first at {:#x}", u.mnemonic, u.count, u.functions, u.example);
    }
    if coverage.uncovered.len() > MOST {
        let _ = writeln!(out, "  ... {} more", coverage.uncovered.len() - MOST);
    }
    let _ = writeln!(out, "indirect jumps the analysis did not resolve: {}", coverage.unresolved_jumps.len());
    let _ = writeln!(out, "C++ frames an exception passes that were not read: {unread_frames}");
    print!("{out}");
    Ok(())
}
