//! A builder over one function: blocks, values with names, and the instruction that defines each.

use crate::inst::{BinOp, Callee, Cast, ICond, Inst, Instruction, Operand, Terminator};
use crate::module::{Block, BlockId, CallConv, Function, Linkage, ValueId};
use crate::types::{FnType, Type};

/// Builds a [`Function`]. Every value gets a unique name from a hint; every block a name.
pub struct FunctionBuilder {
    function: Function,
    /// The address attached to the instructions pushed next.
    pub address: Option<u64>,
    names_used: std::collections::HashMap<String, u32>,
}

impl FunctionBuilder {
    pub fn new(name: &str, ty: FnType, param_names: &[&str]) -> Self {
        let param_types = ty.params.clone();
        let mut b = Self {
            function: Function {
                name: name.to_owned(),
                ty,
                params: Vec::new(),
                blocks: Vec::new(),
                linkage: Linkage::External,
                conv: CallConv::C,
                attrs: Vec::new(),
                address: None,
                value_names: Vec::new(),
                value_types: Vec::new(),
            },
            address: None,
            names_used: std::collections::HashMap::new(),
        };
        for (i, p) in param_types.into_iter().enumerate() {
            let hint = param_names.get(i).copied().unwrap_or("arg");
            let v = b.new_value(hint, p);
            b.function.params.push(v);
        }
        b
    }

    pub fn set_linkage(&mut self, linkage: Linkage) -> &mut Self {
        self.function.linkage = linkage;
        self
    }

    pub fn set_conv(&mut self, conv: CallConv) -> &mut Self {
        self.function.conv = conv;
        self
    }

    pub fn add_attr(&mut self, attr: &str) -> &mut Self {
        self.function.attrs.push(attr.to_owned());
        self
    }

    pub fn set_address(&mut self, address: u64) -> &mut Self {
        self.function.address = Some(address);
        self
    }

    pub fn param(&self, index: usize) -> ValueId {
        self.function.params[index]
    }

    /// A fresh value named after `hint`: `hint`, then `hint.1`, `hint.2`.
    fn new_value(&mut self, hint: &str, ty: Type) -> ValueId {
        let hint = sanitize(hint);
        let count = self.names_used.entry(hint.clone()).or_insert(0);
        let name = if *count == 0 { hint.clone() } else { format!("{hint}.{count}") };
        *count += 1;
        let id = ValueId(self.function.value_names.len() as u32);
        self.function.value_names.push(name);
        self.function.value_types.push(ty);
        id
    }

    /// A new block named after `hint`, placed last. The first block made is the entry.
    pub fn block(&mut self, hint: &str) -> BlockId {
        let id = BlockId(self.function.blocks.len() as u32);
        let hint = sanitize(hint);
        let count = self.names_used.entry(hint.clone()).or_insert(0);
        let name = if *count == 0 { hint.clone() } else { format!("{hint}.{count}") };
        *count += 1;
        self.function.blocks.push(Block { id, name, instrs: Vec::new(), terminator: Terminator::Unreachable, address: self.address });
        id
    }

    pub fn set_block_address(&mut self, block: BlockId, address: u64) {
        self.function.blocks[block.0 as usize].address = Some(address);
    }

    /// Appends `inst` to `block` and returns its value, if it has one.
    pub fn push(&mut self, block: BlockId, inst: Inst, hint: &str) -> Option<ValueId> {
        let ty = inst.result_type();
        let result = if ty == Type::Void { None } else { Some(self.new_value(hint, ty)) };
        self.function.blocks[block.0 as usize].instrs.push(Instruction { result, inst, address: self.address });
        result
    }

    /// Appends an instruction that defines a value.
    pub fn value(&mut self, block: BlockId, inst: Inst, hint: &str) -> ValueId {
        self.push(block, inst, hint).expect("the instruction defines a value")
    }

    pub fn terminate(&mut self, block: BlockId, terminator: Terminator) {
        self.function.blocks[block.0 as usize].terminator = terminator;
    }

    pub fn has_terminator(&self, block: BlockId) -> bool {
        !matches!(self.function.blocks[block.0 as usize].terminator, Terminator::Unreachable)
    }

    // region Shorthands

    pub fn bin(&mut self, block: BlockId, op: BinOp, ty: Type, lhs: impl Into<Operand>, rhs: impl Into<Operand>, hint: &str) -> ValueId {
        self.value(block, Inst::Bin { op, ty, lhs: lhs.into(), rhs: rhs.into() }, hint)
    }

    pub fn icmp(&mut self, block: BlockId, cond: ICond, ty: Type, lhs: impl Into<Operand>, rhs: impl Into<Operand>, hint: &str) -> ValueId {
        self.value(block, Inst::ICmp { cond, ty, lhs: lhs.into(), rhs: rhs.into() }, hint)
    }

    pub fn cast(&mut self, block: BlockId, cast: Cast, from: Type, value: impl Into<Operand>, to: Type, hint: &str) -> ValueId {
        self.value(block, Inst::Cast { cast, from, value: value.into(), to }, hint)
    }

    pub fn load(&mut self, block: BlockId, ty: Type, ptr: impl Into<Operand>, hint: &str) -> ValueId {
        let align = ty.size(8).map_or(1, |s| s.clamp(1, 16) as u32);
        self.value(block, Inst::Load { ty, ptr: ptr.into(), align, volatile: false }, hint)
    }

    pub fn store(&mut self, block: BlockId, ty: Type, value: impl Into<Operand>, ptr: impl Into<Operand>) {
        let align = ty.size(8).map_or(1, |s| s.clamp(1, 16) as u32);
        self.push(block, Inst::Store { ty, value: value.into(), ptr: ptr.into(), align, volatile: false }, "");
    }

    pub fn select(
        &mut self,
        block: BlockId,
        cond: impl Into<Operand>,
        ty: Type,
        then: impl Into<Operand>,
        els: impl Into<Operand>,
        hint: &str,
    ) -> ValueId {
        self.value(block, Inst::Select { cond: cond.into(), ty, then: then.into(), els: els.into() }, hint)
    }

    pub fn gep(&mut self, block: BlockId, base_ty: Type, ptr: impl Into<Operand>, indices: Vec<(Type, Operand)>, hint: &str) -> ValueId {
        self.value(block, Inst::Gep { base_ty, ptr: ptr.into(), indices, inbounds: false }, hint)
    }

    pub fn call(&mut self, block: BlockId, fn_ty: FnType, callee: Callee, args: Vec<(Type, Operand)>, hint: &str) -> Option<ValueId> {
        self.push(block, Inst::Call { fn_ty, callee, args, tail: false }, hint)
    }

    pub fn alloca(&mut self, block: BlockId, ty: Type, hint: &str) -> ValueId {
        let align = ty.size(8).map_or(1, |s| s.clamp(1, 16) as u32);
        self.value(block, Inst::Alloca { ty, align }, hint)
    }

    // endregion

    pub fn finish(self) -> Function {
        self.function
    }

    pub fn function(&self) -> &Function {
        &self.function
    }
}

/// A name LLVM accepts without quoting: letters, digits, `_`, `.`, `$`, `-`, not starting with a digit.
fn sanitize(hint: &str) -> String {
    let mut s: String =
        hint.chars().map(|c| if c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '$' | '-') { c } else { '_' }).collect();
    if s.is_empty() || s.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        s.insert(0, 'v');
    }
    s
}
