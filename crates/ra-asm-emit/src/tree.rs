//! The structured tree, run: an interpreter for [`Stmt`] and [`Cond`] over the machine's block
//! runner.
//!
//! A statement that stands for a block runs the block in the function's frame and keeps how it
//! left; the statements after it (an `if`, a `switch`, a `return`, the next block) read that exit.
//! The tree decides where to go and the graph says where the block went: a block the tree runs
//! that the block before it does not lead to, a condition that is not the branch of the block it
//! follows, a `goto` to a block without a label are faults that name the function. So a function
//! runs as its graph and as its tree, and a difference between the two is a structuring bug found
//! without compiling anything.
//!
//! `break` and `continue` are the tree's: they leave or repeat the innermost [`Stmt::Loop`], through
//! any `switch` between.

use std::collections::HashSet;

use ra_asm_ir::interp::{Exit, Frame, Host, Stop, Value, Vm, mask};
use ra_asm_ir::{BlockId, Operand, Terminator};

use crate::structure::{Case, Cond, Stmt, Structured};

/// Runs the function `name` of the machine's module with `args` by its structured `tree` and
/// returns what it returns, as a call of it would. The calls inside its blocks run as graphs.
pub fn run_tree(vm: &mut Vm<'_>, host: &mut dyn Host, name: &str, tree: &Structured, args: Vec<Value>) -> Result<Value, Stop> {
    let frame = vm.frame(name, args)?;
    let mut run = Run { vm, frame, labels: &tree.labels, at: None, fresh: false, ran: 0 };
    let result = run.body(host, &tree.body);
    run.vm.release(&run.frame);
    result
}

/// How a statement leaves.
enum Flow {
    /// To the statement after it.
    Next,
    Break,
    Continue,
    Goto(BlockId),
    Return(Value),
}

/// One run of a tree.
struct Run<'a, 'm> {
    vm: &'a mut Vm<'m>,
    frame: Frame<'m>,
    /// The blocks a `goto` may name.
    labels: &'a HashSet<BlockId>,
    /// The block run last and how it left.
    at: Option<(BlockId, Exit)>,
    /// Whether no condition or `switch` has read that exit yet: a condition after one that did is
    /// the branch of the block the exit leads to, which the structuring folded into the `if`.
    fresh: bool,
    /// The blocks run so far.
    ran: u64,
}

impl<'m> Run<'_, 'm> {
    fn fault(&self, what: &str) -> Stop {
        Stop::fault(format!("the tree of @{}: {what}", self.frame.function().name))
    }

    fn name(&self, b: BlockId) -> String {
        self.frame.function().block(b).map_or_else(|| format!("block {}", b.0), |block| format!("%{}", block.name))
    }

    /// The whole body: it ends in a return.
    fn body(&mut self, host: &mut dyn Host, body: &[Stmt]) -> Result<Value, Stop> {
        match self.stmts(host, body, None)? {
            Flow::Return(value) => Ok(value.coerce(&self.frame.function().ty.ret, &self.vm.layout())),
            Flow::Next => Err(self.fault("the statements end without a return")),
            Flow::Break | Flow::Continue => Err(self.fault("a break or continue outside a loop")),
            Flow::Goto(label) => Err(self.fault(&format!("a goto to {}, which no statement places", self.name(label)))),
        }
    }

    /// Runs `stmts` in order, from the statement that places `enter` when a `goto` comes in. A
    /// `goto` out of a statement to a block these statements place goes on from there.
    fn stmts(&mut self, host: &mut dyn Host, stmts: &[Stmt], mut enter: Option<BlockId>) -> Result<Flow, Stop> {
        let mut i = match enter {
            Some(label) => stmts
                .iter()
                .position(|s| places(s, label))
                .ok_or_else(|| self.fault(&format!("no statement places {}", self.name(label))))?,
            None => 0,
        };
        while let Some(s) = stmts.get(i) {
            match self.stmt(host, s, enter.take())? {
                Flow::Next => i += 1,
                Flow::Goto(label) if let Some(at) = stmts.iter().position(|s| places(s, label)) => {
                    i = at;
                    enter = Some(label);
                }
                other => return Ok(other),
            }
        }
        Ok(Flow::Next)
    }

    /// Runs one statement; with `enter`, from the block of that label inside it.
    fn stmt(&mut self, host: &mut dyn Host, s: &Stmt, enter: Option<BlockId>) -> Result<Flow, Stop> {
        match s {
            Stmt::Block(b) | Stmt::Copy(b) => {
                self.block(host, *b)?;
                Ok(Flow::Next)
            }
            // The phis of the next block read from the block the frame ran last: the hand-over
            // is right when that is the block the statement names.
            Stmt::Moves(b) => match &self.at {
                Some((last, _)) if last == b => Ok(Flow::Next),
                Some((last, _)) => {
                    Err(self.fault(&format!("the values of {} are handed over after {} ran", self.name(*b), self.name(*last))))
                }
                None => Err(self.fault(&format!("the values of {} are handed over before any block ran", self.name(*b)))),
            },
            Stmt::If { cond, then, els } => {
                let first = match enter {
                    Some(label) => then.iter().any(|s| places(s, label)),
                    None => self.cond(host, cond)?,
                };
                self.stmts(host, if first { then } else { els }, enter)
            }
            Stmt::Loop { header, body } => {
                let mut enter = enter;
                loop {
                    let ran = self.ran;
                    match self.stmts(host, body, enter.take())? {
                        Flow::Next | Flow::Continue => {}
                        Flow::Break => return Ok(Flow::Next),
                        other => return Ok(other),
                    }
                    // Nothing a round without a block does can end the loop.
                    if self.ran == ran {
                        return Err(self.fault(&format!("the loop of {} goes round without running a block", self.name(*header))));
                    }
                }
            }
            Stmt::Break => Ok(Flow::Break),
            Stmt::Continue => Ok(Flow::Continue),
            Stmt::Goto(label) if self.labels.contains(label) => Ok(Flow::Goto(*label)),
            Stmt::Goto(label) => Err(self.fault(&format!("a goto to {}, which has no label", self.name(*label)))),
            Stmt::Switch { ty, value, cases } => {
                if let Some(label) = enter {
                    let first = cases
                        .iter()
                        .position(|c| c.body.iter().any(|s| places(s, label)))
                        .ok_or_else(|| self.fault(&format!("no case places {}", self.name(label))))?;
                    return self.cases(host, &cases[first..], enter);
                }
                let (b, terminator, exit) = self.left()?;
                let subject = match (terminator, exit) {
                    (Terminator::Switch { ty: t, value: v, .. }, Exit::Switch { value: subject, .. })
                        if self.fresh && t == ty && v == value =>
                    {
                        *subject
                    }
                    _ => return Err(self.fault(&format!("a switch that is not the switch of {}", self.name(b)))),
                };
                self.fresh = false;
                let ones = mask(self.vm.layout().bits(ty));
                let first = cases
                    .iter()
                    .position(|c| c.values.iter().any(|v| (*v as u128) & ones == subject))
                    .or_else(|| cases.iter().position(|c| c.default))
                    .ok_or_else(|| self.fault(&format!("the switch of {} has no case for {subject}", self.name(b))))?;
                self.cases(host, &cases[first..], None)
            }
            Stmt::Return(value) => {
                let (b, terminator, exit) = self.left()?;
                match (terminator, exit) {
                    (Terminator::Ret(r), Exit::Return(result)) if r == value => Ok(Flow::Return(result.clone())),
                    _ => Err(self.fault(&format!("a return that is not the return of {}", self.name(b)))),
                }
            }
            Stmt::Unreachable => Err(self.fault("a statement the graph never reaches was reached")),
        }
    }

    /// The bodies of `cases` from the first, for as long as one falls through to the next.
    fn cases(&mut self, host: &mut dyn Host, cases: &[Case], mut enter: Option<BlockId>) -> Result<Flow, Stop> {
        for case in cases {
            match self.stmts(host, &case.body, enter.take())? {
                Flow::Next => {}
                other => return Ok(other),
            }
            if !case.falls_through {
                break;
            }
        }
        Ok(Flow::Next)
    }

    /// Runs block `b`, which the block before it must lead to: the entry when none ran.
    fn block(&mut self, host: &mut dyn Host, b: BlockId) -> Result<(), Stop> {
        match &self.at {
            None if self.frame.function().blocks.first().is_some_and(|entry| entry.id == b) => {}
            None => return Err(self.fault(&format!("the statements start at {}, which is not the entry", self.name(b)))),
            Some((_, exit)) if exit.target() == Some(b) => {}
            Some((last, exit)) => {
                let goes = exit.target().map_or_else(|| "returns".to_owned(), |t| format!("goes to {}", self.name(t)));
                return Err(self.fault(&format!("the statements run {} where {} {goes}", self.name(b), self.name(*last))));
            }
        }
        let exit = self.vm.run_block(host, &mut self.frame, b)?;
        self.at = Some((b, exit));
        self.fresh = true;
        self.ran += 1;
        Ok(())
    }

    /// The block run last, its terminator and how it left.
    fn left(&self) -> Result<(BlockId, &'m Terminator, &Exit), Stop> {
        let f = self.frame.function();
        match &self.at {
            Some((b, exit)) if let Some(block) = f.block(*b) => Ok((*b, &block.terminator, exit)),
            _ => Err(self.fault("a statement reads the exit of a block before any block ran")),
        }
    }

    fn cond(&mut self, host: &mut dyn Host, cond: &Cond) -> Result<bool, Stop> {
        Ok(match cond {
            Cond::Is(op) => self.test(host, op)?,
            Cond::Not(c) => !self.cond(host, c)?,
            Cond::And(a, b) => self.cond(host, a)? && self.cond(host, b)?,
            Cond::Or(a, b) => self.cond(host, a)? || self.cond(host, b)?,
        })
    }

    /// One `i1` of a condition: the branch of the block run last, or, when a condition before it
    /// read that branch, of the block the branch went to, which runs now. A short-circuit chain
    /// so walks the condition blocks the graph walks, in its order.
    fn test(&mut self, host: &mut dyn Host, op: &Operand) -> Result<bool, Stop> {
        if !self.fresh {
            let next = self.left()?.2.target().ok_or_else(|| self.fault("a condition after a return"))?;
            self.block(host, next)?;
        }
        self.fresh = false;
        let (b, terminator, exit) = self.left()?;
        match (terminator, exit) {
            (Terminator::CondBr { cond, .. }, Exit::Branch { taken, .. }) if cond == op => Ok(*taken),
            _ => Err(self.fault(&format!("a condition that is not the branch of {}", self.name(b)))),
        }
    }
}

/// Whether `s` is the block of `label` or holds it: where a `goto` to the label lands.
fn places(s: &Stmt, label: BlockId) -> bool {
    match s {
        Stmt::Block(b) => *b == label,
        Stmt::If { then, els, .. } => then.iter().chain(els).any(|s| places(s, label)),
        Stmt::Loop { body, .. } => body.iter().any(|s| places(s, label)),
        Stmt::Switch { cases, .. } => cases.iter().any(|c| c.body.iter().any(|s| places(s, label))),
        _ => false,
    }
}
