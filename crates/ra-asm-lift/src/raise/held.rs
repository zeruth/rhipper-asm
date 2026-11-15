//! The functions the module steps work over, held frozen. A module step reads what it needs of
//! the other functions from tables it builds once (the names, the types, the addresses taken, the
//! widths each function's calls read), so it thaws one function at a time on each thread, works on
//! it and freezes it back: the raised module is never whole while the steps run. A step that must
//! see several functions at once (the decryptor runs of `strings`, the copies `outline` matches)
//! thaws only those, and [`most_together`] counts them.

use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use ra_asm_ir::freeze::{self, Frozen};
use ra_asm_ir::{Function, Module};

/// The most functions one thread held thawed at once in a module step.
static MOST: AtomicUsize = AtomicUsize::new(0);

/// The most functions a module step thawed together, to see them at once.
static TOGETHER: AtomicUsize = AtomicUsize::new(0);

/// The most functions a module step found thawed beside frozen ones, as it began or as it ended.
static BETWEEN: AtomicUsize = AtomicUsize::new(0);

thread_local! {
    /// The functions this thread holds thawed now.
    static THAWED: Cell<usize> = const { Cell::new(0) };
}

/// One function held thawed on this thread while it lives.
struct Thawed;

impl Thawed {
    fn new() -> Self {
        let now = THAWED.get() + 1;
        THAWED.set(now);
        MOST.fetch_max(now, Ordering::Relaxed);
        Self
    }
}

impl Drop for Thawed {
    fn drop(&mut self) {
        THAWED.set(THAWED.get() - 1);
    }
}

/// The most functions one thread held thawed at once in the module steps of any raise of this
/// process: one when no step holds the module.
pub fn most_thawed() -> usize {
    MOST.load(Ordering::Relaxed)
}

/// The most functions a module step of any raise of this process thawed together because it
/// reads them at once; none when no program had a decryptor call or a repeated loop.
pub fn most_together() -> usize {
    TOGETHER.load(Ordering::Relaxed)
}

/// The most functions a module step of any raise of this process found thawed beside frozen ones,
/// when it began or when it ended. None when the module stays frozen from one step to the next:
/// a step adds its functions (the helpers of `outline`, the stubs) frozen, so the next step is
/// handed only frozen ones and does not thaw the module after it.
pub fn thawed_between() -> usize {
    BETWEEN.load(Ordering::Relaxed)
}

/// Records the functions `module` holds thawed beside frozen ones.
fn note_between(module: &Module) {
    if !module.frozen.is_empty() {
        BETWEEN.fetch_max(module.functions.len(), Ordering::Relaxed);
    }
}

/// What `each` makes of each frozen function, thawed one at a time on each thread.
pub(crate) fn map<T: Send>(frozen: &[Frozen], each: impl Fn(usize, &Function) -> T + Sync) -> Vec<T> {
    super::abi::par_map(frozen, |i, z| {
        let _held = Thawed::new();
        each(i, &freeze::thaw(z))
    })
}

/// `each` over each frozen function, thawed one at a time on each thread and frozen back.
pub(crate) fn each_mut(frozen: &mut [Frozen], each: impl Fn(usize, &mut Function) + Sync) {
    super::abi::par_each_mut(frozen, |i, z| {
        let _held = Thawed::new();
        let mut f = freeze::thaw(z);
        each(i, &mut f);
        *z = freeze::freeze(&f);
    });
}

/// What `read` makes of one frozen function, thawed while it reads.
pub(crate) fn read<T>(z: &Frozen, read: impl FnOnce(&Function) -> T) -> T {
    let _held = Thawed::new();
    read(&freeze::thaw(z))
}

/// The functions of `indices` thawed together, in their order, for a step that reads them at once.
pub(crate) fn thaw_together(frozen: &[Frozen], indices: &[usize]) -> Vec<Function> {
    TOGETHER.fetch_max(indices.len(), Ordering::Relaxed);
    let picked: Vec<&Frozen> = indices.iter().map(|&i| &frozen[i]).collect();
    super::abi::par_map(&picked, |_, z| freeze::thaw(z))
}

/// Runs the module step `run` over `module` with its functions frozen: a module handed thawed (a
/// pass case's) is frozen for the step and thawed after it, so the step sees one shape.
pub(crate) fn step(module: &mut Module, run: impl FnOnce(&mut Module)) {
    note_between(module);
    let thawed = !module.functions.is_empty();
    if thawed {
        let functions = std::mem::take(&mut module.functions);
        let more = super::abi::par_map(&functions, |_, f| freeze::freeze(f));
        drop(functions);
        module.frozen.extend(more);
    }
    run(module);
    note_between(module);
    if thawed {
        thaw_all(module);
    }
}

/// Every frozen function of `module` thawed, after its own: what the steps after the raise read.
pub(crate) fn thaw_all(module: &mut Module) {
    let frozen = std::mem::take(&mut module.frozen);
    let thawed = super::abi::par_map(&frozen, |_, z| freeze::thaw(z));
    drop(frozen);
    module.functions.extend(thawed);
}
