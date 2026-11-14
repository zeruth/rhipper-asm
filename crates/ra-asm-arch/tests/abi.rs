//! The convention constants against what `testdata/cases/abi_tables.txt` shows the lift answers
//! today, and the invariants every convention keeps.

use ra_asm_arch::abi::{AAPCS32, AAPCS64, ALL, Abi, Reg, ReturnAddress, WIN64, X86};

use Reg::{Gpr, Vector};

/// The general and vector registers of `abi` a callee keeps, out of the first `gprs` and `vectors`.
fn kept(abi: &Abi, gprs: u8, vectors: u8) -> (Vec<u8>, Vec<u8>) {
    let gpr = (0..gprs).filter(|&n| abi.is_callee_saved(Gpr(n))).collect();
    let vector = (0..vectors).filter(|&n| abi.is_callee_saved(Vector(n))).collect();
    (gpr, vector)
}

#[test]
fn win64() {
    let abi = WIN64;
    assert_eq!(abi.ptr_bytes, 8);
    assert_eq!(abi.int_params, [Gpr(1), Gpr(2), Gpr(8), Gpr(9)]);
    assert_eq!(abi.float_params, [Vector(0), Vector(1), Vector(2), Vector(3)]);
    assert!(abi.shared_positions);
    assert_eq!(abi.stack_pointer, Gpr(4));
    assert_eq!(abi.return_address, ReturnAddress::Stack);
    assert_eq!(abi.pushed_return(), 8);
    assert_eq!(abi.home_space, 32);
    assert_eq!(abi.first_stack_arg(), 40);
    assert_eq!(abi.entry_misalignment(), 8);
    assert_eq!(abi.global_pointer, None);
    let (gprs, vectors) = kept(&abi, 16, 16);
    assert_eq!(gprs, [3, 4, 5, 6, 7, 12, 13, 14, 15]);
    assert_eq!(vectors, (6..16).collect::<Vec<u8>>());
}

#[test]
fn x86() {
    let abi = X86;
    assert_eq!(abi.ptr_bytes, 4);
    assert_eq!(abi.int_params, [Gpr(1), Gpr(2)]);
    assert_eq!(abi.float_params, [Vector(0), Vector(1), Vector(2)]);
    assert!(!abi.shared_positions);
    assert_eq!(abi.stack_pointer, Gpr(4));
    assert_eq!(abi.return_address, ReturnAddress::Stack);
    assert_eq!(abi.pushed_return(), 4);
    assert_eq!(abi.home_space, 0);
    assert_eq!(abi.first_stack_arg(), 4);
    assert_eq!(abi.entry_misalignment(), 12);
    assert_eq!(abi.global_pointer, None);
    let (gprs, vectors) = kept(&abi, 16, 16);
    assert_eq!(gprs, [3, 4, 5, 6, 7]);
    assert!(vectors.is_empty());
}

#[test]
fn aapcs64() {
    let abi = AAPCS64;
    assert_eq!(abi.ptr_bytes, 8);
    assert_eq!(abi.int_params, (0..8).map(Gpr).collect::<Vec<_>>());
    assert_eq!(abi.float_params, (0..8).map(Vector).collect::<Vec<_>>());
    assert!(!abi.shared_positions);
    assert_eq!(abi.stack_pointer, Gpr(31));
    assert_eq!(abi.return_address, ReturnAddress::Link(Gpr(30)));
    assert_eq!(abi.pushed_return(), 0);
    assert_eq!(abi.home_space, 0);
    assert_eq!(abi.first_stack_arg(), 0);
    assert_eq!(abi.entry_misalignment(), 0);
    assert_eq!(abi.global_pointer, None);
    let (gprs, vectors) = kept(&abi, 32, 32);
    assert_eq!(gprs, (18..=29).chain([31]).collect::<Vec<u8>>());
    assert_eq!(vectors, (8..16).collect::<Vec<u8>>());
}

#[test]
fn aapcs32() {
    let abi = AAPCS32;
    assert_eq!(abi.ptr_bytes, 4);
    assert_eq!(abi.int_params, [Gpr(0), Gpr(1), Gpr(2), Gpr(3)]);
    assert!(abi.float_params.is_empty());
    assert!(!abi.shared_positions);
    assert_eq!(abi.stack_pointer, Gpr(13));
    assert_eq!(abi.return_address, ReturnAddress::Link(Gpr(14)));
    assert_eq!(abi.pushed_return(), 0);
    assert_eq!(abi.home_space, 0);
    assert_eq!(abi.first_stack_arg(), 0);
    assert_eq!(abi.entry_misalignment(), 0);
    assert_eq!(abi.global_pointer, None);
    let (gprs, vectors) = kept(&abi, 16, 16);
    assert_eq!(gprs, (4..=11).chain([13]).collect::<Vec<u8>>());
    assert!(vectors.is_empty());
}

#[test]
fn no_parameter_is_callee_saved() {
    for abi in ALL {
        for reg in abi.int_params.iter().chain(abi.float_params) {
            assert!(!abi.is_callee_saved(*reg), "{}: {reg:?} is both a parameter and callee-saved", abi.name);
        }
    }
}

#[test]
fn results_and_return_address_are_not_kept_and_the_stack_pointer_is() {
    for abi in ALL {
        for reg in abi.int_results.iter().chain(abi.float_results) {
            assert!(!abi.is_callee_saved(*reg), "{}: result {reg:?} is callee-saved", abi.name);
        }
        if let ReturnAddress::Link(link) = abi.return_address {
            assert!(!abi.is_callee_saved(link), "{}: link {link:?} is callee-saved", abi.name);
            assert!(!abi.is_param(link), "{}: link {link:?} is a parameter", abi.name);
        }
        assert!(abi.is_callee_saved(abi.stack_pointer), "{}: the stack pointer is not kept", abi.name);
        assert!(!abi.is_param(abi.stack_pointer), "{}: the stack pointer is a parameter", abi.name);
        assert_eq!(abi.stack_align.count_ones(), 1, "{}: stack alignment {} is not a power of two", abi.name, abi.stack_align);
        let mut saved = abi.callee_saved.to_vec();
        saved.sort();
        saved.dedup();
        assert_eq!(saved.len(), abi.callee_saved.len(), "{}: a callee-saved register is listed twice", abi.name);
    }
}
