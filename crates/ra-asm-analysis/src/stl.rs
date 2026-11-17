//! The standard library a program inlines, recognized by the names its build gives the template
//! code: a member function of `std::vector<unsigned int>` or of the tree a `std::map` is, an
//! iterator's, an algorithm's. A compiler instantiates the containers into the program, so the
//! symbols (the PDB, the link map, a signature match) carry the container's whole type in their
//! decorated names, MSVC's or the Itanium ABI's; [`member`] reads one back as the container it
//! belongs to, under the name the source wrote it with (`std::string`, `std::map<std::string, int>`)
//! rather than the library's internals (`std::_Tree<std::_Tmap_traits<…>>`).
//!
//! The layout of a container is the library's, and it changes with the library's version:
//! [`Library::of`] tells which one an image's toolset links, and [`layout`] gives the fields each
//! container has there, `std::string`'s small buffer and its heap pointer sharing one place,
//! `std::vector`'s three pointers, the head and the count of a tree or a list.

use ra_asm_pe::{CxxAbi, Toolset};

/// A container of the standard library, or the iterator of one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    String,
    Vector,
    List,
    /// `std::map` and `std::multimap`: a red-black tree of pairs.
    Map,
    /// `std::set` and `std::multiset`: a red-black tree of keys.
    Set,
    UnorderedMap,
    UnorderedSet,
    /// The iterator of a contiguous container: a pointer to the element.
    Iterator,
    /// The iterator of a list or a tree: a pointer to the node.
    NodeIterator,
}

/// The container a function of the standard library is a member function of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub kind: Kind,
    /// The container as the source names it: `std::vector<unsigned int>`, `std::string`.
    pub container: String,
    /// The member function's name: `push_back`; `ctor` and `dtor` for the structors and
    /// `operator_<name>` for an operator, as the symbols' other names read.
    pub method: String,
}

/// The standard library a toolset links, whose containers the program inlines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Library {
    /// Microsoft's, from Visual Studio 2012 on.
    Msvc,
    /// Microsoft's of Visual Studio 2008 and 2010, whose `std::string` keeps its allocator as a
    /// member of its own ahead of the characters.
    Msvc2010,
    /// GCC's libstdc++ under the C++11 ABI (GCC 5 on), whose `std::string` has a small buffer too.
    Libstdcxx,
}

impl Library {
    /// The library of the toolset that built an image: libstdc++ under the Itanium ABI, else
    /// Microsoft's by the compiler's version; clang under the Microsoft ABI links Microsoft's.
    pub fn of(toolset: &Toolset) -> Self {
        if toolset.cxx_abi() == CxxAbi::Itanium {
            return Self::Libstdcxx;
        }
        // Before Visual Studio 2015 the version is the compiler's: 15 for 2008, 16 for 2010.
        match toolset.msvc {
            Some((15 | 16, _)) => Self::Msvc2010,
            _ => Self::Msvc,
        }
    }

    /// The library as a person names it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Msvc => "Microsoft's",
            Self::Msvc2010 => "Microsoft's of Visual Studio 2010",
            Self::Libstdcxx => "libstdc++'s",
        }
    }
}

/// What a field of a container holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// An address, as wide as the program's pointers.
    Pointer,
    /// A count, as wide as the program's pointers.
    Size,
    /// A 32-bit integer.
    Int,
    Float,
    /// Characters in place: the small buffer of a string.
    Chars(u32),
    /// Two ways of reading one place, each with its name.
    Union(&'static [(&'static str, FieldType)]),
}

/// A field of a container's layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    /// The library's own name for it.
    pub name: &'static str,
    pub offset: u32,
    pub ty: FieldType,
    /// What it holds, as a comment says it.
    pub role: &'static str,
}

const fn field(name: &'static str, offset: u32, ty: FieldType, role: &'static str) -> Field {
    Field { name, offset, ty, role }
}

/// The fields of a container of `kind` in `library`, for a program whose pointers are `pointer`
/// bytes wide; `None` where the library's layout is not known here (the node containers of
/// Visual Studio 2010, which hold their allocator and their comparator among the fields).
pub fn layout(kind: Kind, library: Library, pointer: u32) -> Option<Vec<Field>> {
    use FieldType::{Chars, Float, Int, Pointer, Size, Union};
    let p = pointer;
    Some(match (library, kind) {
        (Library::Msvc, Kind::String) => vec![
            field(
                "_Bx",
                0,
                Union(&[("_Buf", Chars(16)), ("_Ptr", Pointer)]),
                "the characters in place, or the heap's once there are 16 or more",
            ),
            field("_Mysize", 16, Size, "the length"),
            field("_Myres", 16 + p, Size, "the capacity: 15 while the characters are in place"),
        ],
        (Library::Msvc2010, Kind::String) => vec![
            field("_Alval", 0, Pointer, "the allocator, empty"),
            field(
                "_Bx",
                p,
                Union(&[("_Buf", Chars(16)), ("_Ptr", Pointer)]),
                "the characters in place, or the heap's once there are 16 or more",
            ),
            field("_Mysize", p + 16, Size, "the length"),
            field("_Myres", 2 * p + 16, Size, "the capacity: 15 while the characters are in place"),
        ],
        (Library::Msvc, Kind::Vector) => vec![
            field("_Myfirst", 0, Pointer, "the first element"),
            field("_Mylast", p, Pointer, "past the last element"),
            field("_Myend", 2 * p, Pointer, "past the storage"),
        ],
        (Library::Msvc, Kind::List | Kind::Map | Kind::Set) => {
            vec![field("_Myhead", 0, Pointer, "the sentinel node"), field("_Mysize", p, Size, "the number of elements")]
        }
        (Library::Msvc, Kind::UnorderedMap | Kind::UnorderedSet) => vec![
            field("_Max_bucket_size", 0, Float, "the most elements a bucket holds on average"),
            field("_List_head", p, Pointer, "the sentinel node of the list of every element"),
            field("_List_size", 2 * p, Size, "the number of elements"),
            field("_Vec_first", 3 * p, Pointer, "the buckets: the first and the last node of each"),
            field("_Vec_last", 4 * p, Pointer, "past the last bucket"),
            field("_Vec_end", 5 * p, Pointer, "past the buckets' storage"),
            field("_Mask", 6 * p, Size, "the number of buckets less one"),
            field("_Maxidx", 7 * p, Size, "the number of buckets"),
        ],
        (Library::Msvc | Library::Msvc2010, Kind::Iterator | Kind::NodeIterator) => {
            vec![field("_Ptr", 0, Pointer, "the element or the node")]
        }
        (Library::Libstdcxx, Kind::String) => vec![
            field("_M_p", 0, Pointer, "the characters: the small buffer's own or the heap's"),
            field("_M_string_length", p, Size, "the length"),
            field(
                "_M_local",
                2 * p,
                Union(&[("_M_local_buf", Chars(16)), ("_M_allocated_capacity", Size)]),
                "the characters in place, or the capacity once they are on the heap",
            ),
        ],
        (Library::Libstdcxx, Kind::Vector) => vec![
            field("_M_start", 0, Pointer, "the first element"),
            field("_M_finish", p, Pointer, "past the last element"),
            field("_M_end_of_storage", 2 * p, Pointer, "past the storage"),
        ],
        (Library::Libstdcxx, Kind::List) => vec![
            field("_M_next", 0, Pointer, "the first node"),
            field("_M_prev", p, Pointer, "the last node"),
            field("_M_size", 2 * p, Size, "the number of elements"),
        ],
        // The comparator, empty, takes the first word.
        (Library::Libstdcxx, Kind::Map | Kind::Set) => vec![
            field("_M_color", p, Int, "the header's color"),
            field("_M_parent", 2 * p, Pointer, "the root"),
            field("_M_left", 3 * p, Pointer, "the leftmost node"),
            field("_M_right", 4 * p, Pointer, "the rightmost node"),
            field("_M_node_count", 5 * p, Size, "the number of elements"),
        ],
        (Library::Libstdcxx, Kind::UnorderedMap | Kind::UnorderedSet) => vec![
            field("_M_buckets", 0, Pointer, "the buckets"),
            field("_M_bucket_count", p, Size, "the number of buckets"),
            field("_M_before_begin", 2 * p, Pointer, "the first node"),
            field("_M_element_count", 3 * p, Size, "the number of elements"),
            field("_M_max_load_factor", 4 * p, Float, "the most elements a bucket holds on average"),
            field("_M_next_resize", 5 * p, Size, "the count that grows the buckets next"),
            field("_M_single_bucket", 6 * p, Pointer, "the bucket of a table of one"),
        ],
        (Library::Libstdcxx, Kind::Iterator) => vec![field("_M_current", 0, Pointer, "the element")],
        (Library::Libstdcxx, Kind::NodeIterator) => vec![field("_M_node", 0, Pointer, "the node")],
        (Library::Msvc2010, _) => return None,
    })
}

/// The container `symbol` makes its function a member function of: `?insert@?$vector@IV?$allocator@I@std@@@std@@…`
/// is `std::vector<unsigned int>`'s `insert`, `_ZNSt6vectorIjSaIjEE9push_backERKj` its
/// `push_back`. `None` for any other function, a global one among them.
pub fn member(symbol: &str) -> Option<Member> {
    let (scope, method) = if let Some(rest) = symbol.strip_prefix('?') {
        let mut r = Msvc::new(rest);
        let method = r.leaf()?.0;
        let scope = r.qualified()?;
        // A member function's access code: `A` to `X`; `Y` and `Z` are a global function's.
        if !matches!(r.peek()?, b'A'..=b'X') {
            return None;
        }
        (scope, method)
    } else {
        Itanium::member(symbol)?
    };
    let (kind, container) = standard(&scope)?;
    Some(Member { kind, container, method })
}

/// The algorithm of the standard library `symbol` names, with what it is instantiated with:
/// `std::_Sort_unchecked<std::string *, std::less<void>>`; `None` for any other function.
pub fn algorithm(symbol: &str) -> Option<String> {
    if let Some(rest) = symbol.strip_prefix('?') {
        let mut r = Msvc::new(rest);
        let (name, args) = r.leaf()?;
        let scope = r.qualified()?;
        if r.peek()? != b'Y' || !(scope.len() == 1 && scope[0].ident == "std") {
            return None;
        }
        return Some(render(&[scope[0].clone(), Fragment { ident: name, args }]));
    }
    let mut r = Itanium { s: symbol.strip_prefix("_ZSt")?.as_bytes() };
    let fragment = r.fragment()?;
    if fragment.ident.starts_with("operator") {
        return None;
    }
    Some(render(&[Fragment::simple("std"), fragment]))
}

/// Whether an MSVC decorated name is a variable's rather than a function's: a static data member
/// or a global (`?_Id_cnt@id@locale@std@@0HA`, `?id@?$ctype@D@std@@2V0locale@2@A`), whose code
/// after the qualified name is a digit where a function's is a letter.
pub fn is_variable(symbol: &str) -> bool {
    let Some(rest) = symbol.strip_prefix('?') else { return false };
    let mut r = Msvc::new(rest);
    r.leaf().is_some() && r.qualified().is_some() && matches!(r.peek(), Some(b'0'..=b'4'))
}

/// The source name of the function of the standard library `symbol` names: a container's member
/// function as `std::vector<unsigned int>::insert`, an algorithm with what it is instantiated
/// with; `None` for any other function.
pub fn source_name(symbol: &str) -> Option<String> {
    match member(symbol) {
        Some(m) => Some(format!("{}::{}", m.container, m.method)),
        None => algorithm(symbol),
    }
}

/// One part of a qualified name, with the arguments of a template.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Fragment {
    ident: String,
    args: Vec<Arg>,
}

impl Fragment {
    fn simple(ident: &str) -> Self {
        Fragment { ident: ident.to_owned(), args: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Arg {
    Type(Ty),
    Int(i64),
}

impl Arg {
    fn ty(&self) -> Option<&Ty> {
        match self {
            Arg::Type(ty) => Some(ty),
            Arg::Int(_) => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Ty {
    Builtin(&'static str),
    /// A class, struct, union or enum, outermost scope first.
    Class(Vec<Fragment>),
    Pointer(Box<Ty>),
    Ref(Box<Ty>),
    RvalueRef(Box<Ty>),
    Const(Box<Ty>),
    /// A type the reader does not follow: an Itanium substitution, a function type.
    Unknown,
}

impl Ty {
    /// The class's name, outermost first; `None` for any other type.
    fn class(&self) -> Option<&[Fragment]> {
        match self {
            Ty::Class(parts) => Some(parts),
            _ => None,
        }
    }

    fn unconst(&self) -> &Ty {
        match self {
            Ty::Const(inner) => inner,
            other => other,
        }
    }
}

/// The container of the standard library a class is, by its name outermost first, and the name
/// the source gives it.
fn standard(parts: &[Fragment]) -> Option<(Kind, String)> {
    let (last, scope) = parts.split_last()?;
    let scope: Vec<&str> = scope.iter().map(|f| f.ident.as_str()).collect();
    if !matches!(scope.as_slice(), ["std" | "__gnu_cxx"] | ["std", "__cxx11"]) {
        return None;
    }
    let arg = |i: usize| last.args.get(i).and_then(Arg::ty).filter(|t| **t != Ty::Unknown);
    let of = |kind: Kind, template: &str, ty: &Ty| Some((kind, format!("std::{template}<{}>", render_ty(ty))));
    match last.ident.as_str() {
        "basic_string" => Some(string(arg(0)?)),
        "_String_val" => Some(string(simple_type(arg(0)?)?)),
        "vector" | "_Vector_base" => of(Kind::Vector, "vector", arg(0)?),
        "_Vector_val" => of(Kind::Vector, "vector", simple_type(arg(0)?)?),
        "list" | "_List_base" if !scope.contains(&"__gnu_cxx") => of(Kind::List, "list", arg(0)?),
        "_List_val" => of(Kind::List, "list", simple_type(arg(0)?)?),
        "map" | "multimap" | "unordered_map" | "unordered_multimap" => {
            let kind = if last.ident.starts_with("unordered") { Kind::UnorderedMap } else { Kind::Map };
            Some((kind, format!("std::{}<{}, {}>", last.ident, render_ty(arg(0)?), render_ty(arg(1)?))))
        }
        "set" | "multiset" | "unordered_set" | "unordered_multiset" => {
            let kind = if last.ident.starts_with("unordered") { Kind::UnorderedSet } else { Kind::Set };
            Some((kind, format!("std::{}<{}>", last.ident, render_ty(arg(0)?))))
        }
        // Microsoft's trees and hash tables take their key, their value and whether a key may
        // repeat from their traits.
        "_Tree" | "_Hash" => {
            let traits = arg(0)?.class()?.last()?;
            let targ = |i: usize| traits.args.get(i).and_then(Arg::ty).filter(|t| **t != Ty::Unknown);
            let flag = |i: usize| matches!(traits.args.get(i), Some(Arg::Int(n)) if *n != 0);
            let hashed = last.ident == "_Hash";
            match traits.ident.as_str() {
                "_Tmap_traits" | "_Umap_traits" => {
                    let multi = if flag(4) { "multi" } else { "" };
                    let (kind, template) =
                        if hashed { (Kind::UnorderedMap, format!("unordered_{multi}map")) } else { (Kind::Map, format!("{multi}map")) };
                    Some((kind, format!("std::{template}<{}, {}>", render_ty(targ(0)?), render_ty(targ(1)?))))
                }
                "_Tset_traits" | "_Uset_traits" => {
                    let multi = if flag(3) { "multi" } else { "" };
                    let (kind, template) =
                        if hashed { (Kind::UnorderedSet, format!("unordered_{multi}set")) } else { (Kind::Set, format!("{multi}set")) };
                    Some((kind, format!("std::{template}<{}>", render_ty(targ(0)?))))
                }
                _ => None,
            }
        }
        "_Tree_val" => tree(simple_type(arg(0)?)?, false),
        // libstdc++'s tree and hash table hold their value type second: a pair of a map's.
        "_Rb_tree" => tree(arg(1)?, false),
        "_Hashtable" => tree(arg(1)?, true),
        "_Vector_iterator" | "_Vector_const_iterator" | "_String_iterator" | "_String_const_iterator" => {
            let container = standard(arg(0)?.class()?)?.1;
            Some((Kind::Iterator, iterator(&container, &last.ident)))
        }
        "_Tree_iterator"
        | "_Tree_const_iterator"
        | "_Tree_unchecked_iterator"
        | "_Tree_unchecked_const_iterator"
        | "_List_iterator"
        | "_List_const_iterator"
        | "_List_unchecked_iterator"
        | "_List_unchecked_const_iterator" => {
            // Microsoft's iterate a container's value holder; libstdc++'s list iterator its element.
            let container = match arg(0)?.class().and_then(standard) {
                Some((_, container)) => container,
                None if last.ident.starts_with("_List") => format!("std::list<{}>", render_ty(arg(0)?)),
                None => return None,
            };
            Some((Kind::NodeIterator, iterator(&container, &last.ident)))
        }
        "_Rb_tree_iterator" | "_Rb_tree_const_iterator" => {
            let container = tree(arg(0)?, false)?.1;
            Some((Kind::NodeIterator, iterator(&container, &last.ident)))
        }
        "__normal_iterator" => {
            let container = standard(arg(1)?.class()?)?.1;
            let constant = matches!(arg(0)?, Ty::Pointer(p) if matches!(**p, Ty::Const(_)));
            Some((Kind::Iterator, iterator(&container, if constant { "const_iterator" } else { "iterator" })))
        }
        _ => None,
    }
}

/// `std::string` for a string of `char`, `std::wstring` of `wchar_t`.
fn string(element: &Ty) -> (Kind, String) {
    let name = match element {
        Ty::Builtin("char") => "std::string".to_owned(),
        Ty::Builtin("wchar_t") => "std::wstring".to_owned(),
        other => format!("std::basic_string<{}>", render_ty(other)),
    };
    (Kind::String, name)
}

/// The element of Microsoft's `_Simple_types<T>`, `_List_simple_types<T>`, `_Tree_simple_types<T>`.
fn simple_type(ty: &Ty) -> Option<&Ty> {
    let last = ty.class()?.last()?;
    if !last.ident.ends_with("imple_types") {
        return None;
    }
    last.args.first()?.ty()
}

/// The tree or the hash table of `value`: a map's when it is a `std::pair<const K, V>`, else a set's.
fn tree(value: &Ty, hashed: bool) -> Option<(Kind, String)> {
    if let Some(parts) = value.class()
        && let Some(pair) = parts.last()
        && pair.ident == "pair"
        && let [Arg::Type(Ty::Const(key)), Arg::Type(mapped)] = pair.args.as_slice()
    {
        let (kind, template) = if hashed { (Kind::UnorderedMap, "unordered_map") } else { (Kind::Map, "map") };
        return Some((kind, format!("std::{template}<{}, {}>", render_ty(key), render_ty(mapped))));
    }
    if *value == Ty::Unknown {
        return None;
    }
    let (kind, template) = if hashed { (Kind::UnorderedSet, "unordered_set") } else { (Kind::Set, "set") };
    Some((kind, format!("std::{template}<{}>", render_ty(value.unconst()))))
}

/// The iterator of `container` an iterator class of the library stands for: `const_iterator`
/// when its name says it reads only.
fn iterator(container: &str, class: &str) -> String {
    if class.contains("const") { format!("{container}::const_iterator") } else { format!("{container}::iterator") }
}

/// A qualified name as the source writes it: a container of the standard library by its own name.
fn render(parts: &[Fragment]) -> String {
    if let Some((_, name)) = standard(parts) {
        return name;
    }
    let parts: Vec<String> = parts
        .iter()
        .map(|f| {
            if f.args.is_empty() {
                return f.ident.clone();
            }
            let args: Vec<String> = f
                .args
                .iter()
                .map(|a| match a {
                    Arg::Type(ty) => render_ty(ty),
                    Arg::Int(n) => n.to_string(),
                })
                .collect();
            format!("{}<{}>", f.ident, args.join(", "))
        })
        .collect();
    parts.join("::")
}

fn render_ty(ty: &Ty) -> String {
    match ty {
        Ty::Builtin(name) => (*name).to_owned(),
        Ty::Class(parts) => render(parts),
        Ty::Pointer(inner) => format!("{} *", render_ty(inner)),
        Ty::Ref(inner) => format!("{} &", render_ty(inner)),
        Ty::RvalueRef(inner) => format!("{} &&", render_ty(inner)),
        Ty::Const(inner) => format!("const {}", render_ty(inner)),
        Ty::Unknown => "?".to_owned(),
    }
}

/// A reader of an MSVC decorated name, past its leading `?`, as far as the qualified name.
struct Msvc<'a> {
    s: &'a [u8],
    /// The names a digit refers back to, in the template argument list being read.
    names: Vec<Fragment>,
}

impl<'a> Msvc<'a> {
    fn new(s: &'a str) -> Self {
        Msvc { s: s.as_bytes(), names: Vec::new() }
    }

    fn peek(&self) -> Option<u8> {
        self.s.first().copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let (&c, rest) = self.s.split_first()?;
        self.s = rest;
        Some(c)
    }

    fn eat(&mut self, c: u8) -> bool {
        let found = self.peek() == Some(c);
        if found {
            self.s = &self.s[1..];
        }
        found
    }

    fn memorize(&mut self, fragment: &Fragment) {
        if self.names.len() < 10 && !self.names.contains(fragment) {
            self.names.push(fragment.clone());
        }
    }

    /// An identifier up to its `@`.
    fn identifier(&mut self) -> Option<String> {
        let end = self.s.iter().position(|&c| c == b'@').filter(|&end| end > 0)?;
        let ident = std::str::from_utf8(&self.s[..end]).ok()?.to_owned();
        self.s = &self.s[end + 1..];
        Some(ident)
    }

    /// The function's own name: an identifier, a member template's name and arguments, or the
    /// code of a structor or an operator. A template's or a structor's is no name a digit refers to.
    fn leaf(&mut self) -> Option<(String, Vec<Arg>)> {
        if !self.eat(b'?') {
            let ident = self.identifier()?;
            self.memorize(&Fragment::simple(&ident));
            return Some((ident, Vec::new()));
        }
        if self.eat(b'$') {
            let template = self.template()?;
            return Some((template.ident, template.args));
        }
        let code = match self.bump()? {
            b'_' => [b'_', self.bump()?],
            c => [c, 0],
        };
        let name = match &code {
            b"0\0" => "ctor",
            b"1\0" => "dtor",
            b"4\0" => "operator_assign",
            b"5\0" => "operator_shr",
            b"6\0" => "operator_shl",
            b"8\0" => "operator_eq",
            b"9\0" => "operator_ne",
            b"A\0" => "operator_index",
            b"C\0" => "operator_arrow",
            b"D\0" => "operator_deref",
            b"E\0" => "operator_inc",
            b"F\0" => "operator_dec",
            b"G\0" => "operator_sub",
            b"H\0" => "operator_add",
            b"M\0" => "operator_lt",
            b"R\0" => "operator_call",
            b"Y\0" => "operator_add_assign",
            b"_G" => "scalar_deleting_dtor",
            b"_E" => "vector_deleting_dtor",
            _ => return None,
        };
        Some((name.to_owned(), Vec::new()))
    }

    /// A qualified name up to its closing `@`, outermost first.
    fn qualified(&mut self) -> Option<Vec<Fragment>> {
        let mut parts = Vec::new();
        while !self.eat(b'@') {
            parts.push(self.fragment()?);
        }
        parts.reverse();
        Some(parts)
    }

    /// One fragment of a qualified name: a back reference, a template or an identifier, each of
    /// the last two a name later digits refer to.
    fn fragment(&mut self) -> Option<Fragment> {
        match self.peek()? {
            c @ b'0'..=b'9' => {
                self.bump();
                self.names.get(usize::from(c - b'0')).cloned()
            }
            b'?' if self.s.get(1) == Some(&b'$') => {
                self.s = &self.s[2..];
                let template = self.template()?;
                self.memorize(&template);
                Some(template)
            }
            // An anonymous namespace, a numbered scope or a nested symbol: a lambda's.
            b'?' => None,
            _ => {
                let fragment = Fragment::simple(&self.identifier()?);
                self.memorize(&fragment);
                Some(fragment)
            }
        }
    }

    /// A template's name and arguments, past its `?$`: the arguments refer back to names of
    /// their own.
    fn template(&mut self) -> Option<Fragment> {
        let outer = std::mem::take(&mut self.names);
        let ident = self.identifier()?;
        self.memorize(&Fragment::simple(&ident));
        let mut args = Vec::new();
        while !self.eat(b'@') {
            if self.s.starts_with(b"$0") {
                self.s = &self.s[2..];
                args.push(Arg::Int(self.number()?));
            } else if self.s.starts_with(b"$$V") || self.s.starts_with(b"$$Z") {
                // An empty parameter pack.
                self.s = &self.s[3..];
            } else {
                args.push(Arg::Type(self.ty()?));
            }
        }
        self.names = outer;
        Some(Fragment { ident, args })
    }

    /// An encoded number: a digit for one to ten, else hexadecimal in `A` to `P` up to an `@`.
    fn number(&mut self) -> Option<i64> {
        let negative = self.eat(b'?');
        let first = self.peek()?;
        let value = if first.is_ascii_digit() {
            self.bump();
            i64::from(first - b'0') + 1
        } else {
            let mut value = 0i64;
            loop {
                let c = self.bump()?;
                if c == b'@' {
                    break;
                }
                if !(b'A'..=b'P').contains(&c) {
                    return None;
                }
                value = value.checked_mul(16)? + i64::from(c - b'A');
            }
            value
        };
        Some(if negative { -value } else { value })
    }

    /// A type.
    fn ty(&mut self) -> Option<Ty> {
        let c = self.bump()?;
        Some(match c {
            b'T' | b'U' | b'V' => Ty::Class(self.qualified()?),
            b'W' => {
                if self.bump()? != b'4' {
                    return None;
                }
                Ty::Class(self.qualified()?)
            }
            // A pointer or a reference: the qualifiers of what it points at, then that.
            b'P' | b'Q' | b'R' | b'S' | b'A' | b'B' => {
                self.eat(b'E');
                self.eat(b'I');
                let pointee = self.qualified_type()?;
                if matches!(c, b'A' | b'B') { Ty::Ref(Box::new(pointee)) } else { Ty::Pointer(Box::new(pointee)) }
            }
            b'$' => {
                if !self.eat(b'$') {
                    return None;
                }
                match self.bump()? {
                    b'C' => self.qualified_type()?,
                    b'Q' | b'R' => {
                        self.eat(b'E');
                        Ty::RvalueRef(Box::new(self.qualified_type()?))
                    }
                    _ => return None,
                }
            }
            b'_' => Ty::Builtin(match self.bump()? {
                b'J' => "__int64",
                b'K' => "unsigned __int64",
                b'N' => "bool",
                b'W' => "wchar_t",
                b'S' => "char16_t",
                b'U' => "char32_t",
                b'Q' => "char8_t",
                _ => return None,
            }),
            other => Ty::Builtin(match other {
                b'C' => "signed char",
                b'D' => "char",
                b'E' => "unsigned char",
                b'F' => "short",
                b'G' => "unsigned short",
                b'H' => "int",
                b'I' => "unsigned int",
                b'J' => "long",
                b'K' => "unsigned long",
                b'M' => "float",
                b'N' => "double",
                b'O' => "long double",
                b'X' => "void",
                _ => return None,
            }),
        })
    }

    /// A type after its qualifiers: `A` none, `B` const, `C` volatile, `D` both.
    fn qualified_type(&mut self) -> Option<Ty> {
        let cv = self.bump()?;
        if !matches!(cv, b'A'..=b'D') {
            return None;
        }
        let ty = self.ty()?;
        Some(if matches!(cv, b'B' | b'D') { Ty::Const(Box::new(ty)) } else { ty })
    }
}

/// A reader of an Itanium ABI name as far as its nested name. A substitution (`S_`, `S1_`) or a
/// template parameter reads as an unknown type, which no container is recognized by.
struct Itanium<'a> {
    s: &'a [u8],
}

impl Itanium<'_> {
    /// The scope and the name of a member function: `_ZN`, its qualifiers, then the nested name.
    fn member(symbol: &str) -> Option<(Vec<Fragment>, String)> {
        let mut r = Itanium { s: symbol.strip_prefix("_ZN")?.as_bytes() };
        while matches!(r.peek(), Some(b'r' | b'V' | b'K' | b'R' | b'O')) {
            r.bump();
        }
        let mut parts = r.nested()?;
        let method = parts.pop()?.ident;
        Some((parts, method))
    }

    fn peek(&self) -> Option<u8> {
        self.s.first().copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let (&c, rest) = self.s.split_first()?;
        self.s = rest;
        Some(c)
    }

    fn eat(&mut self, prefix: &[u8]) -> bool {
        let found = self.s.starts_with(prefix);
        if found {
            self.s = &self.s[prefix.len()..];
        }
        found
    }

    /// The parts of a nested name up to its `E`, outermost first.
    fn nested(&mut self) -> Option<Vec<Fragment>> {
        let mut parts = Vec::new();
        if self.eat(b"St") {
            parts.push(Fragment::simple("std"));
        }
        while !self.eat(b"E") {
            parts.push(self.fragment()?);
        }
        Some(parts)
    }

    /// A source name, a structor or an operator, with its template arguments.
    fn fragment(&mut self) -> Option<Fragment> {
        let ident = match self.peek()? {
            b'0'..=b'9' => {
                let mut length = 0usize;
                while let Some(c @ b'0'..=b'9') = self.peek() {
                    self.bump();
                    length = length.checked_mul(10)? + usize::from(c - b'0');
                }
                if length == 0 || length > self.s.len() {
                    return None;
                }
                let ident = std::str::from_utf8(&self.s[..length]).ok()?.to_owned();
                self.s = &self.s[length..];
                ident
            }
            b'C' => {
                self.bump();
                self.bump()?;
                "ctor".to_owned()
            }
            b'D' => {
                self.bump();
                self.bump()?;
                "dtor".to_owned()
            }
            b'a'..=b'z' => {
                let code = [self.bump()?, self.bump()?];
                let name = match &code {
                    b"ix" => "operator_index",
                    b"aS" => "operator_assign",
                    b"pL" => "operator_add_assign",
                    b"pp" => "operator_inc",
                    b"mm" => "operator_dec",
                    b"de" => "operator_deref",
                    b"pt" => "operator_arrow",
                    b"eq" => "operator_eq",
                    b"ne" => "operator_ne",
                    b"lt" => "operator_lt",
                    b"cl" => "operator_call",
                    _ => return None,
                };
                name.to_owned()
            }
            _ => return None,
        };
        let args = if self.eat(b"I") { self.arguments()? } else { Vec::new() };
        Some(Fragment { ident, args })
    }

    /// Template arguments up to their `E`.
    fn arguments(&mut self) -> Option<Vec<Arg>> {
        let mut args = Vec::new();
        while !self.eat(b"E") {
            if self.eat(b"L") {
                // A literal: its type, its digits, `E`.
                self.ty()?;
                let negative = self.eat(b"n");
                let mut value = 0i64;
                while let Some(c @ b'0'..=b'9') = self.peek() {
                    self.bump();
                    value = value.checked_mul(10)? + i64::from(c - b'0');
                }
                if !self.eat(b"E") {
                    return None;
                }
                args.push(Arg::Int(if negative { -value } else { value }));
            } else {
                args.push(Arg::Type(self.ty()?));
            }
        }
        Some(args)
    }

    fn ty(&mut self) -> Option<Ty> {
        let c = self.peek()?;
        let std = |name: &str, r: &mut Self| -> Option<Ty> {
            let args = if r.eat(b"I") { r.arguments()? } else { Vec::new() };
            Some(Ty::Class(vec![Fragment::simple("std"), Fragment { ident: name.to_owned(), args }]))
        };
        Some(match c {
            b'P' | b'R' | b'O' | b'K' => {
                self.bump();
                let inner = Box::new(self.ty()?);
                match c {
                    b'P' => Ty::Pointer(inner),
                    b'R' => Ty::Ref(inner),
                    b'O' => Ty::RvalueRef(inner),
                    _ => Ty::Const(inner),
                }
            }
            b'N' => {
                self.bump();
                Ty::Class(self.nested()?)
            }
            b'0'..=b'9' => Ty::Class(vec![self.fragment()?]),
            b'S' => {
                self.bump();
                match self.bump()? {
                    b't' => {
                        let mut fragment = self.fragment()?;
                        if fragment.args.is_empty() && self.eat(b"I") {
                            fragment.args = self.arguments()?;
                        }
                        Ty::Class(vec![Fragment::simple("std"), fragment])
                    }
                    b'a' => std("allocator", self)?,
                    b'b' => std("basic_string", self)?,
                    b's' => Ty::Class(vec![
                        Fragment::simple("std"),
                        Fragment {
                            ident: "basic_string".to_owned(),
                            args: vec![
                                Arg::Type(Ty::Builtin("char")),
                                Arg::Type(Ty::Class(vec![
                                    Fragment::simple("std"),
                                    Fragment { ident: "char_traits".to_owned(), args: vec![Arg::Type(Ty::Builtin("char"))] },
                                ])),
                            ],
                        },
                    ]),
                    // A substitution: what it refers to is not followed.
                    other => {
                        let mut c = other;
                        while c != b'_' {
                            if !c.is_ascii_alphanumeric() {
                                return None;
                            }
                            c = self.bump()?;
                        }
                        if self.eat(b"I") {
                            self.arguments()?;
                        }
                        Ty::Unknown
                    }
                }
            }
            b'T' => {
                self.bump();
                while self.bump()? != b'_' {}
                Ty::Unknown
            }
            _ => {
                self.bump();
                Ty::Builtin(match c {
                    b'v' => "void",
                    b'b' => "bool",
                    b'c' => "char",
                    b'a' => "signed char",
                    b'h' => "unsigned char",
                    b's' => "short",
                    b't' => "unsigned short",
                    b'i' => "int",
                    b'j' => "unsigned int",
                    b'l' => "long",
                    b'm' => "unsigned long",
                    b'x' => "long long",
                    b'y' => "unsigned long long",
                    b'f' => "float",
                    b'd' => "double",
                    b'e' => "long double",
                    b'w' => "wchar_t",
                    _ => return None,
                })
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{Kind, Library, algorithm, is_variable, layout, member, source_name};

    /// The imports of msvcp140 that are variables, apart from its functions.
    #[test]
    fn the_variables_of_the_library_read_apart_from_its_functions() {
        assert!(is_variable("?_Id_cnt@id@locale@std@@0HA"));
        assert!(is_variable("?id@?$ctype@D@std@@2V0locale@2@A"));
        assert!(!is_variable("?_Xlength_error@std@@YAXPEBD@Z"));
        assert!(!is_variable("?setbuf@?$basic_streambuf@DU?$char_traits@D@std@@@std@@MEAAPEAV12@PEAD_J@Z"));
        assert!(!is_variable("??0_Lockit@std@@QEAA@H@Z"));
        assert!(!is_variable("_Xlength_error"));
    }

    fn reads(symbol: &str) -> Option<(Kind, String, String)> {
        member(symbol).map(|m| (m.kind, m.container, m.method))
    }

    /// Member functions of the rhtest `stl` program, by the names its link map gives them.
    #[test]
    fn the_msvc_names_read_as_their_containers() {
        let cases: &[(&str, Kind, &str, &str)] = &[
            ("??1?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@QAE@XZ", Kind::String, "std::string", "dtor"),
            (
                "?insert@?$vector@IV?$allocator@I@std@@@std@@QAE?AV?$_Vector_iterator@V?$_Vector_val@U?$_Simple_types@I@std@@@std@@@2@V?$_Vector_const_iterator@V?$_Vector_val@U?$_Simple_types@I@std@@@std@@@2@IABI@Z",
                Kind::Vector,
                "std::vector<unsigned int>",
                "insert",
            ),
            ("?shrink_to_fit@?$vector@IV?$allocator@I@std@@@std@@QAEXXZ", Kind::Vector, "std::vector<unsigned int>", "shrink_to_fit"),
            ("?_Xlength@?$vector@IV?$allocator@I@std@@@std@@CAXXZ", Kind::Vector, "std::vector<unsigned int>", "_Xlength"),
            (
                "??$_Emplace_reallocate@I@?$vector@IV?$allocator@I@std@@@std@@AAEPAIQAI$$QAI@Z",
                Kind::Vector,
                "std::vector<unsigned int>",
                "_Emplace_reallocate",
            ),
            (
                "??1?$vector@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@V?$allocator@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@2@@std@@QAE@XZ",
                Kind::Vector,
                "std::vector<std::string>",
                "dtor",
            ),
            (
                "?count@?$_Tree@V?$_Tmap_traits@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@HU?$less@V?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@@2@V?$allocator@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@H@std@@@2@$0A@@std@@@std@@QBEIABV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@2@@Z",
                Kind::Map,
                "std::map<std::string, int>",
                "count",
            ),
            (
                "?_Insert_node@?$_Tree_val@U?$_Tree_simple_types@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@H@std@@@std@@@std@@QAEPAU?$_Tree_node@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@H@std@@PAX@2@U?$_Tree_id@PAU?$_Tree_node@U?$pair@$$CBV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@H@std@@PAX@std@@@2@QAU32@@Z",
                Kind::Map,
                "std::map<std::string, int>",
                "_Insert_node",
            ),
            (
                "??D?$_Vector_const_iterator@V?$_Vector_val@U?$_Simple_types@I@std@@@std@@@std@@QBEABIXZ",
                Kind::Iterator,
                "std::vector<unsigned int>::const_iterator",
                "operator_deref",
            ),
        ];
        for &(symbol, kind, container, method) in cases {
            assert_eq!(reads(symbol), Some((kind, container.to_owned(), method.to_owned())), "{symbol}");
        }
        // A global function, a class of the program and the library's streams are no container's.
        for symbol in [
            "?_Xlength_error@std@@YAXPBD@Z",
            "?area@Rect@@UBEIXZ",
            "??0?$basic_istringstream@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@QAE@ABV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@1@H@Z",
            "?_Id_cnt@id@locale@std@@0HA",
        ] {
            assert_eq!(reads(symbol), None, "{symbol}");
        }
    }

    #[test]
    fn the_itanium_names_read_as_their_containers() {
        let cases: &[(&str, Kind, &str, &str)] = &[
            ("_ZNSt6vectorIjSaIjEE9push_backERKj", Kind::Vector, "std::vector<unsigned int>", "push_back"),
            ("_ZNKSt6vectorIiSaIiEE4sizeEv", Kind::Vector, "std::vector<int>", "size"),
            ("_ZNSt12_Vector_baseIjSaIjEE11_M_allocateEj", Kind::Vector, "std::vector<unsigned int>", "_M_allocate"),
            ("_ZNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEE6appendEPKc", Kind::String, "std::string", "append"),
            ("_ZNSt7__cxx1112basic_stringIcSt11char_traitsIcESaIcEEC2EPKcRKS3_", Kind::String, "std::string", "ctor"),
            (
                "_ZNSt8_Rb_treeIiSt4pairIKiiESt10_Select1stIS2_ESt4lessIiESaIS2_EE8_M_eraseEPSt13_Rb_tree_nodeIS2_E",
                Kind::Map,
                "std::map<int, int>",
                "_M_erase",
            ),
            ("_ZNSt3mapIiiSt4lessIiESaISt4pairIKiiEEEixERS3_", Kind::Map, "std::map<int, int>", "operator_index"),
        ];
        for &(symbol, kind, container, method) in cases {
            assert_eq!(reads(symbol), Some((kind, container.to_owned(), method.to_owned())), "{symbol}");
        }
        assert_eq!(reads("_ZN4Rect4areaEv"), None);
        assert_eq!(reads("_ZSt4sortIPiEvT_S1_"), None);
    }

    #[test]
    fn the_algorithms_read_with_what_they_sort() {
        assert_eq!(
            algorithm(
                "??$_Sort_unchecked@PAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@std@@U?$less@X@2@@std@@YAXPAV?$basic_string@DU?$char_traits@D@std@@V?$allocator@D@2@@0@0HU?$less@X@0@@Z"
            )
            .as_deref(),
            Some("std::_Sort_unchecked<std::string *, std::less<void>>")
        );
        assert_eq!(algorithm("_ZSt16__introsort_loopIPjiEvT_S1_T0_").as_deref(), Some("std::__introsort_loop<unsigned int *, int>"));
        assert_eq!(algorithm("?shrink_to_fit@?$vector@IV?$allocator@I@std@@@std@@QAEXXZ"), None);
    }

    #[test]
    fn a_function_of_the_library_reads_under_its_container() {
        assert_eq!(
            source_name("?shrink_to_fit@?$vector@IV?$allocator@I@std@@@std@@QAEXXZ").as_deref(),
            Some("std::vector<unsigned int>::shrink_to_fit")
        );
        assert_eq!(source_name("_ZNSt6vectorIjSaIjEE9push_backERKj").as_deref(), Some("std::vector<unsigned int>::push_back"));
        assert_eq!(source_name("_ZSt16__introsort_loopIPjiEvT_S1_T0_").as_deref(), Some("std::__introsort_loop<unsigned int *, int>"));
        assert_eq!(source_name("?area@Rect@@UBEIXZ"), None);
    }

    /// The layouts the x86 builds of each library give: `std::string` 24 bytes in Microsoft's
    /// (28 in Visual Studio 2010's) and libstdc++'s, a vector 12, the 64-bit string 32.
    #[test]
    fn the_layouts_end_where_the_libraries_end() {
        let end = |kind, library, pointer| {
            layout(kind, library, pointer).map(|fields| fields.last().map(|f| f.offset + pointer).unwrap_or_default())
        };
        assert_eq!(end(Kind::String, Library::Msvc, 4), Some(24));
        assert_eq!(end(Kind::String, Library::Msvc, 8), Some(32));
        assert_eq!(end(Kind::String, Library::Msvc2010, 4), Some(28));
        assert_eq!(end(Kind::Vector, Library::Msvc, 4), Some(12));
        assert_eq!(end(Kind::Map, Library::Msvc, 8), Some(16));
        assert_eq!(end(Kind::UnorderedMap, Library::Msvc, 8), Some(64));
        assert_eq!(end(Kind::Map, Library::Libstdcxx, 8), Some(48));
        assert_eq!(end(Kind::UnorderedMap, Library::Libstdcxx, 8), Some(56));
        assert_eq!(end(Kind::Vector, Library::Msvc2010, 4), None);
    }
}
