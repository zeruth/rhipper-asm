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
}

impl Function {
    pub fn value_name(&self, v: ValueId) -> &str {
        &self.value_names[v.0 as usize]
    }

    pub fn value_type(&self, v: ValueId) -> &Type {
        &self.value_types[v.0 as usize]
    }

    pub fn block(&self, id: BlockId) -> Option<&Block> {
        self.blocks.iter().find(|b| b.id == id)
    }

    pub fn block_mut(&mut self, id: BlockId) -> Option<&mut Block> {
        self.blocks.iter_mut().find(|b| b.id == id)
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
        let mut name = hint.to_owned();
        let mut n = 0;
        while self.value_names.contains(&name) {
            n += 1;
            name = format!("{hint}.{n}");
        }
        let id = ValueId(self.value_names.len() as u32);
        self.value_names.push(name);
        self.value_types.push(ty);
        id
    }

    /// Replaces every use of the values in `map` by their replacement, following chains.
    pub fn replace_uses(&mut self, map: &HashMap<ValueId, crate::inst::Operand>) {
        use crate::inst::Operand;
        let resolve = |o: &mut Operand| {
            let mut guard = 0;
            while let Operand::Local(v) = o {
                let Some(r) = map.get(v) else { break };
                *o = r.clone();
                guard += 1;
                if guard > 64 {
                    break;
                }
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
        crate::printer::print(self)
    }
}
