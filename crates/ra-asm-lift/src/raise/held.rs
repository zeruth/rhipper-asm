//! The functions the module steps work over, held frozen. A module step reads what it needs of
//! the other functions from one table of them, a [`Row`] each (the names its operands take, its
//! callees, its parameters and their widths), which the raise writes as it freezes each function
//! and each step keeps as it rewrites, adds or drops one; so a step thaws only the functions it
//! rewrites or whose code it reads, one at a time on each thread, and freezes them back: the
//! raised module is never whole while the steps run. A step that must see several functions at
//! once (the decryptor runs of `strings`, the copies `outline` matches) thaws only those, and
//! [`most_together`] counts them.

use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};

use ra_asm_ir::freeze::{self, Frozen};
use ra_asm_ir::{Function, Module, Operand, Type, ValueId};

/// The most functions one thread held thawed at once in a module step.
static MOST: AtomicUsize = AtomicUsize::new(0);

/// The most functions a module step thawed together, to see them at once.
static TOGETHER: AtomicUsize = AtomicUsize::new(0);

/// The most functions a module step found thawed beside frozen ones, as it began or as it ended.
static BETWEEN: AtomicUsize = AtomicUsize::new(0);

/// The times a module step built the table of the functions again, finding it out of step.
static REBUILT: AtomicUsize = AtomicUsize::new(0);

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

/// The times a module step of any raise of this process found the table of the functions out of
/// step with them and built it again, thawing every function: none when the raise wrote it and
/// every step kept it. A pass case, whose context has no table, builds it once.
pub fn tables_rebuilt() -> usize {
    REBUILT.load(Ordering::Relaxed)
}

/// What the module steps read of one frozen function without thawing it.
#[derive(Default)]
pub(crate) struct Row {
    /// The function's name: a row is the one of the function of its name at its index.
    pub name: String,
    /// The globals and functions its instructions' operands name, each once (see
    /// [`super::stubs::named_in`]): where it takes an address.
    pub named: Vec<String>,
    /// The globals and functions its terminators' operands name, each once.
    pub ended: Vec<String>,
    /// The functions it calls directly, each once.
    pub callees: Vec<String>,
    pub params: Vec<ValueId>,
    /// Whether each parameter is an `i64`.
    pub wide_params: Vec<bool>,
    /// Whether it returns an `i64`.
    pub wide_ret: bool,
}

impl Row {
    pub(crate) fn of(f: &Function) -> Self {
        let mut named: Vec<String> = super::stubs::named_in(f).cloned().collect();
        named.sort_unstable();
        named.dedup();
        let mut ended: Vec<String> = f
            .blocks
            .iter()
            .flat_map(|b| b.terminator.operands())
            .filter_map(|o| match o {
                Operand::Address { global, .. } | Operand::Global(global) => Some(global.clone()),
                _ => None,
            })
            .collect();
        ended.sort_unstable();
        ended.dedup();
        Self {
            name: f.name.clone(),
            named,
            ended,
            callees: super::widths::callees(f),
            params: f.params.clone(),
            wide_params: f.params.iter().map(|p| *f.value_type(*p) == Type::I64).collect(),
            wide_ret: f.ty.ret == Type::I64,
        }
    }
}

/// Whether `rows` are the rows of `frozen`, one each in its order.
fn in_step(rows: &[Row], frozen: &[Frozen]) -> bool {
    rows.len() == frozen.len() && rows.iter().zip(frozen).all(|(r, z)| r.name == z.name)
}

/// `rows` made the rows of `frozen`: as they are when they are in step, built again from every
/// function, each thawed alone, when they are not (a pass case's module, which no raise tabled).
pub(crate) fn table(rows: &mut Vec<Row>, frozen: &[Frozen]) {
    if !in_step(rows, frozen) {
        REBUILT.fetch_add(1, Ordering::Relaxed);
        *rows = map(frozen, |_, f| Row::of(f));
    }
}

/// `each` over the frozen functions `pick` takes, each thawed alone on each thread, frozen back and
/// its row written again; the others are not thawed. Rows out of step with `frozen` stay out of
/// step (an empty name each where none is written), for the next reader to build again.
pub(crate) fn rewrite(
    frozen: &mut [Frozen],
    rows: &mut Vec<Row>,
    pick: impl Fn(usize) -> bool,
    each: impl Fn(usize, &mut Function) + Sync,
) {
    if !in_step(rows, frozen) {
        *rows = (0..frozen.len()).map(|_| Row::default()).collect();
    }
    let mut picked: Vec<(usize, &mut Frozen, &mut Row)> =
        frozen.iter_mut().zip(rows.iter_mut()).enumerate().filter(|(i, _)| pick(*i)).map(|(i, (z, r))| (i, z, r)).collect();
    super::abi::par_each_mut(&mut picked, |_, (i, z, row)| {
        let _held = Thawed::new();
        let mut f = freeze::thaw(z);
        each(*i, &mut f);
        **row = Row::of(&f);
        **z = freeze::freeze(&f);
    });
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
/// A run of [`THAW_RUN`] functions at a time, whose frozen bytes go once it is thawed: the frozen
/// module and the thawed one are never both whole.
pub(crate) fn thaw_all(module: &mut Module) {
    let mut frozen = std::mem::take(&mut module.frozen);
    module.functions.reserve(frozen.len());
    while !frozen.is_empty() {
        let run: Vec<Frozen> = frozen.drain(..THAW_RUN.min(frozen.len())).collect();
        module.functions.extend(super::abi::par_map(&run, |_, z| freeze::thaw(z)));
    }
}

/// The functions [`thaw_all`] thaws before it frees their frozen bytes: enough that every thread
/// has work, few enough that their bytes are a small part of the module's.
const THAW_RUN: usize = 4096;
