//! The ELF container of an executable or a shared object, 32 or 64 bits, either byte order: the
//! program headers that map the file by virtual address, the section headers, the symbol table
//! when the file keeps one, the dynamic section (the libraries it needs, its own name, the dynamic
//! symbols with the library each one's version names, the relocations the dynamic loader applies)
//! and the function extents `.eh_frame` records, one FDE per function; the relocations a static
//! link kept; and a MIPS file's ABI, ISA and `$gp` from its flags, `.MIPS.abiflags` and `.reginfo`,
//! with the addresses its `HI16`/`LO16` pairs build; and a PSP module's (`ET_PRX`) module
//! information, its NID tables and its relocations, the file moved to where the PSP loads it.
//! What a container of this
//! shape means to the pipeline (an import, an export, a section) is the loader's to say; this
//! reads what the file holds, and every refusal names the field and its offset.

use std::collections::HashMap;

use anyhow::{Context, Result, bail, ensure};

use crate::Reader;

pub const MAGIC: &[u8; 4] = b"\x7fELF";

pub const ET_EXEC: u16 = 2;
pub const ET_DYN: u16 = 3;
/// A PSP module, linked at zero and relocated by the PSP's own records; see [`psp`].
pub const ET_PRX: u16 = 0xffa0;
/// A `PlayStation 2` IOP module, linked at zero and relocated by its static relocations; see [`iop`].
pub const ET_IRX: u16 = 0xff80;

pub const EM_386: u16 = 3;
/// MIPS of either byte order; ELF's class says 32 or 64 bits.
pub const EM_MIPS: u16 = 8;
pub const EM_ARM: u16 = 40;
pub const EM_X86_64: u16 = 62;
pub const EM_AARCH64: u16 = 183;

pub const PT_LOAD: u32 = 1;
pub const PT_DYNAMIC: u32 = 2;
pub const PT_INTERP: u32 = 3;
pub const PT_GNU_EH_FRAME: u32 = 0x6474_e550;
pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_R: u32 = 4;

pub const SHT_PROGBITS: u32 = 1;
pub const SHT_SYMTAB: u32 = 2;
pub const SHT_RELA: u32 = 4;
pub const SHT_DYNSYM: u32 = 11;
pub const SHT_NOBITS: u32 = 8;
pub const SHT_REL: u32 = 9;
pub const SHF_WRITE: u64 = 1;
pub const SHF_ALLOC: u64 = 2;
pub const SHF_EXECINSTR: u64 = 4;
pub const SHF_TLS: u64 = 0x400;

pub const STT_OBJECT: u8 = 1;
pub const STT_FUNC: u8 = 2;
pub const STT_GNU_IFUNC: u8 = 10;
pub const STB_GLOBAL: u8 = 1;
pub const STB_WEAK: u8 = 2;
/// The first reserved section index: a symbol at or past it is absolute or common, not in a section.
pub const SHN_LORESERVE: u16 = 0xff00;

const DT_NULL: u64 = 0;
const DT_NEEDED: u64 = 1;
const DT_PLTRELSZ: u64 = 2;
const DT_HASH: u64 = 4;
const DT_STRTAB: u64 = 5;
const DT_SYMTAB: u64 = 6;
const DT_RELA: u64 = 7;
const DT_RELASZ: u64 = 8;
const DT_STRSZ: u64 = 10;
const DT_SYMENT: u64 = 11;
const DT_SONAME: u64 = 14;
const DT_REL: u64 = 17;
const DT_RELSZ: u64 = 18;
const DT_PLTREL: u64 = 20;
const DT_JMPREL: u64 = 23;
const DT_RELRSZ: u64 = 35;
const DT_RELR: u64 = 36;
const DT_VERSYM: u64 = 0x6fff_fff0;
const DT_VERNEED: u64 = 0x6fff_fffe;
const DT_VERNEEDNUM: u64 = 0x6fff_ffff;

/// The bound of the tables the reader walks: a real file is far below it, a corrupt count is not a
/// reason to loop for long.
const MAX_ENTRIES: usize = 1 << 20;

/// The kinds of relocation the loader gives a meaning to, by machine.
pub mod reloc {
    pub const X86_64_64: u32 = 1;
    pub const X86_64_COPY: u32 = 5;
    pub const X86_64_GLOB_DAT: u32 = 6;
    pub const X86_64_JUMP_SLOT: u32 = 7;
    pub const X86_64_RELATIVE: u32 = 8;
    pub const I386_32: u32 = 1;
    pub const I386_COPY: u32 = 5;
    pub const I386_GLOB_DAT: u32 = 6;
    pub const I386_JMP_SLOT: u32 = 7;
    pub const I386_RELATIVE: u32 = 8;
    pub const AARCH64_ABS64: u32 = 257;
    pub const AARCH64_COPY: u32 = 1024;
    pub const AARCH64_GLOB_DAT: u32 = 1025;
    pub const AARCH64_JUMP_SLOT: u32 = 1026;
    pub const AARCH64_RELATIVE: u32 = 1027;
    /// A 32-bit word that holds the address.
    pub const MIPS_32: u32 = 2;
    /// The 26-bit target field of a `j` or a `jal`: the address's bits 2 to 27, its top four
    /// bits those of the delay slot's address.
    pub const MIPS_26: u32 = 4;
    /// The high half of an address in a `lui`, adjusted for the sign of the low half.
    pub const MIPS_HI16: u32 = 5;
    /// The low half of an address in an `addiu`, a load or a store, sign-extended.
    pub const MIPS_LO16: u32 = 6;
    /// The PSP's own kinds in a PRX's records: a high half of the kernel's form, which the
    /// reader does not apply, and the target field of a `j` and of a `jal`, whose opcode the
    /// loader writes with the field.
    pub const MIPS_X_HI16: u32 = 13;
    pub const MIPS_X_J26: u32 = 14;
    pub const MIPS_X_JAL26: u32 = 15;

    /// The kind that adds the load bias to the word in place, which a packed `DT_RELR` entry stands for.
    pub fn relative(machine: u16) -> Option<u32> {
        match machine {
            super::EM_X86_64 => Some(X86_64_RELATIVE),
            super::EM_386 => Some(I386_RELATIVE),
            super::EM_AARCH64 => Some(AARCH64_RELATIVE),
            _ => None,
        }
    }
}

/// One program header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub kind: u32,
    pub flags: u32,
    pub offset: u64,
    pub vaddr: u64,
    /// `p_paddr`: unused by a process, but the file offset of the module information in a PRX's
    /// first segment.
    pub paddr: u64,
    pub filesz: u64,
    pub memsz: u64,
    pub align: u64,
}

impl Segment {
    pub fn is_load(&self) -> bool {
        self.kind == PT_LOAD
    }
}

/// One section header, with its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionHeader {
    pub name: String,
    pub kind: u32,
    pub flags: u64,
    pub addr: u64,
    pub offset: u64,
    pub size: u64,
    pub link: u32,
    pub info: u32,
    pub entsize: u64,
}

impl SectionHeader {
    /// Whether the section takes room in the process's memory: allocated, and not the TLS template
    /// of zeros, which each thread gets a copy of and the image itself does not hold.
    pub fn is_mapped(&self) -> bool {
        self.flags & SHF_ALLOC != 0 && self.addr != 0 && self.size != 0 && !(self.kind == SHT_NOBITS && self.flags & SHF_TLS != 0)
    }
}

/// One record of a symbol table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub value: u64,
    pub size: u64,
    /// `STT_*`, the low nibble of `st_info`.
    pub kind: u8,
    /// `STB_*`, the high nibble of `st_info`.
    pub binding: u8,
    /// `STV_*`, the low two bits of `st_other`.
    pub visibility: u8,
    /// The index of the section that defines the symbol; zero when it is undefined here.
    pub section: u16,
    /// The library the symbol's version requires it from, for a dynamic symbol the file imports
    /// under a version (`libc.so.6` for `puts@GLIBC_2.2.5`).
    pub library: Option<String>,
}

impl Symbol {
    /// Whether the file defines the symbol in one of its sections.
    pub fn is_defined(&self) -> bool {
        self.section != 0 && self.section < SHN_LORESERVE
    }

    pub fn is_function(&self) -> bool {
        matches!(self.kind, STT_FUNC | STT_GNU_IFUNC)
    }

    /// Whether another module may bind to it: global or weak, and of default or protected visibility.
    pub fn is_visible(&self) -> bool {
        matches!(self.binding, STB_GLOBAL | STB_WEAK) && matches!(self.visibility, 0 | 3)
    }
}

/// One relocation: the place it patches, its kind, and the symbol it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Relocation {
    /// The address of the place: `r_offset` of a linked file's record.
    pub offset: u64,
    pub kind: u32,
    /// The index of the symbol in the dynamic symbol table for a dynamic relocation, in `.symtab`
    /// for a static one; zero for none.
    pub symbol: u32,
    /// The explicit addend of a `RELA` record; zero for a `REL` one, whose addend is in place.
    pub addend: i64,
}

/// The `EF_MIPS_*` fields of a MIPS file's `e_flags`, and the section and segment types of its
/// register information and its ABI flags.
pub mod mips {
    pub const EF_MIPS_NOREORDER: u32 = 0x1;
    pub const EF_MIPS_PIC: u32 = 0x2;
    pub const EF_MIPS_CPIC: u32 = 0x4;
    /// The n32 ABI: 64-bit registers, 32-bit pointers, in a 32-bit file.
    pub const EF_MIPS_ABI2: u32 = 0x20;
    pub const EF_MIPS_ABI: u32 = 0x0000_f000;
    pub const EF_MIPS_ABI_O32: u32 = 0x0000_1000;
    pub const EF_MIPS_ABI_O64: u32 = 0x0000_2000;
    pub const EF_MIPS_ABI_EABI32: u32 = 0x0000_3000;
    pub const EF_MIPS_ABI_EABI64: u32 = 0x0000_4000;
    /// The processor the code is for beyond its ISA (`EF_MIPS_MACH_5900`, `_ALLEGREX`…).
    pub const EF_MIPS_MACH: u32 = 0x00ff_0000;
    /// The Toshiba R5900, the `PlayStation 2`'s Emotion Engine.
    pub const EF_MIPS_MACH_5900: u32 = 0x0092_0000;
    pub const EF_MIPS_ARCH: u32 = 0xf000_0000;
    pub const SHT_MIPS_REGINFO: u32 = 0x7000_0006;
    pub const SHT_MIPS_ABIFLAGS: u32 = 0x7000_002a;
    pub const PT_MIPS_REGINFO: u32 = 0x7000_0000;
    pub const PT_MIPS_ABIFLAGS: u32 = 0x7000_0003;
    /// The size of an `Elf32_RegInfo` and of an `Elf_MIPS_ABIFlags_v0`.
    pub const REGINFO_SIZE: u64 = 24;
    pub const ABIFLAGS_SIZE: u64 = 24;
}

/// The calling convention a MIPS file's code follows, as its header says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MipsAbi {
    O32,
    N32,
    O64,
    Eabi32,
    Eabi64,
    N64,
}

impl MipsAbi {
    pub fn name(self) -> &'static str {
        match self {
            Self::O32 => "o32",
            Self::N32 => "n32",
            Self::O64 => "o64",
            Self::Eabi32 => "eabi32",
            Self::Eabi64 => "eabi64",
            Self::N64 => "n64",
        }
    }
}

/// What `.MIPS.abiflags` records: the ISA the code needs and the register widths it assumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MipsAbiFlags {
    pub version: u16,
    /// The ISA's level (1 to 5, 32, 64) and its revision.
    pub isa_level: u8,
    pub isa_rev: u8,
    /// The widths in bits of the general registers and of coprocessors 1 and 2; zero for none.
    pub gpr_bits: u16,
    pub cpr1_bits: u16,
    pub cpr2_bits: u16,
    /// `Val_GNU_MIPS_ABI_FP_*`: how the code passes and keeps floating-point values.
    pub fp_abi: u8,
    pub isa_ext: u32,
    pub ases: u32,
    pub flags1: u32,
    pub flags2: u32,
}

/// What `.reginfo` records: the general and coprocessor registers the code uses and the value of `$gp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MipsRegInfo {
    pub gpr_mask: u32,
    pub cpr_mask: [u32; 4],
    pub gp: u32,
}

/// An address a `lui` and a low half build between them: the place of the `R_MIPS_HI16` and of the
/// `R_MIPS_LO16` that completes it, and the address the two immediates make.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MipsPair {
    pub high: u64,
    pub low: u64,
    pub target: u64,
}

/// What a MIPS file says of its code beyond the machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mips {
    /// `e_flags`, whose fields are the constants of [`mips`].
    pub flags: u32,
    pub abi: MipsAbi,
    pub abiflags: Option<MipsAbiFlags>,
    /// The register information of a 32-bit file; a 64-bit one keeps it in `.MIPS.options`.
    pub reginfo: Option<MipsRegInfo>,
    /// The address pairs of the static relocations, in their records' order: each `R_MIPS_HI16`
    /// with the first `R_MIPS_LO16` of the same symbol after it, as the ABI orders them. A low half
    /// that shares an earlier high half is not a pair of its own.
    pub pairs: Vec<MipsPair>,
}

impl Mips {
    /// The ISA of `EF_MIPS_ARCH`: `mips1` to `mips5`, `mips32`, `mips64` and their revisions.
    pub fn arch(&self) -> Option<&'static str> {
        const NAMES: [&str; 11] =
            ["mips1", "mips2", "mips3", "mips4", "mips5", "mips32", "mips64", "mips32r2", "mips64r2", "mips32r6", "mips64r6"];
        NAMES.get((self.flags >> 28) as usize).copied()
    }

    /// The processor of `EF_MIPS_MACH` the code is for, when the file names one the reader knows.
    pub fn mach(&self) -> Option<&'static str> {
        self.is_r5900().then_some("r5900")
    }

    /// Whether the code is the `PlayStation 2`'s Emotion Engine's, an R5900's.
    pub fn is_r5900(&self) -> bool {
        self.flags & mips::EF_MIPS_MACH == mips::EF_MIPS_MACH_5900
    }

    /// Whether the code calls through the GOT at `$gp`, as abicalls and PIC code do
    /// (`EF_MIPS_CPIC`, `EF_MIPS_PIC`).
    pub fn abicalls(&self) -> bool {
        self.flags & (mips::EF_MIPS_PIC | mips::EF_MIPS_CPIC) != 0
    }

    /// Whether the code fills its own delay slots, so the assembler must not reorder it.
    pub fn noreorder(&self) -> bool {
        self.flags & mips::EF_MIPS_NOREORDER != 0
    }
}

/// The PSP: where its loader puts a module, the types of a PRX's relocation tables and the
/// regions of the address space a module's code reaches.
pub mod psp {
    /// The records of a PRX's relocations, in a section of this type or, in a file stripped of
    /// its sections, a program header of it.
    pub const SHT_PRXRELOC: u32 = 0x7000_00a0;
    pub const PT_PRXRELOC: u32 = 0x7000_00a0;
    /// The packed records of a later firmware's PRX, which the reader does not decode.
    pub const PT_PRXRELOC2: u32 = 0x7000_00a1;
    /// The module attribute of a kernel-mode module.
    pub const MODULE_KERNEL: u16 = 0x1000;
    /// Where the user-mode loader puts the first module: the start of user RAM past the 16 KiB
    /// the kernel keeps there.
    pub const LOAD_ADDRESS: u64 = 0x0880_4000;

    /// One region of the PSP's address space.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Region {
        pub name: &'static str,
        pub start: u64,
        pub size: u64,
        /// Hardware rather than memory: a read or a write there is the device's, not a value kept.
        pub device: bool,
        /// The region whose memory this one shows at another address.
        pub mirror_of: Option<u64>,
    }

    impl Region {
        pub fn contains(&self, va: u64) -> bool {
            va >= self.start && va - self.start < self.size
        }
    }

    /// The regions a module's code addresses: the scratchpad, the video memory the display reads,
    /// the 32 MiB of main RAM (the kernel's 8 MiB, then user RAM) and the kernel's mirror of it.
    pub const REGIONS: &[Region] = &[
        Region { name: "scratchpad", start: 0x0001_0000, size: 0x4000, device: false, mirror_of: None },
        Region { name: "vram", start: 0x0400_0000, size: 0x20_0000, device: true, mirror_of: None },
        Region { name: "kernel ram", start: 0x0800_0000, size: 0x80_0000, device: false, mirror_of: None },
        Region { name: "user ram", start: 0x0880_0000, size: 0x180_0000, device: false, mirror_of: None },
        Region { name: "kernel mirror", start: 0x8800_0000, size: 0x200_0000, device: false, mirror_of: Some(0x0800_0000) },
    ];

    /// The region that holds `va`.
    pub fn region(va: u64) -> Option<&'static Region> {
        REGIONS.iter().find(|r| r.contains(va))
    }
}

/// The `PlayStation 2`'s Emotion Engine: where an EE program loads and the memory map its code
/// addresses. The map folds every mirror onto one address per place: RAM seen through `kseg0`,
/// `kseg1`, the uncached and the uncached-accelerated windows is its `kuseg` address, the one the
/// SDK links at; the registers and the BIOS are their physical addresses.
pub mod ps2 {
    use crate::psx::Region;

    /// Where the SDK's linkfile puts an EE program: past the first MiB, which the kernel keeps.
    pub const LOAD_ADDRESS: u64 = 0x0010_0000;
    /// Main RAM, 32 MiB, at its `kuseg` address.
    pub const RAM: u64 = 0;
    pub const RAM_SIZE: u64 = 0x200_0000;
    /// RAM again, uncached, and uncached through the buffer that accelerates sequential writes.
    pub const UNCACHED: u64 = 0x2000_0000;
    pub const UNCACHED_ACCELERATED: u64 = 0x3000_0000;
    /// The 16 KiB scratchpad, which the TLB maps at this address only.
    pub const SCRATCHPAD: u64 = 0x7000_0000;
    pub const SCRATCHPAD_SIZE: u64 = 0x4000;
    /// The EE's device registers (the timers, the IPU, the GIF, the VIFs, DMA, the interrupt
    /// controller, the serial port) and the memories of the vector units at `0x11000000`.
    pub const HARDWARE: u64 = 0x1000_0000;
    pub const HARDWARE_SIZE: u64 = 0x200_0000;
    /// The BIOS ROM, 4 MiB.
    pub const BIOS: u64 = 0x1fc0_0000;
    pub const BIOS_SIZE: u64 = 0x40_0000;

    /// The place `va` reaches and its folded address, or `None` for an address no place answers.
    /// A 32-bit address the 64-bit registers hold sign-extended, as an R5900 forms `kseg0`'s, is
    /// the same address.
    pub fn fold(va: u64) -> Option<(Region, u64)> {
        let va = u64::from(u32::try_from(va).ok().or_else(|| i32::try_from(va as i64).ok().map(|v| v as u32))?);
        let within = |address: u64, base: u64, size: u64| address >= base && address - base < size;
        for mirror in [UNCACHED, UNCACHED_ACCELERATED] {
            if within(va, mirror, RAM_SIZE) {
                return Some((Region::Ram, va - mirror));
            }
        }
        if within(va, SCRATCHPAD, SCRATCHPAD_SIZE) {
            return Some((Region::Scratchpad, va));
        }
        let physical = match va >> 29 {
            0 | 4 | 5 => va & 0x1fff_ffff,
            _ => return None,
        };
        if within(physical, RAM, RAM_SIZE) {
            Some((Region::Ram, physical))
        } else if within(physical, HARDWARE, HARDWARE_SIZE) {
            Some((Region::Hardware, physical))
        } else if within(physical, BIOS, BIOS_SIZE) {
            Some((Region::Bios, physical))
        } else {
            None
        }
    }
}

/// The `PlayStation 2`'s I/O processor: its modules, IRX files, are ELF files of their own type
/// linked at zero, whose `.iopmod` section says where the module's entry, `$gp` and parts are,
/// and whose imports and exports are tables in the code that name a library and number its
/// functions. The IOP's loader places a module wherever its memory is free; the reader moves one
/// to [`iop::LOAD_ADDRESS`].
pub mod iop {
    /// The module header's section, or in a file stripped of its sections its program header.
    pub const SHT_IOPMOD: u32 = 0x7000_0080;
    /// Where the reader loads a module: inside the IOP's 2 MiB of RAM, past the kernel and the
    /// modules of the boot, on a 64 KiB boundary so the low half of every address stays the link's.
    pub const LOAD_ADDRESS: u64 = 0x0010_0000;
    /// The first word of an export table, a library the module defines.
    pub const EXPORT_MAGIC: u32 = 0x41c0_0000;
    /// The first word of an import table, a library the module calls into.
    pub const IMPORT_MAGIC: u32 = 0x41e0_0000;
}

/// A function of an IOP library, by its number: the stub a call to an import goes through, or
/// what a module exports under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IopEntry {
    pub ordinal: u16,
    pub address: u64,
}

/// A library an IRX imports from or exports, by its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IopLibrary {
    pub name: String,
    /// The major version in the high byte, the minor in the low.
    pub version: u16,
    pub functions: Vec<IopEntry>,
    /// The data of the table in the text, `(start, end)`: an export table from its magic word past
    /// its last entry, an import table its header before the stubs, which are code.
    pub table: (u64, u64),
}

/// What an IRX's `.iopmod` header and its library tables hold, at the address the reader loaded
/// the module at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Irx {
    /// [`iop::LOAD_ADDRESS`]: every address of the file, its segments and its sections are there.
    pub load: u64,
    /// The name `IRX_ID` gives the module, else the header's.
    pub name: String,
    /// The major version in the high byte, the minor in the low.
    pub version: u16,
    /// The value of `$gp` the module's code assumes.
    pub gp: u64,
    pub text_size: u32,
    pub data_size: u32,
    pub bss_size: u32,
    pub exports: Vec<IopLibrary>,
    pub imports: Vec<IopLibrary>,
    /// The static relocations the IOP's loader applies, every one relative to the module's base,
    /// in the file's order.
    pub relocations: Vec<PrxRelocation>,
}

/// One relocation of a PRX, at the address the module loads at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrxRelocation {
    /// The place it patches: the record's offset past the start of the segment it names.
    pub place: u64,
    /// A `reloc::MIPS_*` kind.
    pub kind: u32,
    /// What the place's value is relative to and gets added: the address of the segment the
    /// record names as its base.
    pub base: u64,
    /// The place of the `MIPS_LO16` that completes an `MIPS_HI16`: the first after it of the same
    /// base, whose immediate the high half's address carries the sign of.
    pub low: Option<u64>,
}

/// A NID and the address it stands at: the stub a call to an import goes through, or what a
/// module exports under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PspEntry {
    pub nid: u32,
    pub address: u64,
}

/// A library a PRX imports from or exports, by its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PspLibrary {
    /// The name; `syslib` for the module's own exports the loader calls (`module_start`...),
    /// whose entry has none.
    pub name: String,
    pub version: u16,
    pub attributes: u16,
    pub functions: Vec<PspEntry>,
    pub variables: Vec<PspEntry>,
}

/// What a PRX's module information, `.rodata.sceModuleInfo`, holds, and its relocations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prx {
    /// The address the reader loaded the module at, [`psp::LOAD_ADDRESS`]: every address of the
    /// file, its segments and its sections are there.
    pub load: u64,
    pub name: String,
    pub attributes: u16,
    /// The major and the minor version.
    pub version: (u8, u8),
    /// The value of `$gp` the module's code assumes.
    pub gp: u64,
    pub exports: Vec<PspLibrary>,
    pub imports: Vec<PspLibrary>,
    /// The relocations the loader applies, in the file's order.
    pub relocations: Vec<PrxRelocation>,
}

impl Prx {
    pub fn is_kernel(&self) -> bool {
        self.attributes & psp::MODULE_KERNEL != 0
    }
}

/// What an ELF file holds, as read.
#[derive(Debug, Clone)]
pub struct ElfFile {
    /// `ELFCLASS64`.
    pub wide: bool,
    /// `ELFDATA2MSB`.
    pub big_endian: bool,
    /// `e_type`: [`ET_EXEC`], [`ET_DYN`], ...
    pub kind: u16,
    /// `e_machine`: [`EM_X86_64`], [`EM_386`], ...
    pub machine: u16,
    /// `e_flags`, whose meaning is the machine's.
    pub flags: u32,
    pub entry: u64,
    pub segments: Vec<Segment>,
    /// The section headers, in the file's order; empty for a file stripped of them.
    pub sections: Vec<SectionHeader>,
    /// The bytes from the start of the file to the end of the program headers.
    pub headers_size: u64,
    /// The static symbol table, `.symtab`, in its order; empty when the file was stripped.
    pub symbols: Vec<Symbol>,
    /// The dynamic symbol table, by index.
    pub dynamic_symbols: Vec<Symbol>,
    /// The program interpreter, `PT_INTERP`: the dynamic loader of an executable.
    pub interpreter: Option<String>,
    /// The libraries the dynamic section needs, `DT_NEEDED`, in its order.
    pub needed: Vec<String>,
    /// The file's own name for the dynamic loader, `DT_SONAME`.
    pub soname: Option<String>,
    /// The relocations the dynamic loader applies: `DT_RELA`, `DT_REL`, `DT_JMPREL` and the
    /// packed `DT_RELR`.
    pub relocations: Vec<Relocation>,
    /// The relocations a static link kept (`--emit-relocs`): the records of each `SHT_REL` and
    /// `SHT_RELA` section the process does not map that apply to one it does, in the file's order,
    /// their symbols indices into [`ElfFile::symbols`].
    pub static_relocations: Vec<Relocation>,
    /// The flags, the ABI flags and the register information of a MIPS file.
    pub mips: Option<Mips>,
    /// The module information and the relocations of a PSP module, [`ET_PRX`], which the reader
    /// moves to the address it loads at.
    pub prx: Option<Prx>,
    /// The header and the library tables of an IOP module, [`ET_IRX`], which the reader moves to
    /// the address it loads at.
    pub irx: Option<Irx>,
    /// The functions `.eh_frame` describes, one FDE each: `(begin, end)` in address order.
    pub functions: Vec<(u64, u64)>,
    /// What the reader could not read beyond the headers: a table that fails lands here.
    pub warnings: Vec<String>,
}

/// Whether `bytes` open on an ELF file.
pub fn is_elf(bytes: &[u8]) -> bool {
    bytes.starts_with(MAGIC)
}

/// The fixed-width reads of the file in its own byte order and width.
#[derive(Debug, Clone, Copy)]
struct Data<'a> {
    reader: Reader<'a>,
    big: bool,
    wide: bool,
}

impl<'a> Data<'a> {
    fn bytes(&self) -> &'a [u8] {
        self.reader.bytes()
    }

    fn u8(&self, offset: usize, what: &str) -> Result<u8> {
        self.reader.u8(offset, what)
    }

    fn u16(&self, offset: usize, what: &str) -> Result<u16> {
        let v = self.reader.u16(offset, what)?;
        Ok(if self.big { v.swap_bytes() } else { v })
    }

    fn u32(&self, offset: usize, what: &str) -> Result<u32> {
        let v = self.reader.u32(offset, what)?;
        Ok(if self.big { v.swap_bytes() } else { v })
    }

    fn u64(&self, offset: usize, what: &str) -> Result<u64> {
        let v = self.reader.u64(offset, what)?;
        Ok(if self.big { v.swap_bytes() } else { v })
    }

    /// A word of the file's class: eight bytes in a 64-bit file, four in a 32-bit one.
    fn word(&self, offset: usize, what: &str) -> Result<u64> {
        if self.wide { self.u64(offset, what) } else { self.u32(offset, what).map(u64::from) }
    }

    fn word_size(&self) -> usize {
        if self.wide { 8 } else { 4 }
    }

    /// The NUL-terminated string at `offset`, at most `max` bytes; a byte that is not UTF-8 becomes
    /// U+FFFD, since a name is a label.
    fn string(&self, offset: usize, max: usize, what: &str) -> Result<String> {
        let bytes = self.bytes();
        ensure!(offset <= bytes.len(), "{what} at offset {offset:#x} is past the end of the file ({:#x} bytes)", bytes.len());
        let window = &bytes[offset..bytes.len().min(offset.saturating_add(max))];
        let Some(end) = window.iter().position(|&b| b == 0) else {
            bail!("{what} at offset {offset:#x} has no terminator within {max} bytes")
        };
        Ok(String::from_utf8_lossy(&window[..end]).into_owned())
    }
}

impl ElfFile {
    /// Reads the headers and the tables of an ELF file. The headers must read; a table that does
    /// not (the symbols, the dynamic section, `.eh_frame`) leaves a warning and stays empty.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(is_elf(bytes), "the file does not start with the ELF magic");
        let reader = Reader::new(bytes);
        let class = reader.u8(4, "the ELF class (EI_CLASS)")?;
        let encoding = reader.u8(5, "the ELF data encoding (EI_DATA)")?;
        let wide = match class {
            1 => false,
            2 => true,
            other => bail!("the ELF class at offset 0x4 is {other}, neither 32 (1) nor 64 bits (2)"),
        };
        let big = match encoding {
            1 => false,
            2 => true,
            other => bail!("the ELF data encoding at offset 0x5 is {other}, neither little (1) nor big-endian (2)"),
        };
        let d = Data { reader, big, wide };
        let kind = d.u16(16, "e_type")?;
        let machine = d.u16(18, "e_machine")?;
        let (entry, phoff, shoff, rest) = if wide {
            (d.u64(24, "e_entry")?, d.u64(32, "e_phoff")?, d.u64(40, "e_shoff")?, 52)
        } else {
            (u64::from(d.u32(24, "e_entry")?), u64::from(d.u32(28, "e_phoff")?), u64::from(d.u32(32, "e_shoff")?), 40)
        };
        let flags = d.u32(rest - 4, "e_flags")?;
        let phentsize = usize::from(d.u16(rest + 2, "e_phentsize")?);
        let phnum = usize::from(d.u16(rest + 4, "e_phnum")?);
        let shentsize = usize::from(d.u16(rest + 6, "e_shentsize")?);
        let shnum = usize::from(d.u16(rest + 8, "e_shnum")?);
        let shstrndx = usize::from(d.u16(rest + 10, "e_shstrndx")?);

        let mut segments = Vec::with_capacity(phnum);
        if phnum != 0 {
            ensure!(phentsize >= if wide { 56 } else { 32 }, "e_phentsize {phentsize} is smaller than a program header");
        }
        for i in 0..phnum {
            let at = usize::try_from(phoff).unwrap_or(usize::MAX).saturating_add(i * phentsize);
            let what = |field: &str| format!("program header {i} {field}");
            segments.push(if wide {
                Segment {
                    kind: d.u32(at, &what("p_type"))?,
                    flags: d.u32(at + 4, &what("p_flags"))?,
                    offset: d.u64(at + 8, &what("p_offset"))?,
                    vaddr: d.u64(at + 16, &what("p_vaddr"))?,
                    paddr: d.u64(at + 24, &what("p_paddr"))?,
                    filesz: d.u64(at + 32, &what("p_filesz"))?,
                    memsz: d.u64(at + 40, &what("p_memsz"))?,
                    align: d.u64(at + 48, &what("p_align"))?,
                }
            } else {
                Segment {
                    kind: d.u32(at, &what("p_type"))?,
                    offset: u64::from(d.u32(at + 4, &what("p_offset"))?),
                    vaddr: u64::from(d.u32(at + 8, &what("p_vaddr"))?),
                    paddr: u64::from(d.u32(at + 12, &what("p_paddr"))?),
                    filesz: u64::from(d.u32(at + 16, &what("p_filesz"))?),
                    memsz: u64::from(d.u32(at + 20, &what("p_memsz"))?),
                    flags: d.u32(at + 24, &what("p_flags"))?,
                    align: u64::from(d.u32(at + 28, &what("p_align"))?),
                }
            });
        }
        let headers_size = phoff + (phnum * phentsize) as u64;

        let sections = if shoff == 0 { Vec::new() } else { read_sections(d, shoff, shentsize, shnum, shstrndx)? };
        let mut elf = Self {
            wide,
            big_endian: big,
            kind,
            machine,
            flags,
            entry,
            segments,
            sections,
            headers_size,
            symbols: Vec::new(),
            dynamic_symbols: Vec::new(),
            interpreter: None,
            needed: Vec::new(),
            soname: None,
            relocations: Vec::new(),
            static_relocations: Vec::new(),
            mips: None,
            prx: None,
            irx: None,
            functions: Vec::new(),
            warnings: Vec::new(),
        };
        if let Some(interp) = elf.segments.iter().find(|s| s.kind == PT_INTERP) {
            match d.string(interp.offset as usize, interp.filesz as usize, "the program interpreter") {
                Ok(path) => elf.interpreter = Some(path),
                Err(e) => elf.warnings.push(format!("interpreter: {e:#}")),
            }
        }
        if let Some(table) = elf.sections.iter().find(|s| s.kind == SHT_SYMTAB) {
            match elf.read_symbols(d, table) {
                Ok(symbols) => elf.symbols = symbols,
                Err(e) => elf.warnings.push(format!("symbol table: {e:#}")),
            }
        }
        if let Err(e) = elf.read_dynamic(d) {
            elf.warnings.push(format!("dynamic section: {e:#}"));
        }
        match elf.read_static_relocations(d) {
            Ok(relocations) => elf.static_relocations = relocations,
            Err(e) => elf.warnings.push(format!("static relocations: {e:#}")),
        }
        if machine == EM_MIPS {
            match elf.read_mips(d) {
                Ok(mut info) => {
                    match elf.mips_pairs(d) {
                        Ok(pairs) => info.pairs = pairs,
                        Err(e) => elf.warnings.push(format!("MIPS address pairs: {e:#}")),
                    }
                    elf.mips = Some(info);
                }
                Err(e) => elf.warnings.push(format!("MIPS flags: {e:#}")),
            }
        }
        match elf.read_eh_frame(d) {
            Ok(functions) => elf.functions = functions,
            Err(e) => elf.warnings.push(format!(".eh_frame: {e:#}")),
        }
        if kind == ET_PRX {
            elf.load_prx(d);
        }
        if kind == ET_IRX && machine == EM_MIPS {
            elf.load_irx(d);
        }
        Ok(elf)
    }

    /// Reads an IRX's header and library tables at its link addresses, then moves the file to
    /// [`iop::LOAD_ADDRESS`] as [`ElfFile::load_prx`] moves a PRX, its static relocations with it.
    fn load_irx(&mut self, d: Data<'_>) {
        let load = iop::LOAD_ADDRESS;
        let irx = match self.read_irx(d, load) {
            Ok(irx) => Some(irx),
            Err(e) => {
                self.warnings.push(format!("IRX module header: {e:#}"));
                None
            }
        };
        for s in self.segments.iter_mut().filter(|s| s.is_load()) {
            s.vaddr += load;
        }
        for s in self.sections.iter_mut().filter(|s| s.flags & SHF_ALLOC != 0) {
            s.addr += load;
        }
        self.entry += load;
        for s in self.symbols.iter_mut().chain(&mut self.dynamic_symbols).filter(|s| s.is_defined()) {
            s.value += load;
        }
        for (begin, end) in &mut self.functions {
            *begin += load;
            *end += load;
        }
        for r in &mut self.static_relocations {
            r.offset += load;
        }
        self.irx = irx;
    }

    /// The `.iopmod` header of an IRX (its section, else its program header) and the library
    /// tables its code holds, the addresses moved by `load`. The header gives the entry, `$gp` and
    /// the sizes; the module's name and version are the `IRX_ID` the header points at, else its
    /// own. A table is found by its magic word at a word of the text: an export table is its
    /// library's name and the address of each function in ordinal order up to a zero word; an
    /// import table its name and a stub per function, `jr $ra` over `addiu $zero, $zero, ordinal`,
    /// which the loader writes the jump to the library over.
    fn read_irx(&self, d: Data<'_>, load: u64) -> Result<Irx> {
        let at = match self.sections.iter().find(|s| s.kind == iop::SHT_IOPMOD) {
            Some(s) => s.offset as usize,
            None => self.segments.iter().find(|s| s.kind == iop::SHT_IOPMOD).context("an IRX has no .iopmod header")?.offset as usize,
        };
        let what = |field: &str| format!("the .iopmod header's {field}");
        let module = d.u32(at, &what("module"))?;
        let gp = u64::from(d.u32(at + 8, &what("gp"))?);
        let text_size = d.u32(at + 12, &what("text size"))?;
        let data_size = d.u32(at + 16, &what("data size"))?;
        let bss_size = d.u32(at + 20, &what("bss size"))?;
        let mut version = d.u16(at + 24, &what("version"))?;
        let mut name = d.string(at + 26, 256, &what("name"))?;
        // No `IRX_ID` leaves all ones.
        if module != u32::MAX {
            let id = self.offset_or(u64::from(module), "the IRX_ID")?;
            let pointer = d.u32(id, "the IRX_ID's name")?;
            name = d.string(self.offset_or(u64::from(pointer), "the IRX_ID's name")?, 256, "the IRX_ID's name")?;
            version = d.u16(id + 4, "the IRX_ID's version")?;
        }

        let text = self.segments.iter().find(|s| s.is_load()).context("an IRX has no loaded segment")?;
        let end = text.filesz.min(u64::from(text_size));
        let library = |at: usize| -> Result<(String, u16)> {
            let bytes = d.bytes().get(at + 12..at + 20).context("a library table's name lies past the end of the file")?;
            let name: String = bytes.iter().take_while(|&&b| b != 0).map(|&b| char::from(b)).collect();
            Ok((name, d.u16(at + 8, "a library table's version")?))
        };
        let valid = |name: &str| !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
        let (mut exports, mut imports) = (Vec::new(), Vec::new());
        let mut va = text.vaddr;
        while va + 20 <= text.vaddr + end {
            let at = (text.offset + (va - text.vaddr)) as usize;
            let magic = d.u32(at, "a library table's magic")?;
            if !matches!(magic, iop::EXPORT_MAGIC | iop::IMPORT_MAGIC) || d.u32(at + 4, "a library table's link")? != 0 {
                va += 4;
                continue;
            }
            let (library_name, library_version) = library(at)?;
            if !valid(&library_name) {
                va += 4;
                continue;
            }
            let mut functions = Vec::new();
            let mut entry = va + 20;
            let word = |entry: u64| d.u32((text.offset + (entry - text.vaddr)) as usize, "a library table's entry");
            if magic == iop::EXPORT_MAGIC {
                while entry + 4 <= text.vaddr + end && functions.len() < MAX_ENTRIES {
                    let address = word(entry)?;
                    entry += 4;
                    if address == 0 {
                        break;
                    }
                    let ordinal = u16::try_from(functions.len()).unwrap_or(u16::MAX);
                    functions.push(IopEntry { ordinal, address: u64::from(address) + load });
                }
                exports.push(IopLibrary { name: library_name, version: library_version, functions, table: (va + load, entry + load) });
            } else {
                while entry + 8 <= text.vaddr + end && functions.len() < MAX_ENTRIES {
                    let (jump, ordinal) = (word(entry)?, word(entry + 4)?);
                    if jump != 0x03e0_0008 || ordinal >> 16 != 0x2400 {
                        break;
                    }
                    functions.push(IopEntry { ordinal: ordinal as u16, address: entry + load });
                    entry += 8;
                }
                imports.push(IopLibrary { name: library_name, version: library_version, functions, table: (va + load, va + 20 + load) });
            }
            va = entry.next_multiple_of(4);
        }

        // Each place is the module's own: its value is relative to the base, and a high half
        // pairs with the first low half after it.
        let mut relocations: Vec<PrxRelocation> = self
            .static_relocations
            .iter()
            .filter(|r| r.symbol == 0)
            .map(|r| PrxRelocation { place: r.offset + load, kind: r.kind, base: load, low: None })
            .collect();
        let mut next_low = None;
        for r in relocations.iter_mut().rev() {
            match r.kind {
                reloc::MIPS_LO16 => next_low = Some(r.place),
                reloc::MIPS_HI16 => r.low = next_low,
                _ => {}
            }
        }
        let gp = if gp == 0 { 0 } else { gp + load };
        Ok(Irx { load, name, version, gp, text_size, data_size, bss_size, exports, imports, relocations })
    }

    /// Reads a PRX's relocations and module information at its link addresses, then moves the
    /// file to [`psp::LOAD_ADDRESS`]: the loaded segments, the allocated sections, the entry, the
    /// defined symbols and the `.eh_frame` extents. A PRX keeps no ELF relocations or dynamic
    /// section to move.
    fn load_prx(&mut self, d: Data<'_>) {
        let load = psp::LOAD_ADDRESS;
        let relocations = self.read_prx_relocations(d, load).unwrap_or_else(|e| {
            self.warnings.push(format!("PRX relocations: {e:#}"));
            Vec::new()
        });
        let mut warnings = Vec::new();
        let prx = match self.read_module_info(d, load, &mut warnings) {
            Ok(mut prx) => {
                prx.relocations = relocations;
                Some(prx)
            }
            Err(e) => {
                warnings.push(format!("PRX module information: {e:#}"));
                None
            }
        };
        self.warnings.extend(warnings);
        for s in self.segments.iter_mut().filter(|s| s.is_load()) {
            s.vaddr += load;
        }
        for s in self.sections.iter_mut().filter(|s| s.flags & SHF_ALLOC != 0) {
            s.addr += load;
        }
        self.entry += load;
        for s in self.symbols.iter_mut().chain(&mut self.dynamic_symbols).filter(|s| s.is_defined()) {
            s.value += load;
        }
        for (begin, end) in &mut self.functions {
            *begin += load;
            *end += load;
        }
        self.prx = prx;
    }

    /// The records of a PRX's relocation tables, the sections of [`psp::SHT_PRXRELOC`] or else the
    /// program headers of [`psp::PT_PRXRELOC`]. A record's `r_info` holds the kind in its low byte,
    /// the program header its offset is from in the next and the one its value is relative to in
    /// the third.
    fn read_prx_relocations(&self, d: Data<'_>, load: u64) -> Result<Vec<PrxRelocation>> {
        if let Some(i) = self.segments.iter().position(|s| s.kind == psp::PT_PRXRELOC2) {
            bail!("program header {i} holds the packed relocations of type {:#x}, which the reader does not decode", psp::PT_PRXRELOC2);
        }
        let mut tables: Vec<(u64, u64, String)> =
            self.sections.iter().filter(|s| s.kind == psp::SHT_PRXRELOC).map(|s| (s.offset, s.size, s.name.clone())).collect();
        if tables.is_empty() {
            tables = self
                .segments
                .iter()
                .enumerate()
                .filter(|(_, s)| s.kind == psp::PT_PRXRELOC)
                .map(|(i, s)| (s.offset, s.filesz, format!("program header {i}")))
                .collect();
        }
        let segment = |index: u32, what: &dyn Fn() -> String| -> Result<u64> {
            match self.segments.get(index as usize) {
                Some(s) => Ok(load + s.vaddr),
                None => bail!("{} names program header {index}, past the {} the file has", what(), self.segments.len()),
            }
        };
        let mut relocations = Vec::new();
        for (offset, size, name) in &tables {
            for i in 0..(*size as usize / 8).min(MAX_ENTRIES) {
                let at = *offset as usize + 8 * i;
                let what = |field: &str| format!("{name} record {i} {field}");
                let r_offset = d.u32(at, &what("r_offset"))?;
                let info = d.u32(at + 4, &what("r_info"))?;
                let place = segment((info >> 8) & 0xff, &|| what("offset base"))? + u64::from(r_offset);
                let base = segment((info >> 16) & 0xff, &|| what("address base"))?;
                relocations.push(PrxRelocation { place, kind: info & 0xff, base, low: None });
            }
        }
        // Each high half pairs with the first low half after it of the same base.
        let mut next_low: HashMap<u64, u64> = HashMap::new();
        for r in relocations.iter_mut().rev() {
            match r.kind {
                reloc::MIPS_LO16 => {
                    next_low.insert(r.base, r.place);
                }
                reloc::MIPS_HI16 => r.low = next_low.get(&r.base).copied(),
                _ => {}
            }
        }
        Ok(relocations)
    }

    /// The module information of a PRX: its section, else the file offset the first program
    /// header's `p_paddr` gives. Its tables are read at the link addresses, which the segments
    /// still hold, and their addresses moved by `load`.
    fn read_module_info(&self, d: Data<'_>, load: u64, warnings: &mut Vec<String>) -> Result<Prx> {
        let at = match self.section(".rodata.sceModuleInfo") {
            Some(s) => s.offset as usize,
            None => (self.segments.first().context("a PRX has no program header")?.paddr & 0x7fff_ffff) as usize,
        };
        let what = |field: &str| format!("the module information's {field}");
        let attributes = d.u16(at, &what("attribute"))?;
        let minor = d.u8(at + 2, &what("minor version"))?;
        let major = d.u8(at + 3, &what("major version"))?;
        let name = d.string(at + 4, 28, &what("name"))?;
        let word = |offset: usize, field: &str| -> Result<u64> { Ok(u64::from(d.u32(at + offset, &what(field))?)) };
        let gp = word(32, "gp")?;
        let (ent_top, ent_end, stub_top, stub_end) =
            (word(36, "ent_top")?, word(40, "ent_end")?, word(44, "stub_top")?, word(48, "stub_end")?);
        let library_name = |va: u64, what: &str| -> Result<String> { d.string(self.offset_or(va, what)?, 256, what) };

        let mut exports = Vec::new();
        let mut va = ent_top;
        while va < ent_end && exports.len() < MAX_ENTRIES {
            let at = self.offset_or(va, "an export entry")?;
            let what = |field: &str| format!("the export entry at {va:#x} {field}");
            let name = match d.u32(at, &what("name"))? {
                0 => "syslib".to_owned(),
                pointer => library_name(u64::from(pointer), &what("name"))?,
            };
            let size = usize::from(d.u8(at + 8, &what("size"))?);
            ensure!(size >= 4, "{} is {size} words, smaller than an entry", what("size"));
            let variables = usize::from(d.u8(at + 9, &what("variable count"))?);
            let functions = usize::from(d.u16(at + 10, &what("function count"))?);
            let table = self.offset_or(u64::from(d.u32(at + 12, &what("table"))?), &what("table"))?;
            let total = functions + variables;
            let mut entries = Vec::with_capacity(total);
            for i in 0..total {
                let nid = d.u32(table + 4 * i, &what(&format!("NID {i}")))?;
                let address = u64::from(d.u32(table + 4 * (total + i), &what(&format!("address {i}")))?) + load;
                entries.push(PspEntry { nid, address });
            }
            let variables = entries.split_off(functions);
            exports.push(PspLibrary {
                name,
                version: d.u16(at + 4, &what("version"))?,
                attributes: d.u16(at + 6, &what("attribute"))?,
                functions: entries,
                variables,
            });
            va += 4 * size as u64;
        }

        let mut imports = Vec::new();
        let mut va = stub_top;
        while va < stub_end && imports.len() < MAX_ENTRIES {
            let at = self.offset_or(va, "an import entry")?;
            let what = |field: &str| format!("the import entry at {va:#x} {field}");
            let name = library_name(u64::from(d.u32(at, &what("name"))?), &what("name"))?;
            let size = usize::from(d.u8(at + 8, &what("size"))?);
            ensure!(size >= 5, "{} is {size} words, smaller than an entry", what("size"));
            let variables = d.u8(at + 9, &what("variable count"))?;
            if variables != 0 {
                warnings.push(format!("the {variables} variables {name} imports are not read"));
            }
            let count = usize::from(d.u16(at + 10, &what("function count"))?);
            let nids = self.offset_or(u64::from(d.u32(at + 12, &what("NID table"))?), &what("NID table"))?;
            let stubs = u64::from(d.u32(at + 16, &what("stub table"))?) + load;
            let mut functions = Vec::with_capacity(count);
            for i in 0..count {
                // Each stub is two words the loader writes the jump to the import over.
                functions.push(PspEntry { nid: d.u32(nids + 4 * i, &what(&format!("NID {i}")))?, address: stubs + 8 * i as u64 });
            }
            imports.push(PspLibrary {
                name,
                version: d.u16(at + 4, &what("version"))?,
                attributes: d.u16(at + 6, &what("attribute"))?,
                functions,
                variables: Vec::new(),
            });
            va += 4 * size as u64;
        }
        // A module of no small data has no `$gp`, and its zero is not an address.
        let gp = if gp == 0 { 0 } else { gp + load };
        Ok(Prx { load, name, attributes, version: (major, minor), gp, exports, imports, relocations: Vec::new() })
    }

    /// The file offset of virtual address `va`, when a loaded segment holds it in the file's bytes.
    pub fn offset_of(&self, va: u64) -> Option<u64> {
        self.segments.iter().find(|s| s.is_load() && va >= s.vaddr && va - s.vaddr < s.filesz).map(|s| s.offset + (va - s.vaddr))
    }

    /// The section called `name`.
    pub fn section(&self, name: &str) -> Option<&SectionHeader> {
        self.sections.iter().find(|s| s.name == name)
    }

    fn offset_or(&self, va: u64, what: &str) -> Result<usize> {
        match self.offset_of(va) {
            Some(offset) => Ok(offset as usize),
            None => bail!("{what} at {va:#x} lies in no loaded segment's file bytes"),
        }
    }

    fn read_symbols(&self, d: Data<'_>, table: &SectionHeader) -> Result<Vec<Symbol>> {
        let strings = self.sections.get(table.link as usize).context("the symbol table's string table index")?;
        let entsize = if table.entsize == 0 { if self.wide { 24 } else { 16 } } else { table.entsize as usize };
        let count = (table.size as usize / entsize).min(MAX_ENTRIES);
        let mut symbols = Vec::with_capacity(count);
        for i in 0..count {
            let at = table.offset as usize + i * entsize;
            symbols.push(read_symbol(d, at, |name| d.string(strings.offset as usize + name as usize, 4096, "a symbol name"))?);
        }
        Ok(symbols)
    }

    /// The dynamic section: its strings, the libraries, the dynamic symbols and their versions,
    /// the relocations.
    fn read_dynamic(&mut self, d: Data<'_>) -> Result<()> {
        let Some(dynamic) = self.segments.iter().find(|s| s.kind == PT_DYNAMIC).copied() else { return Ok(()) };
        let entry = 2 * d.word_size();
        let mut tags: Vec<(u64, u64)> = Vec::new();
        for i in 0..(dynamic.filesz as usize / entry).min(MAX_ENTRIES) {
            let at = dynamic.offset as usize + i * entry;
            let tag = d.word(at, &format!("dynamic entry {i} d_tag"))?;
            if tag == DT_NULL {
                break;
            }
            tags.push((tag, d.word(at + d.word_size(), &format!("dynamic entry {i} d_val"))?));
        }
        let value = |tag: u64| tags.iter().find(|&&(t, _)| t == tag).map(|&(_, v)| v);
        let Some(strtab) = value(DT_STRTAB) else { return Ok(()) };
        let strtab = self.offset_or(strtab, "DT_STRTAB")?;
        let strsz = value(DT_STRSZ).unwrap_or(0) as usize;
        let string = |offset: u64, what: &str| -> Result<String> {
            let max = strsz.saturating_sub(offset as usize).max(1).min(4096);
            d.string(strtab + offset as usize, max, what)
        };
        for &(tag, v) in &tags {
            match tag {
                DT_NEEDED => self.needed.push(string(v, "a DT_NEEDED name")?),
                DT_SONAME => self.soname = Some(string(v, "the DT_SONAME")?),
                _ => {}
            }
        }

        // The relocations first: when nothing else counts the dynamic symbols, the highest index
        // a relocation names bounds them.
        let rela_size = if self.wide { 24 } else { 12 };
        let rel_size = if self.wide { 16 } else { 8 };
        let mut tables = Vec::new();
        if let (Some(at), Some(size)) = (value(DT_RELA), value(DT_RELASZ)) {
            tables.push((at, size, true, "DT_RELA"));
        }
        if let (Some(at), Some(size)) = (value(DT_REL), value(DT_RELSZ)) {
            tables.push((at, size, false, "DT_REL"));
        }
        if let (Some(at), Some(size)) = (value(DT_JMPREL), value(DT_PLTRELSZ)) {
            tables.push((at, size, value(DT_PLTREL) == Some(DT_RELA), "DT_JMPREL"));
        }
        for (va, size, rela, what) in tables {
            let start = self.offset_or(va, what)?;
            let entsize = if rela { rela_size } else { rel_size };
            for i in 0..(size as usize / entsize).min(MAX_ENTRIES) {
                let at = start + i * entsize;
                let offset = d.word(at, &format!("{what} record {i} r_offset"))?;
                let info = d.word(at + d.word_size(), &format!("{what} record {i} r_info"))?;
                let (symbol, kind) =
                    if self.wide { ((info >> 32) as u32, (info & 0xffff_ffff) as u32) } else { ((info >> 8) as u32, (info & 0xff) as u32) };
                let addend = match (rela, self.wide) {
                    (false, _) => 0,
                    (true, true) => d.u64(at + 16, &format!("{what} record {i} r_addend"))? as i64,
                    (true, false) => i64::from(d.u32(at + 8, &format!("{what} record {i} r_addend"))? as i32),
                };
                self.relocations.push(Relocation { offset, kind, symbol, addend });
            }
        }
        // The packed relative relocations of `DT_RELR`, which Android's and today's linkers write:
        // each word they name is a `RELATIVE` record whose addend is in place.
        if let (Some(va), Some(size), Some(kind)) = (value(DT_RELR), value(DT_RELRSZ), reloc::relative(self.machine)) {
            let start = self.offset_or(va, "DT_RELR")?;
            let width = d.word_size();
            let mut entries = Vec::new();
            for i in 0..(size as usize / width).min(MAX_ENTRIES) {
                entries.push(d.word(start + i * width, &format!("DT_RELR entry {i}"))?);
            }
            for offset in unpack_relr(&entries, width as u64) {
                self.relocations.push(Relocation { offset, kind, symbol: 0, addend: 0 });
            }
        }

        // The dynamic symbols: as many as `.dynsym` holds, else as the hash table's chain counts,
        // else as the relocations name.
        let Some(symtab) = value(DT_SYMTAB) else { return Ok(()) };
        let symtab = self.offset_or(symtab, "DT_SYMTAB")?;
        let syment = value(DT_SYMENT).map_or(if self.wide { 24 } else { 16 }, |v| v as usize);
        ensure!(syment >= if self.wide { 24 } else { 16 }, "DT_SYMENT {syment} is smaller than a symbol");
        let count = if let Some(table) = self.sections.iter().find(|s| s.kind == SHT_DYNSYM) {
            table.size as usize / syment
        } else if let Some(hash) = value(DT_HASH) {
            d.u32(self.offset_or(hash, "DT_HASH")? + 4, "the DT_HASH chain count")? as usize
        } else {
            self.relocations.iter().map(|r| r.symbol as usize + 1).max().unwrap_or(0)
        };
        let mut symbols = Vec::with_capacity(count.min(MAX_ENTRIES));
        for i in 0..count.min(MAX_ENTRIES) {
            symbols.push(read_symbol(d, symtab + i * syment, |name| string(u64::from(name), "a dynamic symbol name"))?);
        }

        // The versions: each dynamic symbol's index into the versions the file needs, and each of
        // those the library that defines it.
        if let (Some(versym), Some(verneed), Some(count)) = (value(DT_VERSYM), value(DT_VERNEED), value(DT_VERNEEDNUM)) {
            let mut libraries: HashMap<u16, String> = HashMap::new();
            let mut at = self.offset_or(verneed, "DT_VERNEED")?;
            for i in 0..(count as usize).min(MAX_ENTRIES) {
                let what = |field: &str| format!("version need {i} {field}");
                let entries = d.u16(at + 2, &what("vn_cnt"))?;
                let file = string(u64::from(d.u32(at + 4, &what("vn_file"))?), &what("file name"))?;
                let mut aux = at + d.u32(at + 8, &what("vn_aux"))? as usize;
                for _ in 0..entries {
                    let index = d.u16(aux + 6, &what("vna_other"))?;
                    libraries.insert(index, file.clone());
                    let next = d.u32(aux + 12, &what("vna_next"))? as usize;
                    if next == 0 {
                        break;
                    }
                    aux += next;
                }
                let next = d.u32(at + 12, &what("vn_next"))? as usize;
                if next == 0 {
                    break;
                }
                at += next;
            }
            let versym = self.offset_or(versym, "DT_VERSYM")?;
            for (i, symbol) in symbols.iter_mut().enumerate() {
                let version = d.u16(versym + 2 * i, "a DT_VERSYM entry")? & 0x7fff;
                symbol.library = libraries.get(&version).cloned();
            }
        }
        self.dynamic_symbols = symbols;
        Ok(())
    }

    /// The records of the relocation sections a static link kept for the sections the process
    /// maps; the dynamic tables are mapped themselves and are [`ElfFile::read_dynamic`]'s.
    fn read_static_relocations(&self, d: Data<'_>) -> Result<Vec<Relocation>> {
        let symtab = self.sections.iter().position(|s| s.kind == SHT_SYMTAB);
        // An IOP module is linked at zero, its `.text` there too and mapped all the same.
        let applies = |s: &SectionHeader| s.is_mapped() || (self.kind == ET_IRX && s.flags & SHF_ALLOC != 0 && s.size != 0);
        let mut relocations = Vec::new();
        for table in &self.sections {
            if !matches!(table.kind, SHT_REL | SHT_RELA)
                || table.flags & SHF_ALLOC != 0
                || Some(table.link as usize) != symtab
                || !self.sections.get(table.info as usize).is_some_and(applies)
            {
                continue;
            }
            let rela = table.kind == SHT_RELA;
            let size = match (rela, self.wide) {
                (true, true) => 24,
                (true, false) => 12,
                (false, true) => 16,
                (false, false) => 8,
            };
            let entsize = if table.entsize == 0 { size } else { table.entsize as usize };
            ensure!(entsize >= size, "{} has records of {entsize} bytes, smaller than one", table.name);
            for i in 0..(table.size as usize / entsize).min(MAX_ENTRIES) {
                let at = table.offset as usize + i * entsize;
                let what = |field: &str| format!("{} record {i} {field}", table.name);
                let offset = d.word(at, &what("r_offset"))?;
                let info = d.word(at + d.word_size(), &what("r_info"))?;
                let (symbol, kind) =
                    if self.wide { ((info >> 32) as u32, (info & 0xffff_ffff) as u32) } else { ((info >> 8) as u32, (info & 0xff) as u32) };
                let addend = match (rela, self.wide) {
                    (false, _) => 0,
                    (true, true) => d.u64(at + 16, &what("r_addend"))? as i64,
                    (true, false) => i64::from(d.u32(at + 8, &what("r_addend"))? as i32),
                };
                relocations.push(Relocation { offset, kind, symbol, addend });
            }
        }
        Ok(relocations)
    }

    /// A MIPS file's ABI from its header's flags, and its ABI flags and register information from
    /// their sections, else from the segments that map them.
    fn read_mips(&self, d: Data<'_>) -> Result<Mips> {
        let flags = self.flags;
        let abi = if flags & mips::EF_MIPS_ABI2 != 0 {
            MipsAbi::N32
        } else {
            match flags & mips::EF_MIPS_ABI {
                mips::EF_MIPS_ABI_O32 => MipsAbi::O32,
                mips::EF_MIPS_ABI_O64 => MipsAbi::O64,
                mips::EF_MIPS_ABI_EABI32 => MipsAbi::Eabi32,
                mips::EF_MIPS_ABI_EABI64 => MipsAbi::Eabi64,
                // No ABI named: what the GNU tools assume of the class.
                _ if self.wide => MipsAbi::N64,
                _ => MipsAbi::O32,
            }
        };
        let table = |section: u32, segment: u32, size: u64| -> Option<usize> {
            let in_section = self.sections.iter().find(|s| s.kind == section && s.size >= size).map(|s| s.offset);
            let in_segment = || self.segments.iter().find(|s| s.kind == segment && s.filesz >= size).map(|s| s.offset);
            in_section.or_else(in_segment).map(|offset| offset as usize)
        };

        let abiflags = match table(mips::SHT_MIPS_ABIFLAGS, mips::PT_MIPS_ABIFLAGS, mips::ABIFLAGS_SIZE) {
            None => None,
            Some(at) => {
                let bits = |offset: usize, what: &str| -> Result<u16> {
                    match d.u8(at + offset, what)? {
                        0 => Ok(0),
                        1 => Ok(32),
                        2 => Ok(64),
                        3 => Ok(128),
                        other => bail!("{what} at offset {:#x} is {other}, not a register size (0 to 3)", at + offset),
                    }
                };
                Some(MipsAbiFlags {
                    version: d.u16(at, "the ABI flags' version")?,
                    isa_level: d.u8(at + 2, "the ABI flags' isa_level")?,
                    isa_rev: d.u8(at + 3, "the ABI flags' isa_rev")?,
                    gpr_bits: bits(4, "the ABI flags' gpr_size")?,
                    cpr1_bits: bits(5, "the ABI flags' cpr1_size")?,
                    cpr2_bits: bits(6, "the ABI flags' cpr2_size")?,
                    fp_abi: d.u8(at + 7, "the ABI flags' fp_abi")?,
                    isa_ext: d.u32(at + 8, "the ABI flags' isa_ext")?,
                    ases: d.u32(at + 12, "the ABI flags' ases")?,
                    flags1: d.u32(at + 16, "the ABI flags' flags1")?,
                    flags2: d.u32(at + 20, "the ABI flags' flags2")?,
                })
            }
        };

        let reginfo = match table(mips::SHT_MIPS_REGINFO, mips::PT_MIPS_REGINFO, mips::REGINFO_SIZE) {
            Some(at) if !self.wide => {
                let mut cpr_mask = [0; 4];
                for (i, mask) in cpr_mask.iter_mut().enumerate() {
                    *mask = d.u32(at + 4 + 4 * i, &format!("the register information's ri_cprmask[{i}]"))?;
                }
                Some(MipsRegInfo {
                    gpr_mask: d.u32(at, "the register information's ri_gprmask")?,
                    cpr_mask,
                    gp: d.u32(at + 20, "the register information's ri_gp_value")?,
                })
            }
            _ => None,
        };
        Ok(Mips { flags, abi, abiflags, reginfo, pairs: Vec::new() })
    }

    /// The addresses the `R_MIPS_HI16` and `R_MIPS_LO16` records of the static relocations build,
    /// in the order of their low halves: every high half since the last low half of its symbol
    /// pairs with the next one, as the ABI orders the records.
    fn mips_pairs(&self, d: Data<'_>) -> Result<Vec<MipsPair>> {
        let immediate = |va: u64, what: &str| -> Result<u32> { Ok(d.u32(self.offset_or(va, what)?, what)? & 0xffff) };
        let mut pending: HashMap<u32, Vec<u64>> = HashMap::new();
        let mut pairs = Vec::new();
        for r in &self.static_relocations {
            match r.kind {
                reloc::MIPS_HI16 => pending.entry(r.symbol).or_default().push(r.offset),
                reloc::MIPS_LO16 => {
                    let Some(highs) = pending.remove(&r.symbol) else { continue };
                    let low = i32::from(immediate(r.offset, "the instruction of an R_MIPS_LO16")? as u16 as i16);
                    for high in highs {
                        let target = (immediate(high, "the instruction of an R_MIPS_HI16")? << 16).wrapping_add_signed(low);
                        pairs.push(MipsPair { high, low: r.offset, target: u64::from(target) });
                    }
                }
                _ => {}
            }
        }
        Ok(pairs)
    }

    /// The function extents of `.eh_frame`: the section when the file names it, else the table
    /// `PT_GNU_EH_FRAME`'s header points at.
    fn read_eh_frame(&self, d: Data<'_>) -> Result<Vec<(u64, u64)>> {
        let (va, start, end) = if let Some(s) = self.section(".eh_frame").filter(|s| s.kind == SHT_PROGBITS && s.addr != 0) {
            (s.addr, s.offset as usize, (s.offset + s.size) as usize)
        } else if let Some(hdr) = self.segments.iter().find(|s| s.kind == PT_GNU_EH_FRAME) {
            let at = hdr.offset as usize;
            ensure!(d.u8(at, "the .eh_frame_hdr version")? == 1, "the .eh_frame_hdr at offset {at:#x} is not version 1");
            let encoding = d.u8(at + 1, "the .eh_frame_hdr eh_frame_ptr encoding")?;
            let mut cursor = at + 4;
            let Some(va) = read_encoded(d, &mut cursor, encoding, hdr.vaddr + 4, 0)? else { return Ok(Vec::new()) };
            let start = self.offset_or(va, ".eh_frame")?;
            let segment = self.segments.iter().find(|s| s.is_load() && va >= s.vaddr && va - s.vaddr < s.filesz).unwrap();
            (va, start, (segment.offset + segment.filesz) as usize)
        } else {
            return Ok(Vec::new());
        };
        ensure!(end <= d.bytes().len(), ".eh_frame at offset {start:#x} runs past the end of the file");
        let va_of = |offset: usize| va + (offset - start) as u64;

        // The FDE pointer encoding of each CIE, by the CIE's offset.
        let mut cies: HashMap<usize, u8> = HashMap::new();
        let mut functions = Vec::new();
        let mut at = start;
        while at + 4 <= end {
            let mut length = u64::from(d.u32(at, "an .eh_frame record length")?);
            let mut body = at + 4;
            if length == 0 {
                break;
            }
            if length == 0xffff_ffff {
                length = d.u64(body, "an .eh_frame record's 64-bit length")?;
                body += 8;
            }
            let next = body.checked_add(length as usize).filter(|&n| n <= end);
            let Some(next) = next else { bail!("the .eh_frame record at offset {at:#x} runs past the section's end") };
            let id = d.u32(body, "an .eh_frame record's CIE id")?;
            if id == 0 {
                cies.insert(at, cie_encoding(d, body + 4, next)?);
            } else {
                let cie = body.checked_sub(id as usize).with_context(|| format!("the FDE at offset {at:#x} points before the file"))?;
                let encoding = match cies.get(&cie) {
                    Some(&e) => e,
                    None => {
                        // A CIE after its FDE, or one the walk stepped over: its id is past its length.
                        let e = cie_encoding(d, cie + 8, end)?;
                        cies.insert(cie, e);
                        e
                    }
                };
                let mut cursor = body + 4;
                let pc = va_of(cursor);
                let begin = read_encoded(d, &mut cursor, encoding, pc, 0)?;
                // The range has the format of the encoding and none of its base.
                let range = read_encoded(d, &mut cursor, encoding & 0x0f, 0, 0)?;
                if let (Some(begin), Some(range)) = (begin, range)
                    && begin != 0
                    && range != 0
                {
                    functions.push((begin, begin + range));
                }
            }
            at = next;
        }
        functions.sort_unstable();
        functions.dedup();
        Ok(functions)
    }
}

/// The section headers, each with its name from the section name table.
fn read_sections(d: Data<'_>, shoff: u64, shentsize: usize, shnum: usize, shstrndx: usize) -> Result<Vec<SectionHeader>> {
    ensure!(shentsize >= if d.wide { 64 } else { 40 }, "e_shentsize {shentsize} is smaller than a section header");
    let read = |i: usize| -> Result<(u32, SectionHeader)> {
        let at = usize::try_from(shoff).unwrap_or(usize::MAX).saturating_add(i * shentsize);
        let what = |field: &str| format!("section header {i} {field}");
        let name = d.u32(at, &what("sh_name"))?;
        let header = if d.wide {
            SectionHeader {
                name: String::new(),
                kind: d.u32(at + 4, &what("sh_type"))?,
                flags: d.u64(at + 8, &what("sh_flags"))?,
                addr: d.u64(at + 16, &what("sh_addr"))?,
                offset: d.u64(at + 24, &what("sh_offset"))?,
                size: d.u64(at + 32, &what("sh_size"))?,
                link: d.u32(at + 40, &what("sh_link"))?,
                info: d.u32(at + 44, &what("sh_info"))?,
                entsize: d.u64(at + 56, &what("sh_entsize"))?,
            }
        } else {
            SectionHeader {
                name: String::new(),
                kind: d.u32(at + 4, &what("sh_type"))?,
                flags: u64::from(d.u32(at + 8, &what("sh_flags"))?),
                addr: u64::from(d.u32(at + 12, &what("sh_addr"))?),
                offset: u64::from(d.u32(at + 16, &what("sh_offset"))?),
                size: u64::from(d.u32(at + 20, &what("sh_size"))?),
                link: d.u32(at + 24, &what("sh_link"))?,
                info: d.u32(at + 28, &what("sh_info"))?,
                entsize: u64::from(d.u32(at + 36, &what("sh_entsize"))?),
            }
        };
        Ok((name, header))
    };
    // A file with more sections than `e_shnum` holds keeps the count in section 0's size, and the
    // name table's index in its link.
    let first = read(0)?;
    let count = if shnum == 0 { first.1.size as usize } else { shnum };
    let names = if shstrndx == 0xffff { first.1.link as usize } else { shstrndx };
    ensure!(count <= MAX_ENTRIES, "the ELF header names {count} sections");
    let mut headers = vec![first];
    for i in 1..count {
        headers.push(read(i)?);
    }
    let table = headers.get(names).map(|(_, s)| (s.offset as usize, s.size as usize));
    let mut sections = Vec::with_capacity(headers.len());
    for (index, mut s) in headers {
        let index = index as usize;
        if let Some((offset, size)) = table
            && index < size
        {
            s.name = d.string(offset + index, size - index, "a section name")?;
        }
        sections.push(s);
    }
    Ok(sections)
}

/// One symbol record at `at`, with its name from `name_of`.
fn read_symbol(d: Data<'_>, at: usize, name_of: impl Fn(u32) -> Result<String>) -> Result<Symbol> {
    let name = d.u32(at, "st_name")?;
    let (info, other, section, value, size) = if d.wide {
        (
            d.u8(at + 4, "st_info")?,
            d.u8(at + 5, "st_other")?,
            d.u16(at + 6, "st_shndx")?,
            d.u64(at + 8, "st_value")?,
            d.u64(at + 16, "st_size")?,
        )
    } else {
        (
            d.u8(at + 12, "st_info")?,
            d.u8(at + 13, "st_other")?,
            d.u16(at + 14, "st_shndx")?,
            u64::from(d.u32(at + 4, "st_value")?),
            u64::from(d.u32(at + 8, "st_size")?),
        )
    };
    let name = if name == 0 { String::new() } else { name_of(name)? };
    Ok(Symbol { name, value, size, kind: info & 0xf, binding: info >> 4, visibility: other & 3, section, library: None })
}

/// The pointer encoding of the FDEs of the CIE whose body (past its id) starts at `at`: the `R`
/// of its augmentation, or an absolute pointer without one.
fn cie_encoding(d: Data<'_>, at: usize, end: usize) -> Result<u8> {
    let version = d.u8(at, "a CIE version")?;
    let augmentation = d.string(at + 1, end.saturating_sub(at + 1).max(1), "a CIE augmentation")?;
    let mut cursor = at + 1 + augmentation.len() + 1;
    if augmentation.contains("eh") {
        cursor += d.word_size();
    }
    leb128(d, &mut cursor, false)?; // code alignment
    leb128(d, &mut cursor, true)?; // data alignment
    if version == 1 {
        cursor += 1;
    } else {
        leb128(d, &mut cursor, false)?; // return address register
    }
    if !augmentation.starts_with('z') {
        return Ok(0);
    }
    leb128(d, &mut cursor, false)?; // augmentation data length
    for c in augmentation.chars().skip(1) {
        match c {
            'R' => return d.u8(cursor, "a CIE's FDE pointer encoding"),
            'L' => cursor += 1,
            'P' => {
                let encoding = d.u8(cursor, "a CIE's personality encoding")?;
                cursor += 1;
                // Only its length matters here: the base is not needed to skip it.
                read_encoded(d, &mut cursor, encoding & 0x0f, 0, 0)?;
            }
            'S' | 'B' | 'G' => {}
            other => bail!("a CIE's augmentation {augmentation:?} has the unknown letter {other:?}"),
        }
    }
    Ok(0)
}

/// A pointer in the DWARF exception encoding `encoding` at `cursor`, which advances past it:
/// `pc` is the address of the field, the base of a `pcrel` one, and `data` the base of a `datarel`
/// one. `None` for the omitted encoding.
fn read_encoded(d: Data<'_>, cursor: &mut usize, encoding: u8, pc: u64, data: u64) -> Result<Option<u64>> {
    if encoding == 0xff {
        return Ok(None);
    }
    let at = *cursor;
    let what = "an .eh_frame pointer";
    let value = match encoding & 0x0f {
        0x00 => {
            *cursor += d.word_size();
            d.word(at, what)?
        }
        0x01 => leb128(d, cursor, false)?,
        0x09 => leb128(d, cursor, true)?,
        0x02 => {
            *cursor += 2;
            u64::from(d.u16(at, what)?)
        }
        0x0a => {
            *cursor += 2;
            d.u16(at, what)? as i16 as i64 as u64
        }
        0x03 => {
            *cursor += 4;
            u64::from(d.u32(at, what)?)
        }
        0x0b => {
            *cursor += 4;
            i64::from(d.u32(at, what)? as i32) as u64
        }
        0x04 | 0x0c => {
            *cursor += 8;
            d.u64(at, what)?
        }
        other => bail!("the pointer encoding {encoding:#x} at offset {at:#x} has the unknown format {other:#x}"),
    };
    let base = match encoding & 0x70 {
        0x00 => 0,
        0x10 => pc,
        0x30 => data,
        other => bail!("the pointer encoding {encoding:#x} at offset {at:#x} has the unsupported application {other:#x}"),
    };
    ensure!(encoding & 0x80 == 0, "the pointer at offset {at:#x} is indirect ({encoding:#x}), which an extent never is");
    let value = base.wrapping_add(value);
    Ok(Some(if d.wide { value } else { value & 0xffff_ffff }))
}

/// An unsigned or a signed LEB128 number at `cursor`, which advances past it.
fn leb128(d: Data<'_>, cursor: &mut usize, signed: bool) -> Result<u64> {
    let mut value = 0u64;
    let mut shift = 0u32;
    loop {
        let byte = d.u8(*cursor, "a LEB128 number")?;
        *cursor += 1;
        if shift < 64 {
            value |= u64::from(byte & 0x7f) << shift;
        }
        shift += 7;
        if byte & 0x80 == 0 {
            if signed && shift < 64 && byte & 0x40 != 0 {
                value |= u64::MAX << shift;
            }
            return Ok(value);
        }
        ensure!(shift < 70, "the LEB128 number at offset {:#x} is longer than ten bytes", *cursor);
    }
}

/// The addresses a `DT_RELR` table relocates, of words `width` bytes wide: an even entry is an
/// address, and an odd one a bitmap of the words after the last, bit 1 the first of them, each
/// bitmap covering one word fewer than its bits.
fn unpack_relr(entries: &[u64], width: u64) -> Vec<u64> {
    let bits = width * 8;
    let mut addresses = Vec::new();
    let mut next = 0u64;
    for &entry in entries {
        if entry & 1 == 0 {
            addresses.push(entry);
            next = entry.wrapping_add(width);
            continue;
        }
        for bit in 1..bits {
            if (entry >> bit) & 1 != 0 {
                addresses.push(next.wrapping_add((bit - 1) * width));
            }
        }
        next = next.wrapping_add((bits - 1) * width);
    }
    addresses
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 64-bit little-endian executable with one loaded segment and nothing else.
    fn minimal(big: bool) -> Vec<u8> {
        let mut b = vec![0u8; 0x78];
        b[..4].copy_from_slice(MAGIC);
        b[4] = 2;
        b[5] = if big { 2 } else { 1 };
        b[6] = 1;
        let put16 =
            |b: &mut Vec<u8>, at: usize, v: u16| b[at..at + 2].copy_from_slice(&if big { v.to_be_bytes() } else { v.to_le_bytes() });
        let put32 =
            |b: &mut Vec<u8>, at: usize, v: u32| b[at..at + 4].copy_from_slice(&if big { v.to_be_bytes() } else { v.to_le_bytes() });
        let put64 =
            |b: &mut Vec<u8>, at: usize, v: u64| b[at..at + 8].copy_from_slice(&if big { v.to_be_bytes() } else { v.to_le_bytes() });
        put16(&mut b, 16, ET_EXEC);
        put16(&mut b, 18, EM_X86_64);
        put64(&mut b, 24, 0x40_0078);
        put64(&mut b, 32, 0x40);
        put16(&mut b, 54, 56);
        put16(&mut b, 56, 1);
        put32(&mut b, 0x40, PT_LOAD);
        put32(&mut b, 0x44, PF_R | PF_X);
        put64(&mut b, 0x50, 0x40_0000);
        put64(&mut b, 0x60, 0x78);
        put64(&mut b, 0x68, 0x78);
        b
    }

    #[test]
    fn a_minimal_executable_reads_in_either_byte_order() {
        for big in [false, true] {
            let elf = ElfFile::parse(&minimal(big)).unwrap();
            assert_eq!((elf.wide, elf.big_endian, elf.kind, elf.machine, elf.entry), (true, big, ET_EXEC, EM_X86_64, 0x40_0078));
            assert_eq!(elf.segments.len(), 1);
            assert_eq!(elf.segments[0].vaddr, 0x40_0000);
            assert_eq!(elf.offset_of(0x40_0010), Some(0x10));
            assert!(elf.sections.is_empty() && elf.warnings.is_empty(), "{:?}", elf.warnings);
        }
    }

    #[test]
    fn a_truncated_program_header_names_its_field() {
        let mut bytes = minimal(false);
        bytes.truncate(0x50);
        let e = ElfFile::parse(&bytes).unwrap_err();
        assert!(format!("{e:#}").contains("program header 0"), "{e:#}");
    }

    #[test]
    fn the_leb128_numbers_read_as_dwarf_writes_them() {
        let d = Data { reader: Reader::new(&[0xe5, 0x8e, 0x26, 0x7f, 0x80, 0x7f]), big: false, wide: true };
        let mut at = 0;
        assert_eq!(leb128(d, &mut at, false).unwrap(), 624_485);
        assert_eq!(leb128(d, &mut at, true).unwrap() as i64, -1);
        assert_eq!(leb128(d, &mut at, true).unwrap() as i64, -128);
        assert_eq!(at, 6);
    }

    #[test]
    fn a_relr_table_unpacks_to_its_words() {
        // 0x1000, then the words at 0x1008 and 0x1018 (bits 1 and 3), then past the 63 words
        // the bitmap covers the one at 0x1008 + 63 * 8 (bit 1 of the next).
        let entries = [0x1000, 0b1011, 0b11];
        assert_eq!(unpack_relr(&entries, 8), vec![0x1000, 0x1008, 0x1018, 0x1008 + 63 * 8]);
        // A 32-bit table: 31 words a bitmap.
        assert_eq!(unpack_relr(&[0x2000, 0b101, 0b11], 4), vec![0x2000, 0x2008, 0x2004 + 31 * 4]);
    }
}
