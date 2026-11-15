//! `ras bisect`: a difference of a whole program brought down to the first step of the raise and
//! the first function that make it, and written as the case of that function ahead of the step.
//!
//! The vectors come from one run of the image through the interpreter at the machine level, or
//! from a vector file. They replay first at the machine level, which is the lift and no rewriting
//! step: a difference there is the lift's own. Then the image is raised to the ABI level as far as
//! each step in the order of [`steps::steps`], from `promote` on, and the vectors replay through
//! the signatures once the step ran, as `ras verify` replays the ABI level. A line a step says
//! `ok` until the first step after which a function differs: its line names the step, the first
//! function by address that differs, the vector (from one, among the function's) and the
//! difference with its values, and the next says whether the step changed the function's IR. The
//! case of that function as the step is handed it is written as `ras case` writes one, its calls
//! out of the same vectors.
//!
//! The ABI level first replays once `promote` ran, so a difference a step ahead of it makes
//! (`simplify`, the inference) is named `promote`, and no case is written ahead of `promote`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use ra_asm_analysis::Analysis;
use ra_asm_lift::raise::abi::Signature;
use ra_asm_lift::raise::{raise_before, steps};
use ra_asm_lift::{LiftOptions, Lifted, RaiseLevel, X87Unit};
use ra_asm_pe::Image;
use ra_asm_verify::{Replayer, Vector};

/// The first step after which the ABI level replays: the one that takes the functions off the
/// machine state.
const FIRST: &str = "promote";

/// Where the vectors come from, which of them replay and where the case goes.
pub(crate) struct Options<'a> {
    /// The program the image was loaded from, whose name a case written into a directory takes.
    pub program: &'a Path,
    /// A vector file to read instead of recording a run.
    pub vectors: Option<&'a Path>,
    /// The one function to bisect, by name or by address.
    pub function: Option<&'a str>,
    /// Where to write the case: a file, or a directory that keeps it as
    /// `<step>/<program>_<function>.ll`, as `testdata/cases` keeps the cases. Printed without it.
    pub write: Option<&'a Path>,
}

/// The vectors of one function, in the order the run kept them.
struct Function<'v> {
    name: &'v str,
    vectors: Vec<&'v Vector>,
}

/// The first difference of a replay.
struct Found {
    address: u64,
    name: String,
    /// The vector, from one among the function's.
    vector: usize,
    what: String,
}

/// Bisects the functions of `image` and prints a line for each step replayed; whether no step
/// differs. The replay's interpreter recurses with the program's calls, so the command runs on a
/// thread of the stack a run of the interpreter has.
pub(crate) fn bisect(image: &Image, options: &Options<'_>) -> Result<bool> {
    std::thread::scope(|scope| {
        let handle = std::thread::Builder::new()
            .name("ras-bisect".into())
            .stack_size(256 << 20)
            .spawn_scoped(scope, || bisect_here(image, options))
            .context("cannot start the replay's thread")?;
        handle.join().map_err(|_| anyhow::anyhow!("the replay panicked"))?
    })
}

fn bisect_here(image: &Image, options: &Options<'_>) -> Result<bool> {
    let analysis = ra_asm_analysis::analyze(image);
    let machine = crate::verify::lifted(image, &analysis, RaiseLevel::Machine);
    let vectors = match options.vectors {
        Some(path) => {
            let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
            ra_asm_verify::read(&text).with_context(|| format!("reading the vectors of {}", path.display()))?
        }
        None => crate::verify::recorded(image, &machine)?,
    };
    let functions = select(&vectors, options.function)?;

    let found = first(&Replayer::new(image, &machine, RaiseLevel::Machine)?, &functions);
    drop(machine);
    if let Some(found) = found {
        println!("machine {}", found.line());
        println!("the lift differs from the run with no step of the raise: no step is to blame and no case is written");
        return Ok(false);
    }
    println!("machine ok");

    let order: Vec<&str> = steps::steps(RaiseLevel::Abi).iter().map(|s| s.name).collect();
    let from = order.iter().position(|name| *name == FIRST).context("the raise to the abi level promotes")?;
    let mut handed: Option<Lifted> = None;
    for (at, &step) in order.iter().enumerate().skip(from) {
        let (lifted, signatures) = raised(image, &analysis, order.get(at + 1).copied());
        let found = first(&Replayer::with_signatures(image, &lifted, &signatures)?, &functions);
        let Some(found) = found else {
            println!("{step} ok");
            handed = Some(lifted);
            continue;
        };
        println!("{step} {}", found.line());
        match &handed {
            Some(handed) => {
                let text = |l: &Lifted| l.module.function(&found.name).map(ra_asm_ir::printer::function);
                if text(handed) == text(&lifted) {
                    println!("{step} leaves {} as it was handed it", found.name);
                } else {
                    println!("{step} changes {}", found.name);
                }
            }
            None => println!("{step} and the steps ahead of it ({}) raise {} off the machine state", order[..from].join(", "), found.name),
        }
        drop((handed, lifted));
        write_case(image, options, step, &found, &vectors)?;
        return Ok(false);
    }
    let count: usize = functions.values().map(|f| f.vectors.len()).sum();
    println!("no step differs: {} functions, {count} vectors replay after every step", functions.len());
    Ok(true)
}

impl Found {
    /// The difference, as the line of its step says it after the step's name.
    fn line(&self) -> String {
        format!("differs: {} at {:#x}, vector {}: {}", self.name, self.address, self.vector, self.what)
    }
}

/// The vectors of `vectors` by the function they are of, the one `function` names alone when it
/// names one (by name or by address).
fn select<'v>(vectors: &'v [Vector], function: Option<&str>) -> Result<BTreeMap<u64, Function<'v>>> {
    let wanted = function.map(|text| (text, crate::disasm::parse_address(text).ok()));
    let mut functions: BTreeMap<u64, Function<'_>> = BTreeMap::new();
    for vector in vectors {
        if let Some((name, address)) = wanted
            && vector.function != name
            && address != Some(vector.address)
        {
            continue;
        }
        functions.entry(vector.address).or_insert_with(|| Function { name: &vector.function, vectors: Vec::new() }).vectors.push(vector);
    }
    if functions.is_empty() {
        match function {
            Some(text) => bail!("{text} has no vector: it is no function the run entered, by name or by address"),
            None => bail!("there is no vector to replay"),
        }
    }
    Ok(functions)
}

/// The first difference `replayer` finds: the functions in the order of their addresses, each
/// function's vectors in their order.
fn first(replayer: &Replayer<'_>, functions: &BTreeMap<u64, Function<'_>>) -> Option<Found> {
    functions.iter().find_map(|(address, f)| {
        f.vectors.iter().enumerate().find_map(|(n, vector)| {
            let what = match replayer.replay(vector) {
                Ok(differences) => differences.first()?.to_string(),
                Err(error) => format!("does not load: {error:#}"),
            };
            Some(Found { address: *address, name: f.name.to_owned(), vector: n + 1, what })
        })
    })
}

/// `image` lifted standing alone and raised to the ABI level ahead of the step `before` (the
/// whole raise without one), with the signatures of its functions under the names the run knows.
fn raised(image: &Image, analysis: &Analysis<'_>, before: Option<&str>) -> (Lifted, HashMap<String, Signature>) {
    let mut lifted = ra_asm_lift::lift_with(image, analysis, LiftOptions { standalone: true, x87: X87Unit::Double });
    let options = lifted.raise_options(RaiseLevel::Abi);
    let mut signatures = raise_before(&mut lifted.module, &lifted.roots, options, before);
    lifted.apply_names(analysis, Some(&mut signatures));
    (lifted, signatures)
}

/// Writes the case of the function `found` names as `step` is handed it, or prints it, or says
/// why there is none.
fn write_case(image: &Image, options: &Options<'_>, step: &str, found: &Found, vectors: &[Vector]) -> Result<()> {
    let text = match crate::case::text(image, &format!("{:#x}", found.address), step, Some(vectors)) {
        Ok(text) => text,
        Err(error) => {
            println!("no case of {} ahead of {step}: {error:#}", found.name);
            return Ok(());
        }
    };
    let Some(write) = options.write else {
        print!("{text}");
        return Ok(());
    };
    let path = case_file(write, options.program, step, &found.name);
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    println!("the case: {}", path.display());
    Ok(())
}

/// The case file `write` names: itself, or in a directory the step's directory and the program's
/// name without its extension, `_`, the function's name and `.ll`.
fn case_file(write: &Path, program: &Path, step: &str, function: &str) -> PathBuf {
    if !write.is_dir() {
        return write.to_owned();
    }
    let name = program.file_stem().unwrap_or(program.as_os_str());
    write.join(step).join(format!("{}_{function}.ll", name.to_string_lossy()))
}

#[cfg(test)]
mod tests;
