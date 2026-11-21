//! The class file: the constant pool, the class's own names, its fields and methods with their
//! attributes, and a method's code.

use crate::{Error, error, version};

/// A constant of the pool. The indices in one are of the pool itself.
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    /// The slot no constant is at: index zero, and the slot after a `Long` or a `Double`.
    Unusable,
    Utf8(String),
    Integer(i32),
    Float(f32),
    Long(i64),
    Double(f64),
    Class(u16),
    String(u16),
    Field {
        class: u16,
        name_and_type: u16,
    },
    Method {
        class: u16,
        name_and_type: u16,
    },
    InterfaceMethod {
        class: u16,
        name_and_type: u16,
    },
    NameAndType {
        name: u16,
        descriptor: u16,
    },
    MethodHandle {
        kind: u8,
        reference: u16,
    },
    MethodType(u16),
    Dynamic {
        bootstrap: u16,
        name_and_type: u16,
    },
    InvokeDynamic {
        bootstrap: u16,
        name_and_type: u16,
    },
    Module(u16),
    Package(u16),
}

/// An attribute as the file has it: its name's index in the pool and its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub name: u16,
    pub info: Vec<u8>,
}

/// A field or a method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub access: u16,
    pub name: u16,
    pub descriptor: u16,
    pub attributes: Vec<Attribute>,
}

/// A range of a method's code and the handler of what is thrown in it: every exception when
/// `catch_type` is zero, else the class at that index of the pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Handler {
    pub start: u16,
    pub end: u16,
    pub handler: u16,
    pub catch_type: u16,
}

/// A method's code: the depth of its operand stack, the number of its local variables, the
/// bytecode and the exception handlers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Code {
    pub max_stack: u16,
    pub max_locals: u16,
    pub code: Vec<u8>,
    pub handlers: Vec<Handler>,
    pub attributes: Vec<Attribute>,
}

/// A class file.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassFile {
    pub minor: u16,
    pub major: u16,
    pub constants: Vec<Constant>,
    pub access: u16,
    pub this_class: u16,
    pub super_class: u16,
    pub interfaces: Vec<u16>,
    pub fields: Vec<Member>,
    pub methods: Vec<Member>,
    pub attributes: Vec<Attribute>,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        let end = self.at.checked_add(n).filter(|&end| end <= self.bytes.len());
        let Some(end) = end else {
            return error(format!("the file ends at {:#x}, inside what starts at {:#x}", self.bytes.len(), self.at));
        };
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn u1(&mut self) -> Result<u8, Error> {
        Ok(self.take(1)?[0])
    }

    fn u2(&mut self) -> Result<u16, Error> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }

    fn u4(&mut self) -> Result<u32, Error> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn attributes(&mut self) -> Result<Vec<Attribute>, Error> {
        let count = self.u2()?;
        let mut attributes = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            let name = self.u2()?;
            let length = self.u4()? as usize;
            attributes.push(Attribute { name, info: self.take(length)?.to_vec() });
        }
        Ok(attributes)
    }

    fn members(&mut self) -> Result<Vec<Member>, Error> {
        let count = self.u2()?;
        let mut members = Vec::with_capacity(usize::from(count));
        for _ in 0..count {
            members.push(Member { access: self.u2()?, name: self.u2()?, descriptor: self.u2()?, attributes: self.attributes()? });
        }
        Ok(members)
    }
}

/// The text of a `Utf8` constant: the class file's own encoding, in which the null character is
/// two bytes and a character beyond the sixteen bits is a surrogate pair of three bytes each.
fn modified_utf8(bytes: &[u8]) -> String {
    let mut units = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let b = u16::from(bytes[at]);
        let next = |k: usize| u16::from(bytes.get(at + k).copied().unwrap_or(0)) & 0x3f;
        if b & 0x80 == 0 {
            units.push(b);
            at += 1;
        } else if b & 0xe0 == 0xc0 {
            units.push((b & 0x1f) << 6 | next(1));
            at += 2;
        } else {
            units.push((b & 0x0f) << 12 | next(1) << 6 | next(2));
            at += 3;
        }
    }
    String::from_utf16_lossy(&units)
}

impl ClassFile {
    /// Reads a class file, holding it to its release: a version no release wrote, or a constant
    /// of a kind the class's release did not have, is an error.
    ///
    /// # Errors
    /// When the bytes are no class file, end early, or hold what their release had not.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let mut r = Reader { bytes, at: 0 };
        let magic = r.u4()?;
        if magic != 0xcafe_babe {
            return error(format!("no class file: it starts with {magic:#010x}"));
        }
        let (minor, major) = (r.u2()?, r.u2()?);
        if version::release(major).is_none() {
            return error(format!("a class file of version {major}.{minor}, which no release from Java 1.0 to Java 25 writes"));
        }
        let count = r.u2()?;
        let mut constants = vec![Constant::Unusable];
        while constants.len() < usize::from(count) {
            let index = constants.len();
            let tag = r.u1()?;
            let constant = match tag {
                1 => {
                    let length = r.u2()?;
                    Constant::Utf8(modified_utf8(r.take(usize::from(length))?))
                }
                3 => Constant::Integer(r.u4()?.cast_signed()),
                4 => Constant::Float(f32::from_bits(r.u4()?)),
                5 => Constant::Long((u64::from(r.u4()?) << 32 | u64::from(r.u4()?)).cast_signed()),
                6 => Constant::Double(f64::from_bits(u64::from(r.u4()?) << 32 | u64::from(r.u4()?))),
                7 => Constant::Class(r.u2()?),
                8 => Constant::String(r.u2()?),
                9 => Constant::Field { class: r.u2()?, name_and_type: r.u2()? },
                10 => Constant::Method { class: r.u2()?, name_and_type: r.u2()? },
                11 => Constant::InterfaceMethod { class: r.u2()?, name_and_type: r.u2()? },
                12 => Constant::NameAndType { name: r.u2()?, descriptor: r.u2()? },
                15 => Constant::MethodHandle { kind: r.u1()?, reference: r.u2()? },
                16 => Constant::MethodType(r.u2()?),
                17 => Constant::Dynamic { bootstrap: r.u2()?, name_and_type: r.u2()? },
                18 => Constant::InvokeDynamic { bootstrap: r.u2()?, name_and_type: r.u2()? },
                19 => Constant::Module(r.u2()?),
                20 => Constant::Package(r.u2()?),
                _ => return error(format!("constant {index} has the tag {tag}, which is no kind of constant")),
            };
            let since = version::constant_since(tag);
            if major < since {
                return error(format!(
                    "constant {index} is of a kind (tag {tag}) that came with class files of version {since}, and the class is of version {major}"
                ));
            }
            let wide = matches!(constant, Constant::Long(_) | Constant::Double(_));
            constants.push(constant);
            if wide {
                constants.push(Constant::Unusable);
            }
        }
        let (access, this_class, super_class) = (r.u2()?, r.u2()?, r.u2()?);
        let interfaces = (0..r.u2()?).map(|_| r.u2()).collect::<Result<Vec<u16>, Error>>()?;
        let (fields, methods, attributes) = (r.members()?, r.members()?, r.attributes()?);
        if r.at != bytes.len() {
            return error(format!("{} bytes follow the class", bytes.len() - r.at));
        }
        Ok(Self { minor, major, constants, access, this_class, super_class, interfaces, fields, methods, attributes })
    }

    /// The text of the `Utf8` constant at `index`.
    #[must_use]
    pub fn utf8(&self, index: u16) -> Option<&str> {
        match self.constants.get(usize::from(index))? {
            Constant::Utf8(text) => Some(text),
            _ => None,
        }
    }

    /// The name of the class the `Class` constant at `index` names, as the file has it
    /// (`java/lang/Object`).
    #[must_use]
    pub fn class_name(&self, index: u16) -> Option<&str> {
        match self.constants.get(usize::from(index))? {
            Constant::Class(name) => self.utf8(*name),
            _ => None,
        }
    }

    /// The class's own name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.class_name(self.this_class)
    }

    /// The attribute of `attributes` named `name`.
    #[must_use]
    pub fn attribute<'a>(&self, attributes: &'a [Attribute], name: &str) -> Option<&'a Attribute> {
        attributes.iter().find(|attribute| self.utf8(attribute.name) == Some(name))
    }

    /// The code of `method`: `None` for a method with none (an abstract or a native one).
    ///
    /// # Errors
    /// When the method's `Code` attribute ends early.
    pub fn code(&self, method: &Member) -> Result<Option<Code>, Error> {
        let Some(attribute) = self.attribute(&method.attributes, "Code") else { return Ok(None) };
        let mut r = Reader { bytes: &attribute.info, at: 0 };
        // Before Java 1.0.2 a method's code said its sizes in fewer bytes.
        let old = self.major == 45 && self.minor < 3;
        let (max_stack, max_locals, length) =
            if old { (u16::from(r.u1()?), u16::from(r.u1()?), usize::from(r.u2()?)) } else { (r.u2()?, r.u2()?, r.u4()? as usize) };
        let code = r.take(length)?.to_vec();
        let mut handlers = Vec::new();
        for _ in 0..r.u2()? {
            handlers.push(Handler { start: r.u2()?, end: r.u2()?, handler: r.u2()?, catch_type: r.u2()? });
        }
        Ok(Some(Code { max_stack, max_locals, code, handlers, attributes: r.attributes()? }))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A class file of one class `T` over `java/lang/Object` with one static method `m()V` of
    /// `code`, in the version given, with `extra` constants after its own.
    pub(crate) fn class(major: u16, minor: u16, code: &[u8], extra: &[&[u8]]) -> Vec<u8> {
        fn utf8(out: &mut Vec<u8>, text: &str) {
            out.push(1);
            out.extend((text.len() as u16).to_be_bytes());
            out.extend(text.as_bytes());
        }
        let mut out = vec![0xca, 0xfe, 0xba, 0xbe];
        out.extend(minor.to_be_bytes());
        out.extend(major.to_be_bytes());
        out.extend((8 + extra.len() as u16).to_be_bytes());
        utf8(&mut out, "T"); // 1
        out.extend([7, 0, 1]); // 2
        utf8(&mut out, "java/lang/Object"); // 3
        out.extend([7, 0, 3]); // 4
        utf8(&mut out, "m"); // 5
        utf8(&mut out, "()V"); // 6
        utf8(&mut out, "Code"); // 7
        for constant in extra {
            out.extend(*constant);
        }
        out.extend([0, 0x21, 0, 2, 0, 4, 0, 0, 0, 0]); // access, this, super, no interfaces, no fields
        out.extend([0, 1, 0, 9, 0, 5, 0, 6, 0, 1, 0, 7]); // one method: static, m, ()V, one attribute: Code
        let old = major == 45 && minor < 3;
        let mut body = Vec::new();
        if old {
            body.extend([2u8, 3]);
            body.extend((code.len() as u16).to_be_bytes());
        } else {
            body.extend([0u8, 2, 0, 3]);
            body.extend((code.len() as u32).to_be_bytes());
        }
        body.extend(code);
        body.extend([0, 1, 0, 0, 0, 1, 0, 1, 0, 0]); // one handler of everything over 0..1 at 1
        body.extend([0, 0]);
        out.extend((body.len() as u32).to_be_bytes());
        out.extend(body);
        out.extend([0, 0]); // no attributes of the class
        out
    }

    #[test]
    fn a_class_of_the_first_release_reads() {
        for minor in [2, 3] {
            let file = ClassFile::parse(&class(45, minor, &[0xb1], &[])).unwrap();
            assert_eq!((file.major, file.minor), (45, minor));
            assert_eq!(file.name(), Some("T"));
            assert_eq!(file.class_name(file.super_class), Some("java/lang/Object"));
            let method = &file.methods[0];
            assert_eq!((file.utf8(method.name), file.utf8(method.descriptor)), (Some("m"), Some("()V")));
            let code = file.code(method).unwrap().unwrap();
            assert_eq!((code.max_stack, code.max_locals, code.code.as_slice()), (2, 3, &[0xb1][..]));
            assert_eq!(code.handlers, [Handler { start: 0, end: 1, handler: 1, catch_type: 0 }]);
        }
    }

    #[test]
    fn a_class_is_held_to_its_release() {
        // A call site of `invokedynamic` came with Java 7.
        let site: &[u8] = &[18, 0, 0, 0, 6];
        assert!(ClassFile::parse(&class(51, 0, &[0xb1], &[site])).is_ok());
        let refused = ClassFile::parse(&class(50, 0, &[0xb1], &[site])).unwrap_err();
        assert!(refused.0.contains("version 51"), "{refused}");
        // A constant computed at run time came with Java 11.
        let dynamic: &[u8] = &[17, 0, 0, 0, 6];
        assert!(ClassFile::parse(&class(55, 0, &[0xb1], &[dynamic])).is_ok());
        assert!(ClassFile::parse(&class(54, 0, &[0xb1], &[dynamic])).is_err());
        // No release wrote these.
        assert!(ClassFile::parse(&class(44, 0, &[0xb1], &[])).is_err());
        assert!(ClassFile::parse(&class(70, 0, &[0xb1], &[])).is_err());
    }

    #[test]
    fn what_is_no_class_file_is_refused() {
        assert!(ClassFile::parse(b"MZ\x90\x00").is_err());
        let whole = class(52, 0, &[0xb1], &[]);
        for cut in [3, 9, 20, whole.len() - 1] {
            assert!(ClassFile::parse(&whole[..cut]).is_err(), "cut at {cut}");
        }
        let mut longer = whole.clone();
        longer.push(0);
        assert!(ClassFile::parse(&longer).is_err());
        let unknown: &[u8] = &[2, 0, 0];
        assert!(ClassFile::parse(&class(52, 0, &[0xb1], &[unknown])).is_err());
    }

    #[test]
    fn the_wide_constants_take_two_slots() {
        let long: &[u8] = &[5, 0, 0, 0, 1, 0, 0, 0, 2];
        let text: &[u8] = &[1, 0, 3, 0xc0, 0x80, b'a'];
        // The count is one more than the constants given, for the slot after the long.
        let mut bytes = class(52, 0, &[0xb1], &[long, text]);
        bytes[9] += 1;
        let file = ClassFile::parse(&bytes).unwrap();
        assert_eq!(file.constants[8], Constant::Long(0x1_0000_0002));
        assert_eq!(file.constants[9], Constant::Unusable);
        assert_eq!(file.utf8(10), Some("\0a"));
    }
}
