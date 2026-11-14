//! The x87 vectors: the cases of `testdata/vectors/x87/*.vec`, the processor's answer under WOW64
//! to every x87 form of iced's 32-bit table (written by the harness through `common/x87.rs`),
//! replayed through the exact unit of the x87 lift as the 32-bit integer ones are: the image, the
//! run and the files of `x86_32.rs`, with the unit's globals (`ra_x87_top`, `ra_x87_cw`,
//! `ra_x87_sw` and the eight 16-byte slots of `ra_x87_stack`) set from a case's `fsw`, `fcw` and
//! `fr<i>` before it and checked after it, and the lifted code's `ra_f80_*` calls answered by the
//! runtime's software `f80` through `ra_asm_lift::run::f80_call`, its flags merged into the status
//! word as the runtime merges them. Exactly the forms of `x87/PENDING.txt` differ; the
//! transcendental forms, which the lift has no arm for, are there from the first replay.
//!
//! A physical register is compared when the processor's tag word has it valid or zero after the
//! case: the lift keeps no tags, so what an empty register holds is its own. Of the status word,
//! the condition codes C0, C2 and C3 and the exception flags are compared, the top of stack
//! through the lift's own top; C1, the rounding direction, the lift does not keep.

use ra_asm_ir::FnType;
use ra_asm_ir::interp::{Host, Stop, Value, Vm};
use ra_asm_lift::X87Unit;
use ra_asm_lift::state::{X87_CW, X87_STACK, X87_SW, X87_TOP};
use ra_asm_testing::vectors::State;

use super::Ends;
use super::common::x87 as generator;
use super::x86_32::{Replay, Unit, has_form, pending_header, replay_dir};
use super::x86_64::{number, number_of};

/// The bits of the status word compared: the condition codes C0, C2 and C3 and the exception
/// flags. The top of stack (bits 11 to 13) is compared through the lift's own top.
const SW_COMPARED: u16 = 0x453f;
const F80_SLOT: u64 = ra_asm_lift::x87::F80_SLOT;

/// The exact unit: where its globals lie once the module is placed.
#[derive(Default)]
struct Exact {
    top: u64,
    cw: u64,
    sw: u64,
    stack: u64,
}

impl Host for Exact {
    fn call(&mut self, vm: &mut Vm<'_>, name: &str, ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        match ra_asm_lift::run::f80_call(vm, name, args) {
            Some(answer) => {
                let (value, flags) = answer?;
                if flags != 0 {
                    let sw = vm.memory.read_u16(self.sw)?;
                    vm.memory.write_u16(self.sw, sw | flags)?;
                }
                Ok(value)
            }
            None => Ends.call(vm, name, ty, args),
        }
    }
}

impl Unit for Exact {
    fn x87(&self) -> X87Unit {
        X87Unit::Exact
    }

    fn placed(&mut self, vm: &Vm<'_>) {
        let global = |name: &str| vm.symbol(name).unwrap_or_else(|| panic!("the lifted module has no global {name}"));
        self.top = global(X87_TOP);
        self.cw = global(X87_CW);
        self.sw = global(X87_SW);
        self.stack = global(X87_STACK);
    }

    fn load(&mut self, vm: &mut Vm<'_>, input: &State) {
        let fsw = number(input, "fsw") as u16;
        vm.memory.write_u64(self.top, u64::from((fsw >> 11) & 7)).expect("the x87 top");
        vm.memory.write_u16(self.cw, number(input, "fcw") as u16).expect("the x87 control word");
        vm.memory.write_u16(self.sw, fsw & !0x3800).expect("the x87 status word");
        for i in 0..8u64 {
            // The file writes a register sign and exponent first; the slot holds the tbyte form.
            let value = number(input, &format!("fr{i}")).to_le_bytes();
            let mut slot = [0u8; 16];
            slot[..10].copy_from_slice(&value[..10]);
            vm.memory.write(self.stack + F80_SLOT * i, &slot).expect("an x87 slot");
        }
    }

    fn check(&mut self, vm: &mut Vm<'_>, input: &State, output: &State) -> Option<String> {
        let want = |name: &str| output.registers.get(name).or_else(|| input.registers.get(name)).map_or(0, |v| number_of(v));
        let fsw = want("fsw") as u16;
        let top = vm.memory.read_u64(self.top).expect("the x87 top");
        if top != u64::from((fsw >> 11) & 7) {
            return Some(format!("the top of stack: expected {}, lifted {top}", (fsw >> 11) & 7));
        }
        let cw = vm.memory.read_u16(self.cw).expect("the x87 control word");
        if u128::from(cw) != want("fcw") {
            return Some(format!("fcw: expected {:#06x}, lifted {cw:#06x}", want("fcw")));
        }
        let sw = vm.memory.read_u16(self.sw).expect("the x87 status word");
        if sw & SW_COMPARED != fsw & SW_COMPARED {
            return Some(format!(
                "fsw: expected {:#06x}, lifted {:#06x} (the bits {SW_COMPARED:#06x} compared)",
                fsw & SW_COMPARED,
                sw & SW_COMPARED
            ));
        }
        let ftw = want("ftw") as u16;
        for i in 0..8u64 {
            if (ftw >> (2 * i)) & 3 == 3 {
                continue;
            }
            let slot = vm.memory.read_vec(self.stack + F80_SLOT * i, 10).expect("an x87 slot");
            let mut bytes = [0u8; 16];
            bytes[..10].copy_from_slice(&slot);
            let got = u128::from_le_bytes(bytes);
            let name = format!("fr{i}");
            if got != want(&name) {
                return Some(format!("{name}: expected {:#022x}, lifted {got:#022x}", want(&name)));
            }
        }
        None
    }
}

/// Every x87 vector replays through the exact unit of the lift and the interpreter, and exactly
/// the forms of `x87/PENDING.txt` differ; the files are written first when there are none.
#[test]
fn x87() {
    let header = pending_header("x87", "x87.rs", "x87 forms");
    let Replay { total, forms, failures } =
        replay_dir("x87", &generator::x87_dir(), &mut Exact::default(), generator::write_vectors, &header);
    assert!(total >= 2000, "{total} cases replayed at the 80-bit unit, at least 2000 expected");
    // The loads and stores of every width, the rounding stores, the exchange, the constants, the
    // compares and the transcendental forms are the point of the files.
    for form in [
        "Fld_m32fp",
        "Fld_m64fp",
        "Fld_m80fp",
        "Fstp_m80fp",
        "Fistp_m16int",
        "Fistp_m64int",
        "Fxch_st0_sti",
        "Fldpi",
        "Fucomip_st0_sti",
        "Fsin",
    ] {
        assert!(has_form(&forms, form), "no case of {form} among the vectors");
    }
    assert!(failures.is_empty(), "{} forms are not as PENDING.txt says:\n{}", failures.len(), failures.join("\n"));
}
