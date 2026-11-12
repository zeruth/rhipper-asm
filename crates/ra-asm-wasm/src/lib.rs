//! A WebAssembly interpreter for the plugins of the engine. A plugin is a core-format module
//! (version 1, the MVP with the bulk memory copy and fill and the saturating truncations) that
//! owns its memory: the host decodes it ([`Module::decode`]), instantiates it with the functions it
//! imports ([`Instance::new`]), writes into its memory and calls its exports ([`Instance::call`]).
//!
//! The sandbox is the interpreter's: a plugin reads and writes its own memory and nothing else,
//! calls only what the host gives it, and runs under a budget of instructions ([`Instance::fuel`])
//! and a bound on its memory and its call depth, so a broken or a hostile plugin is an [`Error`],
//! never a fault or a hang of the host. The module is not validated before it runs: what
//! validation would refuse (a stack that underflows, a local or a function of no index) is an
//! error when the interpreter reaches it.

mod decode;
mod exec;

use std::fmt;
use std::sync::Arc;

pub use decode::Module;

/// A value type of the core format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValType {
    I32,
    I64,
    F32,
    F64,
}

/// The type of a function: what it takes and what it returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncType {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

/// Why a module did not decode, instantiate or run: a feature past the core set, a trap of the
/// code (with what trapped), or a bound of the sandbox reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    message: String,
}

impl Error {
    /// An error of `message`: what a host function returns to stop the plugin's call.
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

/// A function the host gives a plugin to import: it gets the plugin's memory and the arguments,
/// each value as its bits (an `i32` or an `f32` in the low half), and returns the results.
pub type HostFn = Box<dyn FnMut(&mut [u8], &[u64]) -> Result<Vec<u64>, Error>>;

/// The size of a page of memory.
pub const PAGE: usize = 65536;

/// The most pages a plugin's memory may grow to, whatever it declares: 256 MiB.
const MAX_PAGES: u32 = 4096;

/// The instructions a plugin may run, by default, before its call is stopped.
pub const DEFAULT_FUEL: u64 = 1_000_000_000;

/// A module instantiated: its memory, globals and table, the host functions it imports, and the
/// instructions it has left to run.
pub struct Instance {
    module: Arc<Module>,
    memory: Vec<u8>,
    max_pages: u32,
    globals: Vec<u64>,
    table: Vec<Option<u32>>,
    hosts: Vec<HostFn>,
    stack: Vec<u64>,
    /// The instructions left to run; a call that spends them stops with an error.
    pub fuel: u64,
}

impl Instance {
    /// Instantiates `module`: `resolve` gives the host function of each import by its module, its
    /// name and its type, and an import it does not give is an error that names it. The data and
    /// element segments are placed, and the start function runs.
    pub fn new(module: Module, mut resolve: impl FnMut(&str, &str, &FuncType) -> Option<HostFn>) -> Result<Self, Error> {
        let mut hosts = Vec::with_capacity(module.imports.len());
        for import in &module.imports {
            let ty = &module.types[import.ty as usize];
            let host = resolve(&import.module, &import.name, ty)
                .ok_or_else(|| Error::new(format!("the module imports {}.{}, which the host does not give", import.module, import.name)))?;
            hosts.push(host);
        }
        let (pages, max_pages) = match module.memory {
            Some(limits) => (limits.min, limits.max.unwrap_or(MAX_PAGES).min(MAX_PAGES)),
            None => (0, 0),
        };
        if pages > max_pages {
            return Err(Error::new(format!("a memory of {pages} pages, past the {MAX_PAGES} a plugin may have")));
        }
        let mut memory = vec![0u8; pages as usize * PAGE];
        for (offset, bytes) in &module.data {
            let at = *offset as u32 as usize;
            let end = at.checked_add(bytes.len()).filter(|&e| e <= memory.len());
            let Some(end) = end else { return Err(Error::new(format!("a data segment at 0x{offset:x} past the end of memory"))) };
            memory[at..end].copy_from_slice(bytes);
        }
        let mut table = vec![None; module.table.map_or(0, |t| t.min.min(1 << 20) as usize)];
        let functions = (module.imports.len() + module.funcs.len()) as u32;
        for (offset, items) in &module.elements {
            let at = *offset as u32 as usize;
            if at + items.len() > table.len() {
                return Err(Error::new(format!("an element segment at {at} past the end of the table")));
            }
            for (slot, &f) in table[at..].iter_mut().zip(items) {
                if f >= functions {
                    return Err(Error::new(format!("an element of no function ({f})")));
                }
                *slot = Some(f);
            }
        }
        let globals = module.globals.iter().map(|g| g.init).collect();
        let start = module.start;
        let mut instance =
            Self { module: Arc::new(module), memory, max_pages, globals, table, hosts, stack: Vec::new(), fuel: DEFAULT_FUEL };
        if let Some(start) = start {
            instance.invoke(start, 0)?;
            instance.stack.clear();
        }
        Ok(instance)
    }

    /// The type of the exported function `name`, when the module exports one.
    pub fn export_type(&self, name: &str) -> Option<&FuncType> {
        let index = self.export_func(name)?;
        self.module.func_type(index).ok()
    }

    fn export_func(&self, name: &str) -> Option<u32> {
        self.module.exports.iter().find(|e| e.name == name && e.kind == decode::ExportKind::Func).map(|e| e.index)
    }

    /// Calls the exported function `name` with `args`, each as its bits, and returns its results.
    pub fn call(&mut self, name: &str, args: &[u64]) -> Result<Vec<u64>, Error> {
        let index = self.export_func(name).ok_or_else(|| Error::new(format!("the module exports no function `{name}`")))?;
        let ty = self.module.func_type(index)?;
        if ty.params.len() != args.len() {
            return Err(Error::new(format!("`{name}` takes {} arguments, not {}", ty.params.len(), args.len())));
        }
        let results = ty.results.len();
        self.stack.clear();
        self.stack.extend_from_slice(args);
        let outcome = self.invoke(index, 0);
        let values = std::mem::take(&mut self.stack);
        match outcome {
            Err(e) => Err(Error::new(format!("`{name}`: {e}"))),
            Ok(()) if values.len() == results => Ok(values),
            Ok(()) => Err(Error::new(format!("`{name}` left a stack that is not its results"))),
        }
    }

    /// The plugin's memory, as it is between calls.
    pub fn memory(&self) -> &[u8] {
        &self.memory
    }

    /// The plugin's memory, for the host to write a request into.
    pub fn memory_mut(&mut self) -> &mut [u8] {
        &mut self.memory
    }
}
