//! `ras verify --built`: the harness of a port, written beside it as `verify.c` with the runtime
//! it includes as `runtime/verify.c` (see the runtime for what a loader and a mock do).
//!
//! The harness holds a loader for each vector the run recorded (or the vector file holds) and a
//! mock for each callee the vectors call. A loader calls the function by the name the port's
//! `functions.json` gives it, with the arguments its signature at the ABI level reads, in the
//! Windows convention of the image's width, from the entry registers and the argument block
//! above the entry stack pointer. A loader puts back and compares only the bytes of the image: a
//! byte of the activation's stack (its frame and its callees' below the entry stack pointer, its
//! argument block above) is the port's own stack, and a vector that reads or writes any other
//! memory cannot be taken by the port as it was recorded. Such a vector, one that did not return,
//! and one whose function the port has no source of keep a loader that says why and runs nothing.
//!
//! The harness is written, not built or run: building it beside the port, with each mock in its
//! callee's place, is not this command's.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::ops::Range;
use std::path::Path;

use anyhow::{Context, Result, bail};
use ra_asm_lift::raise::abi::{Kind, Signature};
use ra_asm_lift::{Isa, RaiseLevel};
use ra_asm_pe::Image;
use ra_asm_verify::{Call, Left, Register, Run, Vector};
use serde_json::Value;

use super::Options;

/// The runtime the harness includes, written under the port's `runtime/`.
const RUNTIME: &str = include_str!("../../../../runtime/verify.c");

/// How far below the entry stack pointer the frames of an activation and its callees reach: the
/// stack a Windows thread reserves.
const FRAME_REACH: u64 = 1 << 20;

/// Writes the harness of the port in `dir` for the vectors `options` name, and its runtime.
pub(super) fn write(image: &Image, options: &Options<'_>, dir: &Path) -> Result<()> {
    let ported = ported(dir)?;
    let analysis = ra_asm_analysis::analyze(image);
    let machine = super::lifted(image, &analysis, RaiseLevel::Machine);
    if !matches!(machine.isa, Isa::X86) || machine.sysv {
        bail!("the harness hands the arguments in the Windows conventions of x86 and x64 alone");
    }
    let convention = Convention { bits: machine.bits };
    let vectors = super::gathered(image, &machine, options)?;
    drop(machine);
    let chosen = super::chosen(options.function);
    let vectors: Vec<&Vector> = vectors.iter().filter(|vector| chosen(vector)).collect();
    if vectors.is_empty() {
        match options.function {
            Some(text) => bail!("{text} has no vector: it is no function the run entered, by name or by address"),
            None => bail!("there is no vector to write a harness for"),
        }
    }
    let (_, signatures) = super::signed(image, &analysis);
    drop(analysis);
    let base = image.image_base();
    let harness = Harness { convention, signatures: &signatures, ported: &ported, image: base..base + u64::from(image.size_of_image()) };
    let program = options.program.file_name().map_or_else(|| "the program".to_owned(), |name| name.to_string_lossy().into_owned());
    let (text, mocks) = harness.text(&program, &vectors);

    let runtime = dir.join("runtime");
    std::fs::create_dir_all(&runtime).with_context(|| format!("creating {}", runtime.display()))?;
    let path = dir.join("verify.c");
    ra_asm_emit::write_if_changed(&path, text.as_bytes()).with_context(|| format!("writing {}", path.display()))?;
    let path = runtime.join("verify.c");
    ra_asm_emit::write_if_changed(&path, RUNTIME.as_bytes()).with_context(|| format!("writing {}", path.display()))?;
    let functions: BTreeSet<u64> = vectors.iter().map(|vector| vector.address).collect();
    eprintln!(
        "a loader for {} vectors of {} functions, {mocks} mocks -> {}",
        vectors.len(),
        functions.len(),
        dir.join("verify.c").display()
    );
    Ok(())
}

/// The functions of the port in `dir` that have source, by address, under the names the port
/// gives them: what its `functions.json` lists with a file.
fn ported(dir: &Path) -> Result<HashMap<u64, String>> {
    let path = dir.join("functions.json");
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("{} holds no port: `ras emit --out` writes one there", dir.display()))?;
    let table: Vec<Value> = serde_json::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
    Ok(table
        .iter()
        .filter(|entry| entry.get("file").is_some())
        .filter_map(|entry| {
            let address = entry.get("addr")?.as_str()?.strip_prefix("0x")?;
            Some((u64::from_str_radix(address, 16).ok()?, entry.get("name")?.as_str()?.to_owned()))
        })
        .collect())
}

/// The Windows calling convention of an x86 image of `bits` address bits: Win64's, or on 32 bits
/// the stack with `ecx` and `edx` for the register parameters a signature has.
#[derive(Clone, Copy)]
struct Convention {
    bits: u32,
}

impl Convention {
    fn stack_pointer(self) -> &'static str {
        if self.bits == 64 { "rsp" } else { "esp" }
    }

    /// The bytes of a stack slot.
    fn slot(self) -> u64 {
        if self.bits == 64 { 8 } else { 4 }
    }

    /// Where the first stack argument is above the entry stack pointer: past the return address,
    /// and on Win64 past the four slots the caller keeps for the register parameters.
    fn stack_start(self) -> u64 {
        if self.bits == 64 { 40 } else { 4 }
    }

    /// The bytes a stack parameter of `kind` takes.
    fn width(self, kind: Kind) -> u64 {
        match kind {
            Kind::Wide | Kind::Double => 8,
            Kind::Int | Kind::Float => self.slot(),
        }
    }

    /// The register of the register parameter at `position`.
    fn register(self, position: usize, kind: Kind) -> Option<String> {
        match (self.bits, kind) {
            (64, Kind::Int | Kind::Wide) => ["rcx", "rdx", "r8", "r9"].get(position).map(|name| (*name).to_owned()),
            (64, Kind::Float | Kind::Double) => (position < 4).then(|| format!("xmm{position}")),
            (_, Kind::Int) => ["ecx", "edx"].get(position).map(|name| (*name).to_owned()),
            _ => None,
        }
    }

    /// The bits of a result of `kind` among `registers`; none when the convention returns it
    /// where the registers do not hold it, as a 32-bit float on the x87 stack.
    fn result(self, registers: &[Register], kind: Kind) -> Option<u64> {
        let register = |name: &str| registers.iter().find(|(n, _)| n == name).map(|(_, value)| low(*value));
        match (self.bits, kind) {
            (64, Kind::Int | Kind::Wide) => register("rax"),
            (64, Kind::Float) => register("xmm0").map(|value| value & 0xffff_ffff),
            (64, Kind::Double) => register("xmm0"),
            (_, Kind::Int) => register("eax").map(|value| value & 0xffff_ffff),
            (_, Kind::Wide) => Some(((register("edx")? & 0xffff_ffff) << 32) | (register("eax")? & 0xffff_ffff)),
            _ => None,
        }
    }

    /// The bits of a result of `kind` the port's function returns that are its value.
    fn mask(self, kind: Kind) -> u64 {
        match kind {
            Kind::Float => 0xffff_ffff,
            Kind::Int if self.bits == 32 => 0xffff_ffff,
            _ => u64::MAX,
        }
    }
}

/// The low 64 bits of a register's value.
fn low(value: u128) -> u64 {
    u64::try_from(value & u128::from(u64::MAX)).unwrap_or_default()
}

/// What the harness is written from.
struct Harness<'h> {
    convention: Convention,
    signatures: &'h HashMap<String, Signature>,
    ported: &'h HashMap<u64, String>,
    image: Range<u64>,
}

/// The mock of one callee: its name in the harness and the types it takes and returns, from the
/// callee's signature or, without one, from the arguments the record holds.
struct Mock {
    ident: String,
    params: Vec<Kind>,
    returns: Option<Kind>,
}

/// A vector the port can take: the function's name in the port, what it takes and returns, the
/// arguments as C, and the bytes of the image it read and wrote.
struct Load<'v> {
    name: String,
    params: Vec<Kind>,
    returns: Option<Kind>,
    args: Vec<String>,
    input: Vec<&'v Run>,
    output: Vec<&'v Run>,
}

impl Harness<'_> {
    /// The text of the harness of `vectors`, and the number of its mocks.
    fn text(&self, program: &str, vectors: &[&Vector]) -> (String, usize) {
        let mocks = self.mocks(vectors);
        let mut out = String::new();
        let _ = writeln!(out, "// Generated by ras verify --built: the harness of the port of {program}, a loader for each of the");
        let _ = writeln!(out, "// {} vectors its run recorded and a mock for each callee they call (see runtime/verify.c).", vectors.len());
        out.push('\n');
        if self.convention.bits == 32 {
            out.push_str("#define RA_VERIFY_WORD UINT64_C(0xffffffff)\n");
        }
        out.push_str("#include \"runtime/verify.c\"\n\n");

        let loads: Vec<Result<Load<'_>, String>> = vectors.iter().map(|vector| self.load(vector)).collect();
        let declared: BTreeSet<&str> = loads.iter().filter_map(|load| load.as_ref().ok()).map(|load| load.name.as_str()).collect();
        if !declared.is_empty() {
            out.push_str("// The functions of the port the vectors are of, each called through the type of its signature.\n");
            for name in &declared {
                let _ = writeln!(out, "extern void {name}(void);");
            }
            out.push('\n');
        }

        let total: HashMap<u64, usize> = vectors.iter().fold(HashMap::new(), |mut total, vector| {
            *total.entry(vector.address).or_default() += 1;
            total
        });
        let mut ordinals: HashMap<u64, usize> = HashMap::new();
        for (index, (vector, load)) in vectors.iter().zip(&loads).enumerate() {
            let ordinal = ordinals.entry(vector.address).or_default();
            *ordinal += 1;
            let _ = writeln!(out, "// {}, vector {} of {} at {:#x}", vector.function, ordinal, total[&vector.address], vector.address);
            match load {
                Ok(load) => self.loader(&mut out, index + 1, *ordinal, vector, load, &mocks),
                Err(why) => {
                    let _ = writeln!(
                        out,
                        "static int ra_verify_load_{}(void) {{ return ra_verify_skip({ordinal}, {}, {}); }}\n",
                        index + 1,
                        c_string(&vector.function),
                        c_string(why)
                    );
                }
            }
        }

        if !mocks.is_empty() {
            out.push_str("// The mocks, one for each callee, which answer from the record.\n");
            for (callee, mock) in &mocks {
                mock_definition(&mut out, callee, mock);
            }
        }
        out.push_str("// Each callee and the mock that answers in its place.\n");
        out.push_str("const struct ra_verify_mock ra_verify_mocks[] = {\n");
        for (callee, mock) in &mocks {
            let _ = writeln!(out, "    {{{}, (void (*)(void)){}}},", c_string(callee), mock.ident);
        }
        out.push_str("    {0, 0},\n};\n\n");

        out.push_str("static const struct ra_verify_vector ra_verify_vectors[] = {\n");
        for (index, vector) in vectors.iter().enumerate() {
            let _ = writeln!(out, "    {{{}, ra_verify_load_{}}},", c_string(&vector.function), index + 1);
        }
        out.push_str("};\n\n");
        out.push_str("// Runs every loader; the vectors that differ.\n");
        out.push_str("int ra_verify(void) {\n");
        out.push_str("    return ra_verify_all(ra_verify_vectors, sizeof ra_verify_vectors / sizeof ra_verify_vectors[0]);\n}\n");
        (out, mocks.len())
    }

    /// The mock of each callee the vectors call, by the callee's name.
    fn mocks(&self, vectors: &[&Vector]) -> BTreeMap<String, Mock> {
        let mut arities: BTreeMap<&str, usize> = BTreeMap::new();
        for call in vectors.iter().flat_map(|vector| &vector.calls) {
            let arity = arities.entry(call.callee.as_str()).or_default();
            *arity = (*arity).max(call.args.len());
        }
        let mut taken: BTreeSet<String> = BTreeSet::new();
        arities
            .into_iter()
            .map(|(callee, arity)| {
                let base = format!("ra_mock_{}", ident(callee));
                let mut name = base.clone();
                let mut n = 1;
                while !taken.insert(name.clone()) {
                    n += 1;
                    name = format!("{base}_{n}");
                }
                let (params, returns) = match self.signatures.get(callee) {
                    Some(signature) => (parameters(signature), signature.returns),
                    None => (vec![Kind::Int; arity], Some(Kind::Int)),
                };
                (callee.to_owned(), Mock { ident: name, params, returns })
            })
            .collect()
    }

    /// What a loader of `vector` calls and puts back, or why the port cannot take the vector.
    fn load<'v>(&self, vector: &'v Vector) -> Result<Load<'v>, String> {
        match &vector.left {
            Left::Return => {}
            Left::Exit(code) => return Err(format!("the activation left by the exit of the process, with {code}")),
            Left::Trap { address, reason } => return Err(format!("the activation stopped at a trap at {address:#x}: {reason}")),
            Left::Exception { code, address } => return Err(format!("the activation raised the exception {code:#x} at {address:#x}")),
        }
        let name = self.ported.get(&vector.address).ok_or("the port has no source of the function")?;
        if ident(name) != *name {
            return Err(format!("the port names the function {name}, which is no C identifier"));
        }
        let signature = self.signatures.get(&vector.function).ok_or("the function has no signature at the ABI level")?;
        if signature.variadic {
            return Err("the function takes the address of its caller's argument block, which is the record's stack".to_owned());
        }
        let convention = self.convention;
        let sp = vector
            .entry
            .iter()
            .find(|(register, _)| register == convention.stack_pointer())
            .map(|(_, value)| low(*value))
            .ok_or_else(|| format!("the entry holds no {}", convention.stack_pointer()))?;
        let read: BTreeMap<u64, u8> = vector.memory_in.iter().flat_map(|(start, bytes)| (*start..).zip(bytes.iter().copied())).collect();

        let mut args = Vec::new();
        for (position, kind) in signature.params.iter().enumerate() {
            let register = convention.register(position, *kind).ok_or_else(|| format!("parameter {} has no register", position + 1))?;
            let value = vector
                .entry
                .iter()
                .find(|(slot, _)| *slot == register)
                .map(|(_, value)| low(*value))
                .ok_or_else(|| format!("the entry holds no {register}"))?;
            args.push(argument(*kind, value, convention));
        }
        let mut at = sp + convention.stack_start();
        for kind in &signature.stack_params {
            let width = convention.width(*kind);
            // A byte of the argument the function did not read is no part of what it did.
            let value = (0..width).rev().fold(0u64, |value, n| (value << 8) | u64::from(read.get(&(at + n)).copied().unwrap_or(0)));
            args.push(argument(*kind, value, convention));
            at += width;
        }
        let stack = sp.saturating_sub(FRAME_REACH)..at;

        let input = self.image_runs(&vector.memory_in, &stack).map_err(|run| format!("it reads memory outside the image, at {run:#x}"))?;
        let output =
            self.image_runs(&vector.memory_out, &stack).map_err(|run| format!("it writes memory outside the image, at {run:#x}"))?;
        for (n, call) in vector.calls.iter().enumerate() {
            self.image_runs(&call.effects, &stack)
                .map_err(|run| format!("call {} of {} writes memory outside the image, at {run:#x}", n + 1, call.callee))?;
        }
        let mut params = signature.params.clone();
        params.extend(&signature.stack_params);
        Ok(Load { name: name.clone(), params, returns: signature.returns, args, input, output })
    }

    /// The runs of `runs` in the image, past those of the activation's `stack`; the address of
    /// the first run that is in neither.
    fn image_runs<'v>(&self, runs: &'v [Run], stack: &Range<u64>) -> Result<Vec<&'v Run>, u64> {
        let mut kept = Vec::new();
        for run in runs {
            if within(&self.image, run) {
                kept.push(run);
            } else if !within(stack, run) {
                return Err(run.0);
            }
        }
        Ok(kept)
    }

    /// The loader of the vector at `index` (from one), the `ordinal`th of its function.
    fn loader(&self, out: &mut String, index: usize, ordinal: usize, vector: &Vector, load: &Load<'_>, mocks: &BTreeMap<String, Mock>) {
        let prefix = format!("ra_v{index}");
        let input = runs(out, &format!("{prefix}_in"), &load.input);
        let output = runs(out, &format!("{prefix}_out"), &load.output);
        let calls = if vector.calls.is_empty() {
            "NULL, 0".to_owned()
        } else {
            let mut table = String::new();
            for (n, call) in vector.calls.iter().enumerate() {
                let returns = mocks.get(&call.callee).and_then(|mock| mock.returns);
                table.push_str(&self.call(out, &format!("{prefix}_c{}", n + 1), call, returns));
            }
            let _ = writeln!(out, "static const struct ra_verify_call {prefix}_calls[] = {{\n{table}}};");
            format!("{prefix}_calls, {}", vector.calls.len())
        };
        let _ = writeln!(out, "static int ra_verify_load_{index}(void) {{");
        let _ = writeln!(out, "    RA_VERIFY_BEGIN({ordinal}, {}, {input}, {calls});", c_string(&vector.function));
        let params = if load.params.is_empty() {
            "void".to_owned()
        } else {
            load.params.iter().map(|kind| c_type(*kind)).collect::<Vec<_>>().join(", ")
        };
        let ret = load.returns.map_or("void", c_type);
        let call = format!("(({ret} (*)({params}))(void (*)(void)){})({})", load.name, load.args.join(", "));
        match load.returns {
            None => {
                let _ = writeln!(out, "    {call};");
                let _ = writeln!(out, "    return ra_verify_end({output});");
            }
            Some(kind) => {
                let _ = writeln!(out, "    uint64_t result = {};", bits(kind, &call));
                if let Some(expected) = self.convention.result(&vector.exit, kind) {
                    let mask = self.convention.mask(kind);
                    let _ = writeln!(out, "    return ra_verify_result(result & {}, {}, {output});", hex(mask), hex(expected & mask));
                } else {
                    let _ = writeln!(out, "    (void)result;");
                    let _ = writeln!(out, "    return ra_verify_unknown({output});");
                }
            }
        }
        out.push_str("}\n\n");
    }

    /// Writes the arguments and the effects of `call` under `prefix`; its line of the vector's
    /// table of calls. The answer is the `return` the record names, or the result register of the
    /// convention for what the mock returns.
    fn call(&self, out: &mut String, prefix: &str, call: &Call, returns: Option<Kind>) -> String {
        let args = if call.args.is_empty() {
            "0, NULL".to_owned()
        } else {
            let values: Vec<String> = call.args.iter().map(|value| hex(*value)).collect();
            let _ = writeln!(out, "static const uint64_t {prefix}_args[] = {{{}}};", values.join(", "));
            format!("{}, {prefix}_args", call.args.len())
        };
        let recorded = call.results.iter().find(|(name, _)| name == ra_asm_verify::record::RETURN).map(|(_, value)| low(*value));
        let answer = match returns {
            None => Some(0),
            Some(kind) => recorded.or_else(|| self.convention.result(&call.results, kind)),
        };
        let effects: Vec<&Run> = call.effects.iter().filter(|run| within(&self.image, run)).collect();
        let effects = runs(out, &format!("{prefix}_effects"), &effects);
        let (answered, result) = answer.map_or((0, 0), |value| (1, value));
        format!("    {{{}, {args}, {answered}, {}, {effects}}},\n", c_string(&call.callee), hex(result))
    }
}

/// Whether every byte of `run` is in `range`.
fn within(range: &Range<u64>, (start, bytes): &Run) -> bool {
    let end = u64::try_from(bytes.len()).ok().and_then(|len| start.checked_add(len));
    range.start <= *start && end.is_some_and(|end| end <= range.end)
}

/// The parameters a call hands a function of `signature`, the register ones first.
fn parameters(signature: &Signature) -> Vec<Kind> {
    let mut params = signature.params.clone();
    if signature.variadic {
        params.push(Kind::Int);
    } else {
        params.extend(&signature.stack_params);
    }
    params
}

/// Writes the definition of the mock of `callee`.
fn mock_definition(out: &mut String, callee: &str, mock: &Mock) {
    let params = if mock.params.is_empty() {
        "void".to_owned()
    } else {
        mock.params.iter().enumerate().map(|(n, kind)| format!("{} a{n}", c_type(*kind))).collect::<Vec<_>>().join(", ")
    };
    let ret = mock.returns.map_or("void", c_type);
    let _ = writeln!(out, "static {ret} {}({params}) {{", mock.ident);
    let args = if mock.params.is_empty() {
        "0, NULL".to_owned()
    } else {
        let values: Vec<String> = mock.params.iter().enumerate().map(|(n, kind)| bits(*kind, &format!("a{n}"))).collect();
        let _ = writeln!(out, "    const uint64_t args[] = {{{}}};", values.join(", "));
        format!("{}, args", mock.params.len())
    };
    let answer = format!("ra_verify_answer({}, {args})", c_string(callee));
    let _ = match mock.returns {
        None => writeln!(out, "    {answer};"),
        Some(Kind::Float) => writeln!(out, "    return ra_verify_f32({answer});"),
        Some(Kind::Double) => writeln!(out, "    return ra_verify_f64({answer});"),
        Some(Kind::Int | Kind::Wide) => writeln!(out, "    return {answer};"),
    };
    out.push_str("}\n\n");
}

/// Writes the bytes of `runs` as arrays under `name`; the pointer and the count a loader hands
/// the runtime.
fn runs(out: &mut String, name: &str, runs: &[&Run]) -> String {
    if runs.is_empty() {
        return "NULL, 0".to_owned();
    }
    for (n, (_, bytes)) in runs.iter().enumerate() {
        let _ = write!(out, "static const uint8_t {name}_{}[] = {{", n + 1);
        for (i, byte) in bytes.iter().enumerate() {
            let separator = if i == 0 {
                ""
            } else if i % 16 == 0 {
                ",\n    "
            } else {
                ", "
            };
            let _ = write!(out, "{separator}0x{byte:02x}");
        }
        out.push_str("};\n");
    }
    let _ = writeln!(out, "static const struct ra_verify_run {name}[] = {{");
    for (n, (start, bytes)) in runs.iter().enumerate() {
        let _ = writeln!(out, "    {{{}, {name}_{}, {}}},", hex(*start), n + 1, bytes.len());
    }
    out.push_str("};\n");
    format!("{name}, {}", runs.len())
}

/// An argument of `kind` whose bits are `value`, as C.
fn argument(kind: Kind, value: u64, convention: Convention) -> String {
    match kind {
        Kind::Int if convention.bits == 32 => hex(value & 0xffff_ffff),
        Kind::Int | Kind::Wide => hex(value),
        Kind::Float => format!("ra_verify_f32({})", hex(value & 0xffff_ffff)),
        Kind::Double => format!("ra_verify_f64({})", hex(value)),
    }
}

/// The bits of `value`, a C expression of `kind`, as a `uint64_t`.
fn bits(kind: Kind, value: &str) -> String {
    match kind {
        Kind::Int | Kind::Wide => format!("(uint64_t)({value})"),
        Kind::Float => format!("ra_verify_bits_f32({value})"),
        Kind::Double => format!("ra_verify_bits_f64({value})"),
    }
}

fn c_type(kind: Kind) -> &'static str {
    match kind {
        Kind::Int | Kind::Wide => "uint64_t",
        Kind::Float => "float",
        Kind::Double => "double",
    }
}

fn hex(value: u64) -> String {
    format!("UINT64_C({value:#x})")
}

/// `name` as a C identifier: every character that cannot be in one an underscore.
fn ident(name: &str) -> String {
    let mut out: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    if out.is_empty() || out.starts_with(|c: char| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

/// `text` as a C string literal.
fn c_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            ' '..='~' => out.push(c),
            _ => {
                let mut buffer = [0u8; 4];
                for byte in c.encode_utf8(&mut buffer).bytes() {
                    let _ = write!(out, "\\{byte:03o}");
                }
            }
        }
    }
    out.push('"');
    out
}
