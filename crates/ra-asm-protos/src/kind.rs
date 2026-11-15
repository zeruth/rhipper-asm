//! The kind of a parameter or a return, as the signatures of the lift and the import prototypes
//! both say it: a plain enumeration, so that the lookup and its kinds live in this crate and the
//! IR type each stands for is the lift's to give.

/// The kind of a parameter or a return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// An integer of the address width.
    Int,
    /// A 64-bit integer on the x86 stack: two slots.
    Wide,
    Float,
    Double,
}

impl Kind {
    /// The bytes a parameter of this kind takes on a stack whose slots are `slot` bytes (the
    /// pointer's size).
    #[must_use]
    pub fn stack_width(self, slot: i64) -> i64 {
        match self {
            Self::Wide | Self::Double => 8,
            Self::Int | Self::Float => slot,
        }
    }
}
