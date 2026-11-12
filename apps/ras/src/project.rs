//! A port project: the `rhipper.json` at its root, which the pipeline and the workbench share.
//!
//! ```json
//! {
//!   "name": "Game",
//!   "exe": "ref/Game.exe",
//!   "map": "ref/Game.map",
//!   "signatures": ["sig/libcmt-14.51-x86.sig"],
//!   "gen": "src/gen",
//!   "lang": "c",
//!   "x87": "double",
//!   "sources": ["src", "vendor"],
//!   "hand": ["fib", "0x140001330"],
//!   "build": { "dir": "build/cmake", "config": "Release", "target": "Game" },
//!   "run": { "exe": "build/cmake/bin/Release/Game.exe", "cwd": ".", "args": [], "with": [], "timeout": 0, "compare": false, "trace": false }
//! }
//! ```
//!
//! `exe` is the reference binary and the only required key. Every path is relative to the root.
//! `map` is the link map of the reference build, which names its functions; without the key, the
//! PDB or the `<exe>.map` beside the binary does, when there is one. `signatures` lists library
//! signatures (`ras signatures` files, or `.lib`/`.a`/`.obj` files to make them from) applied to
//! the image before the analysis: a match names a library function and keeps its body out of the
//! port. `gen` is where `ras emit`
//! writes the translation and `lang` its language (`c`; `cpp` for the
//! same source with C++ linkage; `rust` for a cargo package, built by cargo under `build/cargo`);
//! `x87` is the floating-point unit the lift models for x87 code: `double` (the default, eight
//! doubles) or `exact` (the 80-bit unit through the runtime's software `f80`, for a program that
//! leans on the extended precision); `sources` are the directories of the hand-written
//! port, which the workbench reads and the `CMakeLists.txt` the pipeline writes compiles into the
//! port beside the translation (the files under `gen` excepted). `hand` names the functions the
//! hand-written sources own, by source name or by address: `ras emit` writes each as its
//! declaration, under its C name and linkage, in place of its body, and the function table
//! marks it `hand`; a name no function of the image has is a warning of the emit step. `build` is
//! the `CMake` build of the port, and `run` is how the pipeline starts it: `with`
//! lists side processes that start before the port and stop after it, `timeout` in seconds stops
//! a port that runs on, such as a game, and counts the run as alive, and `compare` runs the
//! reference binary after the port with the same arguments, working directory and timeout and
//! gives the run a verdict: `same`, `differs at line N` or `port crashed`; `trace` emits the port
//! with the trace hooks and makes the run step record the port's trace of calls and returns with
//! their values (`build/pipeline/port.trace`) and the reference's, under a debugger, with
//! breakpoints at the port's functions (`reference.trace`), compared to the first event where
//! they part, which names the function. When the reference is
//! a DLL, the port is a DLL too and `run.exe` is the program that loads it (the game's executable,
//! or a test host): the port's directory leads its DLL search, and the reference's directory the
//! comparison run's.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::Value;

pub(crate) const RHIPPER_JSON: &str = "rhipper.json";

#[derive(Debug, Clone)]
pub(crate) struct Build {
    pub dir: PathBuf,
    pub config: String,
    pub target: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Run {
    pub exe: Option<PathBuf>,
    pub cwd: PathBuf,
    pub args: Vec<String>,
    pub with: Vec<Vec<String>>,
    pub timeout: Option<u64>,
    /// Whether the run step also runs the reference and compares the outputs.
    pub compare: bool,
    /// Whether the port is emitted with the trace hooks and the run step records its trace beside
    /// the reference's, compared to the first event where they part.
    pub trace: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Project {
    pub root: PathBuf,
    pub name: String,
    pub exe: PathBuf,
    /// The link map that names the reference's functions, when the project names one.
    pub map: Option<PathBuf>,
    /// The library signatures applied to the reference before the analysis.
    pub signatures: Vec<PathBuf>,
    /// Where `ras emit` writes the translation.
    pub gen_dir: PathBuf,
    /// The language of the translation: `c`, `cpp` or `rust`.
    pub lang: String,
    /// The x87 unit the lift models: `double` or `exact`.
    pub x87: ra_asm_lift::X87Unit,
    /// The directories of the hand-written sources, compiled into the port.
    pub sources: Vec<PathBuf>,
    /// The functions the hand-written sources own, by source name or address, as written.
    pub hand: Vec<String>,
    pub build: Build,
    pub run: Run,
}

impl Project {
    /// Reads the `rhipper.json` of the project at `root`.
    pub(crate) fn load(root: &Path) -> Result<Self> {
        // Absolute but not canonical: a canonical Windows path starts with `\\?\`, which the
        // shells that CMake and the port's build start cannot take as a working directory.
        let root = std::path::absolute(root).with_context(|| format!("no project directory at {}", root.display()))?;
        if !root.is_dir() {
            bail!("no project directory at {}", root.display());
        }
        let file = root.join(RHIPPER_JSON);
        let text = std::fs::read_to_string(&file).with_context(|| format!("no {RHIPPER_JSON} at {}", root.display()))?;
        let json: Value = serde_json::from_str(&text).with_context(|| format!("{} is not JSON", file.display()))?;
        Self::parse(root, &json)
    }

    pub(crate) fn parse(root: PathBuf, json: &Value) -> Result<Self> {
        let Some(obj) = json.as_object() else { bail!("{RHIPPER_JSON} must hold a JSON object") };
        let string = |key: &str| obj.get(key).and_then(Value::as_str).map(str::to_owned);
        let Some(exe) = string("exe") else { bail!("{RHIPPER_JSON} must set \"exe\", the reference binary") };
        let name =
            string("name").unwrap_or_else(|| root.file_name().map_or_else(|| "port".to_owned(), |n| n.to_string_lossy().into_owned()));
        let section = |key: &str| obj.get(key).and_then(Value::as_object);
        let section_string = |key: &str, field: &str| section(key).and_then(|s| s.get(field)).and_then(Value::as_str).map(str::to_owned);
        let strings = |value: Option<&Value>| -> Vec<String> {
            value.and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect()).unwrap_or_default()
        };
        let resolve = |path: &str| root.join(path);
        let build = Build {
            dir: resolve(&section_string("build", "dir").unwrap_or_else(|| "build/cmake".into())),
            config: section_string("build", "config").unwrap_or_else(|| "Release".into()),
            target: section_string("build", "target").unwrap_or_else(|| name.clone()),
        };
        let run = section("run");
        let run = Run {
            exe: section_string("run", "exe").map(|e| resolve(&e)),
            cwd: resolve(&section_string("run", "cwd").unwrap_or_else(|| ".".into())),
            args: strings(run.and_then(|r| r.get("args"))),
            with: run
                .and_then(|r| r.get("with"))
                .and_then(Value::as_array)
                .map(|a| a.iter().map(|c| strings(Some(c))).filter(|c| !c.is_empty()).collect())
                .unwrap_or_default(),
            timeout: run.and_then(|r| r.get("timeout")).and_then(Value::as_u64).filter(|t| *t > 0),
            compare: run.and_then(|r| r.get("compare")).and_then(Value::as_bool).unwrap_or(false),
            trace: run.and_then(|r| r.get("trace")).and_then(Value::as_bool).unwrap_or(false),
        };
        let x87 = match string("x87") {
            None => ra_asm_lift::X87Unit::Double,
            Some(text) => text.parse().map_err(|e: String| anyhow::anyhow!("{RHIPPER_JSON}: {e}"))?,
        };
        Ok(Self {
            exe: resolve(&exe),
            map: string("map").map(|m| resolve(&m)),
            signatures: strings(obj.get("signatures")).iter().map(|s| resolve(s)).collect(),
            gen_dir: resolve(&string("gen").unwrap_or_else(|| "src/gen".into())),
            lang: string("lang").unwrap_or_else(|| "c".into()),
            x87,
            sources: strings(obj.get("sources")).iter().map(|s| resolve(s)).collect(),
            hand: strings(obj.get("hand")),
            name,
            build,
            run,
            root,
        })
    }

    /// Gives a function to the hand-written sources (`owned`) or takes it back: rewrites the
    /// `hand` list of the project's `rhipper.json`, the other keys as they were, and returns the
    /// list. `function` is the name or the address the list carries; an address matches the entry
    /// written for it in either spelling (`0x1400` and `0x00001400` are one function).
    pub(crate) fn set_hand(&self, function: &str, owned: bool) -> Result<Vec<String>> {
        let file = self.root.join(RHIPPER_JSON);
        let text = std::fs::read_to_string(&file).with_context(|| format!("no {RHIPPER_JSON} at {}", self.root.display()))?;
        let mut json: Value = serde_json::from_str(&text).with_context(|| format!("{} is not JSON", file.display()))?;
        let Some(obj) = json.as_object_mut() else { bail!("{RHIPPER_JSON} must hold a JSON object") };
        let mut hand: Vec<String> = obj
            .get("hand")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect())
            .unwrap_or_default();
        hand.retain(|h| !same_function(h, function));
        if owned {
            hand.push(function.to_owned());
        }
        if hand.is_empty() {
            obj.remove("hand");
        } else {
            obj.insert("hand".into(), Value::Array(hand.iter().map(|h| Value::String(h.clone())).collect()));
        }
        let mut out = serde_json::to_string_pretty(&json)?;
        out.push('\n');
        std::fs::write(&file, out).with_context(|| format!("cannot write {}", file.display()))?;
        Ok(hand)
    }

    /// Loads the reference image, with the names of its functions from the project's link map
    /// when it names one, and the project's library signatures applied.
    pub(crate) fn load_image(&self) -> Result<ra_asm_pe::Image> {
        let mut image = match &self.map {
            Some(map) => ra_asm_pe::load_with_map(&self.exe, map),
            None => ra_asm_pe::load(&self.exe),
        }?;
        if !self.signatures.is_empty() {
            crate::signatures::apply_files(&mut image, &self.signatures)?;
        }
        Ok(image)
    }

    /// The build outputs the pipeline writes: `build/`.
    pub(crate) fn build_dir(&self) -> PathBuf {
        self.root.join("build")
    }

    /// The logs of the pipeline steps: `build/pipeline/`.
    pub(crate) fn log_dir(&self) -> PathBuf {
        self.build_dir().join("pipeline")
    }

    /// Whether the translation is Rust, built by cargo instead of `CMake`.
    pub(crate) fn is_rust(&self) -> bool {
        self.lang.eq_ignore_ascii_case("rust") || self.lang.eq_ignore_ascii_case("rs")
    }

    /// The binary of the port: `run.exe` for a program, else the binary of the cargo package under
    /// `build/cargo` for Rust, else the target in the `CMake` output directory, in its
    /// configuration's subdirectory (a Visual Studio generator) or not (Ninja). The port of a DLL
    /// (`dll`) is the target's `.dll` there: `run.exe` is then the program that loads it.
    pub(crate) fn port_binary(&self, dll: bool) -> PathBuf {
        if !dll && let Some(exe) = &self.run.exe {
            return exe.clone();
        }
        let name = format!("{}.{}", self.build.target, if dll { "dll" } else { "exe" });
        if self.is_rust() {
            let profile = if self.build.config.eq_ignore_ascii_case("debug") { "debug" } else { "release" };
            return self.build_dir().join("cargo").join(profile).join(name);
        }
        let bin = self.build.dir.join("bin");
        let multi = bin.join(&self.build.config).join(&name);
        if multi.exists() { multi } else { bin.join(name) }
    }
}

/// Whether two entries of the `hand` list name one function: the same text, or one address in two
/// spellings.
pub(crate) fn same_function(a: &str, b: &str) -> bool {
    a == b || matches!((hand_address(a), hand_address(b)), (Some(x), Some(y)) if x == y)
}

/// The address an entry of the `hand` list names, when it is one: `0x` and hexadecimal digits. A
/// name is never read as an address, however hexadecimal it looks (`add`, `dead`).
pub(crate) fn hand_address(entry: &str) -> Option<u64> {
    let digits = entry.trim().strip_prefix("0x").or_else(|| entry.trim().strip_prefix("0X"))?;
    u64::from_str_radix(digits, 16).ok()
}
