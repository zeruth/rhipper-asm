//! The classes of the C++ output: a `class` per RTTI class, with its direct bases as the hierarchy
//! lists them and the virtual functions it introduces as its methods.
//!
//! The port keeps the program's own vtables, which hold raw code addresses, and dispatches through
//! them as the program did. So a virtual function's body is written as a member function, and its
//! C name stays as a thin function with the lifted prototype, the one the vtable's entry and every
//! direct call resolve to, which casts its first argument to the class and calls the method. The
//! methods are not `virtual` in the host: the objects are the program's bytes, never constructed by
//! the host's compiler, and a host vtable would be read where the program's lies. A method is
//! non-static when the ABI inference found the function reading its first register parameter
//! (`rcx`, or `ecx` under `thiscall`), which is the object; a function that never touches it is a
//! static method, so no `this` is invented for it.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use ra_asm_analysis::{Analysis, rtti, symbols};
use ra_asm_ir::{Function, Type};
use ra_asm_lift::raise::abi::{Kind, Signature};

use crate::c::{c_type, name};

/// A class of the program, as the C++ output declares it.
#[derive(Debug, Clone)]
pub struct Class {
    /// The C++ identifier: `ns_Inner` for `ns::Inner`.
    pub name: String,
    /// The class name as the source wrote it.
    pub source_name: String,
    pub type_descriptor: u64,
    /// The direct bases, as identifiers, in the hierarchy's order.
    pub bases: Vec<String>,
    /// The virtual functions the class introduces, in vtable order.
    pub methods: Vec<Method>,
}

/// One virtual function of a class.
#[derive(Debug, Clone)]
pub struct Method {
    /// The IR name of the function, which stays the C name of the vtable's entry.
    pub ir_name: String,
    /// The name in the class: the IR name without the class prefix (`Rect_area` is `area`).
    pub name: String,
    /// Whether the function's first parameter is the object: a non-static method.
    pub this: bool,
    /// The offset in the object of the vtable that holds the function, and its slot there.
    pub offset: u32,
    pub slot: usize,
}

/// The names a method cannot take in C++ (the C keywords are already kept out of every function name).
const KEYWORDS: [&str; 36] = [
    "class",
    "this",
    "new",
    "delete",
    "operator",
    "template",
    "typename",
    "virtual",
    "public",
    "private",
    "protected",
    "friend",
    "namespace",
    "using",
    "throw",
    "try",
    "catch",
    "export",
    "bool",
    "true",
    "false",
    "and",
    "or",
    "not",
    "xor",
    "typeid",
    "mutable",
    "explicit",
    "inline",
    "constexpr",
    "nullptr",
    "static",
    "const",
    "wchar_t",
    "char8_t",
    "requires",
];

/// The classes of `analysis` for the C++ output, bases before the classes derived from them. A
/// function is the method of the class its symbol qualifies it with (`?scale@Shape@@…` is
/// `Shape`'s, though only `Square`'s vtable holds it once `Rect` overrides it), else of the class
/// that introduced it as the RTTI tells: the most derived class every class whose vtable holds it
/// derives from. `signatures` (by the final function names) say whether it reads the object.
pub fn classes(analysis: &Analysis<'_>, signatures: &HashMap<String, Signature>) -> Vec<Class> {
    // A class lists every base, direct or not, so one with more bases is never a base of one with fewer.
    let mut order: Vec<&rtti::Class> = analysis.classes.iter().collect();
    order.sort_by_key(|c| (c.bases.len(), c.type_descriptor));
    let taken: HashSet<&str> = analysis.functions.values().map(|f| f.name.as_str()).collect();
    let identifiers: HashMap<&str, String> = order
        .iter()
        .map(|c| {
            let mut identifier = rtti::identifier(&c.name);
            if taken.contains(identifier.as_str()) {
                identifier.push_str("_class");
            }
            (c.name.as_str(), identifier)
        })
        .collect();
    let mut classes: Vec<Class> = order
        .iter()
        .map(|c| {
            let bases = c.direct_bases.iter().filter_map(|b| identifiers.get(b.as_str()).cloned()).collect();
            Class {
                name: identifiers[c.name.as_str()].clone(),
                source_name: c.name.clone(),
                type_descriptor: c.type_descriptor,
                bases,
                methods: Vec::new(),
            }
        })
        .collect();
    let by_name: HashMap<&str, &rtti::Class> = order.iter().map(|c| (c.name.as_str(), *c)).collect();
    let derives = |class: &str, base: &str| class == base || by_name.get(class).is_some_and(|c| c.bases.iter().any(|b| b == base));
    let is_64 = analysis.image.is_64();
    for f in analysis.functions.values() {
        let Some(first) = f.virtuals.first() else { continue };
        let holders: HashSet<&str> = f.virtuals.iter().map(|v| v.class.as_str()).collect();
        let qualifier =
            f.symbol.as_deref().map(|s| symbols::undecorate(s, is_64)).and_then(|n| n.rsplit_once("::").map(|(c, _)| c.to_owned()));
        let owner = qualifier
            .and_then(|q| order.iter().position(|c| c.name == q))
            .or_else(|| {
                (0..order.len()).filter(|&i| holders.iter().all(|h| derives(h, &order[i].name))).max_by_key(|&i| order[i].bases.len())
            })
            .or_else(|| order.iter().position(|c| c.name == first.class));
        let Some(owner) = owner else { continue };
        // The slot in the owner's own vtable, or the same slot of the vtable that inherited it.
        let (offset, slot) = f.virtuals.iter().find(|v| v.class == order[owner].name).map_or((0, first.slot), |v| (v.offset, v.slot));
        let this = signatures.get(&f.name).is_some_and(|s| s.params.first() == Some(&Kind::Int));
        let class = &mut classes[owner];
        let name = method_name(class, &f.name);
        class.methods.push(Method { ir_name: f.name.clone(), name, this, offset, slot });
    }
    for class in &mut classes {
        class.methods.sort_by_key(|m| (m.offset, m.slot));
    }
    classes
}

/// The IR name without the class prefix, when what remains can be a method of the class: not
/// empty, not a keyword, not the class name (a constructor), and not another method's name.
fn method_name(class: &Class, ir_name: &str) -> String {
    let prefix = rtti::identifier(&class.source_name);
    let short = ir_name.strip_prefix(prefix.as_str()).and_then(|rest| rest.strip_prefix('_')).unwrap_or(ir_name);
    let fits = !short.is_empty()
        && !short.starts_with(|c: char| c.is_ascii_digit())
        && !KEYWORDS.contains(&short)
        && short != class.name
        && !class.methods.iter().any(|m| m.name == short);
    if fits { short.to_owned() } else { ir_name.to_owned() }
}

/// The declarations of the classes, before the functions: `class Rect : public Shape, public
/// Named` with the methods it introduces, each with the slot it fills. `functions` are the
/// module's, by name: a method whose function the module lacks is left out.
pub fn declare(classes: &[Class], functions: &HashMap<&str, &Function>) -> String {
    let mut out = String::new();
    for class in classes {
        let _ = writeln!(out, "// {} (type descriptor at {:#x})", class.source_name, class.type_descriptor);
        let _ = write!(out, "class {}", class.name);
        if !class.bases.is_empty() {
            let bases: Vec<String> = class.bases.iter().map(|b| format!("public {b}")).collect();
            let _ = write!(out, " : {}", bases.join(", "));
        }
        let _ = writeln!(out, " {{");
        let methods: Vec<(&Method, &Function)> =
            class.methods.iter().filter_map(|m| Some((m, *functions.get(m.ir_name.as_str())?))).collect();
        if !methods.is_empty() {
            let _ = writeln!(out, "public:");
        }
        for (m, f) in methods {
            let vtable = if m.offset == 0 { String::new() } else { format!(" of the vtable at offset {}", m.offset) };
            let _ = writeln!(out, "    {}; // slot {}{vtable}", method_prototype(f, class, m, false), m.slot);
        }
        let _ = writeln!(out, "}};");
    }
    out
}

/// The prototype of a method: in the class (`static` when it has no object), or `qualified` with
/// the class name for its definition. The object parameter is `this`.
pub(crate) fn method_prototype(f: &Function, class: &Class, m: &Method, qualified: bool) -> String {
    let params: Vec<String> =
        f.ty.params.iter().zip(&f.params).skip(usize::from(m.this)).map(|(t, v)| format!("{} {}", c_type(t), name(f, *v))).collect();
    let storage = if m.this || qualified { "" } else { "static " };
    let noreturn = if f.attrs.iter().any(|a| a == "noreturn") { "RA_NORETURN " } else { "" };
    let owner = if qualified { format!("{}::", class.name) } else { String::new() };
    format!("{storage}{noreturn}{} {owner}{}({})", c_type(&f.ty.ret), m.name, params.join(", "))
}

/// The function under the C name, with the lifted `prototype`: the vtable's entry, which calls
/// the method on its first argument.
pub(crate) fn trampoline(out: &mut String, f: &Function, class: &Class, m: &Method, prototype: &str) {
    let args: Vec<String> = f.params.iter().skip(usize::from(m.this)).map(|v| name(f, *v)).collect();
    let call = if m.this {
        format!("(({} *)(uintptr_t){})->{}({})", class.name, name(f, f.params[0]), m.name, args.join(", "))
    } else {
        format!("{}::{}({})", class.name, m.name, args.join(", "))
    };
    let _ = writeln!(out, "extern \"C\" {prototype} {{");
    if f.attrs.iter().any(|a| a == "noreturn") {
        let _ = writeln!(out, "    {call};\n    RA_UNREACHABLE();");
    } else if f.ty.ret == Type::Void {
        let _ = writeln!(out, "    {call};");
    } else {
        let _ = writeln!(out, "    return {call};");
    }
    let _ = writeln!(out, "}}");
}
