//! The steps of the raise, by name: what a pass case, a bisection or a report names when it means
//! one pass. [`step`] gives the step of a name and [`steps`] the order [`super::raise`] runs them
//! in at a level, which is the list it walks.
//!
//! A step is the pass with what the raise runs around it: a rewriting step ends with the cleanup
//! (the simplification, the common subexpressions and the dead code, to a fixpoint), and a step
//! the raise repeats is its whole repetition. So `simplify` and `dce` are steps of their own for
//! a case to name, and the order lists `simplify` once, where the raise runs it alone ahead of
//! the inference. `stack` is the analysis the inference and the promotion read: it has a name and
//! no place of its own in the order.
//!
//! A step can be broken on purpose, which is how a bisection is tested: when the environment
//! variable [`FAULT`] names a step, every function the step is handed returns [`FAULTY`] in
//! place of its integer result once it ran, wherever the step runs (the raise, a pass case).
//! `abi` and `stack` rewrite nothing and are not broken.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use ra_asm_ir::{Callee, Function, Global, Inst, Module, Operand, Terminator, Type};

use super::abi::{Signature, SummaryCache, VtableIndex};
use super::held::Row;
use super::known::Constants;
use super::liveness::slot_pointers;
use super::stack::{CalleeStack, StackInfo};
use super::{Level, Options, clean};

/// What a step reads beside the code it works on.
pub struct Context<'a> {
    /// The functions whose callers the lift does not see.
    pub roots: &'a BTreeSet<String>,
    pub options: &'a Options,
    /// The signatures the `abi` step inferred; none ahead of it.
    pub signatures: &'a HashMap<String, Signature>,
    pub vtables: VtableIndex<'a>,
    pub summaries: SummaryCache<'a>,
    /// The stack dwords a call through a pointer hands on past the universal six (see
    /// [`super::stubs::extra_dwords`]); none ahead of the signatures.
    pub extra_dwords: usize,
    /// The index of the read-only globals the function steps are handed, built by the first step
    /// that reads it: built for each function, it was the functions times the globals' fields.
    constants: OnceLock<Constants>,
    /// The most a call through a pointer pops (see [`super::stores::most_pops`]), read from the
    /// module's globals once a step first asks.
    pub most_pops: OnceLock<Option<i64>>,
    /// What the module steps read of the frozen functions, a row each in the module's order (see
    /// [`super::held::Row`]): written as the raise freezes each function, and kept by each step
    /// that rewrites, adds or drops one.
    rows: Mutex<Vec<Row>>,
}

impl<'a> Context<'a> {
    pub fn new(roots: &'a BTreeSet<String>, options: &'a Options, signatures: &'a HashMap<String, Signature>) -> Self {
        Self {
            roots,
            options,
            signatures,
            vtables: VtableIndex::new(&options.vtables),
            summaries: SummaryCache::new(signatures, options.target()),
            extra_dwords: super::stubs::extra_dwords(signatures, roots, options.target()),
            constants: OnceLock::new(),
            most_pops: OnceLock::new(),
            rows: Mutex::new(Vec::new()),
        }
    }

    /// The rows of the frozen functions, held for one module step.
    pub(crate) fn rows(&self) -> MutexGuard<'_, Vec<Row>> {
        self.rows.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The index of `globals`, when it is the slice the context indexed first.
    fn constants(&self, globals: &[Global]) -> Option<&Constants> {
        let index = self.constants.get_or_init(|| Constants::of(globals));
        index.indexes(globals).then_some(index)
    }
}

/// The function of a step, by what it works on.
#[derive(Clone, Copy)]
pub enum Pass {
    /// Reads one machine function and changes nothing; `None` for a function with no state
    /// parameter, which is not a machine function.
    Stack(fn(&Function, &Context<'_>) -> Option<StackInfo>),
    /// Reads the machine functions of the module and gives their signatures.
    Signatures(fn(&Module, &Context<'_>) -> HashMap<String, Signature>),
    /// Rewrites one function, reading the module's globals: the functions go side by side.
    Function(fn(&mut Function, &Context<'_>, &[Global])),
    /// Rewrites the module.
    Module(fn(&mut Module, &Context<'_>)),
}

/// A step of the raise.
#[derive(Clone, Copy)]
pub struct Step {
    pub name: &'static str,
    pub pass: Pass,
}

/// Every step, in the order of the ABI level with the two steps outside it last.
const STEPS: [Step; 16] = [
    Step { name: "simplify", pass: Pass::Function(simplify) },
    Step { name: "abi", pass: Pass::Signatures(abi) },
    Step { name: "promote", pass: Pass::Function(promote) },
    Step { name: "conditions", pass: Pass::Function(conditions) },
    Step { name: "frame", pass: Pass::Function(frame) },
    Step { name: "known", pass: Pass::Function(known) },
    Step { name: "flatten", pass: Pass::Function(flatten) },
    Step { name: "strings", pass: Pass::Module(strings) },
    Step { name: "wrappers", pass: Pass::Module(wrappers) },
    Step { name: "outline", pass: Pass::Module(outline) },
    Step { name: "stubs", pass: Pass::Module(stubs) },
    Step { name: "widths", pass: Pass::Module(widths) },
    Step { name: "types", pass: Pass::Module(types) },
    Step { name: "prototypes", pass: Pass::Module(prototypes) },
    Step { name: "stack", pass: Pass::Stack(stack) },
    Step { name: "dce", pass: Pass::Function(dce) },
];

/// The names of the steps of the ABI level, in order.
const ABI: [&str; 14] = [
    "simplify",
    "abi",
    "promote",
    "conditions",
    "frame",
    "known",
    "flatten",
    "strings",
    "wrappers",
    "outline",
    "stubs",
    "widths",
    "types",
    "prototypes",
];

/// The step called `name`.
pub fn step(name: &str) -> Option<Step> {
    STEPS.iter().find(|s| s.name == name).copied()
}

/// The steps [`super::raise`] runs at `level`, in its order: the function steps that prepare the
/// machine functions in place, the inference, the function steps each function then goes through
/// on its own, and the module steps. The machine level infers the signatures and rewrites
/// nothing.
pub fn steps(level: Level) -> Vec<Step> {
    let names: &[&str] = match level {
        Level::Machine => &["abi"],
        Level::Promoted => &["abi", "promote"],
        Level::Abi => &ABI,
    };
    names.iter().map(|name| step(name).expect("the order names steps")).collect()
}

/// The environment variable that names the step to break.
pub const FAULT: &str = "RHIPPER_FAULT_STEP";

/// What a function of the broken step returns, whatever it computed: no Fibonacci number, no
/// small count, no address.
pub const FAULTY: i128 = 0x0bad;

/// Whether the environment names `step` as the one to break; read once.
fn faulty(step: &str) -> bool {
    static NAMED: OnceLock<Option<String>> = OnceLock::new();
    NAMED.get_or_init(|| std::env::var(FAULT).ok()).as_deref() == Some(step)
}

/// `f` as the broken `step` leaves it: every return of an integer of 16 bits or more returns
/// [`FAULTY`]. The control flow and the calls stay, so the function still ends where it did.
fn broken(step: &str, f: &mut Function) {
    if !faulty(step) {
        return;
    }
    for b in &mut f.blocks {
        if let Terminator::Ret(Some((Type::Int(width), value))) = &mut b.terminator
            && *width >= 16
        {
            *value = Operand::Int(FAULTY);
        }
    }
}

/// The module as the broken module step `step` leaves it: each function [`broken`].
fn broken_all(step: &str, module: &mut Module) {
    if faulty(step) {
        module.functions.iter_mut().for_each(|f| broken(step, f));
        super::held::each_mut(&mut module.frozen, |_, f| broken(step, f));
    }
}

fn simplify(f: &mut Function, _: &Context<'_>, _: &[Global]) {
    super::simplify::run(f);
    broken("simplify", f);
}

fn dce(f: &mut Function, _: &Context<'_>, _: &[Global]) {
    super::dce::run(f);
    broken("dce", f);
}

/// The stack analysis of `f`, told of its lifted callees what the signatures say.
fn stack(f: &Function, cx: &Context<'_>) -> Option<StackInfo> {
    let target = cx.options.target();
    let state = *f.params.first()?;
    let pops = |inst: &Inst| -> Option<Vec<CalleeStack>> {
        let Inst::Call { callee: Callee::Direct(name), .. } = inst else { return None };
        let s = cx.signatures.get(name)?;
        Some(vec![CalleeStack { pops: (!s.forwards).then_some(s.pops as i64), arguments: s.stack_bytes(target), forwards: s.forwards }])
    };
    Some(super::stack::analyze(f, state, &slot_pointers(f, state), target, &pops))
}

/// The signatures. At the ABI level the functions are read as the `simplify` step ahead left
/// them, in place and frozen; below it the inference simplifies copies, and the machine IR stays
/// as lifted.
fn abi(module: &Module, cx: &Context<'_>) -> HashMap<String, Signature> {
    let (target, options) = (cx.options.target(), cx.options);
    if options.level == Level::Abi {
        super::abi::infer_simplified(&module.functions, &module.frozen, cx.roots, target, &options.vtables, &options.traced_calls)
    } else {
        super::abi::infer_with(module, cx.roots, target, &options.vtables, &options.traced_calls)
    }
}

fn promote(f: &mut Function, cx: &Context<'_>, _: &[Global]) {
    registers(f, cx);
    broken("promote", f);
}

/// The registers as values. At the ABI level `f` is rewritten under its signature: the registers
/// become its parameters, results and locals, and a function with no signature stays as it is.
/// Below it the `%State` interface stays.
fn registers(f: &mut Function, cx: &Context<'_>) {
    let target = cx.options.target();
    if cx.options.level != Level::Abi {
        super::promote::promote(f, target);
        clean(f);
        return;
    }
    let Some(own) = cx.signatures.get(&f.name) else { return };
    // The constants first: the rewrite recognizes a pushed return address by its value.
    super::simplify::run(f);
    let lifted = |name: &str| cx.signatures.contains_key(name);
    let mut virtual_calls = super::abi::virtual_calls(f, &cx.vtables, target, &lifted);
    let traced = super::abi::traced_calls(f, &cx.options.traced_calls, &lifted);
    let seen: std::collections::HashSet<_> = traced.keys().copied().collect();
    virtual_calls.extend(traced);
    let abi = super::promote::Abi {
        own,
        all: cx.signatures,
        target,
        x87: cx.options.x87,
        virtual_calls: &virtual_calls,
        seen: &seen,
        summaries: Some(&cx.summaries),
        extra_dwords: cx.extra_dwords,
        host_objects: cx.options.host_objects,
    };
    super::promote::promote_abi(f, &abi);
    // The double unit's stack, which the promotion leaves in its globals, into locals where the
    // slots are counted; the exact unit's slots go through the runtime by their addresses.
    if cx.options.x87 == crate::x87::X87Unit::Double {
        super::x87::run(f);
    }
    if own.noreturn && !f.attrs.iter().any(|a| a == "noreturn") {
        f.attrs.push("noreturn".into());
    }
    clean(f);
}

fn conditions(f: &mut Function, _: &Context<'_>, _: &[Global]) {
    super::conditions::run(f);
    clean(f);
    broken("conditions", f);
}

fn frame(f: &mut Function, _: &Context<'_>, _: &[Global]) {
    super::frame::run(f);
    clean(f);
    broken("frame", f);
}

/// Known values and branches, to a fixpoint with the cleanup; then the stores to the frame the
/// forwarded loads left with no reader ([`super::stores`]).
fn known(f: &mut Function, cx: &Context<'_>, globals: &[Global]) {
    // A slice other than the one the context indexed is indexed for this function alone.
    let own;
    let constants = if let Some(constants) = cx.constants(globals) {
        constants
    } else {
        own = Constants::of(globals);
        &own
    };
    for _ in 0..4 {
        if !super::known::run(f, globals, constants) {
            break;
        }
        clean(f);
    }
    if cx.options.level == Level::Abi
        && super::stores::run(f, cx.signatures, *cx.most_pops.get_or_init(|| super::stores::most_pops(globals, cx.signatures)))
    {
        clean(f);
    }
    broken("known", f);
}

/// A flattened function is rebuilt; the known values settle again over the graph each rebuild
/// exposes.
fn flatten(f: &mut Function, cx: &Context<'_>, globals: &[Global]) {
    for _ in 0..4 {
        if !super::flatten::run(f) {
            break;
        }
        known(f, cx, globals);
    }
    broken("flatten", f);
}

// The module steps work over the functions frozen (see [`super::held`]): each reads the others
// through the context's rows and thaws one function at a time, keeping the row of each function
// it rewrites, adds or drops.

/// The strings a decryptor would compute at run time are computed here. The runs read the
/// callers, the decryptors and what they reach at once, and those alone are thawed for them.
fn strings(module: &mut Module, cx: &Context<'_>) {
    super::held::step(module, |module| {
        let mut written = vec![false; module.frozen.len()];
        for index in super::strings::run_frozen(module, !cx.options.target().is_64()) {
            written[index] = true;
        }
        super::held::rewrite(&mut module.frozen, &mut cx.rows(), |i| written[i], |_, f| clean(f));
        broken_all("strings", module);
    });
}

/// The import wrappers served the machine model; the calls now go to the imports.
fn wrappers(module: &mut Module, cx: &Context<'_>) {
    super::held::step(module, |module| {
        module.frozen.retain(|z| crate::import_of_wrapper(&z.name).is_none());
        cx.rows().retain(|r| crate::import_of_wrapper(&r.name).is_none());
        broken_all("wrappers", module);
    });
}

/// The helpers the compiler inlined, read back as functions: a loop whose shape repeats across
/// the functions becomes one, and each copy a call to it.
fn outline(module: &mut Module, cx: &Context<'_>) {
    super::held::step(module, |module| {
        if super::outline::run_frozen(module) {
            clean_all(module, &mut cx.rows());
        }
        broken_all("outline", module);
    });
}

/// An x86 function a pointer reaches gets its universal entry point; under a register convention
/// the convention is the universal call.
fn stubs(module: &mut Module, cx: &Context<'_>) {
    super::held::step(module, |module| {
        let target = cx.options.target();
        if !target.register_arguments() {
            super::stubs::add_universal_stubs(module, &mut cx.rows(), cx.signatures, target, cx.extra_dwords);
        }
        broken_all("stubs", module);
    });
}

/// The widths, to a fixpoint: each narrowing exposes the next through the cleanup.
fn widths(module: &mut Module, cx: &Context<'_>) {
    super::held::step(module, |module| {
        let mut rows = cx.rows();
        for _ in 0..4 {
            if !super::widths::run(module, &mut rows, cx.roots) {
                break;
            }
            clean_all(module, &mut rows);
        }
        broken_all("widths", module);
    });
}

/// The structs behind the pointers, once the widths are settled; the address arithmetic the
/// field pointers replace goes with the cleanup.
fn types(module: &mut Module, cx: &Context<'_>) {
    super::held::step(module, |module| {
        let mut rows = cx.rows();
        if super::types::run(module, &mut rows) {
            clean_all(module, &mut rows);
        }
        broken_all("types", module);
    });
}

/// Each import is declared with the arity the prototype table gives it, and each access of a
/// console's device register at the address the raise folded is the call on the platform runtime
/// that makes it, declared as it is called.
fn prototypes(module: &mut Module, cx: &Context<'_>) {
    for d in &mut module.declarations {
        if let Some(arity) = super::prototypes::arity(&d.name)
            && d.ty.params.len() == 8
        {
            d.ty = ra_asm_ir::FnType::new(ra_asm_ir::Type::I64, vec![ra_asm_ir::Type::I64; arity]);
        }
    }
    super::held::step(module, |module| {
        if let Some(devices) = &cx.options.devices {
            let called = Mutex::new(Vec::new());
            let mut rows = cx.rows();
            super::held::rewrite(
                &mut module.frozen,
                &mut rows,
                |_| true,
                |_, f| {
                    let declarations = crate::data::device_accesses(f, devices);
                    called.lock().unwrap_or_else(PoisonError::into_inner).extend(declarations);
                },
            );
            let mut called = called.into_inner().unwrap_or_else(PoisonError::into_inner);
            called.sort_by(|a, b| a.name.cmp(&b.name));
            for d in called {
                module.declare(d);
            }
        }
        broken_all("prototypes", module);
    });
}

/// The cleanup over every frozen function, each thawed alone, and its row written again.
fn clean_all(module: &mut Module, rows: &mut Vec<Row>) {
    super::held::rewrite(&mut module.frozen, rows, |_| true, |_, f| clean(f));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(level: Level) -> Vec<&'static str> {
        steps(level).iter().map(|s| s.name).collect()
    }

    #[test]
    fn every_name_resolves() {
        let named =
            ["promote", "abi", "stack", "frame", "conditions", "simplify", "dce", "widths", "known", "flatten", "outline", "strings"];
        for name in named {
            assert_eq!(step(name).map(|s| s.name), Some(name), "no step `{name}`");
        }
        for s in &STEPS {
            assert_eq!(STEPS.iter().filter(|other| other.name == s.name).count(), 1, "two steps `{}`", s.name);
        }
        assert!(step("raise").is_none());
    }

    #[test]
    fn the_abi_level_lists_the_order_raise_runs() {
        assert_eq!(
            names(Level::Abi),
            [
                "simplify",
                "abi",
                "promote",
                "conditions",
                "frame",
                "known",
                "flatten",
                "strings",
                "wrappers",
                "outline",
                "stubs",
                "widths",
                "types",
                "prototypes"
            ]
        );
        assert_eq!(names(Level::Machine), ["abi"]);
        assert_eq!(names(Level::Promoted), ["abi", "promote"]);
    }

    /// The shape `raise` walks: function steps, the inference once, function steps, module steps.
    #[test]
    fn every_level_has_the_shape_raise_walks() {
        for level in [Level::Machine, Level::Promoted, Level::Abi] {
            let order = steps(level);
            let phase = |s: &Step| match s.pass {
                Pass::Signatures(_) => 1,
                Pass::Function(_) => 0,
                Pass::Module(_) => 3,
                Pass::Stack(_) => 4,
            };
            let at = order.iter().position(|s| phase(s) == 1).expect("a level infers the signatures");
            assert!(order[..at].iter().all(|s| phase(s) == 0), "{level:?}");
            let rest = &order[at + 1..];
            let whole = rest.iter().position(|s| phase(s) != 0).unwrap_or(rest.len());
            assert!(rest[whole..].iter().all(|s| phase(s) == 3), "{level:?}");
        }
    }

    /// `raise` over an empty module walks every step of every level and infers nothing.
    #[test]
    fn raise_walks_the_steps_of_an_empty_module() {
        for level in [Level::Machine, Level::Promoted, Level::Abi] {
            let mut module = Module::default();
            let signatures = crate::raise(&mut module, &BTreeSet::new(), Options { level, ..Options::default() });
            assert!(signatures.is_empty(), "{level:?}");
            assert!(module.functions.is_empty(), "{level:?}");
        }
    }
}
