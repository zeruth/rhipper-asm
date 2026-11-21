//! The names of a lifted module as C has them.
//!
//! The lift names a function and a static field as the machine does, `Class.name:descriptor`,
//! which is no name a C compiler takes. [`c_names`] renames everything a module defines, declares
//! and refers to into identifiers, so the C backend's source compiles and a runtime written in C
//! defines the functions the module calls under the same names:
//!
//! * `pkg/Class.name:(args)ret` is `pkg_Class__name__args_ret`: the class, the member and the
//!   descriptor apart by two underscores, the descriptor's parameters and its result by one;
//! * a constructor is `new` and a static initializer `static`, which no method is called;
//! * in each part `/` is `_`, and what an identifier cannot hold is an escape: `_1` for `_`, `_2`
//!   for `;`, `_3` for `[`, and `_0` with four hexadecimal digits for any other character;
//! * a name of the lift's own (`jvm.new`, `jvm.text.3`) has its dots as underscores.
//!
//! So `java/lang/StringBuilder.append:(Ljava/lang/String;)Ljava/lang/StringBuilder;` is
//! `java_lang_StringBuilder__append__Ljava_lang_String_2_Ljava_lang_StringBuilder_2`.

use std::collections::HashMap;

use ra_asm_ir::{Callee, Const, Inst, Module, Operand};

fn escape(part: &str, out: &mut String) {
    for c in part.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '$' => out.push(c),
            '/' | ')' => out.push('_'),
            '_' => out.push_str("_1"),
            ';' => out.push_str("_2"),
            '[' => out.push_str("_3"),
            '(' => {}
            other => {
                let mut units = [0u16; 2];
                for unit in other.encode_utf16(&mut units) {
                    out.push_str(&format!("_0{unit:04x}"));
                }
            }
        }
    }
}

/// The identifier a C source names `name` by.
#[must_use]
pub fn c_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 8);
    let Some((member, descriptor)) = name.split_once(':') else {
        // A name of the lift's own: `jvm.new`, `jvm.text.3`.
        for part in name.split('.') {
            if !out.is_empty() {
                out.push('_');
            }
            escape(part, &mut out);
        }
        return out;
    };
    let (class, member) = member.rsplit_once('.').unwrap_or(("", member));
    escape(class, &mut out);
    out.push_str("__");
    match member {
        "<init>" => out.push_str("new"),
        "<clinit>" => out.push_str("static"),
        member => escape(member, &mut out),
    }
    out.push_str("__");
    escape(descriptor, &mut out);
    out
}

/// Renames the functions, the declarations and the globals of `module`, and every reference to
/// them, to the identifiers of [`c_name`].
pub fn c_names(module: &mut Module) {
    let mut names: HashMap<String, String> = HashMap::new();
    let mut rename = |name: &mut String| {
        let new = names.entry(name.clone()).or_insert_with(|| c_name(name)).clone();
        *name = new;
    };
    fn constant(c: &mut Const, rename: &mut impl FnMut(&mut String)) {
        match c {
            Const::Global(name) | Const::Address { global: name, .. } => rename(name),
            Const::Array(items) | Const::Struct(items) => items.iter_mut().for_each(|item| constant(item, rename)),
            _ => {}
        }
    }
    for global in &mut module.globals {
        rename(&mut global.name);
        if let Some(init) = &mut global.init {
            constant(init, &mut rename);
        }
    }
    for declaration in &mut module.declarations {
        rename(&mut declaration.name);
    }
    for function in &mut module.functions {
        rename(&mut function.name);
        for block in &mut function.blocks {
            for instruction in &mut block.instrs {
                if let Inst::Call { callee: Callee::Direct(name), .. } = &mut instruction.inst {
                    rename(name);
                }
                for operand in instruction.inst.operands_mut() {
                    if let Operand::Global(name) | Operand::Address { global: name, .. } = operand {
                        rename(name);
                    }
                }
            }
            for operand in block.terminator.operands_mut() {
                if let Operand::Global(name) | Operand::Address { global: name, .. } = operand {
                    rename(name);
                }
            }
        }
    }
    for variable in &mut module.variables {
        rename(&mut variable.global);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_an_identifier_that_says_what_it_was() {
        for (name, expected) in [
            ("jvm.new", "jvm_new"),
            ("jvm.text.12", "jvm_text_12"),
            ("Opcodes.counter:I", "Opcodes__counter__I"),
            ("Opcodes.<init>:(J)V", "Opcodes__new__J_V"),
            ("Opcodes.<clinit>:()V", "Opcodes__static___V"),
            ("Thrown$Mine.code:I", "Thrown$Mine__code__I"),
            ("a/b_c/D.index:([II)I", "a_b_1c_D__index___3II_I"),
            ("a/D.index:(II)I", "a_D__index__II_I"),
            (
                "java/lang/StringBuilder.append:(Ljava/lang/String;)Ljava/lang/StringBuilder;",
                "java_lang_StringBuilder__append__Ljava_lang_String_2_Ljava_lang_StringBuilder_2",
            ),
            ("A.lambda$main$0:()D", "A__lambda$main$0___D"),
            ("A.caf\u{e9}:()V", "A__caf_000e9___V"),
        ] {
            assert_eq!(c_name(name), expected);
            assert!(c_name(name).chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$'));
        }
    }
}
