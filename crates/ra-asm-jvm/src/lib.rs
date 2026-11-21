//! The Java virtual machine as a machine to lift: the class file ([`class`]) and the bytecode of
//! its methods ([`decode`]), told apart by the release that wrote them ([`version`]).
//!
//! A class file says which release of Java compiled it in its major version, and each release is
//! a target of its own: what a class may hold (a constant of a kind, an instruction) is what its
//! release had. A class of Java 6 has no `invokedynamic`; one of Java 7 has no `jsr`. The reader
//! and the decoder hold a class to its release, so what comes out of them is a program of that
//! release and of no other.
//!
//! The machine is a stack machine with typed instructions: every instruction says the type it
//! works on in its name, and the stack's depth at each instruction is fixed by the code alone.

pub mod class;
pub mod decode;
pub mod version;

pub use class::{Attribute, ClassFile, Code, Constant, Handler, Member};
pub use decode::{Instruction, Operand, decode};

/// What is wrong with a class file or with a method's code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

pub(crate) fn error<T>(message: impl Into<String>) -> Result<T, Error> {
    Err(Error(message.into()))
}
