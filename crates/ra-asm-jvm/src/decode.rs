//! The decoder of a method's bytecode: every instruction of the machine, with the operands its
//! bytes say, held to the release of the class it is in.

use crate::{Error, error, version};

/// The instructions' names by their opcodes, `nop` (0x00) to `jsr_w` (0xc9).
pub const NAMES: [&str; 202] = [
    "nop",
    "aconst_null",
    "iconst_m1",
    "iconst_0",
    "iconst_1",
    "iconst_2",
    "iconst_3",
    "iconst_4",
    "iconst_5",
    "lconst_0",
    "lconst_1",
    "fconst_0",
    "fconst_1",
    "fconst_2",
    "dconst_0",
    "dconst_1",
    "bipush",
    "sipush",
    "ldc",
    "ldc_w",
    "ldc2_w",
    "iload",
    "lload",
    "fload",
    "dload",
    "aload",
    "iload_0",
    "iload_1",
    "iload_2",
    "iload_3",
    "lload_0",
    "lload_1",
    "lload_2",
    "lload_3",
    "fload_0",
    "fload_1",
    "fload_2",
    "fload_3",
    "dload_0",
    "dload_1",
    "dload_2",
    "dload_3",
    "aload_0",
    "aload_1",
    "aload_2",
    "aload_3",
    "iaload",
    "laload",
    "faload",
    "daload",
    "aaload",
    "baload",
    "caload",
    "saload",
    "istore",
    "lstore",
    "fstore",
    "dstore",
    "astore",
    "istore_0",
    "istore_1",
    "istore_2",
    "istore_3",
    "lstore_0",
    "lstore_1",
    "lstore_2",
    "lstore_3",
    "fstore_0",
    "fstore_1",
    "fstore_2",
    "fstore_3",
    "dstore_0",
    "dstore_1",
    "dstore_2",
    "dstore_3",
    "astore_0",
    "astore_1",
    "astore_2",
    "astore_3",
    "iastore",
    "lastore",
    "fastore",
    "dastore",
    "aastore",
    "bastore",
    "castore",
    "sastore",
    "pop",
    "pop2",
    "dup",
    "dup_x1",
    "dup_x2",
    "dup2",
    "dup2_x1",
    "dup2_x2",
    "swap",
    "iadd",
    "ladd",
    "fadd",
    "dadd",
    "isub",
    "lsub",
    "fsub",
    "dsub",
    "imul",
    "lmul",
    "fmul",
    "dmul",
    "idiv",
    "ldiv",
    "fdiv",
    "ddiv",
    "irem",
    "lrem",
    "frem",
    "drem",
    "ineg",
    "lneg",
    "fneg",
    "dneg",
    "ishl",
    "lshl",
    "ishr",
    "lshr",
    "iushr",
    "lushr",
    "iand",
    "land",
    "ior",
    "lor",
    "ixor",
    "lxor",
    "iinc",
    "i2l",
    "i2f",
    "i2d",
    "l2i",
    "l2f",
    "l2d",
    "f2i",
    "f2l",
    "f2d",
    "d2i",
    "d2l",
    "d2f",
    "i2b",
    "i2c",
    "i2s",
    "lcmp",
    "fcmpl",
    "fcmpg",
    "dcmpl",
    "dcmpg",
    "ifeq",
    "ifne",
    "iflt",
    "ifge",
    "ifgt",
    "ifle",
    "if_icmpeq",
    "if_icmpne",
    "if_icmplt",
    "if_icmpge",
    "if_icmpgt",
    "if_icmple",
    "if_acmpeq",
    "if_acmpne",
    "goto",
    "jsr",
    "ret",
    "tableswitch",
    "lookupswitch",
    "ireturn",
    "lreturn",
    "freturn",
    "dreturn",
    "areturn",
    "return",
    "getstatic",
    "putstatic",
    "getfield",
    "putfield",
    "invokevirtual",
    "invokespecial",
    "invokestatic",
    "invokeinterface",
    "invokedynamic",
    "new",
    "newarray",
    "anewarray",
    "arraylength",
    "athrow",
    "checkcast",
    "instanceof",
    "monitorenter",
    "monitorexit",
    "wide",
    "multianewarray",
    "ifnull",
    "ifnonnull",
    "goto_w",
    "jsr_w",
];

/// What an instruction's bytes say beside its opcode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    None,
    /// A local variable's index.
    Local(u16),
    /// A constant in the instruction itself (`bipush`, `sipush`).
    Int(i32),
    /// The index of a constant of the pool.
    Constant(u16),
    /// Where a branch goes, as an offset in the method's code.
    Branch(u32),
    Iinc {
        local: u16,
        by: i16,
    },
    /// The element type of `newarray`, by its code (4 `boolean` to 11 `long`).
    NewArray(u8),
    InvokeInterface {
        index: u16,
        count: u8,
    },
    MultiANewArray {
        index: u16,
        dimensions: u8,
    },
    /// `tableswitch`: the targets of `low` and of each key after it, and of every other key.
    Table {
        low: i32,
        default: u32,
        targets: Vec<u32>,
    },
    /// `lookupswitch`: each key with its target, and the target of every other key.
    Lookup {
        default: u32,
        pairs: Vec<(i32, u32)>,
    },
}

/// An instruction of a method: where it is in the code, its opcode, its name (a `wide` form's
/// ends in `_w`), its operand and its length in bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    pub offset: u32,
    pub opcode: u8,
    pub name: &'static str,
    pub operand: Operand,
    pub length: u32,
}

impl Instruction {
    /// The offsets control may go to from here beside the next instruction's.
    #[must_use]
    pub fn targets(&self) -> Vec<u32> {
        match &self.operand {
            Operand::Branch(target) => vec![*target],
            Operand::Table { default, targets, .. } => targets.iter().copied().chain([*default]).collect(),
            Operand::Lookup { default, pairs } => pairs.iter().map(|&(_, target)| target).chain([*default]).collect(),
            _ => Vec::new(),
        }
    }

    /// Whether control never goes on to the next instruction: an unconditional jump, a switch,
    /// a return, a throw, a subroutine's `ret`.
    #[must_use]
    pub fn ends_block(&self) -> bool {
        matches!(self.opcode, 0xa7 | 0xa9..=0xb1 | 0xbf | 0xc8) || (self.opcode == 0xc4 && self.name == "ret_w")
    }
}

/// The names of `newarray`'s element types, by their codes.
const ELEMENTS: [&str; 8] = ["boolean", "char", "float", "double", "byte", "short", "int", "long"];

/// The instruction as `javap -c` prints it: a switch on lines of its own, a key and its target
/// each.
impl std::fmt::Display for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.offset, self.name)?;
        match &self.operand {
            Operand::None => Ok(()),
            Operand::Local(n) => write!(f, " {n}"),
            Operand::Int(n) => write!(f, " {n}"),
            Operand::Constant(n) if self.opcode == 0xba => write!(f, " #{n}, 0"),
            Operand::Constant(n) => write!(f, " #{n}"),
            Operand::Branch(target) => write!(f, " {target}"),
            Operand::Iinc { local, by } => write!(f, " {local}, {by}"),
            Operand::NewArray(code) => write!(f, " {}", ELEMENTS.get(usize::from(code.wrapping_sub(4))).copied().unwrap_or("?")),
            Operand::InvokeInterface { index, count } => write!(f, " #{index}, {count}"),
            Operand::MultiANewArray { index, dimensions } => write!(f, " #{index}, {dimensions}"),
            Operand::Table { low, default, targets } => {
                writeln!(f, " {{")?;
                for (n, target) in targets.iter().enumerate() {
                    writeln!(f, "{}: {target}", i64::from(*low) + n as i64)?;
                }
                write!(f, "default: {default}\n}}")
            }
            Operand::Lookup { default, pairs } => {
                writeln!(f, " {{")?;
                for (key, target) in pairs {
                    writeln!(f, "{key}: {target}")?;
                }
                write!(f, "default: {default}\n}}")
            }
        }
    }
}

struct Bytes<'a> {
    code: &'a [u8],
    at: usize,
    /// The instruction being read, for what is wrong with it.
    start: usize,
}

impl Bytes<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let Some(slice) = self.code.get(self.at..self.at + N) else {
            return error(format!("the code ends inside the instruction at {}", self.start));
        };
        self.at += N;
        Ok(slice.try_into().unwrap_or([0; N]))
    }

    fn u1(&mut self) -> Result<u8, Error> {
        Ok(self.take::<1>()?[0])
    }

    fn u2(&mut self) -> Result<u16, Error> {
        Ok(u16::from_be_bytes(self.take()?))
    }

    fn i4(&mut self) -> Result<i32, Error> {
        Ok(i32::from_be_bytes(self.take()?))
    }

    /// The place a branch of `offset` from the instruction goes, which is inside the code.
    fn target(&self, offset: i32) -> Result<u32, Error> {
        let target = self.start as i64 + i64::from(offset);
        if target < 0 || target >= self.code.len() as i64 {
            return error(format!("the instruction at {} branches to {target}, outside its method's code", self.start));
        }
        Ok(target as u32)
    }
}

/// Decodes the code of a method of a class of version `major`, first instruction to last.
///
/// # Errors
/// When a byte is no instruction, an instruction is one the class's release had not, the code
/// ends inside one, or a branch leaves the code.
pub fn decode(code: &[u8], major: u16) -> Result<Vec<Instruction>, Error> {
    let mut b = Bytes { code, at: 0, start: 0 };
    let mut out = Vec::new();
    while b.at < code.len() {
        b.start = b.at;
        let opcode = b.u1()?;
        let Some(&name) = NAMES.get(usize::from(opcode)) else {
            return error(format!("the byte {opcode:#04x} at {} is no instruction", b.start));
        };
        if !version::has_opcode(major, opcode) {
            let release = version::name(major).unwrap_or_default();
            return error(format!("`{name}` at {}: a class of Java {release} (version {major}) has no such instruction", b.start));
        }
        let mut name = name;
        let operand = match opcode {
            0x10 => Operand::Int(i32::from(b.u1()?.cast_signed())),
            0x11 => Operand::Int(i32::from(b.u2()?.cast_signed())),
            0x12 => Operand::Constant(u16::from(b.u1()?)),
            0x13 | 0x14 | 0xb2..=0xb8 | 0xbb | 0xbd | 0xc0 | 0xc1 => Operand::Constant(b.u2()?),
            0x15..=0x19 | 0x36..=0x3a | 0xa9 => Operand::Local(u16::from(b.u1()?)),
            0x84 => Operand::Iinc { local: u16::from(b.u1()?), by: i16::from(b.u1()?.cast_signed()) },
            0x99..=0xa8 | 0xc6 | 0xc7 => {
                let offset = i32::from(b.u2()?.cast_signed());
                Operand::Branch(b.target(offset)?)
            }
            0xc8 | 0xc9 => {
                let offset = b.i4()?;
                Operand::Branch(b.target(offset)?)
            }
            0xaa | 0xab => {
                // The operands start at the next offset that is a multiple of four.
                while !b.at.is_multiple_of(4) {
                    b.u1()?;
                }
                let default = b.i4()?;
                let default = b.target(default)?;
                if opcode == 0xaa {
                    let (low, high) = (b.i4()?, b.i4()?);
                    let count = i64::from(high) - i64::from(low) + 1;
                    if count < 1 || count > (code.len() / 4) as i64 {
                        return error(format!("the `tableswitch` at {} has the keys {low} to {high}", b.start));
                    }
                    let mut targets = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        let offset = b.i4()?;
                        targets.push(b.target(offset)?);
                    }
                    Operand::Table { low, default, targets }
                } else {
                    let count = b.i4()?;
                    if count < 0 || count as usize > code.len() / 8 {
                        return error(format!("the `lookupswitch` at {} has {count} keys", b.start));
                    }
                    let mut pairs = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        let (key, offset) = (b.i4()?, b.i4()?);
                        pairs.push((key, b.target(offset)?));
                    }
                    Operand::Lookup { default, pairs }
                }
            }
            0xb9 => {
                let (index, count, zero) = (b.u2()?, b.u1()?, b.u1()?);
                if zero != 0 {
                    return error(format!("the `invokeinterface` at {} has {zero} where a zero is", b.start));
                }
                Operand::InvokeInterface { index, count }
            }
            0xba => {
                let (index, zeros) = (b.u2()?, b.u2()?);
                if zeros != 0 {
                    return error(format!("the `invokedynamic` at {} has {zeros} where two zeros are", b.start));
                }
                Operand::Constant(index)
            }
            0xbc => Operand::NewArray(b.u1()?),
            0xc5 => Operand::MultiANewArray { index: b.u2()?, dimensions: b.u1()? },
            0xc4 => {
                // `wide` gives the instruction after it an index of two bytes.
                let widened = b.u1()?;
                name = match widened {
                    0x15 => "iload_w",
                    0x16 => "lload_w",
                    0x17 => "fload_w",
                    0x18 => "dload_w",
                    0x19 => "aload_w",
                    0x36 => "istore_w",
                    0x37 => "lstore_w",
                    0x38 => "fstore_w",
                    0x39 => "dstore_w",
                    0x3a => "astore_w",
                    0x84 => "iinc_w",
                    0xa9 if version::has_opcode(major, 0xa9) => "ret_w",
                    _ => return error(format!("the `wide` at {} is before the byte {widened:#04x}, which it does not widen", b.start)),
                };
                if widened == 0x84 { Operand::Iinc { local: b.u2()?, by: b.u2()?.cast_signed() } } else { Operand::Local(b.u2()?) }
            }
            _ => Operand::None,
        };
        out.push(Instruction { offset: b.start as u32, opcode, name, operand, length: (b.at - b.start) as u32 });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(code: &[u8], major: u16) -> Vec<String> {
        decode(code, major).unwrap().iter().map(ToString::to_string).collect()
    }

    #[test]
    fn every_opcode_has_its_name() {
        assert_eq!(NAMES.len(), 0xca);
        assert_eq!(
            (NAMES[0x00], NAMES[0x12], NAMES[0x60], NAMES[0xa7], NAMES[0xb1], NAMES[0xba], NAMES[0xc9]),
            ("nop", "ldc", "iadd", "goto", "return", "invokedynamic", "jsr_w")
        );
        let mut names = NAMES.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), NAMES.len());
    }

    #[test]
    fn the_operands_are_what_the_bytes_say() {
        let code = [
            0x10, 0xff, // bipush -1
            0x11, 0x01, 0x00, // sipush 256
            0x12, 0x07, // ldc #7
            0x15, 0x05, // iload 5
            0x84, 0x01, 0xfe, // iinc 1, -2
            0xbc, 0x0a, // newarray int
            0xb9, 0x00, 0x09, 0x02, 0x00, // invokeinterface #9, 2
            0xba, 0x00, 0x0c, 0x00, 0x00, // invokedynamic #12, 0
            0xc5, 0x00, 0x03, 0x02, // multianewarray #3, 2
            0xa7, 0xff, 0xe4, // goto 0
            0xb1, // return
        ];
        assert_eq!(
            lines(&code, 52),
            [
                "0: bipush -1",
                "2: sipush 256",
                "5: ldc #7",
                "7: iload 5",
                "9: iinc 1, -2",
                "12: newarray int",
                "14: invokeinterface #9, 2",
                "19: invokedynamic #12, 0",
                "24: multianewarray #3, 2",
                "28: goto 0",
                "31: return",
            ]
        );
    }

    #[test]
    fn a_switch_is_padded_to_four_bytes() {
        // iload_0; tableswitch 1..2 -> 28, 29, default 30; then three returns at 28, 29, 30.
        let mut code = vec![0x1a, 0xaa, 0, 0];
        for word in [29i32, 1, 2, 27, 28] {
            code.extend(word.to_be_bytes());
        }
        code.resize(28, 0);
        code.extend([0xac, 0xac, 0xac]);
        let decoded = decode(&code, 52).unwrap();
        assert_eq!(decoded[1].operand, Operand::Table { low: 1, default: 30, targets: vec![28, 29] });
        assert_eq!(decoded[1].length, 23);
        assert_eq!(decoded[1].targets(), [28, 29, 30]);
        assert!(decoded[1].ends_block());
        assert_eq!(decoded[1].to_string(), "1: tableswitch {\n1: 28\n2: 29\ndefault: 30\n}");

        // lookupswitch at 0: -5 -> 28, 9 -> 29, default 30.
        let mut code = vec![0xab, 0, 0, 0];
        for word in [30i32, 2, -5, 28, 9, 29] {
            code.extend(word.to_be_bytes());
        }
        code.extend([0xb1, 0xb1, 0xb1]);
        let decoded = decode(&code, 52).unwrap();
        assert_eq!(decoded[0].operand, Operand::Lookup { default: 30, pairs: vec![(-5, 28), (9, 29)] });
        assert_eq!(decoded[0].to_string(), "0: lookupswitch {\n-5: 28\n9: 29\ndefault: 30\n}");
    }

    #[test]
    fn wide_gives_an_index_two_bytes() {
        let code = [0xc4, 0x15, 0x01, 0x00, 0xc4, 0x84, 0x01, 0x00, 0x03, 0xe8, 0xb1];
        assert_eq!(lines(&code, 52), ["0: iload_w 256", "4: iinc_w 256, 1000", "10: return"]);
        assert!(decode(&[0xc4, 0x60], 52).is_err());
    }

    #[test]
    fn a_method_is_held_to_its_release() {
        // jsr 4; return; astore_1; ret 1: a subroutine, which Java 7's class files lost.
        let subroutine = [0xa8, 0x00, 0x04, 0xb1, 0x4c, 0xa9, 0x01];
        assert_eq!(lines(&subroutine, 50), ["0: jsr 4", "3: return", "4: astore_1", "5: ret 1"]);
        let refused = decode(&subroutine, 51).unwrap_err();
        assert!(refused.0.contains("Java 7"), "{refused}");
        // And `invokedynamic` came with them.
        let site = [0xba, 0x00, 0x02, 0x00, 0x00, 0xb1];
        assert!(decode(&site, 51).is_ok());
        assert!(decode(&site, 50).unwrap_err().0.contains("Java 6"));
    }

    #[test]
    fn what_is_no_code_is_refused() {
        assert!(decode(&[0xca], 52).is_err(), "a reserved opcode");
        assert!(decode(&[0x11, 0x00], 52).is_err(), "an instruction cut short");
        assert!(decode(&[0xa7, 0x00, 0x10], 52).is_err(), "a branch past the end");
        assert!(decode(&[0xa7, 0xff, 0xff], 52).is_err(), "a branch before the start");
        assert!(decode(&[0xb9, 0x00, 0x01, 0x01, 0x01], 52).is_err(), "invokeinterface's zero");
        assert!(decode(&[], 52).unwrap().is_empty());
    }

    #[test]
    fn a_class_carries_its_code_to_the_decoder() {
        let bytes = crate::class::tests::class(45, 3, &[0x03, 0x3b, 0xb1], &[]);
        let file = crate::ClassFile::parse(&bytes).unwrap();
        let code = file.code(&file.methods[0]).unwrap().unwrap();
        let names: Vec<&str> = decode(&code.code, file.major).unwrap().iter().map(|i| i.name).collect();
        assert_eq!(names, ["iconst_0", "istore_0", "return"]);
    }
}
