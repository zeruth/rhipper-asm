//! The observer of a run: what a test of one function watches the interpreter through.
//!
//! The machine tells its observer of every call it makes, the module's own functions and the
//! host's alike, as it enters and as it leaves, with the depth of the call. A run without an
//! observer pays one test of an empty option per call.

/// What a run reports its calls to. `function` is the name of an IR function or the name the host
/// answers a call under; `depth` is the number of calls open around this one, zero for the entry
/// of the run. Every `enter` has its `leave` at the same depth, also when the call ended in a
/// [`Stop`](super::Stop) rather than a return, so the calls an observer holds open are always the
/// ones running.
pub trait Observer {
    fn enter(&mut self, function: &str, depth: usize);
    fn leave(&mut self, function: &str, depth: usize);
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::Observer;
    use crate::builder::FunctionBuilder;
    use crate::inst::{BinOp, Callee, Terminator};
    use crate::interp::{Host, Memory, Stop, Value, Vm};
    use crate::module::{Function, Module};
    use crate::types::{FnType, Type};

    /// Writes each report as a line: `> name@depth` and `< name@depth`.
    struct Log(Rc<RefCell<Vec<String>>>);

    impl Observer for Log {
        fn enter(&mut self, function: &str, depth: usize) {
            self.0.borrow_mut().push(format!("> {function}@{depth}"));
        }

        fn leave(&mut self, function: &str, depth: usize) {
            self.0.borrow_mut().push(format!("< {function}@{depth}"));
        }
    }

    /// Answers `host_add` with its argument plus ten and `host_back` with the module's `leaf` of
    /// its argument; anything else is a fault.
    struct TestHost;

    impl Host for TestHost {
        fn call(&mut self, vm: &mut Vm<'_>, name: &str, _ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
            match name {
                "host_add" => Ok(Value::Int(args[0].as_u128() + 10)),
                "host_back" => vm.call_named(self, "leaf", args.to_vec()),
                _ => Err(Stop::fault(format!("no host function {name}"))),
            }
        }
    }

    /// `i32 name(i32 x)`: `x` through each of `callees` in turn, plus `add`.
    fn chain(name: &str, callees: &[&str], add: i64) -> Function {
        let ty = FnType::new(Type::I32, vec![Type::I32]);
        let mut b = FunctionBuilder::new(name, ty.clone(), &["x"]);
        let entry = b.block("entry");
        let mut v = b.param(0);
        for callee in callees {
            v = b
                .call(entry, ty.clone(), Callee::Direct((*callee).to_owned()), vec![(Type::I32, v.into())], "r")
                .expect("the call returns a value");
        }
        let sum = b.bin(entry, BinOp::Add, Type::I32, v, add, "sum");
        b.terminate(entry, Terminator::Ret(Some((Type::I32, sum.into()))));
        b.finish()
    }

    fn module(functions: Vec<Function>) -> Module {
        Module { functions, ..Module::default() }
    }

    /// Runs `main(1)` of `module`, observed or not: the result, the steps, the functions entered
    /// and the observer's lines.
    fn run(module: &Module, observed: bool) -> (u64, u64, Vec<usize>, Vec<String>) {
        let lines = Rc::new(RefCell::new(Vec::new()));
        let mut vm = Vm::new(module, Memory::new());
        if observed {
            vm.observer = Some(Box::new(Log(lines.clone())));
        }
        let result = vm.call_named(&mut TestHost, "main", vec![Value::Int(1)]).expect("the run returns");
        let entered = vm.seen.entered.iter().copied().collect();
        let steps = vm.steps;
        drop(vm);
        let lines = lines.borrow().clone();
        (result.as_u64(), steps, entered, lines)
    }

    #[test]
    fn three_functions_report_their_calls_in_order_with_depths() {
        let module = module(vec![chain("main", &["middle", "leaf"], 0), chain("middle", &["leaf"], 1), chain("leaf", &[], 1)]);
        let (result, _, _, lines) = run(&module, true);
        assert_eq!(result, 4);
        assert_eq!(lines, ["> main@0", "> middle@1", "> leaf@2", "< leaf@2", "< middle@1", "> leaf@1", "< leaf@1", "< main@0"]);
    }

    #[test]
    fn a_host_call_is_reported() {
        let module = module(vec![chain("main", &["host_add"], 0)]);
        let (result, _, _, lines) = run(&module, true);
        assert_eq!(result, 11);
        assert_eq!(lines, ["> main@0", "> host_add@1", "< host_add@1", "< main@0"]);
    }

    #[test]
    fn a_callback_of_the_host_runs_inside_its_call() {
        let module = module(vec![chain("main", &["host_back"], 0), chain("leaf", &[], 1)]);
        let (result, _, _, lines) = run(&module, true);
        assert_eq!(result, 2);
        assert_eq!(lines, ["> main@0", "> host_back@1", "> leaf@2", "< leaf@2", "< host_back@1", "< main@0"]);
    }

    #[test]
    fn a_call_that_stops_still_leaves() {
        let module = module(vec![chain("main", &["middle"], 0), chain("middle", &["host_missing"], 0)]);
        let lines = Rc::new(RefCell::new(Vec::new()));
        let mut vm = Vm::new(&module, Memory::new());
        vm.observer = Some(Box::new(Log(lines.clone())));
        let stop = vm.call_named(&mut TestHost, "main", vec![Value::Int(1)]).expect_err("the host refuses the call");
        assert!(matches!(stop, Stop::Fault(_)));
        assert_eq!(*lines.borrow(), ["> main@0", "> middle@1", "> host_missing@2", "< host_missing@2", "< middle@1", "< main@0"]);
    }

    #[test]
    fn a_run_without_an_observer_is_unchanged() {
        let module =
            module(vec![chain("main", &["middle", "host_add", "host_back"], 0), chain("middle", &["leaf"], 1), chain("leaf", &[], 1)]);
        let (result, steps, entered, lines) = run(&module, false);
        assert!(lines.is_empty());
        assert_eq!(result, 14);
        let observed = run(&module, true);
        assert_eq!((result, steps, entered), (observed.0, observed.1, observed.2));
        assert_eq!(observed.3.len(), 12);
    }
}
