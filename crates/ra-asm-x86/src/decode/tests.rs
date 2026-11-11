use ra_asm_arch::{Access, Arch, Flow, Operand, RegClass, Target};

use super::X86;

fn one(bits: u32, bytes: &[u8], address: u64) -> ra_asm_arch::Instr {
    X86 { bits }.decode(bytes, address).unwrap_or_else(|| panic!("{bytes:02x?} decodes"))
}

#[test]
fn a_rip_relative_call_resolves_its_slot_and_its_flow() {
    // call qword ptr [rip+0x1031] at 0x140001019, 6 bytes: the slot is 0x14000201f + 0x1031.
    let i = one(64, &[0xff, 0x15, 0x31, 0x10, 0x00, 0x00], 0x1_4000_1019);
    assert_eq!(i.len, 6);
    assert_eq!(i.mnemonic, "call");
    // The text shows the resolved address, as a reader wants it; the operand keeps the RIP base.
    assert_eq!(i.text, "call qword ptr [0x140002050]");
    assert_eq!(i.flow, Flow::Call(Target::Indirect));
    assert_eq!(i.absolute_memory_operand(), Some(0x1_4000_2050));
    // The call reads the slot and writes the return address to the stack.
    assert!(i.reads_memory && i.writes_memory);
    let Operand::Mem(m) = &i.operands[0] else { panic!("a memory operand") };
    assert_eq!(m.bits, 64);
    assert_eq!(m.base.unwrap().name, "rip");
    // The call writes the stack pointer and reads it; the stack write counts as a memory write of the push.
    let rsp = X86::X64.stack_pointer();
    assert!(i.reads(rsp) && i.writes(rsp));
}

#[test]
fn a_conditional_jump_has_a_direct_target_and_falls_through() {
    // jne +0x10 at 0x1000, 2 bytes.
    let i = one(64, &[0x75, 0x10], 0x1000);
    assert_eq!(i.flow, Flow::CondJump(Target::Direct(0x1012)));
    assert!(i.flow.falls_through());
    assert_eq!(i.flow.direct_target(), Some(0x1012));
    assert_eq!(i.operands, [Operand::Addr(0x1012)]);
    let flags = i.registers.iter().find(|(r, _)| r.class == RegClass::Flags).expect("the flags are read");
    assert_eq!(flags.1, Access::Read);
}

#[test]
fn sub_registers_name_their_full_register_and_their_place() {
    // mov ah, 1
    let i = one(64, &[0xb4, 0x01], 0);
    let Operand::Reg(ah) = &i.operands[0] else { panic!("a register") };
    assert_eq!((ah.name, ah.bits, ah.shift), ("ah", 8, 8));
    assert_eq!(ah.full, X86::X64.decode(&[0x48, 0x89, 0xc0], 0).unwrap().operands[0].clone().reg().id);
    assert_eq!(i.operands[1], Operand::Imm { value: 1, bits: 8 });
    assert!(i.flow == Flow::Next);
}

#[test]
fn a_32_bit_absolute_operand_is_a_constant_address() {
    // mov eax, [0x403000] in 32-bit mode.
    let i = one(32, &[0xa1, 0x00, 0x30, 0x40, 0x00], 0x40_1000);
    assert_eq!(i.absolute_memory_operand(), Some(0x40_3000));
    assert_eq!(i.text, "mov eax, [0x403000]");
    // sub esp, 8: a stack pointer write, no memory.
    let i = one(32, &[0x83, 0xec, 0x08], 0x40_1005);
    assert!(i.writes(X86::X32.stack_pointer()) && !i.writes_memory);
    assert_eq!(i.operands[1], Operand::Imm { value: 8, bits: 32 });
}

#[test]
fn traps_returns_and_string_moves() {
    assert_eq!(one(64, &[0xcc], 0).flow, Flow::Trap);
    assert_eq!(one(64, &[0x0f, 0x0b], 0).flow, Flow::Trap);
    assert_eq!(one(64, &[0xc3], 0).flow, Flow::Return);
    assert_eq!(one(64, &[0xf4], 0).flow, Flow::Halt);
    assert_eq!(one(64, &[0xcd, 0x29], 0).flow, Flow::Interrupt);
    let movs = one(64, &[0xf3, 0x48, 0xa5], 0);
    assert!(movs.prefixes.rep);
    assert!(movs.reads_memory && movs.writes_memory);
    assert_eq!(movs.text, "rep movsq [rdi], [rsi]");
    let Operand::Mem(m) = &movs.operands[1] else { panic!("a memory operand") };
    assert_eq!((m.base.unwrap().name, m.bits), ("rsi", 64));
}

#[test]
fn invalid_bytes_decode_to_nothing_and_decode_all_stops_there() {
    assert!(X86::X64.decode(&[0x06], 0).is_none()); // push es is invalid in 64-bit mode
    let run = X86::X64.decode_all(&[0x90, 0x90, 0x06, 0x90], 0x2000);
    assert_eq!(run.len(), 2);
    assert_eq!(run[1].address, 0x2001);
}

trait RegOf {
    fn reg(self) -> ra_asm_arch::Register;
}

impl RegOf for Operand {
    fn reg(self) -> ra_asm_arch::Register {
        match self {
            Operand::Reg(r) => r,
            other => panic!("{other:?} is not a register"),
        }
    }
}
