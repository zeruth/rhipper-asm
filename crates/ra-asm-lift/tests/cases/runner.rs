//! The runner's own failures: a wrong pass, a text that differs and a broken case are each
//! caught and named.

use ra_asm_ir::{BinOp, Inst, Module};

use super::super::{Case, cases_dir, check, run};

/// A seed case, by its name under `testdata/cases`.
fn seed(name: &str) -> Case {
    Case::load(&cases_dir().join(format!("{name}.ll"))).unwrap()
}

/// A pass that is wrong on purpose: every `add` becomes a `sub`.
fn subtracts(module: &mut Module) {
    for f in &mut module.functions {
        for b in &mut f.blocks {
            for i in &mut b.instrs {
                if let Inst::Bin { op, .. } = &mut i.inst
                    && *op == BinOp::Add
                {
                    *op = BinOp::Sub;
                }
            }
        }
    }
}

/// A pass that is wrong on purpose: no store is kept.
fn forgets(module: &mut Module) {
    for f in &mut module.functions {
        for b in &mut f.blocks {
            b.instrs.retain(|i| !matches!(i.inst, Inst::Store { .. }));
        }
    }
}

/// A case of two lines for the runner's own tests; `none` is no step, so `check` gets its pass.
const TWICE: &str = "\
; pass: none
; call: @twice(i32 4)
define i32 @twice(i32 %x) {
entry:
  %r = add i32 %x, %x
  ret i32 %r
}
";

/// A pass that changes what a function returns is caught, and the failure names the case, the
/// call that shows it and what each side returned.
#[test]
fn a_wrong_pass_is_caught_with_the_case_and_the_call() {
    let case = seed("simplify/identities");
    let failure = check(&case, &subtracts).expect_err("7 + 9 is not 7 - 9");
    assert!(failure.starts_with("simplify/identities: `@f(i32 7, i32 9)` through `simplify`: "), "{failure}");
    assert!(failure.ends_with("returns 0x10 before, returns 0xfffffffe after"), "{failure}");
}

/// A pass that returns the same value and leaves the memory different is caught by the global.
#[test]
fn a_pass_that_loses_a_store_is_caught_by_the_memory() {
    let case = seed("simplify/memory");
    let failure = check(&case, &forgets).expect_err("the stores are the function's effect");
    assert!(failure.starts_with("simplify/memory: `@fill(ptr @buf, i32 3)` through `simplify`: @buf differs at byte 0: "), "{failure}");
}

/// The text under `after` is compared with what the functions print as, whatever the calls say.
#[test]
fn the_text_after_the_pass_is_compared() {
    let module = ra_asm_ir::parse::parse(TWICE).unwrap();
    let printed = ra_asm_ir::printer::function(&module.functions[0]);
    let same = Case::parse("inline/twice", &format!("{TWICE}; after:\n\n{printed}")).unwrap();
    assert_eq!(same.calls.len(), 1);
    assert_eq!(check(&same, &|_| ()), Ok(()));

    let other = Case::parse("inline/twice", &format!("{TWICE}; after:\n{}", printed.replace("%r", "%q"))).unwrap();
    let failure = check(&other, &|_| ()).expect_err("the function does not print as the case says");
    assert!(failure.starts_with("inline/twice: the text after `none` is not the case's"), "{failure}");
    assert!(failure.contains("%q") && failure.contains("%r"), "{failure}");
}

/// A case the runner cannot run says why, by name: no pass, no call, a call it cannot spell, a
/// pass or a function there is none of.
#[test]
fn a_broken_case_is_refused_by_name() {
    let refusal = |text: &str| Case::parse("inline/broken", text).expect_err("the case is broken");
    assert!(refusal("; call: @f(i32 1)\n").contains("inline/broken: the case names no pass"));
    assert!(refusal("; pass: simplify\n").contains("inline/broken: the case has no call"));
    assert!(refusal("; pass: simplify\n; call: @f(i32 x)\n").contains("`i32 x` is not an argument"));
    assert!(refusal("; pass: simplify\n; call: f(i32 1)\n").contains("a call is `@name("));

    let unknown = Case::parse("inline/twice", TWICE).unwrap();
    assert_eq!(run(&unknown), Err("inline/twice: there is no pass `none`".to_owned()));
    let missing = Case::parse("inline/twice", &TWICE.replace("call: @twice", "call: @thrice")).unwrap();
    let failure = check(&missing, &|_| ()).expect_err("the module has no such function");
    assert!(failure.contains("`@thrice(i32 4)`") && failure.contains("no function @thrice"), "{failure}");
}
