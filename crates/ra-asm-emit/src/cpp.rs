//! The classes of the C++ output: a `class` per RTTI class, with its direct bases as the hierarchy
//! lists them and the virtual functions it introduces as its methods, and a `class` per class the
//! symbols alone name, with the member functions they qualify with it.
//!
//! The port keeps the program's own vtables, which hold raw code addresses, and dispatches through
//! them as the program did. So a virtual function's body is written as a member function, and its
//! C name stays as a thin function with the lifted prototype, the one the vtable's entry and every
//! direct call resolve to, which casts its first argument to the class and calls the method. The
//! methods are not `virtual` in the host: the objects are the program's bytes, never constructed by
//! the host's compiler, and a host vtable would be read where the program's lies; a method that
//! fills a base's slot says so beside its slot. A method is non-static when the ABI inference
//! found the function reading its first register parameter (`rcx`, or `ecx` under `thiscall`),
//! which is the object; a function that never touches it is a static method, so no `this` is
//! invented for it.
//!
//! The fields are the offsets the methods read and write through the object, typed as they are
//! accessed, each declared by the most basic class whose methods reach it. They document the
//! object: the methods keep addressing the program's bytes, whose layout the host's compiler
//! never decides.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write;

use ra_asm_analysis::{Analysis, rtti, symbols};
use ra_asm_ir::{BinOp, Cast, Function, Inst, Operand, Type, ValueId};
use ra_asm_lift::raise::abi::{Kind, Signature};

use crate::c::{c_type, declaration, name};

/// A class of the program, as the C++ output declares it.
#[derive(Debug, Clone)]
pub struct Class {
    /// The C++ identifier: `ns_Inner` for `ns::Inner`.
    pub name: String,
    /// The class name as the source wrote it.
    pub source_name: String,
    /// The type descriptor of an RTTI class; none for a class only its member functions' symbols name.
    pub type_descriptor: Option<u64>,
    /// The direct bases, as identifiers, in the hierarchy's order.
    pub bases: Vec<String>,
    /// Every base whose subobject has a fixed place, direct or not, with that offset in the object.
    pub base_offsets: Vec<(String, u32)>,
    /// The offsets of the vtable pointers in the object: no fields of the source.
    pub vptrs: Vec<u32>,
    /// The virtual functions the class introduces or overrides, in vtable order, then its other
    /// member functions, in address order.
    pub methods: Vec<Method>,
}

/// One method of a class.
#[derive(Debug, Clone)]
pub struct Method {
    /// The IR name of the function, which stays the C name of the vtable's entry.
    pub ir_name: String,
    /// The name in the class: the IR name without the class prefix (`Rect_area` is `area`).
    pub name: String,
    /// Whether the function's first parameter is the object: a non-static method.
    pub this: bool,
    /// The offset in the object of the vtable that holds the function, and its slot there; no
    /// slot for a member function that is not virtual.
    pub offset: u32,
    pub slot: Option<usize>,
    /// The base whose slot the virtual function fills: an override.
    pub overrides: Option<String>,
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
/// derives from. A function of the program whose symbol makes it a member function (`?add@Counter@@QAEXH@Z`)
/// is a method of that class too, which is declared from the symbols when no RTTI names it.
/// `signatures` (by the final function names) say whether it reads the object.
pub fn classes(analysis: &Analysis<'_>, signatures: &HashMap<String, Signature>) -> Vec<Class> {
    // A class lists every base, direct or not, so one with more bases is never a base of one with fewer.
    let mut order: Vec<&rtti::Class> = analysis.classes.iter().collect();
    order.sort_by_key(|c| (c.bases.len(), c.type_descriptor));
    let taken: HashSet<&str> = analysis.functions.values().map(|f| f.name.as_str()).collect();
    let identifier = |name: &str| {
        let mut identifier = rtti::identifier(name);
        if taken.contains(identifier.as_str()) {
            identifier.push_str("_class");
        }
        identifier
    };
    let identifiers: HashMap<&str, String> = order.iter().map(|c| (c.name.as_str(), identifier(&c.name))).collect();
    let mut classes: Vec<Class> = order
        .iter()
        .map(|c| {
            let bases = c.direct_bases.iter().filter_map(|b| identifiers.get(b.as_str()).cloned()).collect();
            let base_offsets =
                c.bases.iter().zip(&c.base_offsets).filter_map(|(b, o)| Some((identifiers.get(b.as_str())?.clone(), (*o)?))).collect();
            // A class nothing instantiates has its vtable pointer where the classes derived from it keep one for it.
            let mut vptrs: Vec<u32> = c.vtables.iter().map(|v| v.offset).collect();
            let derived_vptr = order.iter().any(|d| {
                d.bases.iter().zip(&d.base_offsets).any(|(b, o)| *b == c.name && o.is_some_and(|o| d.vtables.iter().any(|v| v.offset == o)))
            });
            if derived_vptr {
                vptrs.push(0);
            }
            vptrs.sort_unstable();
            vptrs.dedup();
            Class {
                name: identifiers[c.name.as_str()].clone(),
                source_name: c.name.clone(),
                type_descriptor: Some(c.type_descriptor),
                bases,
                base_offsets,
                vptrs,
                methods: Vec::new(),
            }
        })
        .collect();
    let by_name: HashMap<&str, &rtti::Class> = order.iter().map(|c| (c.name.as_str(), *c)).collect();
    let derives = |class: &str, base: &str| class == base || by_name.get(class).is_some_and(|c| c.bases.iter().any(|b| b == base));
    let is_64 = analysis.image.is_64();
    let reads_object = |f: &ra_asm_analysis::Function| signatures.get(&f.name).is_some_and(|s| s.params.first() == Some(&Kind::Int));
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
        let overrides = overridden(&order, order[owner], offset, slot).map(|b| identifiers[b].clone());
        let class = &mut classes[owner];
        let name = method_name(class, &f.name);
        class.methods.push(Method { ir_name: f.name.clone(), name, this: reads_object(f), offset, slot: Some(slot), overrides });
    }
    // The member functions that are not virtual: the runtime's stay the runtime's.
    let runtime = symbols::RuntimeRanges::of(analysis.image);
    for f in analysis.functions.values() {
        if !f.virtuals.is_empty() || runtime.contains(f.entry) {
            continue;
        }
        let Some(source_name) = f.symbol.as_deref().and_then(|s| member_class(s, is_64)) else { continue };
        let index = if let Some(index) = classes.iter().position(|c| c.source_name == source_name) {
            index
        } else {
            classes.push(Class {
                name: identifier(&source_name),
                source_name,
                type_descriptor: None,
                bases: Vec::new(),
                base_offsets: Vec::new(),
                vptrs: Vec::new(),
                methods: Vec::new(),
            });
            classes.len() - 1
        };
        let class = &mut classes[index];
        let name = method_name(class, &f.name);
        class.methods.push(Method { ir_name: f.name.clone(), name, this: reads_object(f), offset: 0, slot: None, overrides: None });
    }
    for class in &mut classes {
        class.methods.sort_by_key(|m| (m.slot.is_none(), m.offset, m.slot));
    }
    classes
}

/// The class a decorated name makes its function a member function of: `?add@Counter@@QAEXH@Z`
/// is `Counter`'s, as the access code after the qualified name (`A` to `X`) says; a function at a
/// namespace's scope has `Y` there.
fn member_class(symbol: &str, is_64: bool) -> Option<String> {
    let rest = symbol.strip_prefix('?')?;
    let end = rest.find("@@")?;
    let code = rest[end + 2..].chars().next()?;
    if !code.is_ascii_uppercase() || code > 'X' {
        return None;
    }
    symbols::undecorate(symbol, is_64).rsplit_once("::").map(|(class, _)| class.to_owned())
}

/// The base whose slot a virtual function of `class` fills in the vtable at `offset`: the most
/// derived base at that offset whose interface has the slot.
fn overridden<'a>(order: &[&'a rtti::Class], class: &rtti::Class, offset: u32, slot: usize) -> Option<&'a str> {
    class
        .bases
        .iter()
        .zip(&class.base_offsets)
        .filter(|(_, at)| **at == Some(offset))
        .filter_map(|(b, _)| order.iter().copied().find(|c| c.name == *b))
        .filter(|base| interface(order, base, &class.name).is_some_and(|slots| slot < slots))
        .max_by_key(|base| base.bases.len())
        .map(|base| base.name.as_str())
}

/// The slots of a base's interface: its own vtable's, or, for a base nothing instantiates, the
/// fewest the classes derived from it have where they hold it, leaving out `class` and the classes
/// derived from it, whose own virtual functions are in question.
fn interface(order: &[&rtti::Class], base: &rtti::Class, class: &str) -> Option<usize> {
    if let Some(v) = base.vtables.iter().find(|v| v.offset == 0) {
        return Some(v.functions.len());
    }
    order
        .iter()
        .filter(|d| d.name != class && !d.bases.iter().any(|b| b == class))
        .filter_map(|d| {
            let (_, at) = d.bases.iter().zip(&d.base_offsets).find(|(b, _)| **b == base.name)?;
            Some(d.vtables.iter().find(|v| Some(v.offset) == *at)?.functions.len())
        })
        .min()
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
/// Named` with the methods it introduces, each virtual one with the slot it fills, and the fields
/// its methods reach, each with its offset. `functions` are the module's, by name: a method whose
/// function the module lacks is left out. `types` are the module's named structs, whose fields
/// the accesses go through.
pub fn declare(classes: &[Class], functions: &HashMap<&str, &Function>, types: &[(String, Vec<Type>)]) -> String {
    let mut out = String::new();
    let mut declared: HashMap<&str, BTreeMap<u32, Type>> = HashMap::new();
    for class in classes {
        let _ = match class.type_descriptor {
            Some(at) => writeln!(out, "// {} (type descriptor at {at:#x})", class.source_name),
            None => writeln!(out, "// {} (named by the symbols of its member functions)", class.source_name),
        };
        let _ = write!(out, "class {}", class.name);
        if !class.bases.is_empty() {
            let bases: Vec<String> = class.bases.iter().map(|b| format!("public {b}")).collect();
            let _ = write!(out, " : {}", bases.join(", "));
        }
        let _ = writeln!(out, " {{");
        let methods: Vec<(&Method, &Function)> =
            class.methods.iter().filter_map(|m| Some((m, *functions.get(m.ir_name.as_str())?))).collect();
        let fields = fields(class, &methods, types, &declared);
        if !methods.is_empty() || !fields.is_empty() {
            let _ = writeln!(out, "public:");
        }
        for (m, f) in &methods {
            let prototype = method_prototype(f, class, m, false);
            let Some(slot) = m.slot else {
                let _ = writeln!(out, "    {prototype};");
                continue;
            };
            let vtable = if m.offset == 0 { String::new() } else { format!(" of the vtable at offset {}", m.offset) };
            let overrides = m.overrides.as_ref().map_or(String::new(), |b| format!(", overrides {b}"));
            let _ = writeln!(out, "    {prototype}; // slot {slot}{vtable}{overrides}");
        }
        for (offset, ty) in &fields {
            let _ = writeln!(out, "    {}; // offset {offset}", declaration(ty, &format!("f{offset}")));
        }
        let _ = writeln!(out, "}};");
        declared.insert(class.name.as_str(), fields);
    }
    out
}

/// The fields of `class`: the offsets its methods read and write through the object, each with
/// the widest type it is accessed as, leaving out the vtable pointers, the fields a base declares
/// and an access that starts inside a field before it.
fn fields(
    class: &Class,
    methods: &[(&Method, &Function)],
    types: &[(String, Vec<Type>)],
    declared: &HashMap<&str, BTreeMap<u32, Type>>,
) -> BTreeMap<u32, Type> {
    let size = |ty: &Type| ty.size(8).unwrap_or(0);
    let mut fields: BTreeMap<u32, Type> = BTreeMap::new();
    for (m, f) in methods.iter().filter(|(m, f)| m.this && !f.params.is_empty()) {
        // A method of a vtable past the first reads its object from that vtable's subobject.
        // A move wider than a register is a block copy or clear over several fields, not a field.
        for (offset, ty) in accesses(f, f.params[0], types).into_iter().filter(|(_, ty)| (1..=8).contains(&size(ty))) {
            let Some(offset) = offset.checked_add(i64::from(m.offset)).and_then(|o| u32::try_from(o).ok()) else { continue };
            let field = fields.entry(offset).or_insert_with(|| ty.clone());
            if size(&ty) > size(field) {
                *field = ty;
            }
        }
    }
    let inherited = |offset: u32| {
        class.base_offsets.iter().any(|(base, at)| {
            offset.checked_sub(*at).is_some_and(|inner| declared.get(base.as_str()).is_some_and(|d| d.contains_key(&inner)))
        })
    };
    fields.retain(|&offset, _| !class.vptrs.contains(&offset) && !inherited(offset));
    let mut end = 0;
    fields.retain(|&offset, ty| {
        let starts = u64::from(offset) >= end;
        if starts {
            end = u64::from(offset) + size(ty).max(1);
        }
        starts
    });
    fields
}

/// The offsets from `this` the loads and stores of `f` address, with the type each moves.
fn accesses(f: &Function, this: ValueId, types: &[(String, Vec<Type>)]) -> Vec<(i64, Type)> {
    let defs: HashMap<ValueId, &Inst> = f.blocks.iter().flat_map(|b| &b.instrs).filter_map(|i| Some((i.result?, &i.inst))).collect();
    f.blocks
        .iter()
        .flat_map(|b| &b.instrs)
        .filter_map(|i| match &i.inst {
            Inst::Load { ty, ptr, .. } | Inst::Store { ty, ptr, .. } => Some((offset_from(&defs, types, this, ptr, 0)?, ty.clone())),
            _ => None,
        })
        .collect()
}

/// The offset from `this` an operand addresses: through the casts, the additions of a constant
/// and the fields of the structs the type recovery made.
fn offset_from(defs: &HashMap<ValueId, &Inst>, types: &[(String, Vec<Type>)], this: ValueId, o: &Operand, depth: u32) -> Option<i64> {
    let Operand::Local(v) = o else { return None };
    if *v == this {
        return Some(0);
    }
    if depth == 16 {
        return None;
    }
    let at = |o: &Operand| offset_from(defs, types, this, o, depth + 1);
    match defs.get(v)? {
        // A 32-bit program's object pointer arrives as a 64-bit parameter and is narrowed to address.
        Inst::Cast { cast: Cast::IntToPtr | Cast::PtrToInt | Cast::ZExt | Cast::Trunc | Cast::Bitcast, value, .. } => at(value),
        // The same narrowing as a mask: a 32-bit program's object lies below 4 GiB.
        Inst::Bin { op: BinOp::And, lhs, rhs: Operand::Int(0xffff_ffff), .. } => at(lhs),
        Inst::Bin { op: BinOp::Add, ty, lhs, rhs: Operand::Int(c) } | Inst::Bin { op: BinOp::Add, ty, lhs: Operand::Int(c), rhs: lhs } => {
            at(lhs)?.checked_add(constant(ty, *c)?)
        }
        Inst::Bin { op: BinOp::Sub, ty, lhs, rhs: Operand::Int(c) } => at(lhs)?.checked_sub(constant(ty, *c)?),
        Inst::Gep { base_ty: Type::I8, ptr, indices, .. } => {
            let [(_, Operand::Int(c))] = indices.as_slice() else { return None };
            at(ptr)?.checked_add(i64::try_from(*c).ok()?)
        }
        Inst::Gep { base_ty: Type::Named(struct_name), ptr, indices, .. } => {
            let [(_, Operand::Int(0)), (_, Operand::Int(index))] = indices.as_slice() else { return None };
            let (_, fields) = types.iter().find(|(n, _)| n == struct_name)?;
            let (offsets, _) = ra_asm_ir::types::layout(fields, 8)?;
            at(ptr)?.checked_add(i64::try_from(*offsets.get(usize::try_from(*index).ok()?)?).ok()?)
        }
        _ => None,
    }
}

/// A constant as the width of the arithmetic reads it: `add i32 %p, -4` wraps.
fn constant(ty: &Type, c: i128) -> Option<i64> {
    match ty {
        Type::Int(32) => Some(i64::from(c as i32)),
        _ => i64::try_from(c).ok(),
    }
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
