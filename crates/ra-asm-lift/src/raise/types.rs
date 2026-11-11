//! Types: a pointer a function reads and writes at several constant offsets points at a struct,
//! and each access is a field of it. A base is a value the lift made of a register (a parameter,
//! a load, a call's result) that an access reaches through `inttoptr`, an addition of a constant
//! and, in a 32-bit program, a `zext`; never the frame, whose slots are the frame pass's. An
//! access of an `i32` at `%p` and of a `double` at `%p + 16` make `%p` a `{ i32, [12 x i8],
//! double }`. The bases a call passes on to a parameter share one struct, so the function that
//! fills a record and the ones that read it agree on its fields. The struct becomes a named type
//! of the module and the accesses go through `getelementptr` of its fields, which the C backend
//! prints as `p->f16`.
//!
//! The layout is LLVM's natural one: a field sits at its own alignment and the padding between
//! fields is explicit bytes. An access that fits no field (misaligned, wider than a word, or
//! starting inside another field) stays a byte offset from the pointer, and an access of another
//! type at a field's offset (the bits of a double stored as an `i64`) reads the field through a
//! cast. A base with one field is a pointer to a scalar and stays as it is.

use std::collections::{BTreeMap, HashMap, HashSet};

use ra_asm_ir::{BinOp, Callee, Cast, Function, Inst, Instruction, Module, Operand, Type, ValueId};

/// A base in the module: a value of one function.
type Base = (usize, ValueId);

/// An access through a base: the function, the block and the instruction, the offset and the
/// type read or written.
struct Site {
    function: usize,
    block: usize,
    index: usize,
    base: ValueId,
    offset: u64,
    ty: Type,
}

/// The widest field: a register's worth.
const WORD: u64 = 8;
/// An offset past this is no field of a struct a register points at.
const REACH: u64 = 1 << 16;

/// Runs the pass over the module. Returns whether anything changed.
pub fn run(module: &mut Module) -> bool {
    let mut sites = Vec::new();
    let mut classes = Classes::default();
    for (fi, f) in module.functions.iter().enumerate() {
        let view = View::new(f);
        for (bi, b) in f.blocks.iter().enumerate() {
            for (ii, i) in b.instrs.iter().enumerate() {
                let (ptr, ty) = match &i.inst {
                    Inst::Load { ty, ptr, .. } | Inst::Store { ty, ptr, .. } => (ptr, ty),
                    Inst::Call { callee: Callee::Direct(name), args, .. } => {
                        // A value passed to a parameter shares the parameter's struct, whatever
                        // the value is here: a record on the caller's frame links the functions
                        // that fill and read it without being a base itself.
                        if let Some((gi, g)) = module.functions.iter().enumerate().find(|(_, g)| g.name == *name) {
                            for ((_, a), p) in args.iter().zip(&g.params) {
                                let ((Some((v, 0)), _) | (_, &Operand::Local(v))) = (view.resolve_int(a), a) else { continue };
                                classes.union((fi, v), (gi, *p));
                            }
                        }
                        continue;
                    }
                    _ => continue,
                };
                let Some((base, offset)) = view.resolve_ptr(ptr) else { continue };
                if offset < 0 || offset as u64 >= REACH || !scalar(ty) {
                    continue;
                }
                classes.find((fi, base));
                sites.push(Site { function: fi, block: bi, index: ii, base, offset: offset as u64, ty: ty.clone() });
            }
        }
    }

    // The accesses of each class, then its struct.
    let mut accesses: HashMap<Base, BTreeMap<u64, Vec<Type>>> = HashMap::new();
    for s in &sites {
        let root = classes.find((s.function, s.base));
        accesses.entry(root).or_default().entry(s.offset).or_default().push(s.ty.clone());
    }
    let mut by_layout: HashMap<Vec<Type>, String> = HashMap::new();
    let mut structs: HashMap<Base, Struct> = HashMap::new();
    for (root, offsets) in accesses {
        let Some((fields, layout)) = fields_of(&offsets) else { continue };
        let next = format!("S{}", by_layout.len());
        let name = by_layout.entry(layout.clone()).or_insert_with(|| next).clone();
        if !module.types.iter().any(|(n, _)| *n == name) {
            module.types.push((name.clone(), layout));
        }
        structs.insert(root, Struct { name, fields });
    }
    if structs.is_empty() {
        return false;
    }

    // The rewrite, function by function: the base's pointer once, a field pointer per access.
    let mut changed = false;
    for (fi, f) in module.functions.iter_mut().enumerate() {
        let mut edits: Vec<Edit> = Vec::new();
        let mut pointers: HashMap<ValueId, ValueId> = HashMap::new();
        let defs = definitions(f);
        for s in sites.iter().filter(|s| s.function == fi) {
            let Some(st) = structs.get(&classes.find((fi, s.base))) else { continue };
            let Some(&index) = st.fields.get(&s.offset) else { continue };
            let pointer = *pointers.entry(s.base).or_insert_with(|| {
                let (at, casts, p) = pointer_of(f, s.base, defs.get(&s.base).copied());
                edits.push(Edit { block: at.0, index: at.1, first: true, instrs: casts });
                p
            });
            let fp = f.new_value(&format!("{}.f{}", f.value_name(s.base), s.offset), Type::Ptr);
            let gep = Inst::Gep {
                base_ty: Type::Named(st.name.clone()),
                ptr: Operand::Local(pointer),
                indices: vec![(Type::I64, Operand::Int(0)), (Type::I32, Operand::Int(i128::from(index)))],
                inbounds: true,
            };
            let site = &mut f.blocks[s.block].instrs[s.index];
            edits.push(Edit {
                block: s.block,
                index: s.index,
                first: false,
                instrs: vec![Instruction { result: Some(fp), inst: gep, address: site.address }],
            });
            match &mut site.inst {
                Inst::Load { ptr, .. } | Inst::Store { ptr, .. } => *ptr = Operand::Local(fp),
                _ => {}
            }
            changed = true;
        }
        // From the back of each block, so the indices stay; at one index the casts go first.
        edits.sort_by_key(|e| std::cmp::Reverse((e.block, e.index, e.first)));
        for e in edits {
            let instrs = &mut f.blocks[e.block].instrs;
            instrs.splice(e.index..e.index, e.instrs);
        }
    }
    changed
}

/// Instructions to insert before `index` of `block`; `first` puts them before the others there.
struct Edit {
    block: usize,
    index: usize,
    first: bool,
    instrs: Vec<Instruction>,
}

/// A struct a class of bases points at: the module type and the field index of each offset.
struct Struct {
    name: String,
    fields: BTreeMap<u64, u32>,
}

/// Whether `ty` can be a field: an integer or a float of up to a word.
fn scalar(ty: &Type) -> bool {
    matches!(ty, Type::Int(8 | 16 | 32 | 64) | Type::Float | Type::Double)
}

/// The struct a class's accesses make: at each offset the widest access, when it is aligned and
/// starts past the field before; the field index of each offset, and the module type with the
/// padding between the fields as explicit bytes. `None` under two fields: one is a pointer to a
/// scalar, or to one byte of a buffer.
fn fields_of(offsets: &BTreeMap<u64, Vec<Type>>) -> Option<(BTreeMap<u64, u32>, Vec<Type>)> {
    let mut fields = BTreeMap::new();
    let mut layout = Vec::new();
    let mut end = 0;
    for (&offset, types) in offsets {
        let ty = field_type(types);
        let size = ty.size(WORD)?;
        if offset < end || offset % size != 0 {
            continue;
        }
        if offset > end {
            layout.push(Type::array(offset - end, Type::I8));
        }
        fields.insert(offset, layout.len() as u32);
        layout.push(ty);
        end = offset + size;
    }
    if fields.len() < 2 {
        return None;
    }
    Some((fields, layout))
}

/// The type of a field accessed as `types`: the widest, and a float over an integer of the same
/// width, since the integer is how a constant of the float is stored.
fn field_type(types: &[Type]) -> Type {
    let size = types.iter().filter_map(|t| t.size(WORD)).max().unwrap_or(1);
    let widest = types.iter().filter(|t| t.size(WORD) == Some(size));
    widest.clone().find(|t| t.is_float()).or_else(|| widest.clone().next()).cloned().unwrap_or(Type::I8)
}

/// The defining instruction of each value.
fn definitions(f: &Function) -> HashMap<ValueId, (usize, usize)> {
    let mut out = HashMap::new();
    for (bi, b) in f.blocks.iter().enumerate() {
        for (ii, i) in b.instrs.iter().enumerate() {
            if let Some(v) = i.result {
                out.insert(v, (bi, ii));
            }
        }
    }
    out
}

/// The pointer value of `base`, with the casts that make it and where they go: after the base's
/// definition, past the phis of its block, or at the entry for a parameter.
fn pointer_of(f: &mut Function, base: ValueId, def: Option<(usize, usize)>) -> ((usize, usize), Vec<Instruction>, ValueId) {
    let at = match def {
        Some((bi, ii)) => {
            let b = &f.blocks[bi];
            let mut at = ii + 1;
            while at < b.instrs.len() && matches!(b.instrs[at].inst, Inst::Phi { .. }) {
                at += 1;
            }
            (bi, at)
        }
        None => (0, 0),
    };
    let address = def.map(|(bi, ii)| f.blocks[bi].instrs[ii].address).unwrap_or_default();
    let hint = format!("{}.s", f.value_name(base));
    let mut casts = Vec::new();
    let mut value = Operand::Local(base);
    let ty = f.value_type(base).clone();
    if ty == Type::Ptr {
        return (at, casts, base);
    }
    if ty != Type::I64 {
        let wide = f.new_value(&format!("{hint}.w"), Type::I64);
        casts.push(Instruction { result: Some(wide), inst: Inst::Cast { cast: Cast::ZExt, from: ty, value, to: Type::I64 }, address });
        value = Operand::Local(wide);
    }
    let p = f.new_value(&hint, Type::Ptr);
    casts.push(Instruction { result: Some(p), inst: Inst::Cast { cast: Cast::IntToPtr, from: Type::I64, value, to: Type::Ptr }, address });
    (at, casts, p)
}

/// The union-find over the bases of the module.
#[derive(Default)]
struct Classes {
    parent: HashMap<Base, Base>,
}

impl Classes {
    fn find(&mut self, b: Base) -> Base {
        let p = *self.parent.entry(b).or_insert(b);
        if p == b {
            return b;
        }
        let root = self.find(p);
        self.parent.insert(b, root);
        root
    }

    fn union(&mut self, a: Base, b: Base) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent.insert(ra, rb);
        }
    }
}

/// A function read for the resolution of pointers to bases and offsets.
struct View<'a> {
    f: &'a Function,
    defs: HashMap<ValueId, &'a Inst>,
    /// The values derived from an `alloca`: the frame, which the frame pass owns.
    frame: HashSet<ValueId>,
}

impl<'a> View<'a> {
    fn new(f: &'a Function) -> Self {
        let mut defs = HashMap::new();
        for i in f.blocks.iter().flat_map(|b| &b.instrs) {
            if let Some(v) = i.result {
                defs.insert(v, &i.inst);
            }
        }
        let mut frame = HashSet::new();
        // A phi may read a value defined later: two rounds reach what one round misses.
        for _ in 0..2 {
            for i in f.blocks.iter().flat_map(|b| &b.instrs) {
                let Some(v) = i.result else { continue };
                let derived = match &i.inst {
                    Inst::Alloca { .. } => true,
                    Inst::Gep { .. } | Inst::Cast { .. } | Inst::Phi { .. } | Inst::Select { .. } => {
                        i.inst.uses().iter().any(|u| frame.contains(u))
                    }
                    Inst::Bin { op: BinOp::Add | BinOp::Sub, .. } => i.inst.uses().iter().any(|u| frame.contains(u)),
                    _ => false,
                };
                if derived {
                    frame.insert(v);
                }
            }
        }
        Self { f, defs, frame }
    }

    /// The base and the offset a pointer operand addresses.
    fn resolve_ptr(&self, o: &Operand) -> Option<(ValueId, i64)> {
        let Operand::Local(p) = o else { return None };
        if self.frame.contains(p) {
            return None;
        }
        match self.defs.get(p) {
            Some(Inst::Cast { cast: Cast::IntToPtr, value, .. }) => self.resolve_int(value),
            Some(Inst::Gep { base_ty: Type::I8, ptr, indices, .. }) => {
                let [(_, Operand::Int(c))] = indices.as_slice() else { return None };
                let (b, off) = self.resolve_ptr(ptr)?;
                Some((b, off.checked_add(i64::try_from(*c).ok()?)?))
            }
            Some(Inst::Gep { .. } | Inst::Cast { .. }) => None,
            _ => (*self.f.value_type(*p) == Type::Ptr).then_some((*p, 0)),
        }
    }

    /// The base and the offset an integer operand holds as an address.
    fn resolve_int(&self, o: &Operand) -> Option<(ValueId, i64)> {
        let Operand::Local(v) = o else { return None };
        if self.frame.contains(v) {
            return None;
        }
        match self.defs.get(v) {
            Some(Inst::Bin { op: op @ (BinOp::Add | BinOp::Sub), ty, lhs, rhs }) => {
                let (value, c) = match (lhs, rhs) {
                    (value, Operand::Int(c)) => (value, *c),
                    (Operand::Int(c), value) if *op == BinOp::Add => (value, *c),
                    _ => return Some((*v, 0)),
                };
                // The constant as the width of the addition reads it: `add i32 %p, -4` wraps.
                let c = match ty {
                    Type::Int(32) => i64::from(c as i32),
                    _ => c as i64,
                };
                let (b, off) = self.resolve_int(value)?;
                Some((b, if *op == BinOp::Add { off.checked_add(c)? } else { off.checked_sub(c)? }))
            }
            Some(Inst::Cast { cast: Cast::ZExt, from: Type::Int(32), value, .. }) => self.resolve_int(value),
            Some(Inst::Cast { cast: Cast::PtrToInt, value, .. }) => self.resolve_ptr(value),
            Some(Inst::Cast { .. }) => None,
            _ => matches!(self.f.value_type(*v), Type::Int(32 | 64)).then_some((*v, 0)),
        }
    }
}
