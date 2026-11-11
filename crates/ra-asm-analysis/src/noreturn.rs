//! Functions that never return: the imports known not to, and every function whose every path
//! ends in one of them, in a trap, or in a loop.

use std::collections::{BTreeMap, HashSet};

use crate::function::{Function, Terminator};

/// The imports that never return, by name. The C runtime's names cover MSVC and the UCRT.
const IMPORTS: &[&str] = &[
    "ExitProcess",
    "ExitThread",
    "FreeLibraryAndExitThread",
    "RaiseFailFastException",
    "RtlExitUserProcess",
    "RtlExitUserThread",
    "NtTerminateProcess",
    "exit",
    "_exit",
    "_Exit",
    "quick_exit",
    "abort",
    "_abort",
    "terminate",
    "_c_exit",
    "_cexit",
    "_o_exit",
    "_o__exit",
    "__fastfail",
    "_invalid_parameter_noinfo_noreturn",
    "_invoke_watson",
    "_amsg_exit",
    "__report_gsfailure",
    "__report_rangecheckfailure",
    "_CxxThrowException",
    "__std_terminate",
    "__std_exception_destroy_and_rethrow",
    "longjmp",
    "_longjmpex",
    "__longjmp_internal",
];

/// Whether an import, by display name, never returns.
pub fn import_never_returns(name: &str) -> bool {
    IMPORTS.contains(&name)
}

/// Adds to `known` every function that never returns, by its blocks. Returns the entries it added.
pub fn settle(functions: &BTreeMap<u64, Function>, known: &mut HashSet<u64>) -> Vec<u64> {
    let mut added = Vec::new();
    loop {
        let mut changed = false;
        for f in functions.values() {
            if known.contains(&f.entry) || f.blocks.is_empty() {
                continue;
            }
            if never_returns(f, known) {
                known.insert(f.entry);
                added.push(f.entry);
                changed = true;
            }
        }
        if !changed {
            return added;
        }
    }
}

/// A function never returns when no block returns, no block leaves by a path the analysis cannot
/// follow, and every tail call goes to a function that never returns.
fn never_returns(f: &Function, known: &HashSet<u64>) -> bool {
    f.blocks.iter().all(|b| match &b.terminator {
        Terminator::Return | Terminator::IndirectJump | Terminator::Invalid => false,
        Terminator::TailCall(target) => known.contains(target),
        Terminator::ImportTailCall(import) => import_never_returns(import),
        Terminator::FallThrough(_)
        | Terminator::Jump(_)
        | Terminator::CondJump { .. }
        | Terminator::Switch { .. }
        | Terminator::NoReturnCall(_)
        | Terminator::NoReturnImportCall(_)
        | Terminator::Trap
        | Terminator::Halt => true,
    })
}
