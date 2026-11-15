//! The calls a case's header spells: the arguments of a function, the entry of one over the state.

use ra_asm_ir::interp::Value;

use super::super::{Arg, Call};

/// The arguments a call spells: integers at their width, floats, and the address of a global.
#[test]
fn a_call_reads_its_arguments() {
    let call = Call::parse("@f(i8 -1, i64 0x10, i1 true, double 1.5, ptr @buf, ptr null)").unwrap();
    assert_eq!(call.function, "f");
    assert_eq!(
        call.args,
        [
            Arg::Value(Value::Int(0xff)),
            Arg::Value(Value::Int(0x10)),
            Arg::Value(Value::Int(1)),
            Arg::Value(Value::F64(1.5)),
            Arg::Global("buf".to_owned()),
            Arg::Value(Value::Int(0)),
        ]
    );
    assert!(Call::parse("@g()").unwrap().args.is_empty());
}

/// The entry a call over the state spells: general and vector registers, and stack qwords; the
/// stack pointer and the return address are the run's.
#[test]
fn a_state_call_reads_its_registers() {
    let call = Call::parse("@F_401000(rcx 7, r9 -1, xmm1 0x3ff0000000000000, [40] 9)").unwrap();
    assert_eq!(call.args, [Arg::Register(1, 7), Arg::Register(9, u64::MAX), Arg::Vector(1, 0x3ff0_0000_0000_0000), Arg::Stack(40, 9)]);
    assert!(Call::parse("@F_401000(rsp 8)").unwrap_err().contains("the stack pointer is the run's"));
    assert!(Call::parse("@F_401000([0] 8)").unwrap_err().contains("past the return address"));
}
