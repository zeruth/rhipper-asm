//! iced's registers as the neutral [`Register`]: the full register each one lives in, and where.

use iced_x86::Register as R;
use ra_asm_arch::{RegClass, Register};

/// The neutral record of an iced register.
pub fn register(r: R) -> Register {
    let full = r.full_register();
    let class = if r == R::RSP || r == R::ESP || r == R::SP || r == R::SPL {
        RegClass::StackPointer
    } else if r == R::RIP || r == R::EIP {
        RegClass::InstructionPointer
    } else if r.is_segment_register() {
        RegClass::Segment
    } else if r.is_gpr() {
        RegClass::General
    } else if r.is_vector_register() {
        RegClass::Vector
    } else if r.is_st() {
        RegClass::Float
    } else {
        RegClass::Other
    };
    // The high byte registers sit at bit 8 of their full register; everything else at bit 0.
    let shift = if matches!(r, R::AH | R::BH | R::CH | R::DH) { 8 } else { 0 };
    Register { id: r as u16, name: name(r), class, full: full as u16, bits: (r.size() * 8) as u16, shift }
}

/// The full register that holds `id`.
pub fn full_of(id: u16) -> Register {
    register(R::try_from(id as usize).unwrap_or(R::None).full_register())
}

/// The flags register of a mode: a register iced does not have, numbered past its last one.
pub const FLAGS_ID: u16 = u16::MAX;

pub fn flags(bits: u16) -> Register {
    Register { id: FLAGS_ID, name: if bits == 64 { "rflags" } else { "eflags" }, class: RegClass::Flags, full: FLAGS_ID, bits, shift: 0 }
}

/// iced's name for a register, lower case.
pub fn name(r: R) -> &'static str {
    // iced's Debug form is the upper-case name; the table keeps the common ones as static strings.
    macro_rules! names {
        ($($reg:ident => $name:literal),* $(,)?) => {
            match r { $(R::$reg => $name,)* _ => other_name(r) }
        };
    }
    names! {
        AL => "al", CL => "cl", DL => "dl", BL => "bl", AH => "ah", CH => "ch", DH => "dh", BH => "bh",
        SPL => "spl", BPL => "bpl", SIL => "sil", DIL => "dil",
        R8L => "r8b", R9L => "r9b", R10L => "r10b", R11L => "r11b", R12L => "r12b", R13L => "r13b", R14L => "r14b", R15L => "r15b",
        AX => "ax", CX => "cx", DX => "dx", BX => "bx", SP => "sp", BP => "bp", SI => "si", DI => "di",
        R8W => "r8w", R9W => "r9w", R10W => "r10w", R11W => "r11w", R12W => "r12w", R13W => "r13w", R14W => "r14w", R15W => "r15w",
        EAX => "eax", ECX => "ecx", EDX => "edx", EBX => "ebx", ESP => "esp", EBP => "ebp", ESI => "esi", EDI => "edi",
        R8D => "r8d", R9D => "r9d", R10D => "r10d", R11D => "r11d", R12D => "r12d", R13D => "r13d", R14D => "r14d", R15D => "r15d",
        RAX => "rax", RCX => "rcx", RDX => "rdx", RBX => "rbx", RSP => "rsp", RBP => "rbp", RSI => "rsi", RDI => "rdi",
        R8 => "r8", R9 => "r9", R10 => "r10", R11 => "r11", R12 => "r12", R13 => "r13", R14 => "r14", R15 => "r15",
        EIP => "eip", RIP => "rip",
        ES => "es", CS => "cs", SS => "ss", DS => "ds", FS => "fs", GS => "gs",
        XMM0 => "xmm0", XMM1 => "xmm1", XMM2 => "xmm2", XMM3 => "xmm3", XMM4 => "xmm4", XMM5 => "xmm5", XMM6 => "xmm6", XMM7 => "xmm7",
        XMM8 => "xmm8", XMM9 => "xmm9", XMM10 => "xmm10", XMM11 => "xmm11", XMM12 => "xmm12", XMM13 => "xmm13", XMM14 => "xmm14", XMM15 => "xmm15",
        YMM0 => "ymm0", YMM1 => "ymm1", YMM2 => "ymm2", YMM3 => "ymm3", YMM4 => "ymm4", YMM5 => "ymm5", YMM6 => "ymm6", YMM7 => "ymm7",
        YMM8 => "ymm8", YMM9 => "ymm9", YMM10 => "ymm10", YMM11 => "ymm11", YMM12 => "ymm12", YMM13 => "ymm13", YMM14 => "ymm14", YMM15 => "ymm15",
        ST0 => "st0", ST1 => "st1", ST2 => "st2", ST3 => "st3", ST4 => "st4", ST5 => "st5", ST6 => "st6", ST7 => "st7",
        MM0 => "mm0", MM1 => "mm1", MM2 => "mm2", MM3 => "mm3", MM4 => "mm4", MM5 => "mm5", MM6 => "mm6", MM7 => "mm7",
    }
}

fn other_name(r: R) -> &'static str {
    // A register outside the table (a control, debug, mask or upper ZMM register) keeps iced's
    // Debug name, leaked once per distinct register: there are a few hundred at most.
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static NAMES: OnceLock<Mutex<HashMap<u16, &'static str>>> = OnceLock::new();
    let names = NAMES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut names = names.lock().unwrap();
    names.entry(r as u16).or_insert_with(|| Box::leak(format!("{r:?}").to_lowercase().into_boxed_str()))
}
