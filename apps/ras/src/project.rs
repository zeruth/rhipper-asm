//! A port project: the `rhipper.json` at its root, which the pipeline and the workbench share.
//!
//! ```json
//! {
//!   "name": "Game",
//!   "exe": "ref/Game.exe",
//!   "gen": "src/gen",
//!   "lang": "c",
//!   "sources": ["src", "vendor"],
//!   "build": { "dir": "build/cmake", "config": "Release", "target": "Game" },
//!   "run": { "exe": "build/cmake/bin/Release/Game.exe", "cwd": ".", "args": [], "with": [], "timeout": 0, "compare": false }
//! }
//! ```
//!
//! `exe` is the reference binary and the only required key. Every path is relative to the root.
//! `gen` is where `ras emit` writes the translation and `lang` its language (`c`; `cpp` for the
//! same source with C++ linkage; `rust` for a cargo package, built by cargo under `build/cargo`);
//! `sources` are the directories of the hand-written
//! port, which the workbench reads and the pipeline does not. `build` is the `CMake` build of the
//! port, and `run` is how the pipeline starts it: `with`
//! lists side processes that start before the port and stop after it, `timeout` in seconds stops
//! a port that runs on, such as a game, and counts the run as alive, and `compare` runs the
//! reference binary after the port with the same arguments, working directory and timeout and
//! gives the run a verdict: `same`, `differs at line N` or `port crashed`.

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
}

#[derive(Debug, Clone)]
pub(crate) struct Project {
    pub root: PathBuf,
    pub name: String,
    pub exe: PathBuf,
    /// Where `ras emit` writes the translation.
    pub gen_dir: PathBuf,
    /// The language of the translation: `c`, `cpp` or `rust`.
    pub lang: String,
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
        };
        Ok(Self {
            exe: resolve(&exe),
            gen_dir: resolve(&string("gen").unwrap_or_else(|| "src/gen".into())),
            lang: string("lang").unwrap_or_else(|| "c".into()),
            name,
            build,
            run,
            root,
        })
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

    /// The executable of the port: `run.exe`, else the binary of the cargo package under
    /// `build/cargo` for Rust, else the target in the `CMake` output directory, in its
    /// configuration's subdirectory (a Visual Studio generator) or not (Ninja).
    pub(crate) fn port_exe(&self) -> PathBuf {
        if let Some(exe) = &self.run.exe {
            return exe.clone();
        }
        let name = format!("{}.exe", self.build.target);
        if self.is_rust() {
            let profile = if self.build.config.eq_ignore_ascii_case("debug") { "debug" } else { "release" };
            return self.build_dir().join("cargo").join(profile).join(name);
        }
        let bin = self.build.dir.join("bin");
        let multi = bin.join(&self.build.config).join(&name);
        if multi.exists() { multi } else { bin.join(name) }
    }
}
