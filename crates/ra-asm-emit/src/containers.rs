//! The containers of the standard library a program inlines, as the output reads them: a member
//! function of `std::vector<unsigned int>` reaches its object as a `struct std_vector_unsigned_int`
//! whose fields carry the library's names (`_Myfirst`, `_Mylast`, `_Myend` in Microsoft's), where
//! the raise's struct of the object would read `f0`, `f8`, `f16`.
//!
//! The symbols name the container of each member function ([`ra_asm_analysis::stl::member`]) and
//! the toolset its library, whose layout gives the fields by offset. The raise typed the object's
//! fields by their accesses; a struct of the module reads as the container's only when each of its
//! fields lies where one of the container's lies and is as wide, so a struct the raise made of
//! something else is never renamed. The container's struct keeps the raise's types, the ones its
//! accesses are written with, under the library's names, and is declared once for every member
//! function whose fields agree.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

use ra_asm_analysis::Analysis;
use ra_asm_analysis::stl::{self, FieldType, Library};
use ra_asm_ir::{Cast, Function, Inst, Operand, Type, ValueId};

use crate::c::declaration;

/// A container of the standard library the program instantiates, as the output declares it.
#[derive(Debug, Clone)]
pub struct Container {
    /// The C identifier of its struct: `std_vector_unsigned_int`.
    pub name: String,
    /// The container as the source names it: `std::vector<unsigned int>`.
    pub source_name: String,
    /// The library whose layout it has.
    pub library: Library,
    /// The fields of the layout.
    pub fields: Vec<stl::Field>,
    /// The width of the program's pointers, in bytes.
    pub pointer: u32,
    /// The IR names of its member functions that take the object first.
    pub methods: Vec<String>,
}

impl Container {
    /// The library's name of the field at `offset`.
    pub fn field_at(&self, offset: u64) -> Option<&'static str> {
        self.fields.iter().find(|f| u64::from(f.offset) == offset).map(|f| f.name)
    }

    /// Whether an access of `size` bytes at `offset` is one of the fields: as wide as the field,
    /// or as one of the ways of reading it, or within its characters.
    fn holds(&self, offset: u64, size: u64) -> bool {
        let p = u64::from(self.pointer);
        let fits = |ty: FieldType| match ty {
            FieldType::Pointer | FieldType::Size => size == p,
            FieldType::Int | FieldType::Float => size == 4,
            FieldType::Chars(n) => size <= u64::from(n),
            FieldType::Union(_) => false,
        };
        self.fields.iter().filter(|f| u64::from(f.offset) == offset).any(|f| match f.ty {
            FieldType::Union(ways) => ways.iter().any(|(_, ty)| fits(*ty)),
            ty => fits(ty),
        })
    }
}

/// The containers whose member functions `analysis` names, each with the layout of the library
/// the image's toolset links; a container whose layout is not known here is left out.
pub fn containers(analysis: &Analysis<'_>) -> Vec<Container> {
    let library = Library::of_image(analysis.image);
    let pointer = if analysis.image.is_64() { 8 } else { 4 };
    let mut by_source: BTreeMap<String, Container> = BTreeMap::new();
    for f in analysis.functions.values() {
        let Some(member) = f.symbol.as_deref().and_then(stl::member) else { continue };
        if !member.this {
            continue;
        }
        let container = match by_source.entry(member.container) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => {
                let Some(fields) = stl::layout(member.kind, library, pointer) else { continue };
                let name = ra_asm_analysis::rtti::identifier(e.key());
                let source_name = e.key().clone();
                e.insert(Container { name, source_name, library, fields, pointer, methods: Vec::new() })
            }
        };
        container.methods.push(f.name.clone());
    }
    by_source.into_values().collect()
}

/// The pointers at the object in `f`, a member function of `container`, with the struct of `types`
/// each reaches its fields through: the first parameter, or a cast of it, whose struct's fields
/// are all the container's. Empty when any struct the object is reached through is not.
pub(crate) fn object_pointers<'a>(f: &Function, container: &Container, types: &'a [(String, Vec<Type>)]) -> HashMap<ValueId, &'a str> {
    let Some(&this) = f.params.first() else { return HashMap::new() };
    // The values that are the object: the parameter and the casts of it.
    let mut object = vec![this];
    let mut grown = true;
    while grown {
        grown = false;
        for i in f.blocks.iter().flat_map(|b| &b.instrs) {
            if let (Some(r), Inst::Cast { cast: Cast::IntToPtr | Cast::ZExt | Cast::PtrToInt, value: Operand::Local(v), .. }) =
                (i.result, &i.inst)
                && object.contains(v)
                && !object.contains(&r)
            {
                object.push(r);
                grown = true;
            }
        }
    }
    let mut out: HashMap<ValueId, &'a str> = HashMap::new();
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        let Inst::Gep { base_ty: Type::Named(name), ptr: Operand::Local(v), indices, .. } = &i.inst else { continue };
        if indices.len() != 2 || !object.contains(v) {
            continue;
        }
        let Some((name, fields)) = types.iter().find(|(n, _)| n == name) else { return HashMap::new() };
        if fields_of(fields).is_none_or(|fields| !fields.iter().all(|(offset, ty)| ty.size(8).is_some_and(|s| container.holds(*offset, s))))
        {
            return HashMap::new();
        }
        if out.get(v).is_some_and(|s| *s != name.as_str()) {
            return HashMap::new();
        }
        out.insert(*v, name.as_str());
    }
    out
}

/// The fields of the object `f` reads, by offset, with the types the raise gave them; `None` when
/// `f` does not read its object as `container`, or reads one offset as two types.
pub(crate) fn object_fields(f: &Function, container: &Container, types: &[(String, Vec<Type>)]) -> Option<BTreeMap<u64, Type>> {
    let structs = object_pointers(f, container, types);
    if structs.is_empty() {
        return None;
    }
    let mut out: BTreeMap<u64, Type> = BTreeMap::new();
    for s in structs.values() {
        let (_, fields) = types.iter().find(|(n, _)| n == s)?;
        for (offset, ty) in fields_of(fields)? {
            if out.get(&offset).is_some_and(|t| *t != ty) {
                return None;
            }
            out.insert(offset, ty);
        }
    }
    Some(out)
}

/// The fields of a struct of the module, without its padding, by offset.
fn fields_of(fields: &[Type]) -> Option<Vec<(u64, Type)>> {
    let (offsets, _) = ra_asm_ir::types::layout(fields, 8)?;
    Some(
        fields
            .iter()
            .zip(offsets)
            .filter(|(ty, _)| !matches!(ty, Type::Array(_, e) if **e == Type::I8))
            .map(|(ty, offset)| (offset, ty.clone()))
            .collect(),
    )
}

/// The structs of the containers `shapes` holds the fields of, by the container's name: each field
/// under the library's name and with the raise's type, the bytes between them as padding.
pub(crate) fn declare(containers: &[Container], shapes: &BTreeMap<&str, BTreeMap<u64, Type>>) -> String {
    let mut out = String::new();
    for c in containers {
        let Some(shape) = shapes.get(c.name.as_str()).filter(|s| !s.is_empty()) else { continue };
        let _ = writeln!(out, "// {}, in {} layout.", c.source_name, c.library.name());
        let _ = writeln!(out, "struct {} {{", c.name);
        let mut at = 0;
        for (offset, ty) in shape {
            if *offset > at {
                let _ = writeln!(out, "    uint8_t pad{at}[{}];", offset - at);
            }
            let name = c.field_at(*offset).unwrap_or("field");
            let _ = writeln!(out, "    {};", declaration(ty, name));
            at = offset + ty.size(8).unwrap_or(0);
        }
        let _ = writeln!(out, "}};");
    }
    out
}
