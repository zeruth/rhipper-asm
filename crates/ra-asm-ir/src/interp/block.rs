//! One block at a time: the machine runs the instructions of a block in a [`Frame`] and says how
//! the block leaves, as an [`Exit`], without following it.
//!
//! A function's own run is a loop over this, and so is the run of a structured tree, whose
//! statements stand over the same blocks: the tree decides where to go next and checks its own
//! shape against the exit the block reports.

use super::{Host, Stop, Value, Vm, mask};
use crate::inst::{Inst, Terminator};
use crate::module::{BlockId, Function};
use crate::types::Type;

/// The locals of one activation of a function: its values, the block it ran last, which the
/// phis of the next read from, and where the frames region stood when it opened.
#[derive(Debug, Clone)]
pub struct Frame<'m> {
    pub(super) index: usize,
    pub(super) function: &'m Function,
    pub(super) values: Vec<Option<Value>>,
    pub(super) prev: Option<BlockId>,
    pub(super) mark: u64,
}

impl<'m> Frame<'m> {
    /// A frame of function `index` with `args` as its parameters, already of their types.
    pub(super) fn new(index: usize, function: &'m Function, args: Vec<Value>, mark: u64) -> Self {
        let mut values: Vec<Option<Value>> = vec![None; function.value_names.len()];
        for (p, a) in function.params.iter().zip(args) {
            values[p.0 as usize] = Some(a);
        }
        Self { index, function, values, prev: None, mark }
    }

    pub fn function(&self) -> &'m Function {
        self.function
    }
}

/// How a block leaves, once its instructions ran.
#[derive(Debug, Clone, PartialEq)]
pub enum Exit {
    /// An unconditional branch.
    Jump(BlockId),
    /// A conditional branch: what the condition was, and the side it picks.
    Branch { taken: bool, target: BlockId },
    /// A switch: the value at the width of its type, and the case or the default it picks.
    Switch { value: u128, target: BlockId },
    /// A return, with zero for a function that returns nothing.
    Return(Value),
}

impl Exit {
    /// The block the exit leads to, when it is not a return.
    pub fn target(&self) -> Option<BlockId> {
        match self {
            Self::Jump(target) | Self::Branch { target, .. } | Self::Switch { target, .. } => Some(*target),
            Self::Return(_) => None,
        }
    }
}

impl<'m> Vm<'m> {
    /// A frame of the function `name` with `args`, adapted to its parameters as a call adapts
    /// them. The frame is not a call: nothing is recorded, traced or counted in the depth.
    pub fn frame(&self, name: &str, args: Vec<Value>) -> Result<Frame<'m>, Stop> {
        let module = self.module;
        let index = self.functions.get(name).copied().ok_or_else(|| Stop::fault(format!("no function @{name} to open a frame of")))?;
        let f = &module.functions[index];
        Ok(Frame::new(index, f, self.adapt(f, args), self.frames.at))
    }

    /// Gives back what the `alloca`s of `frame` took. Frames close in the reverse of the order
    /// they opened in.
    pub fn release(&mut self, frame: &Frame<'m>) {
        self.frames.at = frame.mark;
    }

    /// Runs the instructions of `block` in `frame` and evaluates its terminator: the exit says
    /// where the block goes, and the caller goes there or not. The phis of the block read the
    /// values of the block the frame ran last.
    pub fn run_block(&mut self, host: &mut dyn Host, frame: &mut Frame<'m>, block: BlockId) -> Result<Exit, Stop> {
        let f = frame.function;
        let at = *self.blocks[frame.index].get(&block).ok_or_else(|| Stop::fault(format!("@{} has no block {}", f.name, block.0)))?;
        let b = &f.blocks[at];
        // The phis read their incoming values together, before any is written.
        let mut first = 0;
        let mut phis = Vec::new();
        while let Some(i) = b.instrs.get(first)
            && let Inst::Phi { ty, incoming } = &i.inst
        {
            let from = frame.prev.ok_or_else(|| Stop::fault(format!("a phi in the entry block of @{}", f.name)))?;
            let (op, _) = incoming
                .iter()
                .find(|(_, p)| *p == from)
                .ok_or_else(|| Stop::fault(format!("a phi in @{} without a value from {}", f.name, from.0)))?;
            phis.push((i.result, self.eval(f, &frame.values, op, ty)?));
            first += 1;
        }
        for (result, v) in phis {
            if let Some(r) = result {
                frame.values[r.0 as usize] = Some(v);
            }
        }
        for i in &b.instrs[first..] {
            self.steps += 1;
            if self.limit != 0 && self.steps > self.limit {
                return Err(Stop::Limit(self.steps));
            }
            self.site = i.address;
            let v = self.exec(host, f, &frame.values, i).map_err(|s| s.at(i.address))?;
            if let Some(r) = i.result {
                frame.values[r.0 as usize] = Some(v);
            }
        }
        frame.prev = Some(b.id);
        Ok(match &b.terminator {
            Terminator::Ret(None) => Exit::Return(Value::Int(0)),
            Terminator::Ret(Some((ty, op))) => Exit::Return(self.eval(f, &frame.values, op, ty)?),
            Terminator::Br(target) => Exit::Jump(*target),
            Terminator::CondBr { cond, then, els } => {
                let taken = self.eval(f, &frame.values, cond, &Type::I1)?.as_bool();
                Exit::Branch { taken, target: if taken { *then } else { *els } }
            }
            Terminator::Switch { ty, value, default, cases } => {
                let bits = self.layout().bits(ty);
                let value = self.eval(f, &frame.values, value, ty)?.as_u128() & mask(bits);
                let target = cases.iter().find(|(c, _)| (*c as u128) & mask(bits) == value).map_or(*default, |(_, t)| *t);
                Exit::Switch { value, target }
            }
            Terminator::Unreachable => {
                let at = b.instrs.last().and_then(|i| i.address).or(b.address);
                return Err(Stop::fault(format!("unreachable code in @{} was reached", f.name)).at(at));
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Exit;
    use crate::builder::FunctionBuilder;
    use crate::inst::{BinOp, Callee, ICond, Inst, Operand, Terminator};
    use crate::interp::{Host, Memory, Stop, Value, Vm};
    use crate::module::{BlockId, Function, Module};
    use crate::types::{FnType, Type};

    /// Answers `host_add` with its argument plus ten; anything else is a fault.
    struct TestHost;

    impl Host for TestHost {
        fn call(&mut self, _vm: &mut Vm<'_>, name: &str, _ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
            match name {
                "host_add" => Ok(Value::Int(args[0].as_u128() + 10)),
                _ => Err(Stop::fault(format!("no host function {name}"))),
            }
        }
    }

    fn unary() -> FnType {
        FnType::new(Type::I32, vec![Type::I32])
    }

    fn module(functions: Vec<Function>) -> Module {
        Module { functions, ..Module::default() }
    }

    fn ret(value: impl Into<Operand>) -> Terminator {
        Terminator::Ret(Some((Type::I32, value.into())))
    }

    /// The id of the block `name` of `f`.
    fn id(f: &Function, name: &str) -> BlockId {
        f.blocks.iter().find(|b| b.name == name).expect("the function has the block").id
    }

    /// `i32 pick(i32 x)`: `entry` branches to `then` when `x` is zero and to `els` otherwise;
    /// both go to `join`, which returns 10 coming from `then` and 20 from `els`.
    fn pick() -> Function {
        let mut b = FunctionBuilder::new("pick", unary(), &["x"]);
        let entry = b.block("entry");
        let then = b.block("then");
        let els = b.block("els");
        let join = b.block("join");
        let x = b.param(0);
        let c = b.icmp(entry, ICond::Eq, Type::I32, x, 0i64, "c");
        b.terminate(entry, Terminator::CondBr { cond: c.into(), then, els });
        b.terminate(then, Terminator::Br(join));
        b.terminate(els, Terminator::Br(join));
        let p = b.value(join, Inst::Phi { ty: Type::I32, incoming: vec![(Operand::Int(10), then), (Operand::Int(20), els)] }, "p");
        b.terminate(join, ret(p));
        b.finish()
    }

    /// `i32 choose(i32 x)`: `entry` switches on `x` to `one`, `minus` (for -1) or `other`.
    fn choose() -> Function {
        let mut b = FunctionBuilder::new("choose", unary(), &["x"]);
        let entry = b.block("entry");
        let one = b.block("one");
        let minus = b.block("minus");
        let other = b.block("other");
        let x = b.param(0);
        b.terminate(entry, Terminator::Switch { ty: Type::I32, value: x.into(), default: other, cases: vec![(1, one), (-1, minus)] });
        b.terminate(one, ret(1i64));
        b.terminate(minus, ret(2i64));
        b.terminate(other, ret(3i64));
        b.finish()
    }

    /// `i32 name(i32 x)`: `entry` calls `callee` with `x` and goes to `done`, which returns the
    /// call's result plus one.
    fn caller(name: &str, callee: &str) -> Function {
        let mut b = FunctionBuilder::new(name, unary(), &["x"]);
        let entry = b.block("entry");
        let done = b.block("done");
        let x = b.param(0);
        let r =
            b.call(entry, unary(), Callee::Direct(callee.to_owned()), vec![(Type::I32, x.into())], "r").expect("the call returns a value");
        b.terminate(entry, Terminator::Br(done));
        let sum = b.bin(done, BinOp::Add, Type::I32, r, 1i64, "sum");
        b.terminate(done, ret(sum));
        b.finish()
    }

    /// `i32 leaf(i32 x)`: `x` times two, through a second block.
    fn leaf() -> Function {
        let mut b = FunctionBuilder::new("leaf", unary(), &["x"]);
        let entry = b.block("entry");
        let done = b.block("done");
        let x = b.param(0);
        let twice = b.bin(entry, BinOp::Mul, Type::I32, x, 2i64, "twice");
        b.terminate(entry, Terminator::Br(done));
        b.terminate(done, ret(twice));
        b.finish()
    }

    #[test]
    fn a_conditional_branch_reports_its_condition_and_its_side() {
        let module = module(vec![pick()]);
        let f = &module.functions[0];
        let mut vm = Vm::new(&module, Memory::new());

        let mut zero = vm.frame("pick", vec![Value::Int(0)]).expect("the function exists");
        let exit = vm.run_block(&mut TestHost, &mut zero, id(f, "entry")).expect("the block runs");
        assert_eq!(exit, Exit::Branch { taken: true, target: id(f, "then") });
        // The comparison ran and nothing after it: the branch is reported, not followed.
        assert_eq!(vm.steps, 1);

        let mut five = vm.frame("pick", vec![Value::Int(5)]).expect("the function exists");
        let exit = vm.run_block(&mut TestHost, &mut five, id(f, "entry")).expect("the block runs");
        assert_eq!(exit, Exit::Branch { taken: false, target: id(f, "els") });
        assert_eq!(exit.target(), Some(id(f, "els")));
    }

    #[test]
    fn a_switch_reports_its_value_and_its_case() {
        let module = module(vec![choose()]);
        let f = &module.functions[0];
        let mut vm = Vm::new(&module, Memory::new());
        let mut exit_of = |x: u128| {
            let mut frame = vm.frame("choose", vec![Value::Int(x)]).expect("the function exists");
            vm.run_block(&mut TestHost, &mut frame, id(f, "entry")).expect("the block runs")
        };
        assert_eq!(exit_of(1), Exit::Switch { value: 1, target: id(f, "one") });
        // A negative case meets the value at the width of the switch.
        assert_eq!(exit_of(0xffff_ffff), Exit::Switch { value: 0xffff_ffff, target: id(f, "minus") });
        assert_eq!(exit_of(7), Exit::Switch { value: 7, target: id(f, "other") });
    }

    #[test]
    fn a_return_reports_its_value() {
        let module = module(vec![leaf()]);
        let f = &module.functions[0];
        let mut vm = Vm::new(&module, Memory::new());
        let mut frame = vm.frame("leaf", vec![Value::Int(21)]).expect("the function exists");
        let exit = vm.run_block(&mut TestHost, &mut frame, id(f, "entry")).expect("the block runs");
        assert_eq!(exit, Exit::Jump(id(f, "done")));
        // The frame keeps the values of the blocks it ran for the ones it runs next.
        let exit = vm.run_block(&mut TestHost, &mut frame, id(f, "done")).expect("the block runs");
        assert_eq!(exit, Exit::Return(Value::Int(42)));
        assert_eq!(exit.target(), None);
    }

    #[test]
    fn a_call_inside_the_block_runs_to_its_return() {
        let module = module(vec![caller("main", "leaf"), leaf(), caller("hosted", "host_add")]);
        let f = &module.functions[0];
        let mut vm = Vm::new(&module, Memory::new());

        let mut frame = vm.frame("main", vec![Value::Int(4)]).expect("the function exists");
        let exit = vm.run_block(&mut TestHost, &mut frame, id(f, "entry")).expect("the block runs");
        assert_eq!(exit, Exit::Jump(id(f, "done")));
        // The callee ran whole, both its blocks; the frame's own function was not entered.
        assert!(vm.seen.entered.contains(&1));
        assert!(!vm.seen.entered.contains(&0));
        assert_eq!(vm.steps, 2);
        let exit = vm.run_block(&mut TestHost, &mut frame, id(f, "done")).expect("the block runs");
        assert_eq!(exit, Exit::Return(Value::Int(9)));

        let hosted = &module.functions[2];
        let mut frame = vm.frame("hosted", vec![Value::Int(4)]).expect("the function exists");
        vm.run_block(&mut TestHost, &mut frame, id(hosted, "entry")).expect("the host answers the call");
        let exit = vm.run_block(&mut TestHost, &mut frame, id(hosted, "done")).expect("the block runs");
        assert_eq!(exit, Exit::Return(Value::Int(15)));
    }

    #[test]
    fn a_phi_reads_from_the_block_the_frame_ran_last() {
        let module = module(vec![pick()]);
        let f = &module.functions[0];
        let mut vm = Vm::new(&module, Memory::new());
        for (x, side, expected) in [(0, "then", 10), (3, "els", 20)] {
            let mut frame = vm.frame("pick", vec![Value::Int(x)]).expect("the function exists");
            let mut block = id(f, "entry");
            let mut path = Vec::new();
            let result = loop {
                path.push(block);
                match vm.run_block(&mut TestHost, &mut frame, block).expect("the block runs") {
                    Exit::Return(v) => break v,
                    exit => block = exit.target().expect("an exit that is no return has a target"),
                }
            };
            assert_eq!(path, [id(f, "entry"), id(f, side), id(f, "join")]);
            assert_eq!(result, Value::Int(expected));
            // The whole function gives the same.
            assert_eq!(vm.call_named(&mut TestHost, "pick", vec![Value::Int(x)]).expect("the run returns"), Value::Int(expected));
        }
    }

    #[test]
    fn a_block_the_function_does_not_have_is_a_fault() {
        let module = module(vec![leaf()]);
        let mut vm = Vm::new(&module, Memory::new());
        let mut frame = vm.frame("leaf", vec![Value::Int(1)]).expect("the function exists");
        let stop = vm.run_block(&mut TestHost, &mut frame, BlockId(9)).expect_err("there is no such block");
        assert_eq!(stop, Stop::fault("@leaf has no block 9"));
        assert!(vm.frame("missing", Vec::new()).is_err());
    }
}
