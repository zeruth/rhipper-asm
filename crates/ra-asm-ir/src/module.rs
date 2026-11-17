//! Modules, functions, blocks, globals and declarations.

use std::collections::HashMap;

use crate::inst::{Instruction, Terminator};
use crate::types::{FnType, Type};

/// A local value of a function. The name lives in the function's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ValueId(pub u32);

/// A block of a function. The name lives in the block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BlockId(pub u32);

/// A calling convention, printed as LLVM spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CallConv {
    #[default]
    C,
    Win64,
    X86Stdcall,
    X86Fastcall,
    X86Thiscall,
}

impl CallConv {
    /// The keyword, or an empty string for the C convention.
    pub fn keyword(self) -> &'static str {
        match self {
            Self::C => "",
            Self::Win64 => "win64cc",
            Self::X86Stdcall => "x86_stdcallcc",
            Self::X86Fastcall => "x86_fastcallcc",
            Self::X86Thiscall => "x86_thiscallcc",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Linkage {
    #[default]
    External,
    Internal,
}

/// A constant initializer of a global.
#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Int(i128),
    Float(f64),
    /// Raw bytes, as `[N x i8]`.
    Bytes(Vec<u8>),
    /// `zeroinitializer`.
    Zero,
    Null,
    /// A pointer to another global.
    Global(String),
    /// The address of a byte of a global, as the field's type: a pointer or an integer.
    Address {
        global: String,
        offset: i64,
    },
    Array(Vec<Const>),
    Struct(Vec<Const>),
}

/// A global variable.
#[derive(Debug, Clone, PartialEq)]
pub struct Global {
    pub name: String,
    pub ty: Type,
    pub init: Option<Const>,
    pub constant: bool,
    pub linkage: Linkage,
    pub align: u32,
    /// The address in the image it came from.
    pub address: Option<u64>,
}

/// A function the module calls but does not define.
#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub name: String,
    pub ty: FnType,
    pub conv: CallConv,
    pub attrs: Vec<String>,
}

/// A basic block.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub id: BlockId,
    pub name: String,
    pub instrs: Vec<Instruction>,
    pub terminator: Terminator,
    /// The machine address the block starts at.
    pub address: Option<u64>,
}

/// A function with a body.
#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub name: String,
    pub ty: FnType,
    /// The parameters, one value per parameter type.
    pub params: Vec<ValueId>,
    pub blocks: Vec<Block>,
    pub linkage: Linkage,
    pub conv: CallConv,
    pub attrs: Vec<String>,
    /// The machine address of the function it was lifted from.
    pub address: Option<u64>,
    /// The name of every local value, by id.
    pub value_names: Vec<String>,
    /// The type of every local value, by id.
    pub value_types: Vec<Type>,
    /// The names of `value_names` as a set, for [`Function::new_value`].
    pub name_index: NameIndex,
}

/// The names a function's values have, as a set: [`Function::new_value`] keeps a new name unique
/// in a lookup, where scanning the names made a pass over a function of 300,000 values the values
/// squared. It catches up with the names appended since it last looked (the builder's, a thaw's)
/// before it answers; a pass that replaces `value_names` resets it. It is a cache: two functions
/// equal or not by their contents, whatever it holds.
#[derive(Debug, Clone, Default)]
pub struct NameIndex {
    names: std::collections::HashSet<String>,
    /// How many of `value_names` are in `names`.
    synced: usize,
    /// For each hint, the lowest suffix not yet known taken: names are never freed, so the lowest
    /// free one only moves up. Without it the k-th `rax` of a function tried k names, and ffxiv.exe's
    /// largest functions spent half a minute in the ABI promotion naming registers.
    next: std::collections::HashMap<String, u32>,
}

impl PartialEq for NameIndex {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Function {
    pub fn value_name(&self, v: ValueId) -> &str {
        &self.value_names[v.0 as usize]
    }

    pub fn value_type(&self, v: ValueId) -> &Type {
        &self.value_types[v.0 as usize]
    }

    /// The block `id`: where its number puts it, else by a search in id order, else by a scan. A
    /// scan alone made a structuring walk quadratic: 20 s of ffxiv.exe's 4932-block function. The
    /// search in order can only miss, never find another block, so the scan stays the answer of
    /// last resort and the result is the scan's.
    pub fn block(&self, id: BlockId) -> Option<&Block> {
        self.block_index(id).map(|n| &self.blocks[n])
    }

    pub fn block_mut(&mut self, id: BlockId) -> Option<&mut Block> {
        self.block_index(id).map(|n| &mut self.blocks[n])
    }

    /// Where the block `id` is in `blocks`: the first block with that id, as a scan finds it.
    fn block_index(&self, id: BlockId) -> Option<usize> {
        let at = id.0 as usize;
        // a block id is unique in a function, so the one found is the scan's
        if self.blocks.get(at).is_some_and(|b| b.id == id) {
            return Some(at);
        }
        if let Ok(n) = self.blocks.binary_search_by_key(&id.0, |b| b.id.0)
            && self.blocks[n].id == id
        {
            return Some(n);
        }
        self.blocks.iter().position(|b| b.id == id)
    }

    pub fn entry(&self) -> &Block {
        &self.blocks[0]
    }

    /// The blocks that lead into each block.
    pub fn predecessors(&self) -> HashMap<BlockId, Vec<BlockId>> {
        let mut preds: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
        for b in &self.blocks {
            for s in b.terminator.successors() {
                preds.entry(s).or_default().push(b.id);
            }
        }
        preds
    }

    pub fn instr_count(&self) -> usize {
        self.blocks.iter().map(|b| b.instrs.len()).sum()
    }

    /// A fresh local value for a pass: named `hint`, made unique with a suffix.
    pub fn new_value(&mut self, hint: &str, ty: Type) -> ValueId {
        let index = &mut self.name_index;
        if index.synced > self.value_names.len() {
            *index = NameIndex::default();
        }
        for name in &self.value_names[index.synced..] {
            index.names.insert(name.clone());
        }
        index.synced = self.value_names.len();
        let mut n = index.next.get(hint).copied().unwrap_or(0);
        let mut name = if n == 0 { hint.to_owned() } else { format!("{hint}.{n}") };
        while index.names.contains(&name) {
            n += 1;
            name = format!("{hint}.{n}");
        }
        index.next.insert(hint.to_owned(), n + 1);
        index.names.insert(name.clone());
        index.synced += 1;
        let id = ValueId(self.value_names.len() as u32);
        self.value_names.push(name);
        self.value_types.push(ty);
        id
    }

    /// Replaces every use of the values in `map` by their replacement, following chains to their
    /// end however long they are: a run of a hundred trivial phis, each the one before it, leaves
    /// no use of a phi the caller removed. A chain that comes back to itself (phis that are only
    /// each other) has no value outside it and ends in `undef`; a value mapped to itself stays.
    pub fn replace_uses(&mut self, map: &HashMap<ValueId, crate::inst::Operand>) {
        use crate::inst::Operand;
        let resolved = resolve_chains(map);
        let resolve = |o: &mut Operand| {
            if let Operand::Local(v) = o
                && let Some(r) = resolved.get(v)
            {
                *o = r.clone();
            }
        };
        for b in &mut self.blocks {
            for i in &mut b.instrs {
                for o in i.inst.operands_mut() {
                    resolve(o);
                }
            }
            for o in b.terminator.operands_mut() {
                resolve(o);
            }
        }
    }

    /// Drops the names and types of the values nothing defines or reads any more, numbering the
    /// rest again in their order, and gives back the room the passes left in the vectors. A pass
    /// that removes an instruction leaves its value's entry behind: after the raise of a large
    /// function four values in five are such leftovers, which outweigh the instructions.
    pub fn compact(&mut self) {
        let n = self.value_names.len();
        let mut keep = vec![false; n];
        let mut mark = |v: ValueId| {
            if let Some(k) = keep.get_mut(v.0 as usize) {
                *k = true;
            }
        };
        self.params.iter().copied().for_each(&mut mark);
        for b in &self.blocks {
            for i in &b.instrs {
                i.result.into_iter().chain(i.inst.uses()).for_each(&mut mark);
            }
            b.terminator.uses().into_iter().for_each(&mut mark);
        }
        if keep.iter().all(|k| *k) {
            self.shrink();
            return;
        }
        let mut renumber = vec![u32::MAX; n];
        let (mut names, mut types) = (Vec::new(), Vec::new());
        for (old, kept) in keep.iter().enumerate() {
            if *kept {
                renumber[old] = names.len() as u32;
                names.push(std::mem::take(&mut self.value_names[old]));
                types.push(std::mem::replace(&mut self.value_types[old], Type::Void));
            }
        }
        let at = |v: ValueId| ValueId(renumber[v.0 as usize]);
        for p in &mut self.params {
            *p = at(*p);
        }
        for b in &mut self.blocks {
            for i in &mut b.instrs {
                if let Some(v) = &mut i.result {
                    *v = at(*v);
                }
                for o in i.inst.operands_mut() {
                    if let crate::inst::Operand::Local(v) = o {
                        *v = at(*v);
                    }
                }
            }
            for o in b.terminator.operands_mut() {
                if let crate::inst::Operand::Local(v) = o {
                    *v = at(*v);
                }
            }
        }
        self.value_names = names;
        self.value_types = types;
        self.name_index = NameIndex::default();
        self.shrink();
    }

    /// Gives back the room the vectors hold beyond their contents.
    fn shrink(&mut self) {
        self.value_names.shrink_to_fit();
        self.value_types.shrink_to_fit();
        self.blocks.shrink_to_fit();
        for b in &mut self.blocks {
            b.instrs.shrink_to_fit();
        }
    }

    /// The number of uses of each value.
    pub fn use_counts(&self) -> HashMap<ValueId, usize> {
        let mut counts: HashMap<ValueId, usize> = HashMap::new();
        for b in &self.blocks {
            for i in &b.instrs {
                for v in i.inst.uses() {
                    *counts.entry(v).or_default() += 1;
                }
            }
            for v in b.terminator.uses() {
                *counts.entry(v).or_default() += 1;
            }
        }
        counts
    }
}

/// Each value of `map` with what its chain of replacements ends in: the first operand that is not
/// a value `map` replaces, `undef` for a chain that comes back to a value it passed (none of the
/// values of a cycle has a definition left outside it), the value itself for one mapped to itself.
/// Each value is walked once: the values a walk passes take the end it found.
fn resolve_chains(map: &HashMap<ValueId, crate::inst::Operand>) -> HashMap<ValueId, crate::inst::Operand> {
    use crate::inst::Operand;
    let mut out: HashMap<ValueId, Operand> = HashMap::with_capacity(map.len());
    let mut path: Vec<ValueId> = Vec::new();
    let mut on_path: std::collections::HashSet<ValueId> = std::collections::HashSet::new();
    for (&start, first) in map {
        if out.contains_key(&start) {
            continue;
        }
        path.clear();
        on_path.clear();
        path.push(start);
        on_path.insert(start);
        let mut at = start;
        let mut next = first;
        let end = loop {
            match next {
                Operand::Local(v) if *v == at => break Operand::Local(*v),
                Operand::Local(v) => {
                    if let Some(done) = out.get(v) {
                        break done.clone();
                    }
                    let Some(after) = map.get(v) else { break next.clone() };
                    if !on_path.insert(*v) {
                        break Operand::Undef;
                    }
                    path.push(*v);
                    at = *v;
                    next = after;
                }
                other => break other.clone(),
            }
        };
        for &v in &path {
            out.insert(v, end.clone());
        }
    }
    out
}

/// A module: the unit the printer writes.
#[derive(Debug, Clone, Default)]
pub struct Module {
    pub name: String,
    pub triple: String,
    pub data_layout: String,
    /// The named struct types, in definition order.
    pub types: Vec<(String, Vec<Type>)>,
    pub globals: Vec<Global>,
    pub declarations: Vec<Declaration>,
    pub functions: Vec<Function>,
    /// Functions the lift froze ([`crate::freeze`]) for a pass to thaw one at a time: the raise
    /// takes them back into `functions`, in their order and before the others.
    pub frozen: Vec<crate::freeze::Frozen>,
    /// The variables the build's symbols name inside the data globals, in address order: a
    /// backend calls a global's bytes there by the variable's name.
    pub variables: Vec<Variable>,
}

/// A variable of the program inside a data global: `size` bytes at `offset` into `global`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable {
    pub global: String,
    pub offset: u64,
    pub size: u64,
    pub name: String,
}

impl Module {
    /// A module for the x86-64 Windows target, which is where a lifted program runs.
    pub fn x86_64_windows(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            triple: "x86_64-pc-windows-msvc".to_owned(),
            data_layout: "e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128".to_owned(),
            ..Self::default()
        }
    }

    /// A module for the x86-64 Linux target: the lift of a Linux program, whose imports are the C
    /// library's by System V and whose port a Linux host builds.
    pub fn x86_64_linux(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            triple: "x86_64-unknown-linux-gnu".to_owned(),
            data_layout: "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-i128:128-f80:128-n8:16:32:64-S128".to_owned(),
            ..Self::default()
        }
    }

    /// A module for the ARM64 Windows target: the lift of an ARM64 image, which runs on an ARM64
    /// host or through the interpreter.
    pub fn aarch64_windows(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            triple: "aarch64-pc-windows-msvc".to_owned(),
            data_layout: "e-m:w-p270:32:32-p271:32:32-p272:64:64-p:64:64-i32:32-i64:64-i128:128-n32:64-S128-Fn32".to_owned(),
            ..Self::default()
        }
    }

    /// A module for the ARM64 Linux target: the lift of a Linux program or of an Android library,
    /// whose imports are the C library's by AAPCS64.
    pub fn aarch64_linux(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            triple: "aarch64-unknown-linux-gnu".to_owned(),
            data_layout: "e-m:e-p270:32:32-p271:32:32-p272:64:64-i8:8:32-i16:16:32-i64:64-i128:128-n32:64-S128-Fn32".to_owned(),
            ..Self::default()
        }
    }

    pub fn function(&self, name: &str) -> Option<&Function> {
        self.functions.iter().find(|f| f.name == name)
    }

    pub fn declaration(&self, name: &str) -> Option<&Declaration> {
        self.declarations.iter().find(|d| d.name == name)
    }

    pub fn global(&self, name: &str) -> Option<&Global> {
        self.globals.iter().find(|g| g.name == name)
    }

    /// Adds a declaration unless one of the name exists.
    pub fn declare(&mut self, declaration: Declaration) {
        if self.declaration(&declaration.name).is_none() {
            self.declarations.push(declaration);
        }
    }

    /// The text of the module in LLVM's syntax.
    pub fn to_ll(&self) -> String {
        assert!(self.frozen.is_empty(), "a module with frozen functions is printed only once the raise has thawed them");
        crate::printer::print(self)
    }

    /// Writes the text of [`Module::to_ll`] to `sink` a function at a time, and returns where
    /// each function landed: the whole text of a large program is never in memory at once.
    pub fn write_ll(&self, sink: &mut dyn std::io::Write) -> std::io::Result<LlLayout> {
        crate::printer::write(self, sink)
    }
}

/// Where [`Module::write_ll`] put the functions in its text, so one function's IR is read back
/// without the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LlLayout {
    /// The length of the whole text.
    pub bytes: u64,
    /// The byte offset and the length of each function's definition, in the order of the
    /// module's functions.
    pub functions: Vec<(u64, u64)>,
}
