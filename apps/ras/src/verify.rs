//! `ras verify`: the vectors of an image replayed at each level of the raise, a line a function.
//!
//! The vectors come from one run of the image through the interpreter at the machine level (kept
//! in a vector file with `--write`), or from a vector file with `--vectors`. Each function they
//! are of replays its vectors at the machine and the promoted level, which take the state, and at
//! the ABI level through its signature, standing alone as a port does. Its line names the levels
//! in that order with the vectors each replayed, up to the first level that differs, and then that
//! level's first difference; the last line counts the functions that replayed at every level.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ra_asm_analysis::Analysis;
use ra_asm_lift::raise::abi::Signature;
use ra_asm_lift::{LiftOptions, Lifted, RaiseLevel, X87Unit};
use ra_asm_pe::Image;
use ra_asm_verify::{Replayer, Vector};

/// Where the vectors come from and which of them replay.
pub(crate) struct Options<'a> {
    /// The program the image was loaded from, whose name a vector file in a directory takes.
    pub program: &'a Path,
    /// A vector file to read instead of recording a run.
    pub vectors: Option<&'a Path>,
    /// Where to write the vectors the run recorded: a vector file, or a directory that keeps
    /// them as `<program's name>.fv`, as `testdata/vectors/fn` keeps the fixtures', with the
    /// comment of how the recorder kept them.
    pub write: Option<&'a Path>,
    /// The one function to verify, by name or by address.
    pub function: Option<&'a str>,
}

/// The vectors of one function, in the order the run kept them.
struct Function<'v> {
    name: &'v str,
    vectors: Vec<&'v Vector>,
}

/// Verifies the functions of `image` and prints a line for each; whether every one replayed at
/// every level. The replay's interpreter recurses with the program's calls, past the main thread's
/// stack, so the command runs on a thread of the stack a run of the interpreter has.
pub(crate) fn verify(image: &Image, options: &Options<'_>) -> Result<bool> {
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("ras-verify".into())
            .stack_size(256 << 20)
            .spawn_scoped(scope, || verify_here(image, options))
            .context("cannot start the replay's thread")?;
        handle.join().map_err(|_| anyhow::anyhow!("the replay panicked"))?
    })
}

fn verify_here(image: &Image, options: &Options<'_>) -> Result<bool> {
    let analysis = ra_asm_analysis::analyze(image);
    let machine = lifted(image, &analysis, RaiseLevel::Machine);
    let vectors = match options.vectors {
        Some(path) => {
            let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
            ra_asm_verify::read(&text).with_context(|| format!("reading the vectors of {}", path.display()))?
        }
        None => recorded(image, &machine)?,
    };
    if let Some(path) = options.write {
        let path = vector_file(path, options.program);
        // Only a run's own vectors are kept as the recorder keeps them.
        let text = if options.vectors.is_some() { ra_asm_verify::write(&vectors)? } else { ra_asm_verify::record::file(&vectors)? };
        std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    }

    let mut functions: BTreeMap<u64, Function<'_>> = BTreeMap::new();
    let wanted = options.function.map(|text| (text, crate::disasm::parse_address(text).ok()));
    for vector in &vectors {
        if let Some((name, address)) = wanted
            && vector.function != name
            && address != Some(vector.address)
        {
            continue;
        }
        functions.entry(vector.address).or_insert_with(|| Function { name: &vector.function, vectors: Vec::new() }).vectors.push(vector);
    }
    if let Some(text) = options.function
        && functions.is_empty()
    {
        bail!("{text} has no vector: it is no function the run entered, by name or by address");
    }

    let mut lines: BTreeMap<u64, String> = functions.iter().map(|(address, f)| (*address, f.name.to_owned())).collect();
    let mut differs: BTreeSet<u64> = BTreeSet::new();
    let promoted = lifted(image, &analysis, RaiseLevel::Promoted);
    let replayers = [
        (RaiseLevel::Machine, Replayer::new(image, &machine, RaiseLevel::Machine)?),
        (RaiseLevel::Promoted, Replayer::new(image, &promoted, RaiseLevel::Promoted)?),
    ];
    for (level, replayer) in &replayers {
        replay(*level, replayer, &functions, &mut lines, &mut differs);
    }
    drop(replayers);
    drop((machine, promoted));
    let (abi, signatures) = signed(image, &analysis);
    let replayer = Replayer::with_signatures(image, &abi, &signatures)?;
    replay(RaiseLevel::Abi, &replayer, &functions, &mut lines, &mut differs);

    for line in lines.values() {
        println!("{line}");
    }
    let verified = functions.len() - differs.len();
    let count: usize = functions.values().map(|f| f.vectors.len()).sum();
    println!("verified {verified} of {} functions, {count} vectors", functions.len());
    Ok(differs.is_empty())
}

/// Replays the vectors of each function at `level` and adds the level to its line: `ok` with the
/// vectors replayed, or the first difference. A function that differed at a level below is not
/// replayed again.
fn replay(
    level: RaiseLevel,
    replayer: &Replayer<'_>,
    functions: &BTreeMap<u64, Function<'_>>,
    lines: &mut BTreeMap<u64, String>,
    differs: &mut BTreeSet<u64>,
) {
    for (address, f) in functions {
        if differs.contains(address) {
            continue;
        }
        let line = lines.get_mut(address).expect("every function has a line");
        let first = f.vectors.iter().enumerate().find_map(|(n, vector)| match replayer.replay(vector) {
            Ok(differences) => differences.first().map(|difference| format!("differs at vector {}: {difference}", n + 1)),
            Err(error) => Some(format!("does not load vector {}: {error:#}", n + 1)),
        });
        let _ = match first {
            None => write!(line, " {} ok {}", level.name(), f.vectors.len()),
            Some(what) => {
                differs.insert(*address);
                write!(line, " {} {what}", level.name())
            }
        };
    }
}

/// The vector file `write` names for `program`: itself, or in a directory the program's name
/// without its extension, `.fv` after it.
fn vector_file(write: &Path, program: &Path) -> PathBuf {
    if !write.is_dir() {
        return write.to_owned();
    }
    let name = program.file_stem().unwrap_or(program.as_os_str());
    write.join(format!("{}.fv", name.to_string_lossy()))
}

/// The vectors of one run of `lifted`, the machine level, through the interpreter. A run that
/// stopped short says so on stderr; the vectors it kept still replay.
fn recorded(image: &Image, lifted: &Lifted) -> Result<Vec<Vector>> {
    let (recording, outcome) = ra_asm_verify::record(image, lifted, &ra_asm_lift::run::Options::default())?;
    if let Some(stop) = &outcome.stop {
        if recording.vectors.is_empty() {
            bail!("the run kept no vector; it stopped: {stop}");
        }
        eprintln!("the run stopped: {stop}; the vectors it kept replay");
    }
    Ok(recording.vectors)
}

/// `image` lifted as it is mapped and raised to `level`, under the names the run knows.
fn lifted(image: &Image, analysis: &Analysis<'_>, level: RaiseLevel) -> Lifted {
    let mut lifted = ra_asm_lift::lift_with(image, analysis, LiftOptions { standalone: false, x87: X87Unit::Double });
    let options = lifted.raise_options(level);
    ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(analysis, None);
    lifted
}

/// `image` lifted standing alone and raised to the ABI level, with the signatures of its
/// functions under the names the run knows.
fn signed(image: &Image, analysis: &Analysis<'_>) -> (Lifted, HashMap<String, Signature>) {
    let mut lifted = ra_asm_lift::lift_with(image, analysis, LiftOptions { standalone: true, x87: X87Unit::Double });
    let options = lifted.raise_options(RaiseLevel::Abi);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(analysis, Some(&mut signatures));
    (lifted, signatures)
}
