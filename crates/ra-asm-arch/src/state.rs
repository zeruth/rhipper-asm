//! The layout of a machine's state as a value: the fields of `%State` in order, each a group of
//! registers of one width or a single one, from which the lift derives the IR type, the byte
//! offset of every field and the names of its slots. A machine is a constant here, not a branch in
//! the passes that read the state.

/// What a field of the state holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// The general registers, the stack pointer among them.
    General,
    ProgramCounter,
    /// The flags, one byte each.
    Flags,
    Vector,
    /// A field of the machine's own, outside the roles above.
    Extra,
}

/// One field of the state: `count` slots of `bits` bits, or one value of `bits` bits when
/// `count` is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Field {
    pub name: &'static str,
    pub role: Role,
    pub count: Option<u32>,
    pub bits: u16,
    /// The names of the slots, in order; fewer names than slots leave the rest unnamed.
    pub names: &'static [&'static str],
}

impl Field {
    /// A group of `count` registers of `bits` bits.
    pub const fn group(name: &'static str, role: Role, count: u32, bits: u16) -> Self {
        Self { name, role, count: Some(count), bits, names: &[] }
    }

    /// A single value of `bits` bits.
    pub const fn single(name: &'static str, role: Role, bits: u16) -> Self {
        Self { name, role, count: None, bits, names: &[] }
    }

    #[must_use]
    pub const fn named(self, names: &'static [&'static str]) -> Self {
        Self { names, ..self }
    }

    /// The slots of the field: one for a single value.
    pub fn slots(&self) -> u32 {
        self.count.unwrap_or(1)
    }

    /// The bytes one slot takes.
    pub fn slot_size(&self) -> u64 {
        u64::from(self.bits.div_ceil(8))
    }

    /// The bytes the field takes.
    pub fn size(&self) -> u64 {
        self.slot_size() * u64::from(self.slots())
    }

    /// The alignment of the field, in bytes: its slot's size rounded up to a power of two, at
    /// most 16, as the x86-64 data layout aligns an integer.
    pub fn align(&self) -> u64 {
        self.slot_size().next_power_of_two().min(16)
    }

    /// The name of slot `slot`, when the field names it.
    pub fn slot_name(&self, slot: u32) -> Option<&'static str> {
        self.names.get(slot as usize).copied()
    }
}

/// The layout of `%State` for one machine: its fields in order, each at its own alignment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateSpec {
    pub machine: &'static str,
    pub fields: &'static [Field],
}

impl StateSpec {
    /// The byte offset of each field.
    pub fn offsets(&self) -> Vec<u64> {
        let mut at = 0u64;
        self.fields
            .iter()
            .map(|f| {
                at = at.next_multiple_of(f.align());
                let here = at;
                at += f.size();
                here
            })
            .collect()
    }

    /// The alignment of the whole state: its widest field's.
    pub fn align(&self) -> u64 {
        self.fields.iter().map(Field::align).max().unwrap_or(1)
    }

    /// The size of the state, in bytes, padded to its alignment.
    pub fn size(&self) -> u64 {
        let end = self.fields.last().zip(self.offsets().last()).map_or(0, |(f, at)| at + f.size());
        end.next_multiple_of(self.align())
    }

    /// The index of the first field of `role`, and the field.
    pub fn field(&self, role: Role) -> Option<(usize, &'static Field)> {
        let fields: &'static [Field] = self.fields;
        fields.iter().enumerate().find(|(_, f)| f.role == role)
    }

    /// The byte offset of slot `slot` of the first field of `role`.
    pub fn slot_offset(&self, role: Role, slot: u32) -> Option<u64> {
        let (index, field) = self.field(role)?;
        (slot < field.slots()).then(|| self.offsets()[index] + u64::from(slot) * field.slot_size())
    }
}

/// x86 and x86-64: the general registers in iced's order, `rip`, the flags as bytes, and the
/// vector registers: `xmm0..xmm15` in the first sixteen slots, the upper halves of `ymm0..ymm15`
/// in the sixteen above them. The driver's C `struct State` has the same layout.
pub const X86: StateSpec = StateSpec {
    machine: "x86",
    fields: &[
        Field::group("gpr", Role::General, 16, 64)
            .named(&["rax", "rcx", "rdx", "rbx", "rsp", "rbp", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12", "r13", "r14", "r15"]),
        Field::single("rip", Role::ProgramCounter, 64),
        Field::group("flags", Role::Flags, 8, 8).named(&["cf", "pf", "af", "zf", "sf", "df", "of"]),
        Field::group("vector", Role::Vector, 32, 128),
    ],
};

/// ARM64: `x0..x30` and `sp`, `pc`, the flags as bytes and `v0..v31`.
pub const ARM64: StateSpec = StateSpec {
    machine: "arm64",
    fields: &[
        Field::group("gpr", Role::General, 32, 64).named(&[
            "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11", "x12", "x13", "x14", "x15", "x16", "x17", "x18",
            "x19", "x20", "x21", "x22", "x23", "x24", "x25", "x26", "x27", "x28", "x29", "x30", "sp",
        ]),
        Field::single("pc", Role::ProgramCounter, 64),
        Field::group("flags", Role::Flags, 8, 8).named(&["n", "z", "c", "v"]),
        Field::group("vector", Role::Vector, 32, 128).named(&[
            "v0", "v1", "v2", "v3", "v4", "v5", "v6", "v7", "v8", "v9", "v10", "v11", "v12", "v13", "v14", "v15", "v16", "v17", "v18",
            "v19", "v20", "v21", "v22", "v23", "v24", "v25", "v26", "v27", "v28", "v29", "v30", "v31",
        ]),
    ],
};

/// 32-bit ARM in Thumb-2: the x86 shape, `r0..r15` in 64-bit slots, the flags as ARM64 keeps
/// them and `q0..q15`.
pub const ARM32: StateSpec = StateSpec {
    machine: "arm32",
    fields: &[
        Field::group("gpr", Role::General, 16, 64)
            .named(&["r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11", "r12", "sp", "lr", "pc"]),
        Field::single("pc", Role::ProgramCounter, 64),
        Field::group("flags", Role::Flags, 8, 8).named(&["n", "z", "c", "v"]),
        Field::group("vector", Role::Vector, 16, 128)
            .named(&["q0", "q1", "q2", "q3", "q4", "q5", "q6", "q7", "q8", "q9", "q10", "q11", "q12", "q13", "q14", "q15"]),
    ],
};
