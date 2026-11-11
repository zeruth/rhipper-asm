//! The raising passes: from machine IR over `%State` to source-shaped IR.
//!
//! The first pass promotes the general registers and the flags to SSA values inside each function,
//! guided by liveness, so a flag no branch reads is never computed and a register travels in a
//! value, not through memory. Dead-code elimination then removes what no one uses. The later
//! passes (stack frames, signatures, types) build on this.

pub mod dce;
pub mod promote;
pub mod simplify;

use ra_asm_ir::Module;

/// The options of a raise.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Keep the machine IR: run no pass.
    pub machine: bool,
}

/// Runs the raising passes over every function of `module`.
pub fn raise(module: &mut Module, options: Options) {
    if options.machine {
        return;
    }
    for f in &mut module.functions {
        promote::promote(f);
        loop {
            let changed = simplify::run(f);
            dce::run(f);
            if !changed {
                break;
            }
        }
    }
}
