//! The functions of outlined rows ([`crate::Row::outlined`]): a row whose semantics runs to
//! hundreds of operations, a coprocessor's command or a packed multiply, is built once as
//! `void @<row>(ptr %state)` over the state it reads and writes, and every instruction of it is a
//! call. The function is an IR function like any other, so no backend knows the machine.
//!
//! A row's semantics takes its registers and constants from the word's fields, so one function
//! serves the instructions of one row whose fields have the same values; the row's name, then
//! those values, name it.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use ra_asm_ir::{Callee, FnType, Function, Operand, Terminator, Type};

use crate::sem::{Fields, Ir, Lift};
use crate::table::Decoded;

/// The functions of a program's outlined rows, each built when the first instruction that needs
/// it is lifted; the module the lifted functions go in takes these too.
#[derive(Debug, Default)]
pub struct Outlines {
    /// The name of the function of each row, by its mask and bits, and field values.
    names: HashMap<(u32, u32, Vec<i64>), String>,
    taken: HashSet<String>,
    functions: Vec<Function>,
}

impl Outlines {
    pub fn new() -> Self {
        Outlines::default()
    }

    /// The functions built so far, in the order they were first needed.
    pub fn functions(&self) -> &[Function] {
        &self.functions
    }

    pub fn into_functions(self) -> Vec<Function> {
        self.functions
    }

    /// Lifts the instruction `decoded`, whose row is outlined, into `ir` as a call to the row's
    /// function, built from `lift` over `fields` the first time.
    pub(crate) fn call(&mut self, ir: &mut Ir<'_>, decoded: &Decoded<'_>, lift: Lift, fields: &Fields<'_>) {
        let key = (decoded.row.mask, decoded.row.bits, decoded.values.clone());
        let name = if let Some(name) = self.names.get(&key) {
            name.clone()
        } else {
            let name = self.fresh(&symbol(decoded.row.name, &decoded.values));
            let mut body = Ir::named(&name, ir.spec, ir.registers, fields.address);
            // The function serves every instruction of the row, so none of its operations is at one address.
            body.b.address = None;
            body.outlined = true;
            lift(&mut body, fields);
            body.b.terminate(body.block, Terminator::Ret(None));
            self.functions.push(body.b.finish());
            self.names.insert(key, name.clone());
            name
        };
        let args = vec![(Type::Ptr, Operand::Local(ir.state))];
        ir.b.call(ir.block, FnType::new(Type::Void, vec![Type::Ptr]), Callee::Direct(name), args, "");
    }

    /// `name`, or `name` with the first number that makes it a name no other function has.
    fn fresh(&mut self, name: &str) -> String {
        let mut fresh = name.to_owned();
        let mut n = 2;
        while self.taken.contains(&fresh) {
            fresh = format!("{name}_{n}");
            n += 1;
        }
        self.taken.insert(fresh.clone());
        fresh
    }
}

/// The name of the function of row `row` over the field values `values`: the row's name with
/// what is not a letter or a digit as `_`, then each value, a negative one after an `m`.
fn symbol(row: &str, values: &[i64]) -> String {
    let mut name: String = row.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
    for &value in values {
        let sign = if value < 0 { "m" } else { "" };
        write!(name, "_{sign}{}", value.unsigned_abs()).unwrap();
    }
    name
}

#[cfg(test)]
mod tests;
