//! The MIPS decoder over the words of the freestanding mipsel hello (`testdata/src/hello.c`, see
//! `testdata/bin/FIXTURES.md`): the transfers the analysis walks, and when each takes effect. The
//! texts are llvm-objdump's for the same words.

use std::path::Path;

use ra_asm_arch::{Arch, Flow, Instr, Operand, Target};
use ra_asm_mips::{Delay, Mips};
use ra_asm_pe::Image;

fn hello() -> Image {
    ra_asm_pe::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin/hello_mipsel")).unwrap()
}

fn word(image: &Image, va: u64) -> u32 {
    u32::from_le_bytes(image.read_to_section_end(va).unwrap()[..4].try_into().unwrap())
}

fn at(image: &Image, va: u64) -> Instr {
    Mips::default().decode(image.read_to_section_end(va).unwrap(), va).unwrap_or_else(|| panic!("{va:#x} decodes"))
}

fn decode(word: u32, va: u64) -> Instr {
    Mips::default().decode(&word.to_le_bytes(), va).unwrap_or_else(|| panic!("{word:#010x} decodes"))
}

#[test]
fn jal_and_bal_are_calls_that_return_past_their_slot() {
    let image = hello();
    // `WriteFile` calls `fs_write` with `move $16, $7` in the slot.
    let jal = at(&image, 0x2_0170);
    assert_eq!(jal.text, "jal\t0x201d8");
    assert_eq!(jal.operands, [Operand::Addr(0x2_01d8)]);
    assert_eq!(jal.flow, Flow::Call(Target::Direct(0x2_01d8)));
    assert_eq!(Mips::delay(&jal), Delay::Slot);
    assert_eq!(Mips::successors(&jal), [0x2_0178]);
    let slot = at(&image, 0x2_0174);
    assert_eq!((slot.text.as_str(), slot.flow), ("move\t$16, $7", Flow::Next));
    assert_eq!(Mips::delay(&slot), Delay::None);
    assert_eq!(Mips::successors(&slot), [0x2_0178]);

    // `bal` reaches the same callee from the same place by an offset from the slot.
    let bal = decode(0x0411_0019, 0x2_0170);
    assert_eq!(bal.text, "bal\t0x201d8");
    assert_eq!(bal.flow, Flow::Call(Target::Direct(0x2_01d8)));
    assert_eq!(Mips::delay(&bal), Delay::Slot);
    assert_eq!(Mips::successors(&bal), [0x2_0178]);
}

#[test]
fn jr_ra_is_a_return_and_jr_of_another_register_a_jump() {
    let image = hello();
    // `GetStdHandle` returns with its result made in the slot.
    let ret = at(&image, 0x2_015c);
    assert_eq!((ret.text.as_str(), ret.flow), ("jr\t$ra", Flow::Return));
    assert_eq!(Mips::delay(&ret), Delay::Slot);
    assert!(Mips::successors(&ret).is_empty());
    assert_eq!(at(&image, 0x2_0160).text, "addiu\t$2, $1, 0x1");

    let jump = decode(0x0020_0008, 0x2_015c);
    assert_eq!((jump.text.as_str(), jump.flow), ("jr\t$1", Flow::Jump(Target::Indirect)));
    assert!(matches!(jump.indirect_operand(), Some(Operand::Reg(r)) if r.name == "$1"));
    assert_eq!(Mips::delay(&jump), Delay::Slot);
    assert!(Mips::successors(&jump).is_empty());
}

#[test]
fn a_conditional_branch_goes_to_its_target_or_past_its_slot() {
    let image = hello();
    // `WriteFile` skips the store through a null `written` pointer.
    let beqz = at(&image, 0x2_0178);
    assert_eq!(beqz.text, "beqz\t$16, 0x20190");
    assert_eq!(beqz.flow, Flow::CondJump(Target::Direct(0x2_0190)));
    assert_eq!(Mips::delay(&beqz), Delay::Slot);
    assert_eq!(Mips::successors(&beqz), [0x2_0190, 0x2_0180]);
    assert_eq!(at(&image, 0x2_017c).text, "nop");
}

#[test]
fn a_likely_branch_runs_its_slot_only_when_taken() {
    let image = hello();
    // The `beqz` of `WriteFile` with the opcode of `beql`: same operands, the slot annulled when
    // the branch falls through.
    let beql = decode(word(&image, 0x2_0178) ^ ((0x04 ^ 0x14) << 26), 0x2_0178);
    assert_eq!(beql.text, "beql\t$16, $zero, 0x20190");
    assert_eq!(beql.flow, Flow::CondJump(Target::Direct(0x2_0190)));
    assert_eq!(Mips::delay(&beql), Delay::Likely);
    assert_eq!(Mips::successors(&beql), [0x2_0190, 0x2_0180]);
}

#[test]
fn j_goes_inside_the_region_of_its_slot() {
    let image = hello();
    // `fs_exit` loops on its `exit` system call.
    let j = at(&image, 0x2_0200);
    assert_eq!(j.text, "j\t0x201f8");
    assert_eq!(j.operands, [Operand::Addr(0x2_01f8)]);
    assert_eq!(j.flow, Flow::Jump(Target::Direct(0x2_01f8)));
    assert_eq!(Mips::delay(&j), Delay::Slot);
    assert_eq!(Mips::successors(&j), [0x2_01f8]);
    let syscall = at(&image, 0x2_01fc);
    assert_eq!((syscall.text.as_str(), syscall.flow), ("syscall", Flow::Interrupt));
    assert_eq!(Mips::successors(&syscall), [0x2_0200]);

    // The region is the slot's top four bits, not the jump's.
    let far = decode(0x0800_0000, 0x0fff_fffc);
    assert_eq!(far.flow, Flow::Jump(Target::Direct(0x1000_0000)));
}

#[test]
fn the_entry_decodes_in_full_and_the_stop_word_does_not() {
    let image = hello();
    let start = image.entry_point().unwrap();
    let run = Mips::default().decode_all(&image.read_to_section_end(start).unwrap()[..24], start);
    let texts: Vec<&str> = run.iter().map(|i| i.text.as_str()).collect();
    assert_eq!(texts, ["addiu\t$sp, $sp, -0x10", "jal\t0x20210", "nop", "move\t$4, $2", "jal\t0x201f0", "nop"]);
    assert_eq!(run[0].operands[0], Operand::Reg(Mips::default().stack_pointer()));
    assert!(run.iter().all(|i| i.len == 4));
    // `ExitProcess` ends on a word no MIPS instruction has.
    assert!(Mips::default().decode(image.read_to_section_end(0x2_01b8).unwrap(), 0x2_01b8).is_none());
}

#[test]
fn a_big_endian_word_is_the_same_instruction() {
    let image = hello();
    let big = Mips { big_endian: true, ..Mips::default() };
    assert_eq!((big.name(), Mips::default().name()), ("mips", "mipsel"));
    let jal = big.decode(&word(&image, 0x2_0170).to_be_bytes(), 0x2_0170).unwrap();
    assert_eq!(jal, at(&image, 0x2_0170));
}
