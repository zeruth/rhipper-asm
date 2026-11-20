//! `ras emit`: source out of an image, in C, C++ or Rust, with the function table beside it. A C or
//! C++ translation follows the modules the build's symbols name, a file and a header each.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, bail};
use ra_asm_analysis::{Analysis, Identity};
use ra_asm_emit::functions::format_address;
use ra_asm_emit::{EmittedFile, EmittedFunction, FunctionInfo};
use ra_asm_lift::raise::timed;
use ra_asm_lift::{Companion, Lifted};
use ra_asm_pe::Image;
use serde_json::{Value, json};

use crate::annotations::Annotation;

/// The C runtime, written beside a C or C++ translation; a Rust translation carries its own.
const RHIPPER_H: &str = include_str!("../../../runtime/rhipper.h");
pub(crate) const DRIVER_C: &str = include_str!("../../../runtime/driver.c");

/// A translation: the files to write, the functions in them, and what the analysis knows.
pub(crate) struct Translation {
    pub files: Vec<EmittedFile>,
    pub functions: Vec<EmittedFunction>,
    pub infos: HashMap<String, FunctionInfo>,
    /// The runtime files, by path under the output directory.
    pub runtime: Vec<(&'static str, &'static str)>,
    /// The image's resources, by path under the output directory: `resources.res` for the link,
    /// and each one as a file under `resources/` for reading.
    pub resources: Vec<(String, Vec<u8>)>,
    /// The module definition file of a DLL's port, `exports.def`: the image's exports as the
    /// port's. `None` for a program.
    pub exports: Option<String>,
    /// What the translation could not carry over, such as an export at no symbol of the port.
    pub warnings: Vec<String>,
}

impl Translation {
    /// The whole translation as text, for the standard output.
    pub(crate) fn text(&self) -> String {
        if self.files.len() == 1 {
            return self.files[0].text.clone();
        }
        let mut out = String::new();
        for f in &self.files {
            out.push_str("// ==== ");
            out.push_str(&f.name);
            out.push('\n');
            out.push_str(&f.text);
            out.push('\n');
        }
        out
    }

    pub(crate) fn instructions_not_lifted(&self) -> usize {
        self.functions.iter().map(|f| f.traps).sum()
    }

    pub(crate) fn bytes(&self) -> usize {
        self.files.iter().map(|f| f.text.len()).sum()
    }

    /// Writes the files, the runtime and the function table into `dir`.
    pub(crate) fn write(&self, dir: &Path) -> Result<()> {
        ra_asm_emit::write_files(dir, &self.files).with_context(|| format!("cannot write into {}", dir.display()))?;
        self.write_beside(dir)
    }

    /// Formats the source files as [`Translation::format`] does and writes each into `dir` as soon
    /// as it is formatted, the files after it still with the formatters, then the runtime beside
    /// them as [`Translation::write`] does. Each file's text goes once it is written: the files
    /// keep their names and their functions' lines, for the table. Returns the bytes of source
    /// written.
    pub(crate) fn format_and_write(&mut self, dir: &Path) -> Result<usize> {
        let bytes = ra_asm_emit::format_and_write_files(dir, &mut self.files, &self.functions)
            .with_context(|| format!("cannot write into {}", dir.display()))?;
        self.write_beside(dir)?;
        Ok(bytes)
    }

    /// Writes the runtime, the resources and the module definition file into `dir`.
    fn write_beside(&self, dir: &Path) -> Result<()> {
        let exports = self.exports.iter().map(|d| ("exports.def", d.as_bytes()));
        let files =
            self.runtime.iter().map(|(p, t)| (*p, t.as_bytes())).chain(self.resources.iter().map(|(p, b)| (p.as_str(), b.as_slice())));
        for (path, bytes) in files.chain(exports) {
            ra_asm_emit::write_if_changed(&dir.join(path), bytes)?;
        }
        Ok(())
    }

    pub(crate) fn table(&self) -> String {
        ra_asm_emit::table(&self.functions, &self.files, &self.infos)
    }

    /// Passes the source files through `clang-format` or `rustfmt` when the tool is on the PATH,
    /// keeping the table's lines on the definitions.
    pub(crate) fn format(&mut self) {
        ra_asm_emit::format_files(&mut self.files, &self.functions);
    }
}

/// What a translation is asked for, beyond the image and its lift.
pub(crate) struct Options<'a> {
    /// The language: `c`, `cpp` or `rust`.
    pub lang: &'a str,
    /// The name of a Rust package and its binary.
    pub package: &'a str,
    /// Instruments a C or C++ translation with the runtime's trace hooks.
    pub trace: bool,
    /// The functions the hand-written sources own, by source name or address, which a C or C++
    /// translation declares instead of defining.
    pub hand: &'a [String],
    /// The person's annotations of the project, by address, which the function table carries.
    pub annotations: &'a BTreeMap<u64, Annotation>,
    /// Writes a C or C++ translation checked: each open assumption the translation can watch is
    /// a `ra_assume` at its site (see [`checks`]).
    pub checked: bool,
    /// The second images `lifted` holds beside the program's ([`ra_asm_lift::lift_frozen_with`]),
    /// whose functions the table lists under their image.
    pub with: &'a [Companion<'a>],
}

/// Translates the raised module of `lifted` as `options` ask. `signatures` (by the final function
/// names) tell the C++ output which virtual functions read their object. A C or C++ translation
/// drops each function's IR once its source is written: `lifted` keeps its tables and its data, and
/// of each function its name and type, not its body. The raised functions may be frozen
/// ([`ra_asm_lift::raise::raise_frozen`]): the translation thaws them as it reads them.
pub(crate) fn translate(
    image: &Image,
    analysis: &Analysis<'_>,
    lifted: &mut Lifted,
    signatures: &HashMap<String, ra_asm_lift::raise::abi::Signature>,
    options: &Options<'_>,
) -> Result<Translation> {
    let Options { lang, package, trace, hand, annotations, checked, with } = *options;
    let infos = timed("translate: infos", || function_infos(image, analysis, lifted, annotations, with));
    let names: HashMap<String, String> = infos.iter().map(|(ir, i)| (ir.clone(), i.name.clone())).collect();
    let images: Vec<&Image> = std::iter::once(image).chain(with.iter().map(|c| c.image)).collect();
    let mut resources = timed("translate: resources", || {
        let mut resources = resource_files(image);
        resources.extend(import_defs(&images, &lifted.bound));
        resources
    });
    let mut warnings = Vec::new();
    let hand = hand_functions(&infos, hand, &mut warnings);
    let exports = image.headers.coff.is_dll().then(|| exports_def(image, lifted, &mut warnings));
    if lang.eq_ignore_ascii_case("rust") || lang.eq_ignore_ascii_case("rs") {
        if exports.is_some() {
            bail!("a DLL ports as C or C++: the Rust translation is a program");
        }
        if !hand.is_empty() {
            warnings.push(format!("{} hand-written functions are named, which a Rust port does not take: they are translated", hand.len()));
        }
        if checked {
            warnings.push("a Rust port is not written checked: its assumptions are not watched".to_owned());
        }
        let imports = images
            .iter()
            .flat_map(|i| &i.imports)
            .filter(|i| !lifted.bound.contains_key(&i.display_name()))
            .filter_map(|i| i.name.clone().map(|n| (n, i.dll.clone())))
            .collect();
        // The Rust backend reads the module whole.
        ra_asm_lift::raise::thaw(&mut lifted.module);
        let entry = lifted.entry.and_then(|e| lifted.functions.get(&e).cloned());
        let options = ra_asm_emit::RustOptions {
            names,
            imports,
            entry,
            package: package.to_owned(),
            pointer_bits: u32::from(image.pointer_size()) * 8,
            resources: !image.resources.is_empty(),
            ..Default::default()
        };
        let mut output = timed("translate: functions", || ra_asm_emit::emit_rust(&lifted.module, &options));
        let calls = platform_calls(&lifted.module.declarations);
        if !calls.is_empty()
            && let Some(runtime) = output.files.iter_mut().find(|f| f.name == "src/rhipper.rs")
        {
            runtime.text.push_str(&platform_rust(&calls));
        }
        return Ok(Translation {
            files: output.files,
            functions: output.functions,
            infos,
            runtime: Vec::new(),
            resources,
            exports,
            warnings,
        });
    }
    let Some(dialect) = ra_asm_emit::Dialect::parse(lang) else {
        bail!("the language `{lang}` is not `c`, `cpp` or `rust`");
    };
    let analyses: Vec<&Analysis<'_>> = std::iter::once(analysis).chain(with.iter().map(|c| c.analysis)).collect();
    let classes: Vec<_> = timed("translate: classes", || analyses.iter().flat_map(|a| ra_asm_emit::classes(a, signatures)).collect());
    let checks = if checked { timed("translate: checks", || checks(&analyses, lifted)) } else { Vec::new() };
    let guest32 = image.pointer_size() == 4;
    // In C++ the object of a container's member function reads as the container.
    let containers = if dialect == ra_asm_emit::Dialect::Cpp { ra_asm_emit::containers(analysis) } else { Vec::new() };
    let options = ra_asm_emit::COptions { names, dialect, classes, trace, hand, checks, guest32, containers };
    // A console's port calls its platform: the prototypes go where `rhipper.h` finds them, the
    // calls and the stub runtime into a source of the build.
    let calls = platform_calls(&lifted.module.declarations);
    if !calls.is_empty() {
        let (header, source) = platform_c(&calls);
        resources.push(("platform.h".to_owned(), header.into_bytes()));
        resources.push(("platform.c".to_owned(), source.into_bytes()));
    }
    // The unit times its own parts: the module's, the functions' and the unit's.
    let mut unit = ra_asm_emit::emit_unit_dropping(&mut lifted.module, &options);
    // The bytes of the large data globals the source embeds go beside it, as the resources do.
    resources.append(&mut unit.blobs);
    for id in &unit.unwatched {
        warnings.push(format!("assumption {id} has no site in the translation: it is not watched"));
    }
    // The modules of the build, when its symbols name two object files or more; else by size.
    let files = timed("translate: files", || {
        let objects = ra_asm_emit::functions::objects(&infos);
        unit.module_files(&objects).unwrap_or_else(|| unit.files(ra_asm_emit::FUNCTIONS_PER_FILE))
    });
    Ok(Translation {
        files,
        functions: unit.functions,
        infos,
        runtime: vec![("runtime/rhipper.h", RHIPPER_H), ("runtime/driver.c", DRIVER_C)],
        resources,
        exports,
        warnings,
    })
}

/// What a call on the platform interface asks the runtime bound for.
#[derive(Debug, Clone, Copy)]
enum PlatformCall<'a> {
    /// An access of a device register.
    Device(ra_asm_lift::data::DeviceCall<'a>),
    /// A function of the console's kernel, by its name.
    Kernel(&'static str),
}

/// The program's calls on the platform interface of its console among the declarations of its
/// raised module, each with its declaration: none for a program of an operating system.
fn platform_calls(declarations: &[ra_asm_ir::Declaration]) -> Vec<(&ra_asm_ir::Declaration, PlatformCall<'_>)> {
    declarations
        .iter()
        .filter_map(|d| {
            let call = match ra_asm_lift::data::DeviceCall::parse(&d.name) {
                Some(device) => PlatformCall::Device(device),
                None => PlatformCall::Kernel(ra_asm_lift::raise::prototypes::ee_syscall_named(&d.name)?.name),
            };
            Some((d, call))
        })
        .collect()
}

/// The C type of a word of the platform interface.
fn c_word(ty: &ra_asm_ir::Type) -> &'static str {
    match ty {
        ra_asm_ir::Type::Void => "void",
        ra_asm_ir::Type::Int(8) => "uint8_t",
        ra_asm_ir::Type::Int(16) => "uint16_t",
        ra_asm_ir::Type::Int(32) => "uint32_t",
        _ => "uint64_t",
    }
}

/// The Rust type of a word of the platform interface.
fn rust_word(ty: &ra_asm_ir::Type) -> &'static str {
    match ty {
        ra_asm_ir::Type::Int(8) => "u8",
        ra_asm_ir::Type::Int(16) => "u16",
        ra_asm_ir::Type::Int(32) => "u32",
        _ => "u64",
    }
}

/// The stub runtime of the C runtime's platform interface (see `runtime/rhipper.h`): each call
/// logged on stderr as the Rust runtime's `StubPlatform` logs it, and answered zero.
const PLATFORM_STUB_C: &str = r#"#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "rhipper.h"

static uint64_t ra_stub_read(const char *device, uint64_t reg, unsigned bits) {
    fprintf(stderr, "ra: %s read%u 0x%llx\n", device, bits, (unsigned long long)reg);
    return 0;
}

static void ra_stub_write(const char *device, uint64_t reg, unsigned bits, uint64_t value) {
    fprintf(stderr, "ra: %s write%u 0x%llx 0x%llx\n", device, bits, (unsigned long long)reg, (unsigned long long)value);
    // The serial port's transmit FIFO is the console.
    if (strcmp(device, "sio") == 0 && reg == RA_SIO_TXFIFO) {
        fputc((int)(uint8_t)value, stdout);
        fflush(stdout);
    }
}

static uint64_t ra_stub_call(const char *name, unsigned count, const uint64_t *args) {
    fprintf(stderr, "ra: call %s(", name);
    for (unsigned i = 0; i < count; i++) {
        fprintf(stderr, i ? ", 0x%llx" : "0x%llx", (unsigned long long)args[i]);
    }
    fprintf(stderr, ")\n");
    // The kernel's exits end the run.
    if (strcmp(name, "ExitThread") == 0 || strcmp(name, "ExitDeleteThread") == 0) {
        fflush(stdout);
        exit(0);
    }
    if (strcmp(name, "KExit") == 0) {
        fflush(stdout);
        exit((int)(uint32_t)args[0]);
    }
    return 0;
}

const ra_platform ra_platform_stub = {ra_stub_read, ra_stub_write, ra_stub_call};
const ra_platform *ra_platform_bound = &ra_platform_stub;

void ra_platform_bind(const ra_platform *platform) {
    ra_platform_bound = platform;
}
"#;

/// `platform.h`, the prototypes of the program's calls on the platform interface, and
/// `platform.c`, each call made on the runtime bound, with the stub runtime.
fn platform_c(calls: &[(&ra_asm_ir::Declaration, PlatformCall<'_>)]) -> (String, String) {
    let mut header = String::from(
        "// Generated by ras emit: the program's calls on the platform interface of rhipper.h.\n\
         #ifndef RA_PLATFORM_H\n#define RA_PLATFORM_H\n#include <stdint.h>\n",
    );
    let mut source = String::from("// Generated by ras emit: the program's calls on the platform interface, and the stub runtime.\n");
    source.push_str(PLATFORM_STUB_C);
    for (d, call) in calls {
        let ret = c_word(&d.ty.ret);
        let params: Vec<String> = d.ty.params.iter().enumerate().map(|(i, t)| format!("{} a{i}", c_word(t))).collect();
        let prototype = format!("{ret} {}({})", d.name, if params.is_empty() { "void".to_owned() } else { params.join(", ") });
        let _ = writeln!(header, "{prototype};");
        let body = match call {
            PlatformCall::Device(device) if device.write => {
                format!("ra_platform_bound->write(\"{}\", a0, {}, (uint64_t)a1);", device.device, device.bits)
            }
            PlatformCall::Device(device) => format!("return ({ret})ra_platform_bound->read(\"{}\", a0, {});", device.device, device.bits),
            PlatformCall::Kernel(name) => {
                let args: Vec<String> = (0..d.ty.params.len()).map(|i| format!("(uint64_t)a{i}")).collect();
                let array = if args.is_empty() { "0".to_owned() } else { format!("(const uint64_t[]){{{}}}", args.join(", ")) };
                let made = format!("ra_platform_bound->call(\"{name}\", {}, {array})", args.len());
                if ret == "void" { format!("{made};") } else { format!("return ({ret}){made};") }
            }
        };
        let _ = write!(source, "\n{prototype} {{\n    {body}\n}}\n");
    }
    header.push_str("#endif\n");
    (header, source)
}

/// The program's calls on the platform interface in Rust, each made on the runtime bound, for the
/// end of the runtime module (`runtime/rhipper.rs`, whose `Platform` trait they call).
fn platform_rust(calls: &[(&ra_asm_ir::Declaration, PlatformCall<'_>)]) -> String {
    let mut out = String::from("\n// The program's calls on the platform interface, written by ras emit.\n");
    for (d, call) in calls {
        let params: Vec<String> = d.ty.params.iter().enumerate().map(|(i, t)| format!("a{i}: {}", rust_word(t))).collect();
        let ret = if d.ty.ret == ra_asm_ir::Type::Void { String::new() } else { format!(" -> {}", rust_word(&d.ty.ret)) };
        let body = match call {
            PlatformCall::Device(device) if device.write => {
                format!("ra_platform().write(\"{}\", a0, {}, a1 as u64);", device.device, device.bits)
            }
            PlatformCall::Device(device) => {
                format!("ra_platform().read(\"{}\", a0, {}) as {}", device.device, device.bits, rust_word(&d.ty.ret))
            }
            PlatformCall::Kernel(name) => {
                let args: Vec<String> = (0..d.ty.params.len()).map(|i| format!("a{i} as u64")).collect();
                let made = format!("ra_platform().call(\"{name}\", &[{}])", args.join(", "));
                if ret.is_empty() { format!("{made};") } else { format!("{made} as {}", rust_word(&d.ty.ret)) }
            }
        };
        let _ = writeln!(out, "#[allow(non_snake_case)]\npub fn {}({}){ret} {{\n    {body}\n}}", d.name, params.join(", "));
    }
    out
}

/// The table's view of the functions of `analysis` and of the second images `with`: the
/// person's annotations on them, and an import a second image answers as a call of its function.
fn function_infos(
    image: &Image,
    analysis: &Analysis<'_>,
    lifted: &Lifted,
    annotations: &BTreeMap<u64, Annotation>,
    with: &[Companion<'_>],
) -> HashMap<String, FunctionInfo> {
    let mut infos = ra_asm_emit::functions::infos(analysis);
    for info in infos.values_mut() {
        if let Some(annotation) = annotations.get(&info.address) {
            info.annotated = true;
            info.ty.clone_from(&annotation.ty);
            info.comment.clone_from(&annotation.comment);
        }
        // An import a second image answers is a call of its lifted function, not an import.
        for bound in info.imports.iter().filter_map(|i| lifted.bound.get(i)).filter_map(|va| lifted.functions.get(va)) {
            if !info.calls.contains(bound) {
                info.calls.push(bound.clone());
            }
        }
        info.imports.retain(|i| !lifted.bound.contains_key(i));
    }
    if !with.is_empty() {
        for info in infos.values_mut() {
            info.image = Some(image_name(image));
        }
        for companion in with {
            let name = image_name(companion.image);
            infos.extend(ra_asm_emit::functions::infos(companion.analysis).into_iter().map(|(ir, mut info)| {
                info.image = Some(name.clone());
                (ir, info)
            }));
        }
    }
    infos
}

/// The checks of a checked translation, numbered in the order of the functions' addresses: the
/// jump tables whose index no compare bounds, each at its jump. What the raise assumes of a call
/// (the registers its callee keeps) and of a return (the stack pointer there) is not among them:
/// the raised module no longer holds those registers, so there is no value to compare until the
/// raise keeps them for a checked build.
fn checks(analyses: &[&Analysis<'_>], lifted: &Lifted) -> Vec<ra_asm_emit::Check> {
    let mut out = Vec::new();
    for f in analyses.iter().flat_map(|a| a.functions.values()) {
        let Some(function) = lifted.functions.get(&f.entry) else { continue };
        for assumption in ra_asm_lift::raise::assume::tables([f]) {
            if let (false, Some(site)) = (assumption.is_discharged(), assumption.site) {
                out.push(ra_asm_emit::Check {
                    id: out.len() as u32,
                    function: function.clone(),
                    kind: assumption.kind.name().to_owned(),
                    site,
                    watch: ra_asm_emit::Watch::Index,
                });
            }
        }
    }
    out
}

/// The IR names of the functions `hand` names, each by its source name, its IR name or its
/// address. An entry that names no function of the image is a warning: the port would still
/// build, with the lift's body where the person meant theirs.
fn hand_functions(infos: &HashMap<String, FunctionInfo>, hand: &[String], warnings: &mut Vec<String>) -> HashSet<String> {
    let mut out = HashSet::new();
    for entry in hand {
        let address = crate::project::hand_address(entry);
        let found = infos.iter().find(|(ir, i)| i.name == *entry || **ir == *entry || address.is_some_and(|a| a == i.address));
        match found {
            Some((ir, _)) => {
                out.insert(ir.clone());
            }
            None => warnings.push(format!("hand-written function `{entry}` is no function of the image: nothing is left to the sources")),
        }
    }
    out
}

/// What an emit keeps of the functions it wrote, in the build directory, for the next emit to
/// compare with: one entry per function, its `addr`, `name`, `identity` and `text`.
pub(crate) const EMITTED_JSON: &str = "emitted.json";
/// The comparison of an emit with the last, for the workbench: `functions` (how many this emit
/// wrote), the counts `moved`, `changed`, `new` and `gone`, and `entries`, one per function that
/// is not as it was: its `name`, `addr` and `change` (`moved` to another address, `changed` in its
/// text at the same one, `new`, `gone`), `was` and `was_name` for the last emit's address and
/// name, and `diff`, the unified diff of its text when the text changed.
pub(crate) const REGENERATED_JSON: &str = "regenerated.json";
/// The same comparison as one unified diff, function by function, to read.
pub(crate) const REGENERATED_DIFF: &str = "regenerated.diff";

/// The context lines around a change in a diff.
const DIFF_CONTEXT: usize = 3;
/// The most cells of the table a diff of two texts fills; two texts longer than that differ as
/// wholes.
const DIFF_CELLS: usize = 1 << 22;
/// The most identity lookups times functions a regeneration spends pairing the functions that
/// moved; past that the rest pair by an exact identity and by name only.
const PAIRING_BUDGET: usize = 1 << 24;

/// One function of a translation, as one emit compares it with another's: this emit's borrow
/// their names and texts from the translation, the last emit's own what `emitted.json` held.
struct Written<'t> {
    address: u64,
    name: std::borrow::Cow<'t, str>,
    identity: Option<Identity>,
    text: std::borrow::Cow<'t, str>,
}

/// A function as `emitted.json` keeps it.
#[derive(serde::Serialize, serde::Deserialize)]
struct Stored<'t> {
    addr: String,
    #[serde(borrow)]
    name: std::borrow::Cow<'t, str>,
    identity: Value,
    #[serde(borrow)]
    text: std::borrow::Cow<'t, str>,
}

impl Written<'_> {
    fn stored(&self) -> Stored<'_> {
        Stored {
            addr: format_address(self.address),
            name: std::borrow::Cow::Borrowed(&self.name),
            identity: self.identity.as_ref().map_or(Value::Null, crate::annotations::identity_json),
            text: std::borrow::Cow::Borrowed(&self.text),
        }
    }

    fn parse(stored: Stored<'_>) -> Option<Written<'static>> {
        Some(Written {
            address: crate::project::hand_address(&stored.addr)?,
            identity: crate::annotations::parse_identity(&stored.identity),
            name: std::borrow::Cow::Owned(stored.name.into_owned()),
            text: std::borrow::Cow::Owned(stored.text.into_owned()),
        })
    }
}

/// The functions `file`, an `emitted.json`, holds; `None` when it is missing or no such list.
/// Read one function at a time: the whole file as one JSON value took several times its size.
fn read_emitted(file: &Path) -> Option<Vec<Written<'static>>> {
    struct Functions(Vec<Written<'static>>);
    impl<'de> serde::Deserialize<'de> for Functions {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct Visit;
            impl<'de> serde::de::Visitor<'de> for Visit {
                type Value = Functions;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a list of functions")
                }
                fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Functions, A::Error> {
                    let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                    while let Some(stored) = seq.next_element::<Stored<'de>>()? {
                        out.extend(Written::parse(stored));
                    }
                    Ok(Functions(out))
                }
            }
            deserializer.deserialize_seq(Visit)
        }
    }
    let reader = std::io::BufReader::new(std::fs::File::open(file).ok()?);
    serde_json::from_reader::<_, Functions>(reader).ok().map(|f| f.0)
}

/// Compares `translation`, of the build whose functions have `identities`
/// ([`ra_asm_analysis::identity::identities`]), with the translation the last emit
/// wrote (its `emitted.json` in `dir`), and writes the comparison beside it: `regenerated.json`
/// and `regenerated.diff`. A function of the last emit is this emit's function of the same
/// identity ([`ra_asm_analysis::identity`]) or, failing that, of the same name when the name is
/// not the lift's own (a person's names follow their functions, [`crate::annotations::reattach`]);
/// one this emit's function two of the last pair with is neither's. Keeps this translation in
/// `emitted.json` for the next. Returns the line that sums the comparison up, `None` on the first
/// emit.
pub(crate) fn regenerate(dir: &Path, identities: &BTreeMap<u64, Identity>, translation: &Translation) -> Result<Option<String>> {
    let now: Vec<Written> = translation
        .functions
        .iter()
        .filter_map(|f| {
            let address = f.address?;
            let name = translation.infos.get(&f.ir_name).map_or(f.ir_name.as_str(), |i| i.name.as_str());
            Some(Written { address, name: name.into(), identity: identities.get(&address).cloned(), text: f.text.as_str().into() })
        })
        .collect();
    let file = dir.join(EMITTED_JSON);
    let last = read_emitted(&file);
    std::fs::create_dir_all(dir)?;
    let write_kept = || -> Result<()> {
        let mut out = std::io::BufWriter::new(std::fs::File::create(&file)?);
        serde_json::to_writer(&mut out, &now.iter().map(Written::stored).collect::<Vec<_>>())?;
        std::io::Write::flush(&mut out)?;
        Ok(())
    };
    write_kept().with_context(|| format!("cannot write {}", file.display()))?;
    let Some(last) = last else { return Ok(None) };

    let by_address: HashMap<u64, usize> = now.iter().enumerate().map(|(n, w)| (w.address, n)).collect();
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut by_identity: HashMap<&Identity, Vec<usize>> = HashMap::new();
    for (n, w) in now.iter().enumerate() {
        by_name.entry(w.name.as_ref()).or_default().push(n);
        if let Some(identity) = &w.identity {
            by_identity.entry(identity).or_default().push(n);
        }
    }
    let index = ra_asm_analysis::identity::Index::new(identities);
    let mut lookups = 0usize;
    let found: Vec<Option<usize>> = last
        .iter()
        .map(|old| {
            // The same build: the function at the old one's address with its identity is it, however
            // many others share that identity (a program's empty functions all do).
            if let Some(&n) = by_address.get(&old.address)
                && now[n].identity == old.identity
            {
                return Some(n);
            }
            let identity = old.identity.as_ref()?;
            if let Some([one]) = by_identity.get(identity).map(Vec::as_slice) {
                return Some(*one);
            }
            lookups += 1;
            if lookups.saturating_mul(now.len()) > PAIRING_BUDGET {
                return None;
            }
            index.find(identity).and_then(|a| by_address.get(&a).copied())
        })
        .zip(&last)
        .map(|(found, old)| {
            found.or_else(|| match by_name.get(old.name.as_ref()).map(Vec::as_slice) {
                Some([one]) if !old.name.starts_with("F_") => Some(*one),
                _ => None,
            })
        })
        .collect();
    let mut claims: HashMap<usize, usize> = HashMap::new();
    for n in found.iter().flatten() {
        *claims.entry(*n).or_default() += 1;
    }
    let found: Vec<Option<usize>> = found.into_iter().map(|f| f.filter(|n| claims[n] == 1)).collect();

    let (mut moved, mut changed, mut new, mut gone) = (0usize, 0usize, 0usize, 0usize);
    let mut entries = Vec::new();
    let mut diff = std::io::BufWriter::new(std::fs::File::create(dir.join(REGENERATED_DIFF))?);
    let mut paired = vec![false; now.len()];
    let mut entry = |change: &str, old: Option<&Written>, w: Option<&Written>, diff: &mut dyn std::io::Write| -> std::io::Result<()> {
        let (before, after) = (old.map_or("", |o| o.text.as_ref()), w.map_or("", |w| w.text.as_ref()));
        let hunks = if before == after { String::new() } else { unified_diff(before, after) };
        let at = |w: Option<&Written>| w.map_or_else(|| "nothing".to_owned(), |w| format!("{} at {}", w.name, format_address(w.address)));
        if !hunks.is_empty() {
            write!(diff, "--- {}\n+++ {}\n{hunks}", at(old), at(w))?;
        }
        let mut e = json!({"name": w.or(old).map(|w| w.name.as_ref()), "addr": w.map(|w| format_address(w.address)), "change": change});
        if let Some(old) = old {
            e["was"] = json!(format_address(old.address));
            if w.is_some_and(|w| w.name != old.name) {
                e["was_name"] = json!(old.name);
            }
        }
        if !hunks.is_empty() {
            e["diff"] = json!(hunks);
        }
        entries.push(e);
        Ok(())
    };
    for (old, found) in last.iter().zip(&found) {
        match found {
            Some(n) => {
                paired[*n] = true;
                let w = &now[*n];
                if w.address != old.address {
                    moved += 1;
                    entry("moved", Some(old), Some(w), &mut diff)?;
                } else if w.text != old.text {
                    changed += 1;
                    entry("changed", Some(old), Some(w), &mut diff)?;
                }
            }
            None => {
                gone += 1;
                entry("gone", Some(old), None, &mut diff)?;
            }
        }
    }
    for (w, _) in now.iter().zip(&paired).filter(|(_, p)| !**p) {
        new += 1;
        entry("new", None, Some(w), &mut diff)?;
    }
    std::io::Write::flush(&mut diff)?;
    drop(last);
    let summary = json!({
        "functions": now.len(), "moved": moved, "changed": changed, "new": new, "gone": gone, "entries": entries,
    });
    let mut out = std::io::BufWriter::new(std::fs::File::create(dir.join(REGENERATED_JSON))?);
    serde_json::to_writer_pretty(&mut out, &summary)?;
    std::io::Write::flush(&mut out)?;
    let line = if moved + changed + new + gone == 0 {
        format!("regenerated: the {} functions are as the last emit wrote them", now.len())
    } else {
        format!(
            "regenerated: {moved} functions moved, {changed} changed, {new} new and {gone} gone of {} -> {}",
            now.len(),
            dir.join(REGENERATED_DIFF).display()
        )
    };
    Ok(Some(line))
}

/// The unified diff of two texts, line by line: the hunks, without the file header.
fn unified_diff(old: &str, new: &str) -> String {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let ops = line_ops(&a, &b);
    let changes: Vec<usize> = ops.iter().enumerate().filter(|(_, (kind, _))| *kind != ' ').map(|(n, _)| n).collect();
    let mut out = String::new();
    let mut k = 0;
    while k < changes.len() {
        let first = changes[k];
        let mut last = first;
        while k + 1 < changes.len() && changes[k + 1] - last <= 2 * DIFF_CONTEXT + 1 {
            k += 1;
            last = changes[k];
        }
        k += 1;
        let (lo, hi) = (first.saturating_sub(DIFF_CONTEXT), (last + DIFF_CONTEXT + 1).min(ops.len()));
        let count = |ops: &[(char, &str)], skip: char| ops.iter().filter(|(kind, _)| *kind != skip).count();
        let (old_start, new_start) = (count(&ops[..lo], '+'), count(&ops[..lo], '-'));
        let (old_len, new_len) = (count(&ops[lo..hi], '+'), count(&ops[lo..hi], '-'));
        let _ =
            writeln!(out, "@@ -{},{old_len} +{},{new_len} @@", old_start + usize::from(old_len > 0), new_start + usize::from(new_len > 0));
        for (kind, line) in &ops[lo..hi] {
            let _ = writeln!(out, "{kind}{line}");
        }
    }
    out
}

/// The lines of `a` and `b` as a diff's operations: ` ` kept, `-` only in `a`, `+` only in `b`,
/// by the longest common subsequence of the lines between the common head and tail.
fn line_ops<'t>(a: &[&'t str], b: &[&'t str]) -> Vec<(char, &'t str)> {
    let head = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let tail = a[head..].iter().rev().zip(b[head..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (x, y) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
    let mut ops: Vec<(char, &str)> = a[..head].iter().map(|l| (' ', *l)).collect();
    let (n, m) = (x.len(), y.len());
    if n.saturating_mul(m) > DIFF_CELLS {
        ops.extend(x.iter().map(|l| ('-', *l)));
        ops.extend(y.iter().map(|l| ('+', *l)));
    } else {
        let at = |i: usize, j: usize| i * (m + 1) + j;
        let mut common = vec![0u32; (n + 1) * (m + 1)];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                common[at(i, j)] = if x[i] == y[j] { common[at(i + 1, j + 1)] + 1 } else { common[at(i + 1, j)].max(common[at(i, j + 1)]) };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && x[i] == y[j] {
                ops.push((' ', x[i]));
                i += 1;
                j += 1;
            } else if i < n && (j == m || common[at(i + 1, j)] >= common[at(i, j + 1)]) {
                ops.push(('-', x[i]));
                i += 1;
            } else {
                ops.push(('+', y[j]));
                j += 1;
            }
        }
    }
    ops.extend(a[a.len() - tail..].iter().map(|l| (' ', *l)));
    ops
}

/// The module definition file of a DLL's port: every export of the image under its name and
/// ordinal, a function as the lifted function at its address, a forwarder as it was, and a data
/// export as the global that starts at its address. An export at no symbol of the port (a
/// function the analysis did not find, data inside a section's global) is left out with a warning.
/// The module definition file of each DLL a Windows program imports by name, `imports/<dll>.def`,
/// from the import tables of `images` (the program's and its second images'): the build makes
/// the import library of each, so a DLL no SDK at hand has a library for (`d3dx9_43`,
/// `xinput1_3`) links as `kernel32` does. The C runtime's DLLs are left out: the host's C library
/// answers their functions, and a library made here would bind them to the program's old runtime
/// instead. An import `bound` to a second image's export is the port's own function and no
/// library's. An import by ordinal binds at run time and needs no library.
fn import_defs(images: &[&Image], bound: &HashMap<String, u64>) -> Vec<(String, Vec<u8>)> {
    let image = images[0];
    if image.elf.is_some() || image.macho.is_some() || image.xbe.is_some() {
        return Vec::new();
    }
    let mut by_dll: std::collections::BTreeMap<String, (String, std::collections::BTreeSet<&str>)> = std::collections::BTreeMap::new();
    for import in images.iter().flat_map(|i| &i.imports) {
        let Some(name) = import.name.as_deref() else { continue };
        if ra_asm_lift::raise::prototypes::is_crt_dll(&import.dll) || bound.contains_key(name) {
            continue;
        }
        let stem = import.dll.to_ascii_lowercase().trim_end_matches(".dll").to_owned();
        by_dll.entry(stem).or_insert_with(|| (import.dll.clone(), std::collections::BTreeSet::new())).1.insert(name);
    }
    by_dll
        .into_iter()
        .map(|(stem, (dll, names))| {
            let mut text = format!("LIBRARY {dll}\nEXPORTS\n");
            for name in names {
                text.push_str("    ");
                text.push_str(name);
                text.push('\n');
            }
            (format!("imports/{stem}.def"), text.into_bytes())
        })
        .collect()
}

fn exports_def(image: &Image, lifted: &Lifted, warnings: &mut Vec<String>) -> String {
    let mut out = String::from("; The exports of the port, as the image had them. Generated by ras emit.\nEXPORTS\n");
    let Some(table) = &image.exports else { return out };
    for export in &table.exports {
        let name = export.name.clone().unwrap_or_else(|| format!("ordinal_{}", export.ordinal));
        let noname = if export.name.is_none() { " NONAME" } else { "" };
        if let Some(target) = &export.forwarder {
            let _ = writeln!(out, "    {name} = {target} @{}{noname}", export.ordinal);
            continue;
        }
        let Some(va) = export.va else { continue };
        if let Some(function) = lifted.functions.get(&va) {
            let internal = if *function == name { String::new() } else { format!(" = {function}") };
            let _ = writeln!(out, "    {name}{internal} @{}{noname}", export.ordinal);
        } else if let Some(global) = lifted.module.globals.iter().find(|g| g.address == Some(va)) {
            let _ = writeln!(out, "    {name} = {} @{}{noname} DATA", global.name, export.ordinal);
        } else {
            let place = lifted
                .data
                .iter()
                .find(|s| va >= s.start && va < s.end)
                .map_or_else(String::new, |s| format!(" ({} bytes into {})", va - s.start, s.global));
            warnings
                .push(format!("export {name} (#{}) at 0x{va:x}{place} is no function or global of the port: not exported", export.ordinal));
        }
    }
    out
}

/// The image's resources as files: `resources.res`, which the port's link folds into its own
/// resource directory, and under `resources/` the string table decoded (`strings.txt`, one
/// `id<tab>text` line each) and every other resource as its bytes. Empty without resources.
fn resource_files(image: &Image) -> Vec<(String, Vec<u8>)> {
    if image.resources.is_empty() {
        return Vec::new();
    }
    let mut files = vec![("resources.res".to_owned(), ra_asm_pe::resources::res_file(image))];
    let strings = ra_asm_pe::resources::string_table(image);
    if !strings.is_empty() {
        let mut text = String::new();
        for (id, s) in &strings {
            text.push_str(&id.to_string());
            text.push('\t');
            text.push_str(&s.replace('\\', "\\\\").replace('\t', "\\t").replace('\r', "\\r").replace('\n', "\\n"));
            text.push('\n');
        }
        files.push(("resources/strings.txt".to_owned(), text.into_bytes()));
    }
    for r in &image.resources {
        if r.kind == ra_asm_pe::ResourceId::Id(6) {
            continue; // the string table, decoded above
        }
        if let Ok(bytes) = r.data(image) {
            files.push((format!("resources/{}", r.file_name(&image.resources)), bytes.to_vec()));
        }
    }
    files
}

/// Keeps of `analysis` the functions at `only` and every function they reach by a call or a tail
/// call, so a slice of a large program translates and builds by itself; nothing when `only` is
/// empty. A function outside the slice that the slice names by address stays an address.
pub(crate) fn keep_reached(analysis: &mut ra_asm_analysis::Analysis<'_>, only: &[String]) -> Result<()> {
    if only.is_empty() {
        return Ok(());
    }
    let mut todo = Vec::new();
    for entry in only {
        let va = crate::project::hand_address(entry).with_context(|| format!("`--only {entry}` is no address"))?;
        anyhow::ensure!(analysis.functions.contains_key(&va), "`--only {entry}`: no function starts at {va:#x}");
        todo.push(va);
    }
    let mut kept = std::collections::BTreeSet::new();
    while let Some(va) = todo.pop() {
        let Some(f) = analysis.functions.get(&va) else { continue };
        if kept.insert(va) {
            todo.extend(f.calls.iter().chain(&f.tail_calls).copied());
        }
    }
    analysis.functions.retain(|va, _| kept.contains(va));
    Ok(())
}

/// The file name of `image`, as the function table names it.
fn image_name(image: &Image) -> String {
    image.path.as_ref().and_then(|p| p.file_name()).map_or_else(|| "image".to_owned(), |n| n.to_string_lossy().into_owned())
}

/// Loads the second images at `paths`, each a PE of the program's machine and address width whose
/// range is apart from the program's and the others'.
pub(crate) fn load_companions(image: &Image, paths: &[std::path::PathBuf]) -> Result<Vec<Image>> {
    let mut images: Vec<Image> = Vec::with_capacity(paths.len());
    for path in paths {
        let with = ra_asm_pe::load(path)?;
        let name = image_name(&with);
        if with.elf.is_some() || with.macho.is_some() || with.xbe.is_some() {
            bail!("{name}: a second image is a Windows PE");
        }
        if with.machine() != image.machine() || with.is_64() != image.is_64() {
            bail!("{name}: a second image is of the program's machine and address width");
        }
        let range = |i: &Image| i.image_base()..i.image_base() + u64::from(i.size_of_image());
        let (lo, hi) = (range(&with).start, range(&with).end);
        if std::iter::once(image).chain(&images).any(|other| lo < range(other).end && range(other).start < hi) {
            bail!("{name}: a second image is linked at {lo:#x}, where another image of the port already is");
        }
        images.push(with);
    }
    Ok(images)
}

/// The analyses of the second images `with`, each named apart from the program's functions and
/// the other images': a name two images share (`entry`, a thunk's) takes its image's stem, so the
/// one module defines each once.
pub(crate) fn analyze_companions<'a>(analysis: &Analysis<'_>, with: &'a [Image]) -> Vec<Analysis<'a>> {
    ra_asm_lift::analyze_companions(analysis, with)
}

/// The second images with their analyses, as the lift and the translation take them.
pub(crate) fn companions_of<'a>(images: &'a [Image], analyses: &'a [Analysis<'a>]) -> Vec<Companion<'a>> {
    images.iter().zip(analyses).map(|(image, analysis)| Companion { image, analysis }).collect()
}

/// [`timed`], and when the times print, the peak working set of the process as the stage ends on
/// stderr, `ra: peak: <name>: <megabytes> MB`. The peak only grows: the first stage whose line
/// jumps is the one that took the memory.
fn staged<T>(name: &str, run: impl FnOnce() -> T) -> T {
    let out = timed(name, run);
    if ra_asm_lift::raise::times()
        && let Some(peak) = crate::pipeline::peak_working_set()
    {
        eprintln!("ra: peak: {name}: {} MB", peak >> 20);
    }
    out
}

/// Writes the translation, the runtime and `functions.json` into `out`, or the translation to
/// stdout without `out`. `hand` names the functions left to hand-written sources; the annotations
/// of the project at `project` rename functions and leave more to the sources. `checked` writes
/// the open assumptions as checks. `only`, when it names functions, translates them and what they
/// call alone. `with` names the second images ported with the program (`--with`). `times` prints
/// the seconds of each stage on stderr (see [`timed`]), and the peak memory after each large one
/// (see [`staged`]).
// The arguments are the command's own flags, one each.
#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub(crate) fn emit(
    image: &Image,
    language: &str,
    out: Option<&Path>,
    quiet: bool,
    x87: ra_asm_lift::X87Unit,
    hand: &[String],
    project: Option<&Path>,
    checked: bool,
    only: &[String],
    with: &[std::path::PathBuf],
    times: bool,
) -> Result<()> {
    if times {
        ra_asm_lift::raise::print_times(true);
    }
    let mut analysis = staged("analysis", || ra_asm_analysis::analyze(image));
    keep_reached(&mut analysis, only)?;
    let (annotations, warnings) = project.map(|root| crate::annotations::annotate(root, &mut analysis)).unwrap_or_default();
    let hand = crate::annotations::hand_list(hand, &annotations);
    let hand = hand.as_slice();
    let with_images = load_companions(image, with)?;
    // The static part of a ported DLL's import library takes the DLL's own objects: it lifts with
    // the program rather than coming from the host's runtime.
    for with_image in &with_images {
        ra_asm_analysis::symbols::port_import_library(image, &mut analysis.functions, with_image);
    }
    let mut with_analyses = timed("analysis: second images", || analyze_companions(&analysis, &with_images));
    let companions = companions_of(&with_images, &with_analyses);
    let mut lifted =
        staged("lift", || ra_asm_lift::lift_frozen_with(image, &analysis, &companions, ra_asm_lift::LiftOptions { standalone: true, x87 }));
    drop(companions);
    // Past the lift the translation reads the IR: the decoded instructions go, unless a checked
    // translation's assumptions still read them.
    if !checked {
        analysis.release_instructions();
        for with in &mut with_analyses {
            with.release_instructions();
        }
    }
    // The raised functions stay frozen: the translation thaws them a run at a time, so the raised
    // module is never whole in memory.
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    let mut signatures = staged("raise", || ra_asm_lift::raise::raise_frozen(&mut lifted.module, &lifted.roots, options));
    timed("apply_names", || {
        lifted.apply_names(&analysis, Some(&mut signatures));
        for with in &with_analyses {
            lifted.apply_names(with, Some(&mut signatures));
        }
    });
    let companions = companions_of(&with_images, &with_analyses);
    let package = image.path.as_ref().and_then(|p| p.file_stem()).map_or_else(|| "port".to_owned(), |s| s.to_string_lossy().into_owned());
    let options = Options { lang: language, package: &package, trace: false, hand, annotations: &annotations, checked, with: &companions };
    let mut translation = staged("translate", || translate(image, &analysis, &mut lifted, &signatures, &options))?;
    // What the rest reads of the raised module and its signatures, the header, taken now: they go
    // before the analysis is read again, and the analysis before the formatters start, which on a
    // large program need the memory the three held.
    let header = staged("header", || out.map(|_| ra_asm_lift::image_header(image, &analysis, &lifted, Some(&signatures))));
    let mut warnings: Vec<String> = analysis.warnings.iter().chain(&warnings).chain(&lifted.warnings).cloned().collect();
    warnings.extend(with_analyses.iter().flat_map(|a| a.warnings.iter().cloned()));
    drop(companions);
    drop((lifted, signatures));
    if let Some(dir) = out {
        timed("note_evidence", || crate::functions::note_evidence(&mut translation.infos, dir))?;
    }
    let identities = staged("identities", || {
        out.map(|_| {
            let mut identities = ra_asm_analysis::identity::identities(&analysis);
            for with in &with_analyses {
                identities.extend(ra_asm_analysis::identity::identities(with));
            }
            identities
        })
    });
    drop((analysis, with_analyses));
    match out {
        None => {
            staged("format", || translation.format());
            staged("write", || print!("{}", translation.text()));
        }
        Some(dir) => {
            // The sources are written as the formatters finish them, and their texts go: the
            // format's time holds their writing, the write's the rest.
            let bytes = staged("format", || translation.format_and_write(dir))?;
            staged("write", || -> Result<()> {
                std::fs::write(dir.join("image.h"), header.unwrap_or_default())?;
                std::fs::write(dir.join("functions.json"), translation.table())?;
                Ok(())
            })?;
            let regenerated = staged("regenerate", || regenerate(dir, &identities.unwrap_or_default(), &translation))?;
            if !quiet {
                if let Some(line) = regenerated {
                    eprintln!("{line}");
                }
                eprintln!(
                    "{} functions, {} bytes of {language} in {} files -> {}",
                    translation.functions.len(),
                    bytes,
                    translation.files.len(),
                    dir.display()
                );
            }
        }
    }
    if !quiet {
        warnings.extend(translation.warnings.iter().cloned());
        for w in &warnings {
            eprintln!("warning: {w}");
        }
    }
    Ok(())
}
