//! The second half of the reader: a function with its body. The header gives the parameters,
//! every block starts with its label and ends with its terminator, and every instruction sits on
//! one line with the value it defines in front and the address the printer left in its comment.
//!
//! The values and the blocks are numbered before the body is read: a `phi` names a value or a
//! block the text defines later, and the ids follow the order of definition, parameters first, as
//! the builder gives them.

use std::collections::HashMap;

use super::{ParseError, Parser, Tok, float_of, refusal, unescape};
use crate::inst::{AtomicOp, BinOp, Callee, Cast, FCond, ICond, Inst, Instruction, Operand, Terminator};
use crate::module::{Block, BlockId, Function, Linkage, NameIndex, ValueId};
use crate::types::{FnType, Type};

const BIN_OPS: [BinOp; 18] = [
    BinOp::Add,
    BinOp::Sub,
    BinOp::Mul,
    BinOp::UDiv,
    BinOp::SDiv,
    BinOp::URem,
    BinOp::SRem,
    BinOp::Shl,
    BinOp::LShr,
    BinOp::AShr,
    BinOp::And,
    BinOp::Or,
    BinOp::Xor,
    BinOp::FAdd,
    BinOp::FSub,
    BinOp::FMul,
    BinOp::FDiv,
    BinOp::FRem,
];

const CASTS: [Cast; 12] = [
    Cast::Trunc,
    Cast::ZExt,
    Cast::SExt,
    Cast::FPTrunc,
    Cast::FPExt,
    Cast::FPToUI,
    Cast::FPToSI,
    Cast::UIToFP,
    Cast::SIToFP,
    Cast::PtrToInt,
    Cast::IntToPtr,
    Cast::Bitcast,
];

const ICONDS: [ICond; 10] =
    [ICond::Eq, ICond::Ne, ICond::Ugt, ICond::Uge, ICond::Ult, ICond::Ule, ICond::Sgt, ICond::Sge, ICond::Slt, ICond::Sle];

const FCONDS: [FCond; 14] = [
    FCond::Oeq,
    FCond::Ogt,
    FCond::Oge,
    FCond::Olt,
    FCond::Ole,
    FCond::One,
    FCond::Ord,
    FCond::Ueq,
    FCond::Ugt,
    FCond::Uge,
    FCond::Ult,
    FCond::Ule,
    FCond::Une,
    FCond::Uno,
];

/// The values and the blocks a function defines, by name and in the order of their definition.
#[derive(Default)]
struct Names {
    values: HashMap<String, ValueId>,
    /// The values' names by id, which is the function's table.
    defined: Vec<String>,
    blocks: HashMap<String, BlockId>,
}

impl Names {
    /// Numbers a value; `false` when the name is taken.
    fn define_value(&mut self, name: &str) -> bool {
        if self.values.contains_key(name) {
            return false;
        }
        self.values.insert(name.to_owned(), ValueId(self.defined.len() as u32));
        self.defined.push(name.to_owned());
        true
    }

    /// Numbers a block; `false` when the name is taken.
    fn define_block(&mut self, name: &str) -> bool {
        if self.blocks.contains_key(name) {
            return false;
        }
        self.blocks.insert(name.to_owned(), BlockId(self.blocks.len() as u32));
        true
    }
}

/// The name of a block label: bare as a word, or quoted and escaped as the printer quotes a name.
fn label_name(tok: &Tok<'_>) -> Option<String> {
    match tok {
        Tok::Word(word) => Some((*word).to_owned()),
        Tok::Str(text) => String::from_utf8(unescape(text)?).ok(),
        _ => None,
    }
}

impl Parser<'_> {
    /// `define [internal] [conv] T @name(T %a, U %b[, ...])[ attributes] {`, the blocks, and the `}`
    /// on a line of its own.
    pub(super) fn function(&mut self) -> Result<Function, ParseError> {
        let line = self.here();
        self.expect_word("define")?;
        let linkage = if self.eat_word("internal") { Linkage::Internal } else { Linkage::External };
        let conv = self.conv();
        let ret = self.ty()?;
        let name = self.global_name()?;
        self.expect(b'(')?;
        let (mut param_types, mut param_at) = (Vec::new(), Vec::new());
        let mut varargs = false;
        if !self.eat(b')') {
            loop {
                if self.peek() == Some(&Tok::Ellipsis) {
                    self.at += 1;
                    self.expect(b')')?;
                    varargs = true;
                    break;
                }
                param_types.push(self.ty()?);
                if !matches!(self.peek(), Some(Tok::Local(_))) {
                    return Err(self.refuse("the `%` name of the parameter is expected"));
                }
                param_at.push(self.at);
                self.at += 1;
                if self.eat(b')') {
                    break;
                }
                self.expect(b',')?;
            }
        }
        let attrs = self.attributes(line);
        self.expect(b'{')?;
        let open = self.at - 1;
        let close = self.body_end(open).ok_or_else(|| {
            let last = self.tokens.last().map_or(line, |t| t.line);
            refusal(last, "end of file", "the body does not close with a `}` on a line of its own")
        })?;
        let names = self.defined_names(&param_at, open, close)?;
        let params: Vec<ValueId> = names.defined[..param_at.len()].iter().map(|n| names.values[n]).collect();
        let mut value_types = vec![Type::Void; names.defined.len()];
        for (id, ty) in params.iter().zip(&param_types) {
            value_types[id.0 as usize] = ty.clone();
        }
        let mut blocks = Vec::new();
        while self.at < close {
            blocks.push(self.block(&names, &mut value_types, BlockId(blocks.len() as u32))?);
        }
        self.at += 1;
        Ok(Function {
            name,
            ty: FnType { ret, params: param_types, varargs },
            params,
            blocks,
            linkage,
            conv,
            attrs,
            address: self.addresses.get(&line).copied(),
            value_names: names.defined,
            value_types,
            name_index: NameIndex::default(),
        })
    }

    /// The index of the `}` that closes the body opened by the `{` at `open`: the first one that
    /// starts a line.
    fn body_end(&self, open: usize) -> Option<usize> {
        (open + 1..self.tokens.len()).find(|&i| self.tokens[i].tok == Tok::Punct(b'}') && self.tokens[i - 1].line != self.tokens[i].line)
    }

    /// The values and the blocks defined between `open` and `close`, after the parameters at the
    /// token indices `params`: a value is the `%name` that starts a line and takes `=`, a block the
    /// label that starts a line and takes `:`.
    fn defined_names(&self, params: &[usize], open: usize, close: usize) -> Result<Names, ParseError> {
        let mut names = Names::default();
        let twice = |i: usize, what: &str| {
            let t = &self.tokens[i];
            refusal(t.line, &self.src[t.start..t.end], what)
        };
        for &i in params {
            if let Tok::Local(name) = &self.tokens[i].tok
                && !names.define_value(name)
            {
                return Err(twice(i, "is a parameter named twice"));
            }
        }
        for i in open + 1..close {
            if self.tokens[i - 1].line == self.tokens[i].line {
                continue;
            }
            match (&self.tokens[i].tok, &self.tokens[i + 1].tok) {
                (Tok::Local(_), Tok::Punct(b'=')) if self.cmpxchg_pair(i) => {}
                (Tok::Local(name), Tok::Punct(b'=')) if !names.define_value(name) => return Err(twice(i, "is defined twice")),
                (tok, Tok::Punct(b':')) => {
                    if let Some(name) = label_name(tok)
                        && !names.define_block(&name)
                    {
                        return Err(twice(i, "names two blocks"));
                    }
                }
                _ => {}
            }
        }
        Ok(names)
    }

    /// Whether the `%name =` at token `at` names the pair of a `cmpxchg`, which is no value of the
    /// function: the value is the one the next line takes out of the pair.
    fn cmpxchg_pair(&self, at: usize) -> bool {
        self.tokens.get(at + 2).is_some_and(|t| t.tok == Tok::Word("cmpxchg"))
    }

    /// `%value = extractvalue { T, i1 } %pair, 0`, the line after a `cmpxchg` that defines its
    /// value; gives the value.
    fn cmpxchg_value(&mut self, names: &Names) -> Result<ValueId, ParseError> {
        let id = match self.peek() {
            Some(Tok::Local(name)) => self.value_of(name, names)?,
            _ => return Err(self.refuse("the value taken out of the `cmpxchg` is expected")),
        };
        self.at += 1;
        self.expect(b'=')?;
        self.expect_word("extractvalue")?;
        self.expect(b'{')?;
        self.ty()?;
        self.expect(b',')?;
        self.expect_word("i1")?;
        self.expect(b'}')?;
        if !matches!(self.peek(), Some(Tok::Local(_))) {
            return Err(self.refuse("the pair of the `cmpxchg` is expected"));
        }
        self.at += 1;
        self.expect(b',')?;
        let field: u64 = self.number("the field of the pair")?;
        if field != 0 {
            return Err(self.refuse("the value of a `cmpxchg` is the pair's field 0"));
        }
        Ok(id)
    }

    /// A label, its instructions and its terminator; the type of each value defined lands in
    /// `value_types`.
    fn block(&mut self, names: &Names, value_types: &mut [Type], id: BlockId) -> Result<Block, ParseError> {
        let line = self.here();
        let name = self.label()?;
        let address = self.addresses.get(&line).copied();
        let mut instrs = Vec::new();
        let terminator = loop {
            let line = self.here();
            let pair = matches!(self.peek(), Some(Tok::Local(_))) && self.cmpxchg_pair(self.at);
            let mut result = match self.peek() {
                Some(Tok::Local(_)) if pair => {
                    self.at += 2;
                    None
                }
                Some(Tok::Local(name)) => {
                    let id = self.value_of(name, names)?;
                    self.at += 1;
                    self.expect(b'=')?;
                    Some(id)
                }
                _ => None,
            };
            if matches!(self.word(), Some("ret" | "br" | "switch" | "unreachable")) {
                if result.is_some() {
                    return Err(self.refuse("a terminator defines no value"));
                }
                let terminator = self.terminator(names)?;
                self.end_of_line(line)?;
                break terminator;
            }
            let opcode = self.at;
            let inst = self.inst(names)?;
            let address = self.addresses.get(&line).copied();
            if pair {
                self.end_of_line(line)?;
                let line = self.here();
                result = Some(self.cmpxchg_value(names)?);
                self.end_of_line(line)?;
            }
            if let Some(id) = result {
                let ty = inst.result_type();
                if ty == Type::Void {
                    self.at = opcode;
                    return Err(self.refuse("defines no value"));
                }
                value_types[id.0 as usize] = ty;
            }
            if !pair {
                self.end_of_line(line)?;
            }
            instrs.push(Instruction { result, inst, address });
        };
        Ok(Block { id, name, instrs, terminator, address })
    }

    /// `name:` or `"a name":`; a word without the colon is an instruction where a label should be.
    fn label(&mut self) -> Result<String, ParseError> {
        let colon = self.tokens.get(self.at + 1).is_some_and(|t| t.tok == Tok::Punct(b':'));
        let Some(name) = self.peek().and_then(label_name).filter(|_| colon) else { return Err(self.refuse("a block label is expected")) };
        self.at += 2;
        Ok(name)
    }

    /// The value the function defines under `name`, which is the next token.
    fn value_of(&self, name: &str, names: &Names) -> Result<ValueId, ParseError> {
        names.values.get(name).copied().ok_or_else(|| self.refuse("is not a value of the function"))
    }

    /// `%name` of a block.
    fn block_ref(&mut self, names: &Names) -> Result<BlockId, ParseError> {
        let block = match self.peek() {
            Some(Tok::Local(name)) => names.blocks.get(name).copied(),
            _ => None,
        };
        let block = block.ok_or_else(|| self.refuse("is not a block of the function"))?;
        self.at += 1;
        Ok(block)
    }

    /// An operand of the type `ty`, which decides how a float, a splat and an address are read.
    fn operand(&mut self, ty: &Type, names: &Names) -> Result<Operand, ParseError> {
        let operand = match self.peek() {
            Some(Tok::Local(name)) => Operand::Local(self.value_of(name, names)?),
            Some(Tok::Int(v)) => Operand::Int(*v),
            Some(Tok::Hex(digits)) => match float_of(ty, digits) {
                Some(v) => Operand::Float(v),
                None => return Err(self.refuse(&format!("is not a float constant of `{ty}`"))),
            },
            Some(Tok::Word("true")) => Operand::Int(1),
            Some(Tok::Word("false")) => Operand::Int(0),
            Some(Tok::Word("null")) => Operand::Null,
            Some(Tok::Word("undef")) => Operand::Undef,
            Some(Tok::Word("zeroinitializer")) => Operand::Zero,
            Some(Tok::Global(name)) => Operand::Global(name.clone()),
            Some(Tok::Word("getelementptr" | "ptrtoint")) => {
                let (global, offset) = self.address(ty)?;
                return Ok(Operand::Address { global, offset });
            }
            Some(Tok::Punct(b'<')) => return self.splat(ty, names),
            Some(_) => return Err(self.refuse("is not an operand")),
            None => return Err(self.refuse("an operand is expected")),
        };
        self.at += 1;
        Ok(operand)
    }

    /// `<T c, T c, …>`: a scalar constant on a vector type, which the printer writes in every lane.
    fn splat(&mut self, ty: &Type, names: &Names) -> Result<Operand, ParseError> {
        let start = self.at;
        let Type::Vector(count, element) = ty else { return Err(self.refuse(&format!("is not an operand of `{ty}`"))) };
        self.expect(b'<')?;
        let mut lanes = Vec::new();
        loop {
            self.typed(element)?;
            lanes.push(self.operand(element, names)?);
            if self.eat(b'>') {
                break;
            }
            self.expect(b',')?;
        }
        if lanes.len() as u64 == u64::from(*count) && lanes.iter().all(|lane| *lane == lanes[0]) {
            return Ok(lanes.swap_remove(0));
        }
        self.at = start;
        Err(self.refuse("the lanes of the vector are not one constant"))
    }

    /// `a, b`, both of `ty`.
    fn pair(&mut self, ty: &Type, names: &Names) -> Result<(Operand, Operand), ParseError> {
        let lhs = self.operand(ty, names)?;
        self.expect(b',')?;
        let rhs = self.operand(ty, names)?;
        Ok((lhs, rhs))
    }

    /// `ptr p`.
    fn pointer(&mut self, names: &Names) -> Result<Operand, ParseError> {
        self.expect_word("ptr")?;
        self.operand(&Type::Ptr, names)
    }

    /// `i32 i`, the index of a vector lane.
    fn index(&mut self, names: &Names) -> Result<Operand, ParseError> {
        self.expect_word("i32")?;
        self.operand(&Type::I32, names)
    }

    /// `, align N`.
    fn align(&mut self) -> Result<u32, ParseError> {
        self.expect(b',')?;
        self.expect_word("align")?;
        self.number("an alignment")
    }

    /// One instruction, from its opcode to the end of its operands.
    fn inst(&mut self, names: &Names) -> Result<Inst, ParseError> {
        let Some(word) = self.word() else { return Err(self.refuse("an instruction is expected")) };
        if let Some(op) = BIN_OPS.into_iter().find(|op| op.name() == word) {
            self.at += 1;
            let ty = self.ty()?;
            let (lhs, rhs) = self.pair(&ty, names)?;
            return Ok(Inst::Bin { op, ty, lhs, rhs });
        }
        if let Some(cast) = CASTS.into_iter().find(|cast| cast.name() == word) {
            self.at += 1;
            let from = self.ty()?;
            let value = self.operand(&from, names)?;
            self.expect_word("to")?;
            let to = self.ty()?;
            return Ok(Inst::Cast { cast, from, value, to });
        }
        let opcode = self.at;
        self.at += 1;
        let inst = match word {
            "alloca" => {
                let ty = self.ty()?;
                let align = self.align()?;
                Inst::Alloca { ty, align }
            }
            "load" => {
                let volatile = self.eat_word("volatile");
                let ty = self.ty()?;
                self.expect(b',')?;
                let ptr = self.pointer(names)?;
                let align = self.align()?;
                Inst::Load { ty, ptr, align, volatile }
            }
            "store" => {
                let volatile = self.eat_word("volatile");
                let ty = self.ty()?;
                let value = self.operand(&ty, names)?;
                self.expect(b',')?;
                let ptr = self.pointer(names)?;
                let align = self.align()?;
                Inst::Store { ty, value, ptr, align, volatile }
            }
            "icmp" => {
                let cond = ICONDS.into_iter().find(|c| Some(c.name()) == self.word());
                let cond = cond.ok_or_else(|| self.refuse("is not an integer comparison"))?;
                self.at += 1;
                let ty = self.ty()?;
                let (lhs, rhs) = self.pair(&ty, names)?;
                Inst::ICmp { cond, ty, lhs, rhs }
            }
            "fcmp" => {
                let cond = FCONDS.into_iter().find(|c| Some(c.name()) == self.word());
                let cond = cond.ok_or_else(|| self.refuse("is not a float comparison"))?;
                self.at += 1;
                let ty = self.ty()?;
                let (lhs, rhs) = self.pair(&ty, names)?;
                Inst::FCmp { cond, ty, lhs, rhs }
            }
            "select" => {
                self.expect_word("i1")?;
                let cond = self.operand(&Type::I1, names)?;
                self.expect(b',')?;
                let ty = self.ty()?;
                let then = self.operand(&ty, names)?;
                self.expect(b',')?;
                self.typed(&ty)?;
                let els = self.operand(&ty, names)?;
                Inst::Select { cond, ty, then, els }
            }
            "getelementptr" => {
                let inbounds = self.eat_word("inbounds");
                let base_ty = self.ty()?;
                self.expect(b',')?;
                let ptr = self.pointer(names)?;
                let mut indices = Vec::new();
                while self.eat(b',') {
                    let ty = self.ty()?;
                    let index = self.operand(&ty, names)?;
                    indices.push((ty, index));
                }
                Inst::Gep { base_ty, ptr, indices, inbounds }
            }
            "extractelement" => {
                let ty = self.ty()?;
                let vector = self.operand(&ty, names)?;
                self.expect(b',')?;
                let index = self.index(names)?;
                Inst::ExtractElement { ty, vector, index }
            }
            "insertelement" => {
                let ty = self.ty()?;
                let vector = self.operand(&ty, names)?;
                self.expect(b',')?;
                let element_ty = self.ty()?;
                let element = self.operand(&element_ty, names)?;
                self.expect(b',')?;
                let index = self.index(names)?;
                Inst::InsertElement { ty, vector, element, index }
            }
            "shufflevector" => {
                let ty = self.ty()?;
                let a = self.operand(&ty, names)?;
                self.expect(b',')?;
                self.typed(&ty)?;
                let b = self.operand(&ty, names)?;
                self.expect(b',')?;
                let mask = self.mask()?;
                Inst::ShuffleVector { ty, a, b, mask }
            }
            "tail" | "call" => {
                let tail = word == "tail";
                if tail {
                    self.expect_word("call")?;
                }
                self.call(names, tail)?
            }
            "phi" => {
                let ty = self.ty()?;
                let mut incoming = Vec::new();
                loop {
                    self.expect(b'[')?;
                    let value = self.operand(&ty, names)?;
                    self.expect(b',')?;
                    let block = self.block_ref(names)?;
                    self.expect(b']')?;
                    incoming.push((value, block));
                    if !self.eat(b',') {
                        break;
                    }
                }
                Inst::Phi { ty, incoming }
            }
            "freeze" => {
                let ty = self.ty()?;
                let value = self.operand(&ty, names)?;
                Inst::Freeze { ty, value }
            }
            "atomicrmw" => {
                let op = AtomicOp::ALL.into_iter().find(|op| Some(op.name()) == self.word());
                let op = op.ok_or_else(|| self.refuse("is not an atomic operation"))?;
                self.at += 1;
                let ptr = self.pointer(names)?;
                self.expect(b',')?;
                let ty = self.ty()?;
                let value = self.operand(&ty, names)?;
                self.expect_word("seq_cst")?;
                Inst::AtomicRmw { op, ty, ptr, value }
            }
            "cmpxchg" => {
                let ptr = self.pointer(names)?;
                self.expect(b',')?;
                let ty = self.ty()?;
                let expected = self.operand(&ty, names)?;
                self.expect(b',')?;
                self.typed(&ty)?;
                let new = self.operand(&ty, names)?;
                self.expect_word("seq_cst")?;
                self.expect_word("seq_cst")?;
                Inst::CmpXchg { ty, ptr, expected, new }
            }
            _ => {
                self.at = opcode;
                return Err(self.refuse("is not an instruction"));
            }
        };
        Ok(inst)
    }

    /// `<N x i32> <i32 m, i32 poison, …>`, the mask of a shuffle.
    fn mask(&mut self) -> Result<Vec<i32>, ParseError> {
        self.expect(b'<')?;
        let count: u64 = self.number("the lane count of the mask")?;
        self.expect_word("x")?;
        self.expect_word("i32")?;
        self.expect(b'>')?;
        let start = self.at;
        self.expect(b'<')?;
        let mut mask = Vec::new();
        if !self.eat(b'>') {
            loop {
                self.expect_word("i32")?;
                mask.push(if self.eat_word("poison") { -1 } else { self.number("a lane of the mask")? });
                if self.eat(b'>') {
                    break;
                }
                self.expect(b',')?;
            }
        }
        if mask.len() as u64 != count {
            self.at = start;
            return Err(self.refuse("the mask has not the lane count of its type"));
        }
        Ok(mask)
    }

    /// After `call`: the type, the callee and the arguments. A varargs call spells the whole
    /// function type; the others only the return type, and the parameters are the arguments'.
    fn call(&mut self, names: &Names, tail: bool) -> Result<Inst, ParseError> {
        let ty = self.ty()?;
        let callee = match self.peek() {
            Some(Tok::Global(name)) => {
                let callee = Callee::Direct(name.clone());
                self.at += 1;
                callee
            }
            _ => Callee::Indirect(self.operand(&Type::Ptr, names)?),
        };
        self.expect(b'(')?;
        let mut args = Vec::new();
        if !self.eat(b')') {
            loop {
                let ty = self.ty()?;
                let arg = self.operand(&ty, names)?;
                args.push((ty, arg));
                if self.eat(b')') {
                    break;
                }
                self.expect(b',')?;
            }
        }
        let fn_ty = match ty {
            Type::Func(fn_ty) => *fn_ty,
            ret => FnType { ret, params: args.iter().map(|(ty, _)| ty.clone()).collect(), varargs: false },
        };
        Ok(Inst::Call { fn_ty, callee, args, tail })
    }

    /// `ret`, `br`, `switch` or `unreachable`, with its operands.
    fn terminator(&mut self, names: &Names) -> Result<Terminator, ParseError> {
        let word = self.word();
        self.at += 1;
        let terminator = match word {
            Some("ret") => {
                if self.eat_word("void") {
                    Terminator::Ret(None)
                } else {
                    let ty = self.ty()?;
                    let value = self.operand(&ty, names)?;
                    Terminator::Ret(Some((ty, value)))
                }
            }
            Some("br") => {
                if self.eat_word("label") {
                    Terminator::Br(self.block_ref(names)?)
                } else {
                    self.expect_word("i1")?;
                    let cond = self.operand(&Type::I1, names)?;
                    self.expect(b',')?;
                    self.expect_word("label")?;
                    let then = self.block_ref(names)?;
                    self.expect(b',')?;
                    self.expect_word("label")?;
                    let els = self.block_ref(names)?;
                    Terminator::CondBr { cond, then, els }
                }
            }
            Some("switch") => {
                let ty = self.ty()?;
                let value = self.operand(&ty, names)?;
                self.expect(b',')?;
                self.expect_word("label")?;
                let default = self.block_ref(names)?;
                self.expect(b'[')?;
                let mut cases = Vec::new();
                while !self.eat(b']') {
                    self.typed(&ty)?;
                    let case = self.number("a case value")?;
                    self.expect(b',')?;
                    self.expect_word("label")?;
                    cases.push((case, self.block_ref(names)?));
                }
                Terminator::Switch { ty, value, default, cases }
            }
            Some("unreachable") => Terminator::Unreachable,
            _ => {
                self.at -= 1;
                return Err(self.refuse("is not a terminator"));
            }
        };
        Ok(terminator)
    }
}
