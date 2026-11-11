//! The raising passes: from machine IR over `%State` to source-shaped IR.
//!
//! Promotion turns the registers and the flags into SSA values inside each function, guided by
//! liveness, so a flag no branch reads is never computed and a register travels in a value, not
//! through memory. In ABI mode the inferred signatures apply too: parameters, a frame, calls with
//! arguments, a returned value, and no `%State` at all. Simplification and dead-code elimination
//! clean up after either.

pub mod abi;
pub mod conditions;
pub mod dce;
pub mod frame;
pub mod known;
pub mod liveness;
pub mod promote;
pub mod prototypes;
pub mod simplify;
pub mod stack;
pub mod widths;

use std::collections::{BTreeSet, HashMap};

use ra_asm_ir::Module;

/// How far to raise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Level {
    /// The machine IR as lifted: `F_x(ptr %state)`, every register in memory.
    Machine,
    /// Registers and flags as SSA values; the `%State` interface stays.
    Promoted,
    /// The signatures applied: parameters, frames, results; no `%State`.
    #[default]
    Abi,
}

/// The options of a raise.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    pub level: Level,
}

impl Options {
    pub fn machine() -> Self {
        Self { level: Level::Machine }
    }
}

/// Runs the raising passes over every function of `module`. `roots` are the functions whose
/// callers the lift does not see (see [`abi::infer`]).
pub fn raise(module: &mut Module, roots: &BTreeSet<String>, options: Options) -> HashMap<String, abi::Signature> {
    let signatures = abi::infer(module, roots);
    match options.level {
        Level::Machine => return signatures,
        Level::Promoted => {
            for f in &mut module.functions {
                promote::promote(f);
                clean(f);
            }
        }
        Level::Abi => {
            for f in &mut module.functions {
                let Some(own) = signatures.get(&f.name) else { continue };
                // The constants first: the rewrite recognizes a pushed return address by its value.
                simplify::run(f);
                promote::promote_abi(f, &promote::Abi { own, all: &signatures });
                if own.noreturn && !f.attrs.iter().any(|a| a == "noreturn") {
                    f.attrs.push("noreturn".into());
                }
                clean(f);
                conditions::run(f);
                clean(f);
                frame::run(f);
                clean(f);
                // Known values and branches, to a fixpoint with the cleanup.
                for _ in 0..4 {
                    if !known::run(f) {
                        break;
                    }
                    clean(f);
                }
            }
            // The import wrappers served the machine model; the calls now go to the imports,
            // each declared with the arity the prototype table gives it.
            module.functions.retain(|f| crate::import_of_wrapper(&f.name).is_none());
            // The widths, to a fixpoint: each narrowing exposes the next through the cleanup.
            for _ in 0..4 {
                if !widths::run(module, roots) {
                    break;
                }
                for f in &mut module.functions {
                    clean(f);
                }
            }
            for d in &mut module.declarations {
                if let Some(arity) = prototypes::arity(&d.name)
                    && d.ty.params.len() == 8
                {
                    d.ty = ra_asm_ir::FnType::new(ra_asm_ir::Type::I64, vec![ra_asm_ir::Type::I64; arity.min(8)]);
                }
            }
        }
    }
    signatures
}

fn clean(f: &mut ra_asm_ir::Function) {
    loop {
        let changed = simplify::run(f) | simplify::cse(f);
        dce::run(f);
        if !changed {
            break;
        }
    }
}
