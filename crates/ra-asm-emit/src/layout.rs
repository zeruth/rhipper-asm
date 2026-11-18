//! The typed view of a data section: the arrays, scalars and strings the code's uses say its
//! bytes are, laid over the lift's byte runs and address words. The IR keeps the bytes as they
//! are; a backend writes the section as these fields and reads it through them, so a table the
//! program indexes reads as an array and a string it passes reads as a string.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use ra_asm_ir::{BinOp, Cast, Const, Function, Global, Inst, Module, Operand, Type, ValueId};

use crate::common::Defs;

/// The flattened bytes and address words of a data section.
type FlatSection = (Vec<u8>, BTreeMap<u64, (Type, Const)>);

/// What a field's elements are.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Elem {
    /// Bytes no use types.
    Bytes,
    /// A NUL-terminated string the code addresses; the count includes the NUL.
    Char,
    /// An address word of the lift (a relocated pointer, an import slot), in the IR's type.
    Word(Type),
    /// An integer or floating-point value the code reads or writes there.
    Value(Type),
}

/// One field of the section's structure.
#[derive(Debug, Clone)]
pub(crate) struct Field {
    pub(crate) offset: u64,
    /// The element count; one for a scalar.
    pub(crate) count: u64,
    pub(crate) elem: Elem,
    /// Whether the field is an array: an indexed use said so, or it holds several elements.
    pub(crate) array: bool,
    /// The program's name for it, from the variable of the build's symbols it is or lies in.
    pub(crate) name: Option<String>,
}

impl Field {
    /// The field's name: the program's variable's, else by its offset, as a struct's fields are
    /// named.
    pub(crate) fn name(&self) -> String {
        self.name.clone().unwrap_or_else(|| format!("f{}", self.offset))
    }

    /// The IR type of one element, or `None` for untyped bytes.
    pub(crate) fn elem_type(&self) -> Option<Type> {
        match &self.elem {
            Elem::Bytes => None,
            Elem::Char => Some(Type::I8),
            Elem::Word(ty) | Elem::Value(ty) => Some(ty.clone()),
        }
    }

    pub(crate) fn elem_size(&self) -> u64 {
        self.elem_type().and_then(|t| t.size(8)).unwrap_or(1)
    }

    pub(crate) fn end(&self) -> u64 {
        self.offset + self.count * self.elem_size()
    }
}

/// A section as typed fields, with the bytes and the address words they are read from.
#[derive(Debug, Clone)]
pub(crate) struct Layout {
    pub(crate) fields: Vec<Field>,
    /// The section's bytes, zero where an address word stands.
    pub(crate) bytes: Vec<u8>,
    /// The address words by offset, with the IR's type and constant for each.
    pub(crate) words: BTreeMap<u64, (Type, Const)>,
    /// The section is read-only: a string of it reads as the literal the source wrote.
    pub(crate) constant: bool,
}

impl Layout {
    /// The C literal of the string field that starts at `offset` of a read-only section.
    pub(crate) fn literal_at(&self, offset: u64) -> Option<String> {
        if !self.constant {
            return None;
        }
        let field = self.fields.iter().find(|f| f.offset == offset && f.elem == Elem::Char)?;
        let text = &self.bytes[field.offset as usize..(field.end() - 1) as usize];
        Some(string_literal(text))
    }

    /// The field that starts at `offset`.
    pub(crate) fn field_starting(&self, offset: u64) -> Option<&Field> {
        self.fields.iter().find(|f| f.offset == offset)
    }
    /// The field that holds the byte at `offset`.
    pub(crate) fn field_at(&self, offset: u64) -> Option<&Field> {
        self.fields.iter().find(|f| offset >= f.offset && offset < f.end())
    }

    /// The bytes of element `i` of `field`.
    pub(crate) fn element(&self, field: &Field, i: u64) -> &[u8] {
        let size = field.elem_size();
        let at = (field.offset + i * size) as usize;
        &self.bytes[at..at + size as usize]
    }
}

/// Where a pointer operand points: a byte of a data global, by a constant offset and, for an
/// indexed access, the index and its scale.
pub(crate) struct Place {
    pub(crate) global: String,
    pub(crate) offset: i64,
    pub(crate) index: Option<Index>,
}

/// The index of an indexed access: `base + value * scale`, `value` in `ty`.
pub(crate) struct Index {
    pub(crate) value: Operand,
    pub(crate) ty: Type,
    pub(crate) scale: u64,
}

/// The place a pointer or address operand names: an address of a global, through the casts
/// between integers and pointers, plus a constant, plus a scaled index. A 32-bit program's
/// address goes through its dword and back (`trunc` to 32 bits, `zext` to 64), which keeps a
/// global's address: the port lays its globals out below 4 GB.
pub(crate) fn place(defs: &Defs<'_>, op: &Operand) -> Option<Place> {
    match op {
        Operand::Address { global, offset } => Some(Place { global: global.clone(), offset: *offset, index: None }),
        Operand::Global(global) => Some(Place { global: global.clone(), offset: 0, index: None }),
        Operand::Local(v) => match defs.def(*v)?.2 {
            Inst::Cast { cast: Cast::IntToPtr | Cast::PtrToInt, value, .. } => place(defs, value),
            Inst::Cast { cast: Cast::Trunc, to: Type::Int(32), value, .. }
            | Inst::Cast { cast: Cast::ZExt, from: Type::Int(32), value, .. } => place(defs, value),
            Inst::Bin { op: BinOp::Add, ty, lhs, rhs } => {
                let (base, other) = match place(defs, lhs) {
                    Some(p) if p.index.is_none() => (p, rhs),
                    _ => match place(defs, rhs) {
                        Some(p) if p.index.is_none() => (p, lhs),
                        _ => return None,
                    },
                };
                let index = match other {
                    Operand::Int(k) => return Some(Place { offset: base.offset.wrapping_add(*k as i64), ..base }),
                    Operand::Local(w) => match defs.def(*w).map(|d| d.2) {
                        Some(Inst::Bin { op: BinOp::Mul, ty, lhs: x, rhs: Operand::Int(k) }) if *k > 0 => {
                            Index { value: x.clone(), ty: ty.clone(), scale: *k as u64 }
                        }
                        Some(Inst::Bin { op: BinOp::Shl, ty, lhs: x, rhs: Operand::Int(s) }) if (0..32).contains(s) => {
                            Index { value: x.clone(), ty: ty.clone(), scale: 1 << s }
                        }
                        _ => Index { value: other.clone(), ty: ty.clone(), scale: 1 },
                    },
                    _ => return None,
                };
                Some(Place { index: Some(index), ..base })
            }
            _ => None,
        },
        _ => None,
    }
}

/// A typed use of a section's bytes: a load or store of `ty` at an offset, indexed or not.
#[derive(Debug, Clone)]
struct Cell {
    ty: Type,
    indexed: bool,
    /// The element count a masked index (`i & 3`) proves for an indexed use.
    extent: Option<u64>,
}

/// The element count an index bounds when it is a value masked to `2^n - 1`, through the casts
/// that widen it.
fn masked_extent(defs: &Defs<'_>, index: &Operand) -> Option<u64> {
    let Operand::Local(v) = index else { return None };
    match defs.def(*v)?.2 {
        Inst::Cast { cast: Cast::ZExt | Cast::SExt, value, .. } => masked_extent(defs, value),
        Inst::Bin { op: BinOp::And, lhs, rhs, .. } => {
            let k = match (lhs, rhs) {
                (_, Operand::Int(k)) | (Operand::Int(k), _) => u64::try_from(*k).ok()?,
                _ => return None,
            };
            (k + 1).is_power_of_two().then_some(k + 1)
        }
        _ => None,
    }
}

/// The uses of the data globals across the module: the typed cells and the addressed strings.
#[derive(Default)]
struct Uses {
    cells: BTreeMap<u64, Cell>,
    strings: BTreeSet<u64>,
    /// The accesses no field type holds (a vector, a 128-bit integer), by offset, the widest size
    /// at each: a string literal ending inside one would be read past its end.
    wide: BTreeMap<u64, u64>,
}

/// The uses of the module's data sections, read one function at a time, which give their layouts:
/// the functions of a large program need not be in memory together for them.
pub(crate) struct LayoutUses {
    flat: HashMap<String, FlatSection>,
    uses: HashMap<String, Uses>,
    /// The program's variables in each section: offset, size and name.
    variables: HashMap<String, Vec<(u64, u64, String)>>,
    /// The read-only sections.
    constant: HashSet<String>,
}

impl LayoutUses {
    /// The data sections of `module`, with no use read yet.
    pub(crate) fn new(module: &Module) -> Self {
        let mut flat: HashMap<String, FlatSection> = HashMap::new();
        for g in &module.globals {
            if g.address.is_some()
                && let Some(f) = flatten(g)
            {
                flat.insert(g.name.clone(), f);
            }
        }
        let mut variables: HashMap<String, Vec<(u64, u64, String)>> = HashMap::new();
        for v in &module.variables {
            variables.entry(v.global.clone()).or_default().push((v.offset, v.size, v.name.clone()));
        }
        let constant = module.globals.iter().filter(|g| g.constant && flat.contains_key(&g.name)).map(|g| g.name.clone()).collect();
        Self { flat, uses: HashMap::new(), variables, constant }
    }

    /// Reads the uses of `f`.
    pub(crate) fn add(&mut self, f: &Function) {
        collect_uses(f, &self.flat, &mut self.uses);
    }

    /// The layouts of the sections, by global, for the globals whose uses type at least one
    /// field; a section no use types keeps the IR's shape.
    pub(crate) fn layouts(self) -> HashMap<String, Layout> {
        let Self { flat, mut uses, variables, constant } = self;
        // An address word that points into a section names a string there as a passed pointer does.
        for (_, words) in flat.values() {
            for (_, item) in words.values() {
                if let Const::Address { global, offset } = item
                    && flat.contains_key(global)
                    && let Ok(offset) = u64::try_from(*offset)
                {
                    uses.entry(global.clone()).or_default().strings.insert(offset);
                }
            }
        }
        let mut out = HashMap::new();
        for (name, (bytes, words)) in flat {
            let Some(u) = uses.get(&name) else { continue };
            let layout = Layout { fields: Vec::new(), bytes, words, constant: constant.contains(&name) };
            let mut fields = fields_of(&layout, u);
            let typed = fields.iter().any(|f| !matches!(f.elem, Elem::Bytes) && (f.array || !matches!(f.elem, Elem::Word(_))));
            if typed {
                name_fields(&mut fields, variables.get(&name).map_or(&[][..], Vec::as_slice));
                out.insert(name, Layout { fields, ..layout });
            }
        }
        out
    }
}

/// Names the fields that the program's variables (`variables`, by offset, size and name, in
/// address order) hold: the field a variable starts at by the variable's name, another field
/// inside it by the name and its offset into it (`players_8`). A name the section uses twice (two
/// files' statics) takes the offset too, and a field no variable holds keeps its offset's name.
fn name_fields(fields: &mut [Field], variables: &[(u64, u64, String)]) {
    let mut used: HashSet<String> = HashSet::new();
    for field in fields.iter_mut() {
        let i = variables.partition_point(|(offset, _, _)| *offset <= field.offset);
        let Some((offset, size, name)) = i.checked_sub(1).map(|i| &variables[i]) else { continue };
        if field.offset >= offset + size || field.end() > offset + size {
            continue;
        }
        let mut chosen = if field.offset == *offset { name.clone() } else { format!("{name}_{}", field.offset - offset) };
        if !used.insert(chosen.clone()) {
            chosen = format!("{chosen}_{}", field.offset);
            used.insert(chosen.clone());
        }
        field.name = Some(chosen);
    }
}

/// The bytes and the address words of a data global, from the shapes the lift writes: one byte
/// array, or a packed structure of byte runs and words.
fn flatten(g: &Global) -> Option<FlatSection> {
    match (&g.ty, g.init.as_ref()?) {
        (Type::Array(n, e), Const::Bytes(b)) if **e == Type::I8 => {
            let mut bytes = b.clone();
            bytes.resize(*n as usize, 0);
            Some((bytes, BTreeMap::new()))
        }
        (Type::Array(n, e), Const::Zero) if **e == Type::I8 => Some((vec![0; *n as usize], BTreeMap::new())),
        (Type::Packed(fields), Const::Struct(items)) if fields.len() == items.len() => {
            let mut bytes = Vec::new();
            let mut words = BTreeMap::new();
            for (field, item) in fields.iter().zip(items) {
                match (field, item) {
                    (Type::Array(n, e), Const::Bytes(b)) if **e == Type::I8 && b.len() as u64 == *n => bytes.extend_from_slice(b),
                    (Type::Int(bits), Const::Address { .. } | Const::Global(_) | Const::Int(_)) if bits % 8 == 0 => {
                        words.insert(bytes.len() as u64, (field.clone(), item.clone()));
                        bytes.extend(std::iter::repeat_n(0, usize::from(bits / 8)));
                    }
                    _ => return None,
                }
            }
            Some((bytes, words))
        }
        _ => None,
    }
}

/// The typed accesses and the addressed strings of one function.
fn collect_uses(f: &Function, flat: &HashMap<String, FlatSection>, uses: &mut HashMap<String, Uses>) {
    let defs = Defs::new(f);
    let address = |op: &Operand, uses: &mut HashMap<String, Uses>| {
        if let Operand::Address { global, offset } = op
            && flat.contains_key(global)
            && let Ok(offset) = u64::try_from(*offset)
        {
            uses.entry(global.clone()).or_default().strings.insert(offset);
        }
    };
    // The accesses first: the address arithmetic written inline into one of them is its address,
    // not a passed pointer, so the general walk below leaves it alone.
    let mut consumed: HashSet<ValueId> = HashSet::new();
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        let access = match &i.inst {
            Inst::Load { ty, ptr, .. } | Inst::Store { ty, ptr, .. } => Some((ty, ptr)),
            _ => None,
        };
        if let Some((ty, ptr)) = access
            && let Some(p) = place(&defs, ptr)
            && flat.contains_key(&p.global)
            && let Ok(offset) = u64::try_from(p.offset)
            && let Some(size) = value_size(ty)
        {
            consume(&defs, ptr, &mut consumed);
            let indexed = match &p.index {
                Some(index) => index.scale == size,
                None => false,
            };
            if p.index.is_none() || indexed {
                let extent = if indexed { p.index.as_ref().and_then(|i| masked_extent(&defs, &i.value)) } else { None };
                let cell = Cell { ty: ty.clone(), indexed, extent };
                let slot = uses.entry(p.global.clone()).or_default().cells.entry(offset).or_insert_with(|| cell.clone());
                // Two uses at one offset: the indexed one, else the wider one, says the type; the
                // largest proven extent stays.
                let extent = slot.extent.max(cell.extent);
                if (cell.indexed && !slot.indexed) || (cell.indexed == slot.indexed && size > value_size(&slot.ty).unwrap_or(0)) {
                    *slot = cell;
                }
                slot.extent = extent;
            }
            if let Some(index) = &p.index {
                address(&index.value, uses);
            }
            if let Inst::Store { value, .. } = &i.inst {
                address(value, uses);
            }
            consumed.extend(i.result);
        } else if let Some((ty, ptr)) = access
            && value_size(ty).is_none()
            && let Some(p) = place(&defs, ptr)
            && p.index.is_none()
            && flat.contains_key(&p.global)
            && let Ok(offset) = u64::try_from(p.offset)
            && let Some(size) = ty.size(8)
        {
            let wide = uses.entry(p.global).or_default().wide.entry(offset).or_insert(0);
            *wide = (*wide).max(size);
        }
    }
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        if i.result.is_some_and(|v| consumed.contains(&v)) {
            continue;
        }
        for op in i.inst.operands() {
            address(op, uses);
        }
    }
    for b in &f.blocks {
        for op in b.terminator.operands() {
            address(op, uses);
        }
    }
}

/// Marks the values of an access's address that are written inline into it: the casts and sums
/// [`place`] walks, each used by that one access.
fn consume(defs: &Defs<'_>, op: &Operand, consumed: &mut HashSet<ValueId>) {
    let Operand::Local(v) = op else { return };
    if !defs.inlinable(*v) || !consumed.insert(*v) {
        return;
    }
    match defs.def(*v).map(|d| d.2) {
        Some(Inst::Cast { value, .. }) => consume(defs, value, consumed),
        Some(Inst::Bin { lhs, rhs, .. }) => {
            consume(defs, lhs, consumed);
            consume(defs, rhs, consumed);
        }
        _ => {}
    }
}

/// The size of a type a field can hold: an integer of whole bytes up to 64 bits, or a float.
fn value_size(ty: &Type) -> Option<u64> {
    match ty {
        Type::Int(8 | 16 | 32 | 64) | Type::Float | Type::Double => ty.size(8),
        _ => None,
    }
}

/// The fields of a section from its uses: the words of the IR stay words, a cell becomes a
/// scalar or an array that runs to the next word, cell or string, an addressed printable run
/// becomes a string unless a vector access reads past its end (a constant whose first lane is a
/// character), and the bytes between stay bytes.
fn fields_of(layout: &Layout, uses: &Uses) -> Vec<Field> {
    let size = layout.bytes.len() as u64;
    let mut starts: BTreeSet<u64> = layout.words.keys().copied().collect();
    starts.extend(uses.cells.keys().copied());
    starts.extend(uses.strings.iter().copied());
    starts.insert(size);
    let mut fields: Vec<Field> = Vec::new();
    let mut at = 0u64;
    for &start in &starts {
        if start < at || start >= size {
            continue;
        }
        let next = array_end(layout, uses, start, &starts);
        let field = if let Some((word_ty, _)) = layout.words.get(&start) {
            word_field(layout, uses, start, word_ty)
        } else if let Some(cell) = uses.cells.get(&start) {
            cell_field(layout, start, next, cell)
        } else {
            string_field(layout, start, next).filter(|f| uses.wide.range(f.offset..f.end()).all(|(&at, &size)| at + size <= f.end()))
        };
        let Some(field) = field else { continue };
        if field.end() > size {
            continue;
        }
        if start > at {
            fields.push(Field { offset: at, count: start - at, elem: Elem::Bytes, array: true, name: None });
        }
        at = field.end();
        fields.push(field);
    }
    if at < size {
        fields.push(Field { offset: at, count: size - at, elem: Elem::Bytes, array: true, name: None });
    }
    fields
}

/// Where the field at `start` ends at the latest: the next known thing, except that an indexed
/// cell's array with a proven extent runs over the scalar reads of its own elements (an unrolled
/// loop reads each one by a constant offset), up to its extent or the first word, string or cell
/// of another type or stride inside it. A scalar right after an array of no proven extent is its
/// own field: nothing says it is an element.
fn array_end(layout: &Layout, uses: &Uses, start: u64, starts: &BTreeSet<u64>) -> u64 {
    let size = layout.bytes.len() as u64;
    let next_start = starts.range(start + 1..).next().copied().unwrap_or(size);
    let Some((cell, extent)) = uses.cells.get(&start).filter(|c| c.indexed).and_then(|c| Some((c, c.extent?))) else {
        return next_start;
    };
    let Some(stride) = value_size(&cell.ty) else { return next_start };
    let end = (start + extent * stride).min(size);
    for &next in starts.range(start + 1..end) {
        let element = (next - start) % stride == 0
            && !layout.words.contains_key(&next)
            && !uses.strings.contains(&next)
            && uses.cells.get(&next).is_some_and(|c| !c.indexed && c.ty == cell.ty);
        if !element {
            return next;
        }
    }
    end
}

/// An address word, or the array of consecutive words an indexed use at the first reads.
fn word_field(layout: &Layout, uses: &Uses, start: u64, ty: &Type) -> Option<Field> {
    let width = ty.size(8)?;
    let indexed = uses.cells.get(&start).is_some_and(|c| c.indexed && c.ty.size(8) == Some(width));
    let mut count = 1;
    if indexed {
        let mut next = start + width;
        while layout.words.get(&next).is_some_and(|(t, _)| t == ty) && !uses.strings.contains(&next) {
            let other = uses.cells.get(&next).is_some_and(|c| c.ty.size(8) != Some(width) || c.indexed);
            if other {
                break;
            }
            count += 1;
            next += width;
        }
    }
    Some(Field { offset: start, count, elem: Elem::Word(ty.clone()), array: indexed, name: None })
}

/// A scalar at `start`, or the array an indexed use reads up to the next known thing. A float
/// that is not a finite number has no literal: the bytes stay bytes.
fn cell_field(layout: &Layout, start: u64, next: u64, cell: &Cell) -> Option<Field> {
    let size = value_size(&cell.ty)?;
    if start + size > next {
        return None;
    }
    let count = if cell.indexed { (next - start) / size } else { 1 };
    let field = Field { offset: start, count, elem: Elem::Value(cell.ty.clone()), array: cell.indexed, name: None };
    if cell.ty.is_float()
        && (0..count).any(|i| !float_literal(&cell.ty, layout.element(&field, i)).is_some_and(|l| !l.contains("inf") && !l.contains("NaN")))
    {
        return None;
    }
    Some(field)
}

/// The printable NUL-terminated run at `start`, when the bytes there are one before `next`.
fn string_field(layout: &Layout, start: u64, next: u64) -> Option<Field> {
    let run = &layout.bytes[start as usize..next as usize];
    let len = run.iter().position(|&b| b == 0)?;
    if len == 0 || !run[..len].iter().all(|&b| b == b'\t' || b == b'\n' || b == b'\r' || (b' '..=b'~').contains(&b)) {
        return None;
    }
    // One character, a zero, another character and a zero is the start of a UTF-16 string, not
    // a one-character C string: as the literal `"t"` the wide text after it was gone, and the
    // C runtime's `_wassert` printed what lay behind the literal for the expression.
    if len == 1 && run.len() >= 4 && run[2] != 0 && run[3] == 0 {
        return None;
    }
    Some(Field { offset: start, count: len as u64 + 1, elem: Elem::Char, array: true, name: None })
}

/// The C literal of a float or double from its bytes: the shortest decimal that reads back to
/// the same value; `inf` and `NaN` have none.
pub(crate) fn float_literal(ty: &Type, bytes: &[u8]) -> Option<String> {
    match ty {
        Type::Float => {
            let v = f32::from_le_bytes(bytes.try_into().ok()?);
            v.is_finite().then(|| format!("{v:?}f"))
        }
        Type::Double => {
            let v = f64::from_le_bytes(bytes.try_into().ok()?);
            v.is_finite().then(|| format!("{v:?}"))
        }
        _ => None,
    }
}

/// The C string literal of a run of printable bytes: the escapes C needs, and a `?` after a `?`
/// escaped so no trigraph forms.
pub(crate) fn string_literal(bytes: &[u8]) -> String {
    let mut out = String::from("\"");
    let mut previous = 0u8;
    for &b in bytes {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            b'\t' => out.push_str("\\t"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'?' if previous == b'?' => out.push_str("\\?"),
            _ => out.push(b as char),
        }
        previous = b;
    }
    out.push('"');
    out
}
