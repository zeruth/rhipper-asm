//! The import wrappers of a 32-bit program's port: the program lays a `WNDCLASSEXA` or a `MSG`
//! out for 4-byte pointers, the host's import takes one laid out for 8. `adapters.c` holds a
//! wrapper for each import that takes such a structure: it widens the program's structure into
//! one of the host's before the call and, unless the import only reads it, narrows the host's
//! back after it, stopping when a value the host wrote does not fit in its 32-bit field. The
//! conversions are generated from the structure's two layouts in `ra-asm-protos`, field by field
//! as the interpreter host's adapter makes them. A structure whose two layouts are the same passes
//! as it is, and an import without one keeps its direct call.
//!
//! A union crosses by its members that keep one layout at both widths (`OVERLAPPED`'s offset,
//! [`converted_fields`]). A field that holds its structure's size (`WNDCLASSEXA`'s `cbSize`), and
//! a parameter that holds the size of the structure another points to (`VirtualQuery`'s), carry
//! the host's size for the program's. A code pointer the host calls back (the class's window
//! procedure) is handed to the host as an adapter of its own, which narrows each argument to the
//! program's width, calls the program's procedure and widens its result.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use ra_asm_ir::{Declaration, Module, Type};
use ra_asm_protos::{Callback, Kind, Record, callback, converted_fields, holds_size, layout, pointees, size_parameter};

use crate::c::c_type;

/// The prefix of a wrapper's name: the program calls `ra_adapt_RegisterClassExA` for
/// `RegisterClassExA`.
pub const WRAPPER_PREFIX: &str = "ra_adapt_";

/// The imports that only read the structures they take: their wrappers write nothing back.
const READ_ONLY: [&str; 4] = ["DispatchMessageA", "RegisterClassA", "RegisterClassExA", "TranslateMessage"];

/// The program's procedures of one callback type the host can hold at once: an adapter each.
const SLOTS: usize = 8;

/// The wrappers of a module's imports.
#[derive(Debug, Clone, Default)]
pub struct Adapters {
    /// The imports that have a wrapper.
    pub imports: BTreeSet<String>,
    /// The text of `adapters.c`; empty when no import takes a structure that changes with the
    /// width.
    pub text: String,
}

/// One field of a structure, nested ones flattened, at both widths.
struct Step {
    /// The field's path from the structure, an array element's by its index.
    field: String,
    at32: u32,
    at64: u32,
    narrow: u32,
    wide: u32,
    /// A signed integer that widens, sign-extended; a pointer or an unsigned integer is
    /// zero-extended, and a field of one width at both is copied.
    sign: bool,
    /// A pointer or a handle that widens, whose -1 stays -1.
    pointer: bool,
    role: Role,
}

/// What a field's value means beyond its bytes.
#[derive(Clone, Copy)]
enum Role {
    /// A value, widened and narrowed as it is.
    Value,
    /// The size of its structure, the program's and the host's.
    Size(u32, u32),
    /// A procedure the host calls back.
    Callback(&'static Callback),
}

/// The wrappers of the imports of `module`, a 32-bit program's, that take a structure whose
/// layouts differ.
pub fn adapters(module: &Module) -> Adapters {
    let defined: BTreeSet<&str> = module.functions.iter().map(|f| f.name.as_str()).collect();
    let mut wrappers = String::new();
    let mut imports = BTreeSet::new();
    // The conversions the wrappers call, by the structure and whether it is the way back.
    let mut conversions: BTreeMap<(&'static str, bool), String> = BTreeMap::new();
    // The callback types the conversions hand the host, and whether a size or a callback is read
    // and written as a value.
    let mut callbacks: BTreeMap<&'static str, &'static Callback> = BTreeMap::new();
    let mut values = false;
    for d in &module.declarations {
        if d.name.starts_with("llvm.") || d.name.starts_with("ra_") || defined.contains(d.name.as_str()) {
            continue;
        }
        let converted: Vec<(usize, &'static Record)> = pointees(&d.name).into_iter().filter(|(_, r)| !r.same).collect();
        if converted.is_empty() {
            continue;
        }
        let back = !READ_ONLY.contains(&d.name.as_str());
        match plans(d, &converted) {
            Ok((structures, sized)) => {
                for (record, steps) in &structures {
                    for step in steps {
                        match step.role {
                            Role::Value => {}
                            Role::Size(..) => values = true,
                            Role::Callback(c) => {
                                values = true;
                                callbacks.insert(c.name, c);
                            }
                        }
                    }
                    conversions.entry((record.name, false)).or_insert_with(|| to_host(record, steps));
                    if back {
                        conversions.entry((record.name, true)).or_insert_with(|| to_guest(record, steps));
                    }
                }
                values |= sized.is_some();
                wrapper(&mut wrappers, d, &converted, back, sized);
            }
            Err(reason) => stop(&mut wrappers, d, &converted, &reason),
        }
        imports.insert(d.name.clone());
    }
    if imports.is_empty() {
        return Adapters::default();
    }
    let mut text = String::from("// Generated by ras emit: the wrappers of the imports that take a structure the 32-bit program\n");
    text.push_str("// lays out for 4-byte pointers and the host for 8, each converting it around the call.\n");
    text.push_str("#include \"rhipper.h\"\n\n");
    for d in module.declarations.iter().filter(|d| imports.contains(&d.name)) {
        let _ = writeln!(text, "extern {};", signature(d));
    }
    text.push('\n');
    text.push_str(WIDEN);
    if conversions.keys().any(|(_, back)| *back) {
        text.push_str(NARROW);
    }
    if values {
        text.push_str(VALUES);
    }
    for c in callbacks.values() {
        text.push_str(&adapters_of(c));
    }
    for conversion in conversions.values() {
        text.push_str(conversion);
    }
    text.push_str(wrappers.trim_end());
    text.push('\n');
    Adapters { imports, text }
}

/// A field widened for the host: zero- or sign-extended from `narrow` bytes to `wide`, a pointer's
/// -1 kept as the host's -1, the way back of [`NARROW`]'s.
const WIDEN: &str = "\
// A field of the program's structure in the host's: zero- or sign-extended to the host's width
// (`extend` 0 or 1). A pointer or a handle (`extend` 2) is zero-extended but for all ones, the -1
// sentinel, which widens to the host's -1: DeleteCriticalSection frees a DebugInfo that is not.
static void ra_widen(uint8_t *host, const uint8_t *guest, unsigned narrow, unsigned wide, int extend) {
    int ones = extend == 2;
    for (unsigned i = 0; i < narrow; i++) ones &= guest[i] == 0xFF;
    uint8_t fill = (ones || (extend == 1 && (guest[narrow - 1] & 0x80))) ? 0xFF : 0;
    for (unsigned i = 0; i < wide; i++) host[i] = i < narrow ? guest[i] : fill;
}

";

/// A field narrowed back for the program, which stops when the host's value does not fit.
const NARROW: &str = "\
// A field of the host's structure in the program's: the host's value must fit in the narrow field.
// All ones is -1 at any width, the sentinel Windows writes in a pointer or a handle (a critical
// section's DebugInfo), and narrows to the program's -1.
static void ra_narrow(uint8_t *guest, const uint8_t *host, unsigned narrow, unsigned wide, int sign, const char *what) {
    uint8_t fill = sign && (host[narrow - 1] & 0x80) ? 0xFF : 0;
    uint64_t value = 0;
    int ones = 1;
    for (unsigned i = 0; i < wide && i < 8; i++) value |= (uint64_t)host[i] << (8 * i);
    for (unsigned i = 0; i < wide; i++) ones &= host[i] == 0xFF;
    for (unsigned i = narrow; i < wide && !ones; i++) {
        if (host[i] != fill) ra_adapt_failed(what, value);
    }
    for (unsigned i = 0; i < narrow; i++) guest[i] = host[i];
}

";

/// Fields and parameters read and written as a value.
const VALUES: &str = "\
// A field read and written as a value: a size that stands for the other width's, a procedure.
// A module may use any of them or none, so an unused one is no warning.
__attribute__((unused)) static inline uint64_t ra_get(const uint8_t *p, unsigned size) {
    uint64_t value = 0;
    for (unsigned i = 0; i < size; i++) value |= (uint64_t)p[i] << (8 * i);
    return value;
}

__attribute__((unused)) static inline void ra_put(uint8_t *p, unsigned size, uint64_t value) {
    for (unsigned i = 0; i < size; i++) p[i] = (uint8_t)(value >> (8 * i));
}

// The structure's size at one width is its size at the other; another size passes as it is.
__attribute__((unused)) static inline uint64_t ra_resize(uint64_t value, uint64_t from, uint64_t to) {
    return value == from ? to : value;
}

";

/// A parameter that holds the size of the structure another points to.
#[derive(Clone, Copy)]
struct SizeParameter {
    param: usize,
    size32: u32,
    size64: u32,
    /// The result is the size written.
    result: bool,
}

/// The result of planning conversions for a declaration: structures with their conversion steps,
/// and the parameter that holds a size if any.
type Plans = (Vec<(&'static Record, Vec<Step>)>, Option<SizeParameter>);

/// The structures `d` converts and the steps of each, and its parameter that holds the size of
/// one; or why one cannot be.
fn plans(d: &Declaration, converted: &[(usize, &'static Record)]) -> Result<Plans, String> {
    let mut out = Vec::new();
    for &(param, record) in converted {
        match d.ty.params.get(param) {
            Some(Type::Int(64) | Type::Ptr) => {}
            _ => return Err(format!("{}: parameter {param}, which points to {}, is not an address", d.name, record.name)),
        }
        let mut steps = Vec::new();
        plan(record, 0, 0, "", &mut steps)?;
        out.push((record, steps));
    }
    let mut sized = None;
    if let Some((param, of, result)) = size_parameter(&d.name)
        && let Some(&(_, record)) = converted.iter().find(|(p, _)| *p == of)
    {
        if d.ty.params.get(param) != Some(&Type::Int(64)) || (result && d.ty.ret != Type::Int(64)) {
            return Err(format!("{}: parameter {param}, the size of {}, or the result is not a 64-bit integer", d.name, record.name));
        }
        sized = Some(SizeParameter { param, size32: record.size32, size64: record.size64, result });
    }
    Ok((out, sized))
}

fn plan(record: &Record, base32: u32, base64: u32, prefix: &str, steps: &mut Vec<Step>) -> Result<(), String> {
    for field in converted_fields(record)? {
        let path = format!("{prefix}{}", field.name);
        let unequal = || {
            format!(
                "{}.{path}: a {:?} field of {} bytes at 32 bits and {} at 64",
                record.name, field.kind, field.at32.size, field.at64.size
            )
        };
        let (at32, at64) = (base32 + field.at32.offset, base64 + field.at64.offset);
        match field.kind {
            Kind::Record => {
                let inner = layout(field.record)
                    .ok_or_else(|| format!("{}.{path}: the layout table does not have {}", record.name, field.record))?;
                for element in 0..field.count {
                    let prefix = if field.count == 1 { format!("{path}.") } else { format!("{path}[{element}].") };
                    plan(inner, at32 + element * inner.size32, at64 + element * inner.size64, &prefix, steps)?;
                }
            }
            Kind::Float | Kind::Bits => {
                if field.at32.size != field.at64.size {
                    return Err(unequal());
                }
                let size = field.at32.size;
                steps.push(Step { field: path, at32, at64, narrow: size, wide: size, sign: false, pointer: false, role: Role::Value });
            }
            Kind::Pointer | Kind::Signed | Kind::Unsigned => {
                let count = field.count.max(1);
                let (narrow, wide) = (field.at32.size / count, field.at64.size / count);
                if wide < narrow {
                    return Err(unequal());
                }
                let role = if let Some(c) = callback(record.name, field.name) {
                    if field.kind != Kind::Pointer || count != 1 || (narrow, wide) != (4, 8) {
                        return Err(format!("{}.{path}: a {} that is not one 4-byte pointer at 32 bits and 8 at 64", record.name, c.name));
                    }
                    let integer = |kind: Kind, size: u32| {
                        matches!(kind, Kind::Pointer | Kind::Signed | Kind::Unsigned) && matches!(size, 1 | 2 | 4 | 8)
                    };
                    if !integer(c.result.0, c.result.1)
                        || c.params.iter().any(|&(kind, narrow, wide)| !integer(kind, narrow) || !integer(kind, wide))
                    {
                        return Err(format!(
                            "{}.{path}: {} takes or returns a value that is not an integer or a pointer",
                            record.name, c.name
                        ));
                    }
                    Role::Callback(c)
                } else if holds_size(record.name, field.name) {
                    if count != 1 || narrow != wide {
                        return Err(format!("{}.{path}: a size that changes width", record.name));
                    }
                    Role::Size(record.size32, record.size64)
                } else {
                    Role::Value
                };
                let sign = field.kind == Kind::Signed && wide > narrow;
                let pointer = field.kind == Kind::Pointer && wide > narrow;
                if count > 1 && narrow == wide {
                    // An array of one width at both copies as its bytes.
                    steps.push(Step { field: path, at32, at64, narrow: field.at32.size, wide: field.at64.size, sign, pointer, role });
                    continue;
                }
                for element in 0..count {
                    let field = if count == 1 { path.clone() } else { format!("{path}[{element}]") };
                    let (at32, at64) = (at32 + element * narrow, at64 + element * wide);
                    steps.push(Step { field, at32, at64, narrow, wide, sign, pointer, role });
                }
            }
        }
    }
    Ok(())
}

/// The conversion of the program's `record` into the host's; the host's padding is zero.
fn to_host(record: &Record, steps: &[Step]) -> String {
    let mut out = format!("// {}: {} bytes at 32 bits, {} at 64.\n", record.name, record.size32, record.size64);
    let _ = writeln!(out, "static void ra_to_host_{}(uint8_t *host, const uint8_t *guest) {{", record.name);
    let _ = writeln!(out, "    for (unsigned i = 0; i < {}; i++) host[i] = 0;", record.size64);
    for s in steps {
        let (host, guest, field) = (s.at64, s.at32, &s.field);
        let _ = match s.role {
            Role::Value => {
                let extend = if s.pointer { 2 } else { u8::from(s.sign) };
                writeln!(out, "    ra_widen(host + {host}, guest + {guest}, {}, {}, {extend}); // {field}", s.narrow, s.wide)
            }
            Role::Size(size32, size64) => writeln!(
                out,
                "    ra_put(host + {host}, {}, ra_resize(ra_get(guest + {guest}, {}), {size32}, {size64})); // {field}",
                s.wide, s.narrow
            ),
            Role::Callback(c) => writeln!(
                out,
                "    ra_put(host + {host}, {}, ra_callback_to_host_{}(ra_get(guest + {guest}, {}))); // {field}",
                s.wide, c.name, s.narrow
            ),
        };
    }
    out.push_str("}\n\n");
    out
}

/// The fields whose value a 32-bit program sees differently from the 64-bit host's, as `WoW64`
/// shows them to an x86 process: its own 2 GB of address space and an Intel processor
/// (`GetSystemInfo` in rhtest's sys).
const X86_VIEW: &[(&str, &str, u64)] = &[
    // the union of wProcessorArchitecture and its reserved word: PROCESSOR_ARCHITECTURE_INTEL
    ("_SYSTEM_INFO", "dwOemId", 0),
    ("_SYSTEM_INFO", "dwProcessorType", 586),
    ("_SYSTEM_INFO", "lpMinimumApplicationAddress", 0x0001_0000),
    ("_SYSTEM_INFO", "lpMaximumApplicationAddress", 0x7FFE_FFFF),
];

/// The conversion of the host's `record` back into the program's; the program's padding keeps
/// what it held.
fn to_guest(record: &Record, steps: &[Step]) -> String {
    let mut out = format!("static void ra_to_guest_{}(uint8_t *guest, const uint8_t *host) {{\n", record.name);
    for s in steps {
        let what = format!("the host's {}.{} does not fit in {} bytes", record.name, s.field, s.narrow);
        let (host, guest) = (s.at64, s.at32);
        if let Some(&(_, _, value)) = X86_VIEW.iter().find(|&&(r, f, _)| r == record.name && f == s.field) {
            let _ = writeln!(out, "    ra_put(guest + {guest}, {}, {value:#x}); // {}: what an x86 process sees", s.narrow, s.field);
            continue;
        }
        let _ = match s.role {
            Role::Value => {
                writeln!(out, "    ra_narrow(guest + {guest}, host + {host}, {}, {}, {}, \"{what}\");", s.narrow, s.wide, u8::from(s.sign))
            }
            Role::Size(size32, size64) => writeln!(
                out,
                "    ra_put(guest + {guest}, {}, ra_resize(ra_get(host + {host}, {}), {size64}, {size32})); // {}",
                s.narrow, s.wide, s.field
            ),
            Role::Callback(c) => writeln!(
                out,
                "    ra_put(guest + {guest}, {}, ra_callback_to_guest_{}(ra_get(host + {host}, {}), \"{what}\")); // {}",
                s.narrow, c.name, s.wide, s.field
            ),
        };
    }
    out.push_str("}\n\n");
    out
}

/// The C type of an integer or pointer of `size` bytes, signed for a [`Kind::Signed`].
fn int_type(kind: Kind, size: u32) -> String {
    format!("{}int{}_t", if kind == Kind::Signed { "" } else { "u" }, size * 8)
}

/// The adapters the host calls for the program's procedures of the callback type `c`, one per
/// slot, and the conversions of a procedure between the program's address and the host's. A
/// procedure the program hands the host takes the first free slot and keeps it; the host's
/// address of an adapter reads back as the program's procedure it calls.
fn adapters_of(c: &Callback) -> String {
    let name = c.name;
    let params: Vec<String> = c.params.iter().map(|&(kind, _, wide)| int_type(kind, wide)).collect();
    let named: Vec<String> = params.iter().enumerate().map(|(i, ty)| format!("{ty} a{i}")).collect();
    let (kind, size) = c.result;
    let result = int_type(kind, 8);
    let mut out = format!("// {name}: the program's procedures the host calls back, each through the adapter of its slot, which\n");
    out.push_str("// narrows each argument to the program's width, calls the program's procedure and widens its result.\n");
    let _ = writeln!(out, "static uint32_t ra_guest_{name}[{SLOTS}];\n");
    let _ = writeln!(out, "static {result} ra_call_{name}(unsigned slot, {}) {{", named.join(", "));
    for (i, &(kind, narrow, wide)) in c.params.iter().enumerate() {
        if narrow >= wide {
            continue;
        }
        let fits =
            if kind == Kind::Signed { format!("a{i} != ({})a{i}", int_type(kind, narrow)) } else { format!("a{i} >> {}", narrow * 8) };
        let what = format!("the host's {name} argument {i} does not fit in {narrow} bytes");
        let _ = writeln!(out, "    if ({fits}) ra_adapt_failed(\"{what}\", (uint64_t)a{i});");
    }
    let pointer = format!("uint64_t (*)({})", vec!["uint64_t"; c.params.len()].join(", "));
    let _ = writeln!(out, "    {} = ({pointer})(uintptr_t)ra_guest_{name}[slot];", pointer.replace("(*)", "(*procedure)"));
    let args: Vec<String> =
        c.params.iter().enumerate().map(|(i, &(_, narrow, _))| format!("({})a{i}", int_type(Kind::Unsigned, narrow))).collect();
    let _ = writeln!(out, "    return ({result})({})procedure({});", int_type(kind, size), args.join(", "));
    out.push_str("}\n\n");
    let forwarded: Vec<String> = (0..c.params.len()).map(|i| format!("a{i}")).collect();
    for slot in 0..SLOTS {
        let _ = writeln!(out, "static {result} ra_callback_{name}_{slot}({}) {{", named.join(", "));
        let _ = writeln!(out, "    return ra_call_{name}({slot}, {});", forwarded.join(", "));
        out.push_str("}\n\n");
    }
    let _ = writeln!(out, "static {result} (*const ra_host_{name}[{SLOTS}])({}) = {{", params.join(", "));
    for slot in 0..SLOTS {
        let _ = writeln!(out, "    ra_callback_{name}_{slot},");
    }
    out.push_str("};\n\n");
    let _ = writeln!(out, "// The host's procedure for the program's: the adapter of the slot that holds it.");
    let _ = writeln!(out, "static uint64_t ra_callback_to_host_{name}(uint64_t guest) {{");
    out.push_str("    if (!guest) return 0;\n");
    let _ = writeln!(out, "    for (unsigned i = 0; i < {SLOTS}; i++) {{");
    let _ = writeln!(out, "        if (!ra_guest_{name}[i]) ra_guest_{name}[i] = (uint32_t)guest;");
    let _ = writeln!(out, "        if (ra_guest_{name}[i] == guest) return (uint64_t)(uintptr_t)ra_host_{name}[i];");
    out.push_str("    }\n");
    let _ = writeln!(out, "    ra_adapt_failed(\"the program hands the host more {name} procedures than its {SLOTS} adapters\", guest);");
    out.push_str("}\n\n");
    let _ = writeln!(out, "// The program's procedure for the host's: the one its adapter calls, or the host's own when it fits.");
    // Inline: a module whose imports only read the procedure has no way back to call.
    let _ = writeln!(out, "static inline uint64_t ra_callback_to_guest_{name}(uint64_t host, const char *what) {{");
    let _ = writeln!(out, "    for (unsigned i = 0; i < {SLOTS}; i++) {{");
    let _ = writeln!(out, "        if (ra_guest_{name}[i] && host == (uint64_t)(uintptr_t)ra_host_{name}[i]) return ra_guest_{name}[i];");
    out.push_str("    }\n");
    out.push_str("    if (host >> 32) ra_adapt_failed(what, host);\n");
    out.push_str("    return host;\n");
    out.push_str("}\n\n");
    out
}

/// The C declaration of the import `d`, in the types the program calls it with.
fn signature(d: &Declaration) -> String {
    let params: Vec<String> = d.ty.params.iter().map(c_type).collect();
    format!("{} {}({})", c_type(&d.ty.ret), d.name, if params.is_empty() { "void".to_owned() } else { params.join(", ") })
}

/// The definition line of `d`'s wrapper, with its parameters named `a0`, `a1`, ….
fn definition(d: &Declaration) -> String {
    let params: Vec<String> = d.ty.params.iter().enumerate().map(|(i, ty)| format!("{} a{i}", c_type(ty))).collect();
    let params = if params.is_empty() { "void".to_owned() } else { params.join(", ") };
    format!("{} {WRAPPER_PREFIX}{}({params})", c_type(&d.ty.ret), d.name)
}

/// The wrapper of `d`: each structure widened into a buffer of the host's, the call, and each
/// one narrowed back when `back`. A null pointer passes as it is; the size of a structure, in its
/// parameter and the result, is the host's for the call.
fn wrapper(out: &mut String, d: &Declaration, converted: &[(usize, &'static Record)], back: bool, sized: Option<SizeParameter>) {
    let structures: Vec<String> = converted.iter().map(|(p, r)| format!("{} (parameter {p})", r.name)).collect();
    let way = if back { "widened for the host and narrowed back" } else { "widened for the host" };
    let _ = writeln!(out, "// {}: {} {way}.", d.name, structures.join(", "));
    let _ = writeln!(out, "{} {{", definition(d));
    for &(p, record) in converted {
        let guest = if d.ty.params[p] == Type::Ptr { format!("a{p}") } else { format!("(uint8_t *)(uintptr_t)a{p}") };
        let _ = writeln!(out, "    uint8_t *guest{p} = {guest};");
        let _ = writeln!(out, "    RA_ALIGNED({}) uint8_t host{p}[{}];", record.align64, record.size64);
        let _ = writeln!(out, "    if (guest{p}) ra_to_host_{}(host{p}, guest{p});", record.name);
    }
    let args: Vec<String> = (0..d.ty.params.len())
        .map(|i| match converted.iter().find(|(p, _)| *p == i) {
            Some(_) if d.ty.params[i] == Type::Ptr => format!("guest{i} ? host{i} : 0"),
            Some(_) => format!("guest{i} ? ({})(uintptr_t)host{i} : 0", c_type(&d.ty.params[i])),
            None => match sized {
                Some(s) if s.param == i => format!("ra_resize(a{i}, {}, {})", s.size32, s.size64),
                _ => format!("a{i}"),
            },
        })
        .collect();
    let call = format!("{}({})", d.name, args.join(", "));
    let returns = d.ty.ret != Type::Void;
    if returns {
        let _ = writeln!(out, "    {} result = {call};", c_type(&d.ty.ret));
    } else {
        let _ = writeln!(out, "    {call};");
    }
    if let Some(s) = sized
        && s.result
    {
        let _ = writeln!(out, "    result = ra_resize(result, {}, {});", s.size64, s.size32);
    }
    if back {
        for &(p, record) in converted {
            let _ = writeln!(out, "    if (guest{p}) ra_to_guest_{}(guest{p}, host{p});", record.name);
        }
    }
    if returns {
        out.push_str("    return result;\n");
    }
    out.push_str("}\n\n");
}

/// The wrapper of `d` when a structure it takes does not convert: the call stops, naming why.
fn stop(out: &mut String, d: &Declaration, converted: &[(usize, &'static Record)], reason: &str) {
    let _ = writeln!(out, "// {}: a structure it takes does not convert, and the call stops.", d.name);
    let _ = writeln!(out, "{} {{", definition(d));
    let pointer = converted.first().map_or_else(|| "0".to_owned(), |(p, _)| format!("(uint64_t)a{p}"));
    let _ = writeln!(out, "    ra_adapt_failed(\"{}\", {pointer});", reason.replace('\\', "\\\\").replace('"', "\\\""));
    out.push_str("}\n\n");
}
