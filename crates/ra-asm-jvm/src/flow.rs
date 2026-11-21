//! The operand stack of a method, instruction by instruction: what a lift needs before it can
//! make values of the stack's slots.
//!
//! The machine's stack has one shape at each instruction, whatever path control took there: its
//! depth and the kind of every slot are fixed by the code alone. [`flow`] walks a method's code
//! from its entry and from its exception handlers, gives every instruction control reaches the
//! stack it finds, and refuses a method whose paths disagree.

use std::collections::HashMap;

use crate::class::{ClassFile, Code, Constant};
use crate::decode::{Instruction, Operand, decode};
use crate::{Error, error};

/// The kind of a value on the operand stack or in a local variable: the machine computes on
/// `int`s (which `boolean`, `byte`, `char` and `short` are on the stack), `long`s, `float`s,
/// `double`s and references, and a subroutine's return address is a value too.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Int,
    Long,
    Float,
    Double,
    Reference,
    ReturnAddress,
}

impl Kind {
    /// The slots a value of the kind takes, of the stack's depth and among the locals: two for a
    /// `long` and a `double`.
    #[must_use]
    pub fn slots(self) -> u16 {
        if matches!(self, Kind::Long | Kind::Double) { 2 } else { 1 }
    }

    fn of(letter: u8) -> Option<Self> {
        Some(match letter {
            b'I' | b'Z' | b'B' | b'C' | b'S' => Kind::Int,
            b'J' => Kind::Long,
            b'F' => Kind::Float,
            b'D' => Kind::Double,
            b'L' | b'[' | b'A' => Kind::Reference,
            b'R' => Kind::ReturnAddress,
            _ => return None,
        })
    }
}

/// The kind of a field of `descriptor` (`I`, `[D`, `Ljava/lang/String;`).
///
/// # Errors
/// When `descriptor` is no field's.
pub fn field_kind(descriptor: &str) -> Result<Kind, Error> {
    match descriptor.bytes().next().and_then(Kind::of) {
        Some(kind) if kind != Kind::ReturnAddress && !descriptor.starts_with('A') => Ok(kind),
        _ => error(format!("`{descriptor}` is no field's descriptor")),
    }
}

/// The kinds of the parameters of a method of `descriptor`, and of what it returns (`None` for
/// `void`).
///
/// # Errors
/// When `descriptor` is no method's.
pub fn method_kinds(descriptor: &str) -> Result<(Vec<Kind>, Option<Kind>), Error> {
    let bad = || Error(format!("`{descriptor}` is no method's descriptor"));
    let bytes = descriptor.as_bytes();
    if bytes.first() != Some(&b'(') {
        return Err(bad());
    }
    let mut parameters = Vec::new();
    let mut at = 1;
    loop {
        let start = at;
        while bytes.get(at) == Some(&b'[') {
            at += 1;
        }
        match bytes.get(at) {
            Some(b')') if start == at => break,
            Some(b'L') => at += bytes[at..].iter().position(|&b| b == b';').ok_or_else(bad)? + 1,
            Some(b'I' | b'Z' | b'B' | b'C' | b'S' | b'J' | b'F' | b'D') => at += 1,
            _ => return Err(bad()),
        }
        parameters.push(field_kind(&descriptor[start..at])?);
    }
    let returned = &descriptor[at + 1..];
    Ok((parameters, if returned == "V" { None } else { Some(field_kind(returned)?) }))
}

/// A method's code with the operand stack each instruction finds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Flow {
    pub instructions: Vec<Instruction>,
    /// The stack before each instruction, bottom first; `None` for one control never reaches.
    pub stacks: Vec<Option<Vec<Kind>>>,
    /// The deepest the stack gets, in slots: what the class file says as `max_stack`.
    pub max_stack: u16,
}

/// What an instruction takes from the stack and what it leaves, as letters of kinds around a
/// `>`; `?` for the instructions whose effect their operand or the stack itself says.
fn effect(opcode: u8) -> &'static str {
    match opcode {
        0x00 | 0x84 | 0xa7 | 0xa9 | 0xb1 | 0xc8 => ">",
        0x01 | 0x19 | 0x2a..=0x2d | 0xbb => ">A",
        0x02..=0x08 | 0x10 | 0x11 | 0x15 | 0x1a..=0x1d => ">I",
        0x09 | 0x0a | 0x16 | 0x1e..=0x21 => ">J",
        0x0b..=0x0d | 0x17 | 0x22..=0x25 => ">F",
        0x0e | 0x0f | 0x18 | 0x26..=0x29 => ">D",
        0x2e | 0x33..=0x35 => "AI>I",
        0x2f => "AI>J",
        0x30 => "AI>F",
        0x31 => "AI>D",
        0x32 => "AI>A",
        0x36 | 0x3b..=0x3e | 0x99..=0x9e | 0xaa | 0xab | 0xac => "I>",
        0x37 | 0x3f..=0x42 | 0xad => "J>",
        0x38 | 0x43..=0x46 | 0xae => "F>",
        0x39 | 0x47..=0x4a | 0xaf => "D>",
        0x3a | 0x4b..=0x4e | 0xb0 | 0xbf | 0xc2 | 0xc3 | 0xc6 | 0xc7 => "A>",
        0x4f | 0x54..=0x56 => "AII>",
        0x50 => "AIJ>",
        0x51 => "AIF>",
        0x52 => "AID>",
        0x53 => "AIA>",
        0x60 | 0x64 | 0x68 | 0x6c | 0x70 | 0x78 | 0x7a | 0x7c | 0x7e | 0x80 | 0x82 => "II>I",
        0x61 | 0x65 | 0x69 | 0x6d | 0x71 | 0x7f | 0x81 | 0x83 => "JJ>J",
        0x62 | 0x66 | 0x6a | 0x6e | 0x72 => "FF>F",
        0x63 | 0x67 | 0x6b | 0x6f | 0x73 => "DD>D",
        0x74 | 0x91..=0x93 => "I>I",
        0x75 => "J>J",
        0x76 => "F>F",
        0x77 => "D>D",
        0x79 | 0x7b | 0x7d => "JI>J",
        0x85 => "I>J",
        0x86 => "I>F",
        0x87 => "I>D",
        0x88 => "J>I",
        0x89 => "J>F",
        0x8a => "J>D",
        0x8b => "F>I",
        0x8c => "F>J",
        0x8d => "F>D",
        0x8e => "D>I",
        0x8f => "D>J",
        0x90 => "D>F",
        0x94 => "JJ>I",
        0x95 | 0x96 => "FF>I",
        0x97 | 0x98 => "DD>I",
        0x9f..=0xa4 => "II>",
        0xa5 | 0xa6 => "AA>",
        0xa8 | 0xc9 => ">R",
        0xbc | 0xbd => "I>A",
        0xbe | 0xc1 => "A>I",
        0xc0 => "A>A",
        _ => "?",
    }
}

/// The instruction a `wide` form widens, by the form's name.
pub(crate) fn widened(name: &str) -> u8 {
    match name {
        "iload_w" => 0x15,
        "lload_w" => 0x16,
        "fload_w" => 0x17,
        "dload_w" => 0x18,
        "aload_w" => 0x19,
        "istore_w" => 0x36,
        "lstore_w" => 0x37,
        "fstore_w" => 0x38,
        "dstore_w" => 0x39,
        "astore_w" => 0x3a,
        "iinc_w" => 0x84,
        _ => 0xa9,
    }
}

struct Stack<'a> {
    values: Vec<Kind>,
    at: &'a Instruction,
}

impl Stack<'_> {
    fn wrong<T>(&self, what: &str) -> Result<T, Error> {
        error(format!("`{}` at {}: {what}", self.at.name, self.at.offset))
    }

    fn pop(&mut self, kind: Kind) -> Result<(), Error> {
        // A subroutine stores the address it returns to with `astore`.
        let stores = matches!(self.at.opcode, 0x3a | 0x4b..=0x4e) || self.at.name == "astore_w";
        match self.values.pop() {
            Some(found) if found == kind || (stores && found == Kind::ReturnAddress) => Ok(()),
            Some(found) => self.wrong(&format!("the stack has a {found:?} where it takes a {kind:?}")),
            None => self.wrong("the stack is empty"),
        }
    }

    /// The values on top that take `slots` slots, bottom first.
    fn take(&mut self, slots: u16) -> Result<Vec<Kind>, Error> {
        let (mut taken, mut have) = (Vec::new(), 0);
        while have < slots {
            let Some(value) = self.values.pop() else { return self.wrong("the stack is empty") };
            have += value.slots();
            taken.insert(0, value);
        }
        if have == slots { Ok(taken) } else { self.wrong("a value of two slots is split") }
    }

    /// `top` slots duplicated under the `under` slots beneath them.
    fn dup(&mut self, top: u16, under: u16) -> Result<(), Error> {
        let top = self.take(top)?;
        let under = self.take(under)?;
        self.values.extend(top.iter().chain(&under).chain(&top));
        Ok(())
    }
}

/// The name and the descriptor of the member (or the call site, or the computed constant) the
/// constant at `index` is.
fn descriptor(file: &ClassFile, index: u16) -> Option<&str> {
    let name_and_type = match file.constants.get(usize::from(index))? {
        Constant::Field { name_and_type, .. }
        | Constant::Method { name_and_type, .. }
        | Constant::InterfaceMethod { name_and_type, .. }
        | Constant::Dynamic { name_and_type, .. }
        | Constant::InvokeDynamic { name_and_type, .. } => *name_and_type,
        _ => return None,
    };
    match file.constants.get(usize::from(name_and_type))? {
        Constant::NameAndType { descriptor, .. } => file.utf8(*descriptor),
        _ => None,
    }
}

/// Applies `instruction` to the stack.
fn apply(file: &ClassFile, instruction: &Instruction, values: Vec<Kind>) -> Result<Vec<Kind>, Error> {
    let mut stack = Stack { values, at: instruction };
    let opcode = if instruction.opcode == 0xc4 { widened(instruction.name) } else { instruction.opcode };
    let letters = effect(opcode);
    if let Some((takes, leaves)) = letters.split_once('>') {
        for letter in takes.bytes().rev() {
            stack.pop(Kind::of(letter).unwrap_or(Kind::Int))?;
        }
        stack.values.extend(leaves.bytes().filter_map(Kind::of));
        return Ok(stack.values);
    }
    let constant = match instruction.operand {
        Operand::Constant(index) | Operand::InvokeInterface { index, .. } | Operand::MultiANewArray { index, .. } => index,
        _ => 0,
    };
    let described = || match descriptor(file, constant) {
        Some(text) => Ok(text),
        None => stack.wrong(&format!("constant {constant} names no member")),
    };
    match opcode {
        0x12..=0x14 => {
            let kind = match file.constants.get(usize::from(constant)) {
                Some(Constant::Integer(_)) => Kind::Int,
                Some(Constant::Float(_)) => Kind::Float,
                Some(Constant::Long(_)) => Kind::Long,
                Some(Constant::Double(_)) => Kind::Double,
                Some(Constant::String(_) | Constant::Class(_) | Constant::MethodType(_) | Constant::MethodHandle { .. }) => Kind::Reference,
                Some(Constant::Dynamic { .. }) => field_kind(described()?)?,
                _ => return stack.wrong(&format!("constant {constant} is none a program loads")),
            };
            if (opcode == 0x14) != (kind.slots() == 2) {
                return stack.wrong(&format!("constant {constant} is a {kind:?}"));
            }
            stack.values.push(kind);
        }
        0x57 => drop(stack.take(1)?),
        0x58 => drop(stack.take(2)?),
        0x59 => stack.dup(1, 0)?,
        0x5a => stack.dup(1, 1)?,
        0x5b => stack.dup(1, 2)?,
        0x5c => stack.dup(2, 0)?,
        0x5d => stack.dup(2, 1)?,
        0x5e => stack.dup(2, 2)?,
        0x5f => {
            let (top, under) = (stack.take(1)?, stack.take(1)?);
            stack.values.extend(top.iter().chain(&under));
        }
        0xb2..=0xb5 => {
            let kind = field_kind(described()?)?;
            if matches!(opcode, 0xb3 | 0xb5) {
                stack.pop(kind)?;
            }
            if matches!(opcode, 0xb4 | 0xb5) {
                stack.pop(Kind::Reference)?;
            }
            if matches!(opcode, 0xb2 | 0xb4) {
                stack.values.push(kind);
            }
        }
        0xb6..=0xba => {
            let (parameters, returned) = method_kinds(described()?)?;
            for kind in parameters.into_iter().rev() {
                stack.pop(kind)?;
            }
            if matches!(opcode, 0xb6 | 0xb7 | 0xb9) {
                stack.pop(Kind::Reference)?;
            }
            stack.values.extend(returned);
        }
        0xc5 => {
            let Operand::MultiANewArray { dimensions, .. } = instruction.operand else { return stack.wrong("no dimensions") };
            for _ in 0..dimensions {
                stack.pop(Kind::Int)?;
            }
            stack.values.push(Kind::Reference);
        }
        _ => return stack.wrong("an instruction with no effect on the stack known"),
    }
    Ok(stack.values)
}

/// Walks the code of a method of `file`, giving each instruction the stack it finds.
///
/// # Errors
/// When the code does not decode, an instruction finds a stack it cannot work on, two paths
/// reach an instruction with different stacks, or control runs off the code's end.
pub fn flow(file: &ClassFile, code: &Code) -> Result<Flow, Error> {
    let instructions = decode(&code.code, file.major)?;
    let index: HashMap<u32, usize> = instructions.iter().enumerate().map(|(n, i)| (i.offset, n)).collect();
    let at = |offset: u32| match index.get(&offset) {
        Some(&n) => Ok(n),
        None => error(format!("control goes to {offset}, inside an instruction")),
    };
    let mut stacks: Vec<Option<Vec<Kind>>> = vec![None; instructions.len()];
    let mut work: Vec<(usize, Vec<Kind>)> = Vec::new();
    // A handler finds what was thrown, alone on the stack.
    for handler in code.handlers.iter().rev() {
        work.push((at(u32::from(handler.handler))?, vec![Kind::Reference]));
    }
    if !instructions.is_empty() {
        work.push((0, Vec::new()));
    }
    let mut deepest = 0;
    while let Some((n, stack)) = work.pop() {
        let instruction = &instructions[n];
        if let Some(found) = &stacks[n] {
            if *found != stack {
                return error(format!("the stack at {} is {found:?} by one path and {stack:?} by another", instruction.offset));
            }
            continue;
        }
        stacks[n] = Some(stack.clone());
        let after = apply(file, instruction, stack.clone())?;
        let depth = |values: &[Kind]| values.iter().map(|kind| kind.slots()).sum::<u16>();
        deepest = deepest.max(depth(&after)).max(depth(&stack));
        for target in instruction.targets() {
            work.push((at(target)?, after.clone()));
        }
        // A subroutine returns to the instruction after its `jsr`, with the stack the `jsr` found.
        let subroutine = matches!(instruction.opcode, 0xa8 | 0xc9);
        if subroutine || !instruction.ends_block() {
            if n + 1 == instructions.len() {
                return error(format!("control runs off the end of the code after `{}` at {}", instruction.name, instruction.offset));
            }
            work.push((n + 1, if subroutine { stack } else { after }));
        }
    }
    Ok(Flow { instructions, stacks, max_stack: deepest })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::tests::class;

    fn walk(code: &[u8], major: u16, extra: &[&[u8]]) -> Result<Flow, Error> {
        let file = ClassFile::parse(&class(major, 3, code, extra)).unwrap();
        let code = file.code(&file.methods[0]).unwrap().unwrap();
        flow(&file, &code)
    }

    #[test]
    fn a_descriptor_says_its_kinds() {
        use Kind::{Double, Float, Int, Long, Reference};
        assert_eq!(method_kinds("()V").unwrap(), (vec![], None));
        assert_eq!(
            method_kinds("(IJLjava/lang/String;[[DZF)[I").unwrap(),
            (vec![Int, Long, Reference, Reference, Int, Float], Some(Reference))
        );
        assert_eq!(method_kinds("(D)J").unwrap(), (vec![Double], Some(Long)));
        for bad in ["", "I", "(", "(L)V", "(Q)V", "()", "(I)Q"] {
            assert!(method_kinds(bad).is_err(), "{bad}");
        }
        assert_eq!(field_kind("[Ljava/lang/Object;").unwrap(), Reference);
        assert!(field_kind("V").is_err());
    }

    #[test]
    fn each_instruction_finds_its_stack() {
        // The test class has a handler at 1, which finds what was thrown there.
        // iconst_1; lconst_1; ...: the path from 0 comes to 1 with an int, the handler with a
        // reference.
        let flow = walk(&[0x04, 0x0a, 0x5d, 0x58, 0x57, 0x58, 0xb1], 52, &[]);
        assert!(flow.unwrap_err().0.contains("by one path"));
        // nop; pop: the path from 0 comes to the pop with nothing.
        let refused = walk(&[0x00, 0x57, 0xb1], 52, &[]).unwrap_err();
        assert!(refused.0.contains("empty"), "{refused}");
    }

    #[test]
    fn the_stack_is_as_deep_as_its_widest_values() {
        // aconst_null; pop (where the handler comes in, with its reference); iconst_1; lconst_1;
        // dup2_x1; pop2; pop; pop2; return.
        use Kind::{Int, Long, Reference};
        let code = [0x01, 0x57, 0x04, 0x0a, 0x5d, 0x58, 0x57, 0x58, 0xb1];
        let flow = walk(&code, 52, &[]).unwrap();
        let stacks: Vec<Vec<Kind>> = flow.stacks.iter().map(|s| s.clone().unwrap()).collect();
        assert_eq!(
            stacks,
            [vec![], vec![Reference], vec![], vec![Int], vec![Int, Long], vec![Long, Int, Long], vec![Long, Int], vec![Long], vec![],]
        );
        assert_eq!(flow.max_stack, 5);
    }

    #[test]
    fn a_subroutine_returns_after_its_jsr() {
        // 0: aconst_null; 1: pop; 2: jsr 6; 5: return; 6: astore_1; 7: ret 1
        let code = [0x01, 0x57, 0xa8, 0x00, 0x04, 0xb1, 0x4c, 0xa9, 0x01];
        let flow = walk(&code, 48, &[]).unwrap();
        assert_eq!(flow.stacks[4], Some(vec![Kind::ReturnAddress]));
        assert_eq!(flow.stacks[3], Some(vec![]));
        assert_eq!(flow.max_stack, 1);
    }

    #[test]
    fn what_cannot_run_is_refused() {
        // The handler's reference is popped at 1 in each.
        assert!(walk(&[0x01, 0x57, 0x60, 0xb1], 52, &[]).unwrap_err().0.contains("empty"));
        assert!(walk(&[0x01, 0x57, 0x0b, 0x04, 0x60, 0xb1], 52, &[]).unwrap_err().0.contains("Float"));
        assert!(walk(&[0x01, 0x57, 0x04], 52, &[]).unwrap_err().0.contains("runs off"));
        assert!(walk(&[0x01, 0x57, 0x0a, 0x57, 0xb1], 52, &[]).unwrap_err().0.contains("split"));
    }
}
