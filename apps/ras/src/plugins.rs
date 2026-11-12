//! The plugins of the engine: WebAssembly modules run in the sandbox of `ra-asm-wasm`, each in a
//! directory of its own that holds `<name>.wasm` and the files it reads, under the project's
//! `rhipper/plugins/` and the user's plugin directory (`RHIPPER_PLUGINS`, else
//! `%APPDATA%/rhipper/plugins` or `~/.config/rhipper/plugins`); a project's plugin hides the
//! user's of the same name.
//!
//! The interface is ABI 1. A plugin exports its memory, `ra_abi() -> i32` (1), `ra_alloc(size:
//! i32) -> i32` (where the host writes a request) and one export per thing it adds: `ra_names`
//! a name source, `ra_raise` a raise pass, `ra_signatures` a signature library, `ra_emit` an
//! output language, `ra_device` a device model of the platform runtime. It may import
//! `ra.log(text: i32, length: i32)`, a line the host shows as a warning. An export takes the
//! request's address and length and returns the reply's as `address << 32 | length`.
//!
//! The request is text: a `file <name> <length>` line with the bytes of each file of the plugin's
//! directory after it, then a `function 0x<address> <name>` line for each function of the
//! analysis, in address order. A name source replies with `0x<address> <name>` lines; the engine
//! checks each name as it checks an annotation's, and an annotation's name wins over a plugin's.
//! The engine runs the name sources (the emit and lift steps, `ras emit --project`, `ras serve`);
//! the other kinds are listed for the workbench and wait for their hooks in the engine.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{Context, Result, anyhow, bail};
use ra_asm_analysis::{Analysis, Kind};
use ra_asm_emit::functions::format_address;
use ra_asm_wasm::{FuncType, HostFn, Instance, Module, ValType};
use serde_json::{Value, json};

/// Where a project's plugins live, under its root.
pub(crate) const PLUGINS_DIR: &str = "rhipper/plugins";

/// The interface this engine speaks.
const ABI: u64 = 1;

/// What a plugin can add, by the export that adds it.
const KINDS: [(&str, &str); 5] =
    [("ra_names", "names"), ("ra_raise", "raise"), ("ra_signatures", "signatures"), ("ra_emit", "emit"), ("ra_device", "device")];

/// The most bytes of files a plugin is given: a bound on the request the host builds.
const MAX_FILES: u64 = 64 << 20;

/// A plugin found on disk.
pub(crate) struct Plugin {
    pub name: String,
    /// `project` or `user`.
    pub origin: &'static str,
    pub dir: PathBuf,
}

impl Plugin {
    fn wasm(&self) -> PathBuf {
        self.dir.join(format!("{}.wasm", self.name))
    }
}

/// The user's plugin directory.
fn user_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("RHIPPER_PLUGINS") {
        return (!dir.is_empty()).then(|| PathBuf::from(dir));
    }
    std::env::var_os("APPDATA")
        .map(|d| PathBuf::from(d).join("rhipper/plugins"))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config/rhipper/plugins")))
}

/// The plugins of `dir`, by name: each subdirectory that holds its `<name>.wasm`.
fn plugins_in(dir: &Path, origin: &'static str) -> Vec<Plugin> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Plugin> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?.to_owned();
            path.join(format!("{name}.wasm")).is_file().then_some(Plugin { name, origin, dir: path })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// The plugins of the project at `root` and the user's, the project's first.
pub(crate) fn discover(root: Option<&Path>) -> Vec<Plugin> {
    let mut out = root.map(|r| plugins_in(&r.join(PLUGINS_DIR), "project")).unwrap_or_default();
    if let Some(dir) = user_dir() {
        for plugin in plugins_in(&dir, "user") {
            if !out.iter().any(|p| p.name == plugin.name) {
                out.push(plugin);
            }
        }
    }
    out
}

/// A plugin instantiated, with the lines it logged.
struct Loaded {
    instance: Instance,
    log: Rc<RefCell<Vec<String>>>,
}

impl Loaded {
    fn open(plugin: &Plugin) -> Result<Self> {
        let path = plugin.wasm();
        let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
        let module = Module::decode(&bytes).map_err(|e| anyhow!("{} is no plugin: {e}", path.display()))?;
        let log = Rc::new(RefCell::new(Vec::new()));
        let sink = Rc::clone(&log);
        let instance = Instance::new(module, |module, name, ty| -> Option<HostFn> {
            let takes_text = *ty == FuncType { params: vec![ValType::I32, ValType::I32], results: vec![] };
            (module == "ra" && name == "log" && takes_text).then(|| {
                let sink = Rc::clone(&sink);
                Box::new(move |memory: &mut [u8], args: &[u64]| {
                    let (at, length) = (args[0] as u32 as usize, args[1] as u32 as usize);
                    let text = memory.get(at..at + length).ok_or_else(|| ra_asm_wasm::Error::new("a log line out of bounds"))?;
                    sink.borrow_mut().push(String::from_utf8_lossy(text).into_owned());
                    Ok(Vec::new())
                }) as HostFn
            })
        })
        .map_err(|e| anyhow!("{}: {e}", path.display()))?;
        let mut loaded = Self { instance, log };
        let abi = loaded.instance.call("ra_abi", &[]).map_err(|e| anyhow!("{}: {e}", path.display()))?;
        if abi.first().map(|&v| u64::from(v as u32)) != Some(ABI) {
            bail!("{} speaks plugin ABI {:?}, not {ABI}", path.display(), abi.first());
        }
        Ok(loaded)
    }

    /// What the plugin adds, by the exports it has.
    fn kinds(&self) -> Vec<&'static str> {
        KINDS.iter().filter(|(export, _)| self.instance.export_type(export).is_some()).map(|&(_, kind)| kind).collect()
    }

    /// Calls `export` with `request` written into the plugin's memory, and returns the reply.
    fn ask(&mut self, export: &str, request: &[u8]) -> Result<String> {
        let length = u32::try_from(request.len()).map_err(|_| anyhow!("a request of {} bytes", request.len()))?;
        let at = self.instance.call("ra_alloc", &[u64::from(length)])?.first().copied().unwrap_or(0) as u32 as usize;
        let room = self.instance.memory_mut().get_mut(at..at + request.len()).filter(|_| at != 0);
        let Some(room) = room else { bail!("`ra_alloc` gave no room for a request of {length} bytes") };
        room.copy_from_slice(request);
        let packed = self.instance.call(export, &[at as u64, u64::from(length)])?.first().copied().unwrap_or(0);
        let (at, length) = ((packed >> 32) as u32 as usize, packed as u32 as usize);
        let reply = self.instance.memory().get(at..at + length).ok_or_else(|| anyhow!("`{export}` replied out of its memory"))?;
        Ok(String::from_utf8_lossy(reply).into_owned())
    }
}

/// The request of a plugin in `dir` over `analysis`: its files, then the functions.
fn request(dir: &Path, analysis: &Analysis<'_>) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("cannot read {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().is_none_or(|e| e != "wasm"))
        .collect();
    paths.sort();
    let mut total = 0;
    for path in paths {
        let Some(name) = path.file_name().and_then(|n| n.to_str()).filter(|n| !n.contains(char::is_whitespace)) else { continue };
        let bytes = std::fs::read(&path).with_context(|| format!("cannot read {}", path.display()))?;
        total += bytes.len() as u64;
        if total > MAX_FILES {
            bail!("the files of {} pass {MAX_FILES} bytes", dir.display());
        }
        out.extend_from_slice(format!("file {name} {}\n", bytes.len()).as_bytes());
        out.extend_from_slice(&bytes);
        out.push(b'\n');
    }
    for f in analysis.functions.values() {
        out.extend_from_slice(format!("function {} {}\n", format_address(f.entry), f.name).as_bytes());
    }
    Ok(out)
}

/// The names `plugin`'s name source gives, by address, with the lines it logged.
fn names_of(plugin: &Plugin, analysis: &Analysis<'_>, warnings: &mut Vec<String>) -> Result<Vec<(u64, String)>> {
    let mut loaded = Loaded::open(plugin)?;
    if !loaded.kinds().contains(&"names") {
        return Ok(Vec::new());
    }
    let reply = loaded.ask("ra_names", &request(&plugin.dir, analysis)?);
    warnings.extend(loaded.log.borrow().iter().map(|line| format!("the plugin {}: {line}", plugin.name)));
    let mut out = Vec::new();
    for line in reply?.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let parsed = line.split_once(' ').and_then(|(a, n)| Some((crate::project::hand_address(a)?, n.trim())));
        match parsed {
            Some((address, name)) if !name.is_empty() => out.push((address, name.to_owned())),
            _ => warnings.push(format!("the plugin {} replied `{line}`, which is not an address and a name", plugin.name)),
        }
    }
    Ok(out)
}

/// Gives the functions of `analysis` the names the name sources among the plugins of the project
/// at `root` and the user's give them, in the order the plugins are found. A plugin that does not
/// load or run, and a name the function cannot take (as [`crate::annotations::name_fault`] has
/// it), is a warning, and the function keeps its name.
pub(crate) fn apply_names(root: &Path, analysis: &mut Analysis<'_>, warnings: &mut Vec<String>) {
    for plugin in discover(Some(root)) {
        let names = match names_of(&plugin, analysis, warnings) {
            Ok(names) => names,
            Err(e) => {
                warnings.push(format!("the plugin {} did not run: {e:#}", plugin.name));
                continue;
            }
        };
        for (address, name) in names {
            let fault = match analysis.functions.get(&address) {
                None => Some(format!("no function starts at {}", format_address(address))),
                Some(f) if matches!(f.kind, Kind::Runtime(_)) => Some(format!("the host's runtime supplies {}", format_address(address))),
                Some(f) if f.name == name => continue,
                Some(_) => crate::annotations::name_fault(analysis, address, &name),
            };
            match fault {
                Some(fault) => warnings.push(format!("the plugin {} names `{name}`: {fault}; the function keeps its name", plugin.name)),
                None => {
                    if let Some(f) = analysis.functions.get_mut(&address) {
                        f.name = name;
                    }
                }
            }
        }
    }
}

/// The plugins of the project at `root` and the user's, as the workbench lists them: the name,
/// where it was found, the directory, what it adds, and why it does not load when it does not.
pub(crate) fn list_json(root: Option<&Path>) -> Value {
    let plugins: Vec<Value> = discover(root)
        .iter()
        .map(|plugin| {
            let loaded = Loaded::open(plugin);
            json!({
                "name": plugin.name,
                "origin": plugin.origin,
                "dir": plugin.dir.display().to_string(),
                "kinds": loaded.as_ref().map(Loaded::kinds).unwrap_or_default(),
                "error": loaded.as_ref().err().map(|e| format!("{e:#}")),
            })
        })
        .collect();
    json!({ "abi": ABI, "user": user_dir().map(|d| d.display().to_string()), "plugins": plugins })
}

/// `ras plugins`: the plugins of the project at `root` and the user's.
pub(crate) fn list(root: Option<&Path>, json: bool) -> Result<()> {
    let listed = list_json(root);
    if json {
        println!("{}", serde_json::to_string_pretty(&listed)?);
        return Ok(());
    }
    let plugins = listed["plugins"].as_array().cloned().unwrap_or_default();
    if plugins.is_empty() {
        println!("no plugins");
    }
    for p in plugins {
        let kinds: Vec<&str> = p["kinds"].as_array().map(|k| k.iter().filter_map(Value::as_str).collect()).unwrap_or_default();
        let state = p["error"].as_str().map_or_else(|| kinds.join(","), |e| format!("error: {e}"));
        println!(
            "{:<20} {:<8} {:<30} {}",
            p["name"].as_str().unwrap_or(""),
            p["origin"].as_str().unwrap_or(""),
            state,
            p["dir"].as_str().unwrap_or("")
        );
    }
    Ok(())
}
