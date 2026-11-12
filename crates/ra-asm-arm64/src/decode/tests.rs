// An instruction word reads as the disassembler prints it, without separators.
#![allow(clippy::unreadable_literal)]

use ra_asm_arch::{Access, Arch, Flow, Operand, RegClass, Target};

use super::Arm64;

fn one(word: u32, address: u64) -> ra_asm_arch::Instr {
    Arm64.decode(&word.to_le_bytes(), address).unwrap_or_else(|| panic!("{word:08x} decodes"))
}

fn full(word: u32, address: u64) -> super::Decoded {
    Arm64.decode_full(&word.to_le_bytes(), address).unwrap_or_else(|| panic!("{word:08x} decodes"))
}

#[test]
fn probe_yaxpeax_conventions() {
    // The instructions of hello's mainCRTStartup, with the decoder's own records, for the record.
    for (word, address) in [
        (0xd10083ff, 0x1_4000_1008u64), // sub sp, sp, #0x20
        (0xf9000bf3, 0x1_4000_100c),    // str x19, [sp, #0x10]
        (0xb0000008, 0x1_4000_1014),    // adrp x8, 0x140002000
        (0x12800140, 0x1_4000_1018),    // mov w0, #-0xb
        (0xf9403908, 0x1_4000_101c),    // ldr x8, [x8, #0x70]
        (0xd63f0100, 0x1_4000_1020),    // blr x8
        (0xaa0003f3, 0x1_4000_1024),    // mov x19, x0
        (0xb9000fff, 0x1_4000_1030),    // str wzr, [sp, #0xc]
        (0x97fffff2, 0x1_4000_1038),    // bl 0x140001000
        (0x910023e1, 0x1_4000_1040),    // add x1, sp, #0x8
        (0x72acccc8, 0x1_4000_1048),    // movk w8, #0x6666, lsl #16
        (0x9b287c08, 0x1_4000_1054),    // smull x8, w0, w8
        (0xd362fd09, 0x1_4000_1058),    // lsr x9, x8, #34
        (0x8b48fd28, 0x1_4000_105c),    // add x8, x9, x8, lsr #63
        (0x1b098108, 0x1_4000_1064),    // msub w8, w8, w9, w0
        (0xd4200020, 0x1_4000_1098),    // brk #0x1
        (0xf8627820, 0x1000),           // ldr x0, [x1, x2, lsl #3]
        (0xf8408420, 0x1000),           // ldr x0, [x1], #8
        (0xf81f8c20, 0x1000),           // str x0, [x1, #-8]!
        (0xa9bf7bfd, 0x1000),           // stp x29, x30, [sp, #-16]!
        (0x8b22c020, 0x1000),           // add x0, x1, w2, sxtw
        (0x54000041, 0x1000),           // b.ne +8
        (0xb4000040, 0x1000),           // cbz x0, +8
        (0x37280040, 0x1000),           // tbnz w0, #5, +8
        (0x9a9f17e0, 0x1000),           // cset x0, eq (csinc x0, xzr, xzr, ne)
        (0x7a4a1900, 0x1000),           // ccmp w8, #10, #0, ne
        (0x58000040, 0x1000),           // ldr x0, +8
        (0xd65f03c0, 0x1000),           // ret
        (0x10000040, 0x1000),           // adr x0, +8
    ] {
        let d = full(word, address);
        eprintln!("{word:08x} {:<40} {:?}", d.instr.text, d.inst.operands);
        eprintln!(
            "    {:?} flow={:?} regs={:?}",
            d.instr.operands,
            d.instr.flow,
            d.instr.registers.iter().map(|(r, a)| (r.name, *a)).collect::<Vec<_>>()
        );
    }
}

#[test]
fn a_branch_with_link_calls_its_target_and_writes_the_link_register() {
    let i = one(0x97fffff2, 0x1_4000_1038);
    assert_eq!(i.len, 4);
    assert_eq!(i.mnemonic, "bl");
    assert_eq!(i.flow, Flow::Call(Target::Direct(0x1_4000_1000)));
    assert_eq!(i.operands, [Operand::Addr(0x1_4000_1000)]);
    assert!(i.text.contains("0x140001000"), "{}", i.text);
    let x30 = i.registers.iter().find(|(r, _)| r.name == "x30").expect("the link register");
    assert_eq!(x30.1, Access::Write);
}

#[test]
fn adrp_names_the_page_of_its_target() {
    let i = one(0xb0000008, 0x1_4000_1014);
    assert_eq!(i.mnemonic, "adrp");
    assert_eq!(i.operands[1], Operand::Addr(0x1_4000_2000));
    // The load after it takes the whole address of the slot, across an instruction that writes
    // another register; not across one that writes the base.
    let other = one(0x12800140, 0x1_4000_1018); // mov w0, #-0xb
    let mut ldr = one(0xf9403908, 0x1_4000_101c);
    Arm64.link(&[i.clone(), other.clone()], &mut ldr);
    assert_eq!(ldr.absolute_memory_operand(), Some(0x1_4000_2070));
    assert!(ldr.reads_memory && !ldr.writes_memory);
    let clobber = one(0xaa0003e8, 0x1_4000_1018); // mov x8, x0
    let mut ldr = one(0xf9403908, 0x1_4000_101c);
    Arm64.link(&[i, other, clobber], &mut ldr);
    assert_eq!(ldr.absolute_memory_operand(), None);
}

#[test]
fn a_store_reads_its_register_and_a_pre_index_writes_its_base() {
    let i = one(0xa9bf7bfd, 0x1000); // stp x29, x30, [sp, #-16]!
    assert_eq!(i.mnemonic, "stp");
    assert!(i.writes_memory && !i.reads_memory);
    let sp = Arm64.stack_pointer();
    assert!(i.reads(sp) && i.writes(sp));
    let Operand::Mem(m) = &i.operands[2] else { panic!("a memory operand: {:?}", i.operands) };
    assert_eq!(m.disp, -16);
    assert_eq!(m.bits, 128);
    let x29 = i.registers.iter().find(|(r, _)| r.name == "x29").expect("x29 is read");
    assert_eq!(x29.1, Access::Read);
}

#[test]
fn a_conditional_branch_reads_the_flags_and_falls_through() {
    let i = one(0x54000041, 0x1000);
    assert_eq!(i.mnemonic, "b.ne");
    assert_eq!(i.flow, Flow::CondJump(Target::Direct(0x1008)));
    assert!(i.flow.falls_through());
    let flags = i.registers.iter().find(|(r, _)| r.class == RegClass::Flags).expect("the flags are read");
    assert_eq!(flags.1, Access::Read);
    let z = one(0xb4000040, 0x1000);
    assert_eq!(z.flow, Flow::CondJump(Target::Direct(0x1008)));
    assert!(!z.registers.iter().any(|(r, _)| r.class == RegClass::Flags));
}

#[test]
fn the_zero_register_and_the_stack_pointer_are_told_apart() {
    let i = one(0xb9000fff, 0x1000); // str wzr, [sp, #0xc]
    let Operand::Reg(wzr) = &i.operands[0] else { panic!("a register") };
    assert_eq!(wzr.name, "wzr");
    assert!(!i.registers.iter().any(|(r, _)| r.name == "wzr"), "the zero register is no access");
    let add = one(0x910023e1, 0x1000); // add x1, sp, #8
    let Operand::Reg(sp) = &add.operands[1] else { panic!("a register") };
    assert_eq!(sp.class, RegClass::StackPointer);
    let w = one(0xaa0003f3, 0x1000); // mov x19, x0 = orr x19, xzr, x0
    assert_eq!(w.mnemonic, "mov");
    assert_eq!(w.text, "mov x19, x0");
    let Operand::Reg(x19) = &w.operands[0] else { panic!("a register") };
    assert!(x19.is_full() && x19.bits == 64);
    let movk = one(0x72acccc8, 0x1000);
    let Operand::Reg(w8) = &movk.operands[0] else { panic!("a register") };
    assert_eq!((w8.full, w8.bits), (8, 32));
}

#[test]
fn a_run_stops_at_a_word_that_is_no_instruction() {
    let bytes = [0xd65f03c0u32.to_le_bytes(), 0xd503201fu32.to_le_bytes(), [0, 0, 0, 0]].concat();
    let run = Arm64.decode_all(&bytes, 0x2000);
    // Zero is `udf #0`, which decodes as a trap; the run holds all three.
    assert_eq!(run.len(), 3);
    assert_eq!(run[0].flow, Flow::Return);
    assert_eq!(run[1].mnemonic, "nop");
    assert_eq!(run[2].flow, Flow::Trap);
    assert!(Arm64.decode(&[0xc0, 0x03], 0).is_none(), "a short tail is no instruction");
}
