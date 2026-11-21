//! The lift of the Java machine: the methods of a set of classes as functions of the IR.
//!
//! A method is one function, named `Class.name:descriptor`, of the method's own parameters (the
//! receiver first, for an instance method) at the machine's kinds: `i32`, `i64`, `float`,
//! `double`, and `ptr` for a reference. The operand stack is gone: inside a block of the code each
//! slot is the value that was pushed, and where control joins, a slot is a variable of the
//! function the paths write and the join reads. A local variable is a variable too, one per kind
//! it holds.
//!
//! What the machine does beside computing is a call:
//!
//! * a method of a lifted class, called by name (`invokestatic`, `invokespecial`), is a call of
//!   its function; a method of a class that was not lifted is a call of a function of the same
//!   name the module declares, which the machine's runtime answers;
//! * a method found by the object's class (`invokevirtual`, `invokeinterface`) is a call through
//!   the pointer `jvm.virtual` finds;
//! * the rest of the machine (an object or an array made, an element found, a cast checked, a
//!   division, a throw) is a call of a `jvm.` function.
//!
//! An exception is not a second way out of a function: a throw leaves the exception pending in
//! the runtime and returns, and after every call that may have thrown the code asks whether one
//! is pending, takes it to the handler of its place in the code that catches its class, or
//! returns in its turn.
//!
//! An object is its class's number in its first eight bytes and, after them, the fields of the
//! lifted classes it is of, eight bytes each, the superclass's first. An array is the number, its
//! length at eight and its elements from sixteen. A class is a global, `class:Name`, that says its
//! name, its superclass, its interfaces, the bytes of its own fields and its methods by name, for
//! the runtime to find them; a static field is a global, `Class.name:descriptor`.
//!
//! A call site of `invokedynamic` is lifted by what its bootstrap method makes: a lambda is a
//! class of the module with the one method, which calls the lambda's body with what the site
//! captured; a string concatenation is the appends of a `StringBuilder`.

use std::collections::{BTreeMap, HashMap};

use ra_asm_ir::Operand as Ir;
use ra_asm_ir::{
    BinOp, BlockId, CallConv, Callee, Cast, Const, Declaration, FCond, FnType, FunctionBuilder, Global, ICond, Inst, Linkage, Module,
    Terminator, Type, ValueId,
};

use crate::class::{ClassFile, Code, Constant, Member};
use crate::decode::Operand;
use crate::flow::{Flow, Kind, field_kind, flow, method_kinds, widened};
use crate::{Error, error};

/// What the name of a class's global starts with.
pub const CLASS: &str = "class:";

/// The bytes of an object before its fields: its class's number.
pub const HEADER: u64 = 8;

/// Where an array's elements start: after the class's number and the length.
pub const ELEMENTS: u64 = 16;

/// The name of the function a method is, and of the global a static field is.
#[must_use]
pub fn symbol(class: &str, name: &str, descriptor: &str) -> String {
    format!("{class}.{name}:{descriptor}")
}

fn ty(kind: Kind) -> Type {
    match kind {
        Kind::Int | Kind::ReturnAddress => Type::I32,
        Kind::Long => Type::I64,
        Kind::Float => Type::Float,
        Kind::Double => Type::Double,
        Kind::Reference => Type::Ptr,
    }
}

fn zero(kind: Kind) -> Ir {
    match kind {
        Kind::Int | Kind::Long | Kind::ReturnAddress => Ir::Int(0),
        Kind::Float | Kind::Double => Ir::Float(0.0),
        Kind::Reference => Ir::Null,
    }
}

/// The type of the function a method of `descriptor` is.
///
/// # Errors
/// When `descriptor` is no method's.
pub fn signature(descriptor: &str, receiver: bool) -> Result<FnType, Error> {
    let (parameters, returned) = method_kinds(descriptor)?;
    let mut types = if receiver { vec![Type::Ptr] } else { Vec::new() };
    types.extend(parameters.into_iter().map(ty));
    Ok(FnType::new(returned.map_or(Type::Void, ty), types))
}

/// The descriptors of a method's parameters, and of what it returns.
fn split(descriptor: &str) -> Result<(Vec<&str>, &str), Error> {
    method_kinds(descriptor)?;
    let bytes = descriptor.as_bytes();
    let (mut at, mut parameters) = (1, Vec::new());
    while bytes[at] != b')' {
        let start = at;
        while bytes[at] == b'[' {
            at += 1;
        }
        at += if bytes[at] == b'L' { bytes[at..].iter().position(|&b| b == b';').unwrap_or(0) + 1 } else { 1 };
        parameters.push(&descriptor[start..at]);
    }
    Ok((parameters, &descriptor[at + 1..]))
}

fn name_and_type(file: &ClassFile, index: u16) -> Result<(&str, &str), Error> {
    if let Some(Constant::NameAndType { name, descriptor }) = file.constants.get(usize::from(index))
        && let (Some(name), Some(descriptor)) = (file.utf8(*name), file.utf8(*descriptor))
    {
        return Ok((name, descriptor));
    }
    error(format!("constant {index} is no name and type"))
}

/// The class, the name and the descriptor of the field or the method the constant at `index` is.
fn member(file: &ClassFile, index: u16) -> Result<(&str, &str, &str), Error> {
    let (Constant::Field { class, name_and_type: named }
    | Constant::Method { class, name_and_type: named }
    | Constant::InterfaceMethod { class, name_and_type: named }) = file.constants.get(usize::from(index)).unwrap_or(&Constant::Unusable)
    else {
        return error(format!("constant {index} is no field and no method"));
    };
    let (name, descriptor) = name_and_type(file, *named)?;
    match file.class_name(*class) {
        Some(class) => Ok((class, name, descriptor)),
        None => error(format!("constant {index} names no class")),
    }
}

fn class_at(file: &ClassFile, index: u16) -> Result<&str, Error> {
    file.class_name(index).ok_or_else(|| Error(format!("constant {index} is no class")))
}

fn text_at(file: &ClassFile, index: u16) -> Result<&str, Error> {
    file.utf8(index).ok_or_else(|| Error(format!("constant {index} is no text")))
}

/// How a call finds its method.
#[derive(Clone, Copy, PartialEq, Eq)]
enum How {
    /// By name: `invokestatic`, `invokespecial`.
    Named,
    /// By the receiver's class: `invokevirtual`, `invokeinterface`.
    Found,
}

/// Where a field is.
enum Place {
    /// The global of a static field.
    Global(String),
    /// The offset of an instance field in its object.
    Offset(u64),
}

/// Lifts `classes` into one module: a function for every method with code, a global for every
/// class and every static field.
///
/// # Errors
/// When a method's code does not decode or its stack does not walk, or it holds what the lift
/// does not do yet: a constant computed at run time, a call site whose bootstrap method is none
/// of the compiler's own, a lambda whose body takes its arguments at other kinds.
pub fn lift(classes: &[ClassFile]) -> Result<Module, Error> {
    let mut by_name = HashMap::new();
    for (n, file) in classes.iter().enumerate() {
        let name = file.name().ok_or_else(|| Error("a class with no name".to_owned()))?;
        by_name.insert(name, n);
    }
    let mut lift =
        Lift { classes, by_name, module: Module::x86_64_windows("java"), texts: HashMap::new(), strings: HashMap::new(), lambdas: 0 };
    for file in classes {
        lift.class(file)?;
    }
    Ok(lift.module)
}

struct Lift<'a> {
    classes: &'a [ClassFile],
    by_name: HashMap<&'a str, usize>,
    module: Module,
    /// The global of each text the code names: a class's name, a method's.
    texts: HashMap<String, String>,
    /// The global of each string constant, with its length in UTF-16 units.
    strings: HashMap<String, (String, u32)>,
    lambdas: usize,
}

impl<'a> Lift<'a> {
    /// The global that holds `text` and a zero after it.
    fn text(&mut self, text: &str) -> String {
        if let Some(name) = self.texts.get(text) {
            return name.clone();
        }
        let name = format!("text.{}", self.texts.len());
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0);
        self.module.globals.push(Global {
            name: name.clone(),
            ty: Type::array(bytes.len() as u64, Type::I8),
            init: Some(Const::Bytes(bytes)),
            constant: true,
            linkage: Linkage::Internal,
            align: 1,
            address: None,
        });
        self.texts.insert(text.to_owned(), name.clone());
        name
    }

    /// The global that holds the UTF-16 units of `text`, and how many they are.
    fn string(&mut self, text: &str) -> (String, u32) {
        if let Some(found) = self.strings.get(text) {
            return found.clone();
        }
        let units: Vec<u16> = text.encode_utf16().collect();
        let name = format!("string.{}", self.strings.len());
        let items: Vec<Const> = units.iter().map(|&unit| Const::Int(i128::from(unit))).chain([Const::Int(0)]).collect();
        self.module.globals.push(Global {
            name: name.clone(),
            ty: Type::array(items.len() as u64, Type::I16),
            init: Some(Const::Array(items)),
            constant: true,
            linkage: Linkage::Internal,
            align: 2,
            address: None,
        });
        let found = (name, units.len() as u32);
        self.strings.insert(text.to_owned(), found.clone());
        found
    }

    fn declare(&mut self, name: &str, ty: FnType) {
        self.module.declare(Declaration { name: name.to_owned(), ty, conv: CallConv::C, attrs: Vec::new() });
    }

    /// A call of the runtime's function `name`.
    fn runtime(&mut self, b: &mut FunctionBuilder, block: BlockId, name: &str, ret: Type, args: Vec<(Type, Ir)>) -> Option<ValueId> {
        let fn_ty = FnType::new(ret, args.iter().map(|(ty, _)| ty.clone()).collect());
        self.declare(name, fn_ty.clone());
        b.call(block, fn_ty, Callee::Direct(name.to_owned()), args, name.strip_prefix("jvm.").unwrap_or(name))
    }

    /// The function a call by name of `class`'s method reaches: the method of the class or of
    /// the nearest superclass that has it, and whether it is a function of the module.
    fn resolve(&self, class: &str, name: &str, descriptor: &str) -> (String, bool) {
        let mut at = class;
        while let Some(&n) = self.by_name.get(at) {
            let file = &self.classes[n];
            let found = file.methods.iter().find(|m| file.utf8(m.name) == Some(name) && file.utf8(m.descriptor) == Some(descriptor));
            if let Some(method) = found {
                return (symbol(at, name, descriptor), file.attribute(&method.attributes, "Code").is_some());
            }
            match file.class_name(file.super_class) {
                Some(above) => at = above,
                None => break,
            }
        }
        (symbol(at, name, descriptor), false)
    }

    /// The instance fields of the lifted classes from `class` up.
    fn instance_fields(&self, class: &str) -> u64 {
        let (mut at, mut count) = (class, 0);
        while let Some(&n) = self.by_name.get(at) {
            let file = &self.classes[n];
            count += file.fields.iter().filter(|f| f.access & 0x0008 == 0).count() as u64;
            match file.class_name(file.super_class) {
                Some(above) => at = above,
                None => break,
            }
        }
        count
    }

    fn field(&mut self, class: &str, name: &str, descriptor: &str, is_static: bool) -> Result<Place, Error> {
        let mut at = class;
        while let Some(&n) = self.by_name.get(at) {
            let file = &self.classes[n];
            let is = |f: &&Member| (f.access & 0x0008 != 0) == is_static;
            let found =
                file.fields.iter().filter(is).position(|f| file.utf8(f.name) == Some(name) && file.utf8(f.descriptor) == Some(descriptor));
            let above = file.class_name(file.super_class);
            if let Some(index) = found {
                if is_static {
                    return Ok(Place::Global(symbol(at, name, descriptor)));
                }
                let before = above.map_or(0, |above| self.instance_fields(above));
                return Ok(Place::Offset(HEADER + 8 * (before + index as u64)));
            }
            match above {
                Some(above) => at = above,
                None => break,
            }
        }
        if !is_static {
            return error(format!("the field {name} of {class}, a class that was not lifted"));
        }
        // A static field of a class that was not lifted is the runtime's to fill.
        let global = symbol(class, name, descriptor);
        if self.module.global(&global).is_none() {
            self.module.globals.push(Global {
                name: global.clone(),
                ty: ty(field_kind(descriptor)?),
                init: None,
                constant: false,
                linkage: Linkage::External,
                align: 8,
                address: None,
            });
        }
        Ok(Place::Global(global))
    }

    /// A call of the method `name` of `class` with `args`, the receiver first: what it returns.
    #[allow(clippy::too_many_arguments)]
    fn call(
        &mut self,
        b: &mut FunctionBuilder,
        block: BlockId,
        how: How,
        receiver: bool,
        class: &str,
        name: &str,
        descriptor: &str,
        args: Vec<Ir>,
    ) -> Result<Option<ValueId>, Error> {
        let fn_ty = signature(descriptor, receiver)?;
        let typed: Vec<(Type, Ir)> = fn_ty.params.iter().cloned().zip(args).collect();
        let callee = match how {
            How::Named => {
                let (function, lifted) = self.resolve(class, name, descriptor);
                if !lifted {
                    self.declare(&function, fn_ty.clone());
                }
                Callee::Direct(function)
            }
            How::Found => {
                let named = self.text(&format!("{name}:{descriptor}"));
                let found = self.runtime(b, block, "jvm.virtual", Type::Ptr, vec![typed[0].clone(), (Type::Ptr, Ir::Global(named))]);
                Callee::Indirect(found.expect("jvm.virtual returns a pointer").into())
            }
        };
        Ok(b.call(block, fn_ty, callee, typed, name.trim_matches(['<', '>'])))
    }

    /// The global of a class: its name, its superclass's, the bytes of its own fields, its
    /// interfaces' names, and its methods as pairs of `name:descriptor` and function.
    fn class_global(&mut self, name: &str, above: Option<&str>, interfaces: &[&str], fields: u64, methods: &[(String, String)]) {
        let pointer = |global: String| Const::Global(global);
        let name_text = self.text(name);
        let above = above.map_or(Const::Null, |above| pointer(self.text(above)));
        let interfaces_global = if interfaces.is_empty() {
            Const::Null
        } else {
            let items = interfaces.iter().map(|interface| pointer(self.text(interface))).collect::<Vec<_>>();
            let global = format!("interfaces:{name}");
            self.module.globals.push(Global {
                name: global.clone(),
                ty: Type::array(items.len() as u64, Type::Ptr),
                init: Some(Const::Array(items)),
                constant: true,
                linkage: Linkage::Internal,
                align: 8,
                address: None,
            });
            pointer(global)
        };
        let methods_global = if methods.is_empty() {
            Const::Null
        } else {
            let items = methods
                .iter()
                .map(|(named, function)| Const::Struct(vec![pointer(self.text(named)), pointer(function.clone())]))
                .collect::<Vec<_>>();
            let global = format!("methods:{name}");
            self.module.globals.push(Global {
                name: global.clone(),
                ty: Type::array(items.len() as u64, Type::Struct(vec![Type::Ptr, Type::Ptr])),
                init: Some(Const::Array(items)),
                constant: true,
                linkage: Linkage::Internal,
                align: 8,
                address: None,
            });
            pointer(global)
        };
        self.module.globals.push(Global {
            name: format!("{CLASS}{name}"),
            ty: Type::Struct(vec![Type::Ptr, Type::Ptr, Type::I64, Type::I64, Type::Ptr, Type::I64, Type::Ptr]),
            init: Some(Const::Struct(vec![
                pointer(name_text),
                above,
                Const::Int(i128::from(fields)),
                Const::Int(interfaces.len() as i128),
                interfaces_global,
                Const::Int(methods.len() as i128),
                methods_global,
            ])),
            constant: true,
            linkage: Linkage::External,
            align: 8,
            address: None,
        });
    }

    fn class(&mut self, file: &'a ClassFile) -> Result<(), Error> {
        let name = file.name().unwrap_or_default();
        let mut own = 0;
        for field in &file.fields {
            let (field_name, descriptor) = (text_at(file, field.name)?, text_at(file, field.descriptor)?);
            let kind = field_kind(descriptor)?;
            if field.access & 0x0008 == 0 {
                own += 8;
                continue;
            }
            // A constant field starts as its constant; a string's is the runtime's to make.
            let constant = file.attribute(&field.attributes, "ConstantValue").and_then(|a| {
                let index = u16::from_be_bytes([*a.info.first()?, *a.info.get(1)?]);
                match file.constants.get(usize::from(index))? {
                    Constant::Integer(v) => Some(Const::Int(i128::from(*v))),
                    Constant::Long(v) => Some(Const::Int(i128::from(*v))),
                    Constant::Float(v) => Some(Const::Float(f64::from(*v))),
                    Constant::Double(v) => Some(Const::Float(*v)),
                    _ => None,
                }
            });
            self.module.globals.push(Global {
                name: symbol(name, field_name, descriptor),
                ty: ty(kind),
                init: Some(constant.unwrap_or(Const::Zero)),
                constant: false,
                linkage: Linkage::External,
                align: 8,
                address: None,
            });
        }
        let mut methods = Vec::new();
        for method in &file.methods {
            let (method_name, descriptor) = (text_at(file, method.name)?, text_at(file, method.descriptor)?);
            let Some(code) = file.code(method)? else { continue };
            let is_static = method.access & 0x0008 != 0;
            let function = symbol(name, method_name, descriptor);
            Body::lift(self, file, &function, descriptor, is_static, &code).map_err(|e| Error(format!("{function}: {e}")))?;
            if !is_static && !method_name.starts_with('<') {
                methods.push((format!("{method_name}:{descriptor}"), function));
            }
        }
        let interfaces = file.interfaces.iter().map(|&index| class_at(file, index)).collect::<Result<Vec<_>, _>>()?;
        self.class_global(name, file.class_name(file.super_class), &interfaces, own, &methods);
        Ok(())
    }

    /// The class a lambda's call site makes an object of: one method, the interface's, that
    /// calls the lambda's body with what the site captured, which the object holds as fields.
    /// The class's name.
    fn lambda(&mut self, file: &ClassFile, name: &str, descriptor: &str, arguments: &[u16]) -> Result<String, Error> {
        let (captures, interface) = split(descriptor)?;
        let interface = interface.strip_prefix('L').and_then(|i| i.strip_suffix(';')).unwrap_or(interface);
        let (Some(Constant::MethodType(erased)), Some(Constant::MethodHandle { kind, reference })) = (
            arguments.first().and_then(|&a| file.constants.get(usize::from(a))),
            arguments.get(1).and_then(|&a| file.constants.get(usize::from(a))),
        ) else {
            return error("a lambda's call site without its method type and its body");
        };
        let erased = text_at(file, *erased)?;
        let (class, body, body_descriptor) = member(file, *reference)?;
        self.lambdas += 1;
        let made = format!("{}$$Lambda${}", file.name().unwrap_or_default(), self.lambdas);
        let function = symbol(&made, name, erased);
        let mut b = FunctionBuilder::new(&function, signature(erased, true)?, &["this"]);
        let block = b.block("entry");
        let this = b.param(0);
        let mut values: Vec<(Ir, Kind)> = Vec::new();
        for (n, capture) in captures.iter().enumerate() {
            let kind = field_kind(capture)?;
            let at = b.gep(block, Type::I8, this, vec![(Type::I64, Ir::Int((HEADER + 8 * n as u64).into()))], "captured.at");
            values.push((b.load(block, ty(kind), at, "captured").into(), kind));
        }
        let (parameters, returned) = method_kinds(erased)?;
        for (n, kind) in parameters.into_iter().enumerate() {
            values.push((b.param(n + 1).into(), kind));
        }
        let (takes, gives) = method_kinds(body_descriptor)?;
        // A handle of a constructor makes the object the constructor gets.
        let (receiver, gives) = match kind {
            6 => (false, gives),
            5 | 7 | 9 => (true, gives),
            8 => {
                let named = self.text(class);
                let object = self.runtime(&mut b, block, "jvm.new", Type::Ptr, vec![(Type::Ptr, Ir::Global(named))]);
                values.insert(0, (object.expect("jvm.new returns a pointer").into(), Kind::Reference));
                (true, Some(Kind::Reference))
            }
            _ => return error(format!("a lambda whose body is a method handle of kind {kind}")),
        };
        let expected: Vec<Kind> = receiver.then_some(Kind::Reference).into_iter().chain(takes).collect();
        if values.iter().map(|(_, kind)| *kind).ne(expected.iter().copied()) {
            return error(format!("the lambda {name} of {interface} takes {body_descriptor} at other kinds than {erased} gives it"));
        }
        let object = values.first().map(|(value, _)| value.clone());
        let how = if matches!(kind, 5 | 9) { How::Found } else { How::Named };
        let result = self.call(&mut b, block, how, receiver, class, body, body_descriptor, values.into_iter().map(|(v, _)| v).collect())?;
        let result: Option<Ir> = if *kind == 8 { object } else { result.map(Ir::from) };
        let ret = match (returned, gives, result) {
            (None, _, _) => None,
            (Some(wanted), Some(given), Some(value)) if wanted == given => Some((ty(wanted), value)),
            _ => return error(format!("the lambda {name} of {interface} returns {body_descriptor} at another kind than {erased} wants")),
        };
        b.terminate(block, Terminator::Ret(ret));
        self.module.functions.push(b.finish());
        self.class_global(
            &made,
            Some("java/lang/Object"),
            &[interface],
            8 * captures.len() as u64,
            &[(format!("{name}:{erased}"), function)],
        );
        Ok(made)
    }
}

/// One method's code on its way to a function.
struct Body<'l, 'a> {
    lift: &'l mut Lift<'a>,
    file: &'a ClassFile,
    code: &'l Code,
    flow: &'l Flow,
    b: FunctionBuilder,
    entry: BlockId,
    /// The block the code is being written to.
    cur: BlockId,
    /// Whether control comes to the next instruction from the one before it.
    live: bool,
    /// The operand stack: each slot's value and its kind.
    stack: Vec<(Ir, Kind)>,
    /// The variable of each local variable, by the kind it holds.
    locals: HashMap<(u16, Kind), ValueId>,
    /// The variable of each slot of the stack where control joins, by its place and its kind.
    slots: HashMap<(usize, Kind), ValueId>,
    /// The block of each instruction control jumps to, by its offset.
    blocks: BTreeMap<u32, BlockId>,
    /// The block that takes a pending exception to its handler, by the handlers of the place.
    dispatches: HashMap<Vec<usize>, BlockId>,
    returned: Option<Kind>,
    /// Where each `jsr` of the code returns to; a return address is a number of this list.
    subroutines: Vec<u32>,
}

impl<'a> Body<'_, 'a> {
    fn lift(lift: &mut Lift<'a>, file: &'a ClassFile, function: &str, descriptor: &str, is_static: bool, code: &Code) -> Result<(), Error> {
        let walked = flow(file, code)?;
        let (parameters, returned) = method_kinds(descriptor)?;
        let mut b = FunctionBuilder::new(function, signature(descriptor, !is_static)?, if is_static { &[] } else { &["this"] });
        let entry = b.block("entry");
        let mut body = Body {
            lift,
            file,
            code,
            flow: &walked,
            b,
            entry,
            cur: entry,
            live: false,
            stack: Vec::new(),
            locals: HashMap::new(),
            slots: HashMap::new(),
            blocks: BTreeMap::new(),
            dispatches: HashMap::new(),
            returned,
            subroutines: Vec::new(),
        };
        // The parameters are the first local variables.
        let kinds = (!is_static).then_some(Kind::Reference).into_iter().chain(parameters);
        let mut index = 0;
        for (n, kind) in kinds.enumerate() {
            let variable = body.local(index, kind);
            let parameter = body.b.param(n);
            body.b.store(entry, ty(kind), parameter, variable);
            index += kind.slots();
        }
        body.leaders();
        if let Some(&first) = body.blocks.get(&0) {
            body.b.terminate(entry, Terminator::Br(first));
        }
        for n in 0..walked.instructions.len() {
            let Some(stack) = &walked.stacks[n] else { continue };
            let offset = walked.instructions[n].offset;
            if let Some(&block) = body.blocks.get(&offset) {
                if body.live {
                    body.spill();
                    body.b.terminate(body.cur, Terminator::Br(block));
                }
                body.cur = block;
                body.reload(stack);
                body.live = true;
            } else if !body.live {
                return error(format!("control reaches {offset} from nowhere the lift knows"));
            }
            body.b.address = Some(u64::from(offset));
            body.instruction(n).map_err(|e| Error(format!("`{}` at {offset}: {e}", walked.instructions[n].name)))?;
        }
        body.lift.module.functions.push(body.b.finish());
        Ok(())
    }

    /// Makes a block for every instruction control jumps to: a branch's target, a handler, and
    /// the instruction after one that branches or ends its block.
    fn leaders(&mut self) {
        let flow = self.flow;
        let mut offsets: Vec<u32> = vec![0];
        offsets.extend(self.code.handlers.iter().map(|h| u32::from(h.handler)));
        for (n, instruction) in flow.instructions.iter().enumerate() {
            let targets = instruction.targets();
            if (!targets.is_empty() || instruction.ends_block())
                && let Some(next) = flow.instructions.get(n + 1)
            {
                offsets.push(next.offset);
                if matches!(instruction.opcode, 0xa8 | 0xc9) {
                    self.subroutines.push(next.offset);
                }
            }
            offsets.extend(targets);
        }
        offsets.sort_unstable();
        offsets.dedup();
        let reached: std::collections::HashSet<u32> =
            flow.instructions.iter().zip(&flow.stacks).filter(|(_, stack)| stack.is_some()).map(|(i, _)| i.offset).collect();
        for offset in offsets {
            if reached.contains(&offset) {
                self.b.address = Some(u64::from(offset));
                let block = self.b.block(&format!("at.{offset}"));
                self.blocks.insert(offset, block);
            }
        }
    }

    fn block_at(&self, offset: u32) -> Result<BlockId, Error> {
        self.blocks.get(&offset).copied().ok_or_else(|| Error(format!("no block at {offset}")))
    }

    fn local(&mut self, index: u16, kind: Kind) -> ValueId {
        if let Some(&variable) = self.locals.get(&(index, kind)) {
            return variable;
        }
        let variable = self.b.alloca(self.entry, ty(kind), &format!("local.{index}"));
        self.locals.insert((index, kind), variable);
        variable
    }

    fn slot(&mut self, place: usize, kind: Kind) -> ValueId {
        if let Some(&variable) = self.slots.get(&(place, kind)) {
            return variable;
        }
        let variable = self.b.alloca(self.entry, ty(kind), &format!("stack.{place}"));
        self.slots.insert((place, kind), variable);
        variable
    }

    /// Writes the stack to its slots' variables, for the block control goes to.
    fn spill(&mut self) {
        for (place, (value, kind)) in self.stack.clone().into_iter().enumerate() {
            let variable = self.slot(place, kind);
            self.b.store(self.cur, ty(kind), value, variable);
        }
    }

    /// Reads the stack a block starts with from its slots' variables.
    fn reload(&mut self, kinds: &[Kind]) {
        self.stack.clear();
        for (place, &kind) in kinds.iter().enumerate() {
            let variable = self.slot(place, kind);
            let value = self.b.load(self.cur, ty(kind), variable, "top");
            self.stack.push((value.into(), kind));
        }
    }

    fn push(&mut self, value: impl Into<Ir>, kind: Kind) {
        self.stack.push((value.into(), kind));
    }

    fn pop(&mut self) -> Result<Ir, Error> {
        self.stack.pop().map(|(value, _)| value).ok_or_else(|| Error("the stack is empty".to_owned()))
    }

    /// The values on top that take `slots` slots, bottom first.
    fn take(&mut self, slots: u16) -> Result<Vec<(Ir, Kind)>, Error> {
        let (mut taken, mut have) = (Vec::new(), 0);
        while have < slots {
            let Some(value) = self.stack.pop() else { return error("the stack is empty") };
            have += value.1.slots();
            taken.insert(0, value);
        }
        Ok(taken)
    }

    fn dup(&mut self, top: u16, under: u16) -> Result<(), Error> {
        let top = self.take(top)?;
        let under = self.take(under)?;
        self.stack.extend(top.iter().cloned().chain(under).chain(top.iter().cloned()));
        Ok(())
    }

    fn rt(&mut self, name: &str, ret: Type, args: Vec<(Type, Ir)>) -> Ir {
        self.lift.runtime(&mut self.b, self.cur, name, ret, args).map_or(Ir::Undef, Ir::from)
    }

    fn text(&mut self, text: &str) -> (Type, Ir) {
        (Type::Ptr, Ir::Global(self.lift.text(text)))
    }

    fn goto(&mut self, offset: u32) -> Result<(), Error> {
        let block = self.block_at(offset)?;
        self.spill();
        self.b.terminate(self.cur, Terminator::Br(block));
        self.live = false;
        Ok(())
    }

    fn branch(&mut self, cond: ValueId, target: u32, next: Option<u32>) -> Result<(), Error> {
        let then = self.block_at(target)?;
        let els = self.block_at(next.ok_or_else(|| Error("a branch ends the code".to_owned()))?)?;
        self.spill();
        self.b.terminate(self.cur, Terminator::CondBr { cond: cond.into(), then, els });
        self.live = false;
        Ok(())
    }

    /// The block that takes the pending exception to the handler of the instruction at `offset`
    /// that catches its class, or returns with it still pending.
    fn dispatch(&mut self, offset: u32) -> Result<BlockId, Error> {
        let handlers: Vec<usize> = (0..self.code.handlers.len())
            .filter(|&n| (u32::from(self.code.handlers[n].start)..u32::from(self.code.handlers[n].end)).contains(&offset))
            .collect();
        if let Some(&block) = self.dispatches.get(&handlers) {
            return Ok(block);
        }
        let first = self.b.block(if handlers.is_empty() { "unwind" } else { "thrown" });
        let mut at = first;
        for &n in &handlers {
            let handler = self.code.handlers[n];
            let caught = self.b.block("caught");
            if handler.catch_type == 0 {
                self.b.terminate(at, Terminator::Br(caught));
            } else {
                let class = self.lift.text(class_at(self.file, handler.catch_type)?);
                let pending = self.lift.runtime(&mut self.b, at, "jvm.pending", Type::Ptr, Vec::new()).expect("a pointer");
                let is = self
                    .lift
                    .runtime(
                        &mut self.b,
                        at,
                        "jvm.instanceof",
                        Type::I32,
                        vec![(Type::Ptr, pending.into()), (Type::Ptr, Ir::Global(class))],
                    )
                    .expect("an int");
                let is = self.b.icmp(at, ICond::Ne, Type::I32, is, 0, "is");
                let next = self.b.block("thrown");
                self.b.terminate(at, Terminator::CondBr { cond: is.into(), then: caught, els: next });
                at = next;
            }
            // The handler finds what was thrown, alone on the stack.
            let thrown = self.lift.runtime(&mut self.b, caught, "jvm.caught", Type::Ptr, Vec::new()).expect("a pointer");
            let variable = self.slot(0, Kind::Reference);
            self.b.store(caught, Type::Ptr, thrown, variable);
            let target = self.block_at(u32::from(handler.handler))?;
            self.b.terminate(caught, Terminator::Br(target));
            if handler.catch_type == 0 {
                self.dispatches.insert(handlers, first);
                return Ok(first);
            }
        }
        self.b.terminate(at, Terminator::Ret(self.returned.map(|kind| (ty(kind), zero(kind)))));
        self.dispatches.insert(handlers, first);
        Ok(first)
    }

    /// After a call that may have thrown: on with the code when nothing is pending.
    fn check(&mut self, offset: u32) -> Result<(), Error> {
        let pending = self.rt("jvm.pending", Type::Ptr, Vec::new());
        let none = self.b.icmp(self.cur, ICond::Eq, Type::Ptr, pending, Ir::Null, "none");
        let on = self.b.block("on");
        let thrown = self.dispatch(offset)?;
        self.b.terminate(self.cur, Terminator::CondBr { cond: none.into(), then: on, els: thrown });
        self.cur = on;
        Ok(())
    }

    /// A use of the object `reference`: a null one throws.
    fn null_check(&mut self, reference: &Ir, offset: u32) -> Result<(), Error> {
        let null = self.b.icmp(self.cur, ICond::Eq, Type::Ptr, reference.clone(), Ir::Null, "null");
        let (on, npe) = (self.b.block("on"), self.b.block("null"));
        self.b.terminate(self.cur, Terminator::CondBr { cond: null.into(), then: npe, els: on });
        self.lift.runtime(&mut self.b, npe, "jvm.null", Type::Void, Vec::new());
        let thrown = self.dispatch(offset)?;
        self.b.terminate(npe, Terminator::Br(thrown));
        self.cur = on;
        Ok(())
    }

    fn bin(&mut self, op: BinOp, kind: Kind) -> Result<(), Error> {
        let (rhs, lhs) = (self.pop()?, self.pop()?);
        let value = self.b.bin(self.cur, op, ty(kind), lhs, rhs, "v");
        self.push(value, kind);
        Ok(())
    }

    fn cast(&mut self, cast: Cast, from: Kind, to: Kind) -> Result<(), Error> {
        let value = self.pop()?;
        let value = self.b.cast(self.cur, cast, ty(from), value, ty(to), "v");
        self.push(value, to);
        Ok(())
    }

    /// An `int` narrowed to `bits` and widened again, by its sign or without.
    fn narrow(&mut self, bits: u16, signed: bool) -> Result<(), Error> {
        let value = self.pop()?;
        let narrow = self.b.cast(self.cur, Cast::Trunc, Type::I32, value, Type::Int(bits), "narrow");
        let value = self.b.cast(self.cur, if signed { Cast::SExt } else { Cast::ZExt }, Type::Int(bits), narrow, Type::I32, "v");
        self.push(value, Kind::Int);
        Ok(())
    }

    /// The three-way comparison of two floats: -1, 0 or 1, and `unordered` when one is no number.
    fn compare(&mut self, kind: Kind, unordered: i32) -> Result<(), Error> {
        let (rhs, lhs) = (self.pop()?, self.pop()?);
        let is =
            |body: &mut Self, cond| body.b.value(body.cur, Inst::FCmp { cond, ty: ty(kind), lhs: lhs.clone(), rhs: rhs.clone() }, "is");
        let (less, equal, more) = (is(self, FCond::Olt), is(self, FCond::Oeq), is(self, FCond::Ogt));
        let value = self.b.select(self.cur, more, Type::I32, 1, unordered, "v");
        let value = self.b.select(self.cur, equal, Type::I32, 0, value, "v");
        let value = self.b.select(self.cur, less, Type::I32, -1, value, "v");
        self.push(value, Kind::Int);
        Ok(())
    }

    /// The address of the element `index` of `array`, of `size` bytes: a null array or an index
    /// outside it throws.
    fn element(&mut self, array: Ir, index: Ir, size: u64, offset: u32) -> Result<Ir, Error> {
        let at = self.rt("jvm.element", Type::Ptr, vec![(Type::Ptr, array), (Type::I32, index), (Type::I32, Ir::Int(size.into()))]);
        self.check(offset)?;
        Ok(at)
    }

    fn field_at(&mut self, object: Ir, offset: u64) -> ValueId {
        self.b.gep(self.cur, Type::I8, object, vec![(Type::I64, Ir::Int(offset.into()))], "field")
    }

    #[allow(clippy::too_many_lines)]
    fn instruction(&mut self, n: usize) -> Result<(), Error> {
        use Kind::{Double, Float, Int, Long, Reference, ReturnAddress};
        const KINDS: [Kind; 5] = [Int, Long, Float, Double, Reference];
        // The element of each array instruction: its type, its bytes and its kind on the stack.
        const ELEMENT: [(Type, u64, Kind); 8] = [
            (Type::I32, 4, Int),
            (Type::I64, 8, Long),
            (Type::Float, 4, Float),
            (Type::Double, 8, Double),
            (Type::Ptr, 8, Reference),
            (Type::I8, 1, Int),
            (Type::I16, 2, Int),
            (Type::I16, 2, Int),
        ];
        let flow = self.flow;
        let instruction = &flow.instructions[n];
        let offset = instruction.offset;
        let next = flow.instructions.get(n + 1).map(|i| i.offset);
        let opcode = if instruction.opcode == 0xc4 { widened(instruction.name) } else { instruction.opcode };
        let constant = match instruction.operand {
            Operand::Constant(index) | Operand::InvokeInterface { index, .. } | Operand::MultiANewArray { index, .. } => index,
            _ => 0,
        };
        let operand_local = match instruction.operand {
            Operand::Local(index) => index,
            _ => 0,
        };
        let file = self.file;
        match opcode {
            0x00 => {}
            0x01 => self.push(Ir::Null, Reference),
            0x02..=0x08 => self.push(Ir::Int(i128::from(opcode) - 3), Int),
            0x09 | 0x0a => self.push(Ir::Int(i128::from(opcode) - 9), Long),
            0x0b..=0x0d => self.push(Ir::Float(f64::from(opcode - 0x0b)), Float),
            0x0e | 0x0f => self.push(Ir::Float(f64::from(opcode - 0x0e)), Double),
            0x10 | 0x11 => {
                let Operand::Int(value) = instruction.operand else { return error("no constant") };
                self.push(Ir::Int(value.into()), Int);
            }
            0x12..=0x14 => match file.constants.get(usize::from(constant)) {
                Some(Constant::Integer(v)) => self.push(Ir::Int((*v).into()), Int),
                Some(Constant::Long(v)) => self.push(Ir::Int((*v).into()), Long),
                Some(Constant::Float(v)) => self.push(Ir::Float(f64::from(*v)), Float),
                Some(Constant::Double(v)) => self.push(Ir::Float(*v), Double),
                Some(Constant::String(text)) => {
                    let (global, units) = self.lift.string(text_at(file, *text)?);
                    let string =
                        self.rt("jvm.string", Type::Ptr, vec![(Type::Ptr, Ir::Global(global)), (Type::I32, Ir::Int(units.into()))]);
                    self.push(string, Reference);
                }
                Some(Constant::Class(name)) => {
                    let name = self.text(text_at(file, *name)?);
                    let class = self.rt("jvm.class", Type::Ptr, vec![name]);
                    self.push(class, Reference);
                }
                _ => return error(format!("constant {constant} is one the lift does not load yet")),
            },
            0x15..=0x2d => {
                let (kind, index) = if opcode <= 0x19 {
                    (KINDS[usize::from(opcode - 0x15)], operand_local)
                } else {
                    (KINDS[usize::from(opcode - 0x1a) / 4], u16::from(opcode - 0x1a) % 4)
                };
                let variable = self.local(index, kind);
                let value = self.b.load(self.cur, ty(kind), variable, &format!("local.{index}.v"));
                self.push(value, kind);
            }
            0x2e..=0x35 => {
                let (element, size, kind) = ELEMENT[usize::from(opcode - 0x2e)].clone();
                let (index, array) = (self.pop()?, self.pop()?);
                let at = self.element(array, index, size, offset)?;
                let mut value = self.b.load(self.cur, element.clone(), at, "element");
                if element != ty(kind) {
                    // A `char` is unsigned; a `byte` and a `short` carry their sign.
                    let cast = if opcode == 0x34 { Cast::ZExt } else { Cast::SExt };
                    value = self.b.cast(self.cur, cast, element, value, Type::I32, "element");
                }
                self.push(value, kind);
            }
            0x36..=0x4e => {
                let index = if opcode <= 0x3a { operand_local } else { u16::from(opcode - 0x3b) % 4 };
                let Some((value, kind)) = self.stack.pop() else { return error("the stack is empty") };
                let variable = self.local(index, kind);
                self.b.store(self.cur, ty(kind), value, variable);
            }
            0x4f..=0x56 => {
                let (element, size, kind) = ELEMENT[usize::from(opcode - 0x4f)].clone();
                let (mut value, index, array) = (self.pop()?, self.pop()?, self.pop()?);
                let at = self.element(array, index, size, offset)?;
                if element != ty(kind) {
                    value = self.b.cast(self.cur, Cast::Trunc, Type::I32, value, element.clone(), "element").into();
                }
                self.b.store(self.cur, element, value, at);
            }
            0x57 => drop(self.take(1)?),
            0x58 => drop(self.take(2)?),
            0x59 => self.dup(1, 0)?,
            0x5a => self.dup(1, 1)?,
            0x5b => self.dup(1, 2)?,
            0x5c => self.dup(2, 0)?,
            0x5d => self.dup(2, 1)?,
            0x5e => self.dup(2, 2)?,
            0x5f => {
                let (top, under) = (self.take(1)?, self.take(1)?);
                self.stack.extend(top.into_iter().chain(under));
            }
            0x60..=0x73 => {
                let (group, kind) = (usize::from(opcode - 0x60) / 4, KINDS[usize::from(opcode - 0x60) % 4]);
                let real = matches!(kind, Float | Double);
                if group >= 3 && !real {
                    // A division by zero throws, and the least number by minus one is itself.
                    let name = ["jvm.idiv", "jvm.ldiv", "", "", "jvm.irem", "jvm.lrem"][(group - 3) * 4 + usize::from(kind == Long)];
                    let (rhs, lhs) = (self.pop()?, self.pop()?);
                    let value = self.rt(name, ty(kind), vec![(ty(kind), lhs), (ty(kind), rhs)]);
                    self.push(value, kind);
                    self.check(offset)?;
                } else {
                    let ops = if real {
                        [BinOp::FAdd, BinOp::FSub, BinOp::FMul, BinOp::FDiv, BinOp::FRem]
                    } else {
                        [BinOp::Add, BinOp::Sub, BinOp::Mul, BinOp::SDiv, BinOp::SRem]
                    };
                    self.bin(ops[group], kind)?;
                }
            }
            0x74..=0x77 => {
                let kind = KINDS[usize::from(opcode - 0x74)];
                let value = self.pop()?;
                let value = if matches!(kind, Float | Double) {
                    self.b.bin(self.cur, BinOp::FSub, ty(kind), Ir::Float(-0.0), value, "v")
                } else {
                    self.b.bin(self.cur, BinOp::Sub, ty(kind), 0, value, "v")
                };
                self.push(value, kind);
            }
            0x78..=0x7d => {
                // A shift counts by the low bits of its count: five for an int, six for a long.
                let op = [BinOp::Shl, BinOp::AShr, BinOp::LShr][usize::from(opcode - 0x78) / 2];
                let kind = if opcode & 1 == 0 { Int } else { Long };
                let (count, value) = (self.pop()?, self.pop()?);
                let mut count = self.b.bin(self.cur, BinOp::And, Type::I32, count, if kind == Int { 31 } else { 63 }, "count");
                if kind == Long {
                    count = self.b.cast(self.cur, Cast::ZExt, Type::I32, count, Type::I64, "count");
                }
                let value = self.b.bin(self.cur, op, ty(kind), value, count, "v");
                self.push(value, kind);
            }
            0x7e..=0x83 => {
                let op = [BinOp::And, BinOp::Or, BinOp::Xor][usize::from(opcode - 0x7e) / 2];
                self.bin(op, if opcode & 1 == 0 { Int } else { Long })?;
            }
            0x84 => {
                let Operand::Iinc { local, by } = instruction.operand else { return error("no increment") };
                let variable = self.local(local, Int);
                let value = self.b.load(self.cur, Type::I32, variable, "v");
                let value = self.b.bin(self.cur, BinOp::Add, Type::I32, value, i64::from(by), "v");
                self.b.store(self.cur, Type::I32, value, variable);
            }
            0x85 => self.cast(Cast::SExt, Int, Long)?,
            0x86 => self.cast(Cast::SIToFP, Int, Float)?,
            0x87 => self.cast(Cast::SIToFP, Int, Double)?,
            0x88 => self.cast(Cast::Trunc, Long, Int)?,
            0x89 => self.cast(Cast::SIToFP, Long, Float)?,
            0x8a => self.cast(Cast::SIToFP, Long, Double)?,
            0x8d => self.cast(Cast::FPExt, Float, Double)?,
            0x90 => self.cast(Cast::FPTrunc, Double, Float)?,
            0x8b | 0x8c | 0x8e | 0x8f => {
                // A float that is no number is zero, and one out of range the nearest in it.
                let (name, from, to) = match opcode {
                    0x8b => ("jvm.f2i", Float, Int),
                    0x8c => ("jvm.f2l", Float, Long),
                    0x8e => ("jvm.d2i", Double, Int),
                    _ => ("jvm.d2l", Double, Long),
                };
                let value = self.pop()?;
                let value = self.rt(name, ty(to), vec![(ty(from), value)]);
                self.push(value, to);
            }
            0x91 => self.narrow(8, true)?,
            0x92 => self.narrow(16, false)?,
            0x93 => self.narrow(16, true)?,
            0x94 => {
                let (rhs, lhs) = (self.pop()?, self.pop()?);
                let less = self.b.icmp(self.cur, ICond::Slt, Type::I64, lhs.clone(), rhs.clone(), "less");
                let more = self.b.icmp(self.cur, ICond::Sgt, Type::I64, lhs, rhs, "more");
                let value = self.b.select(self.cur, more, Type::I32, 1, 0, "v");
                let value = self.b.select(self.cur, less, Type::I32, -1, value, "v");
                self.push(value, Int);
            }
            0x95 => self.compare(Float, -1)?,
            0x96 => self.compare(Float, 1)?,
            0x97 => self.compare(Double, -1)?,
            0x98 => self.compare(Double, 1)?,
            0x99..=0xa6 | 0xc6 | 0xc7 => {
                const CONDS: [ICond; 6] = [ICond::Eq, ICond::Ne, ICond::Slt, ICond::Sge, ICond::Sgt, ICond::Sle];
                let Operand::Branch(target) = instruction.operand else { return error("no target") };
                let (cond, kind, rhs) = match opcode {
                    0x99..=0x9e => (CONDS[usize::from(opcode - 0x99)], Type::I32, Ir::Int(0)),
                    0x9f..=0xa4 => (CONDS[usize::from(opcode - 0x9f)], Type::I32, self.pop()?),
                    0xa5 | 0xa6 => (CONDS[usize::from(opcode - 0xa5)], Type::Ptr, self.pop()?),
                    _ => (CONDS[usize::from(opcode - 0xc6)], Type::Ptr, Ir::Null),
                };
                let lhs = self.pop()?;
                let cond = self.b.icmp(self.cur, cond, kind, lhs, rhs, "if");
                self.branch(cond, target, next)?;
            }
            0xa7 | 0xc8 => {
                let Operand::Branch(target) = instruction.operand else { return error("no target") };
                self.goto(target)?;
            }
            0xa8 | 0xc9 => {
                let Operand::Branch(target) = instruction.operand else { return error("no target") };
                let back = next.and_then(|next| self.subroutines.iter().position(|&o| o == next)).unwrap_or(0);
                self.push(Ir::Int(back as i128), ReturnAddress);
                self.goto(target)?;
            }
            0xa9 => {
                let variable = self.local(operand_local, ReturnAddress);
                let value = self.b.load(self.cur, Type::I32, variable, "back");
                let nowhere = self.b.block("nowhere");
                let mut cases = Vec::new();
                for (n, &back) in self.subroutines.clone().iter().enumerate() {
                    // A subroutine the walk never came back from returns nowhere.
                    if let Ok(block) = self.block_at(back) {
                        cases.push((n as i128, block));
                    }
                }
                self.spill();
                self.b.terminate(self.cur, Terminator::Switch { ty: Type::I32, value: value.into(), default: nowhere, cases });
                self.live = false;
            }
            0xaa | 0xab => {
                let (default, pairs): (u32, Vec<(i32, u32)>) = match &instruction.operand {
                    Operand::Table { low, default, targets } => {
                        (*default, targets.iter().enumerate().map(|(n, &target)| (low.wrapping_add(n as i32), target)).collect())
                    }
                    Operand::Lookup { default, pairs } => (*default, pairs.clone()),
                    _ => return error("no cases"),
                };
                let value = self.pop()?;
                let default = self.block_at(default)?;
                let cases =
                    pairs.iter().map(|&(key, target)| Ok((i128::from(key), self.block_at(target)?))).collect::<Result<Vec<_>, Error>>()?;
                self.spill();
                self.b.terminate(self.cur, Terminator::Switch { ty: Type::I32, value, default, cases });
                self.live = false;
            }
            0xac..=0xb0 => {
                let kind = KINDS[usize::from(opcode - 0xac)];
                let value = self.pop()?;
                self.b.terminate(self.cur, Terminator::Ret(Some((ty(kind), value))));
                self.live = false;
            }
            0xb1 => {
                self.b.terminate(self.cur, Terminator::Ret(None));
                self.live = false;
            }
            0xb2..=0xb5 => {
                let (class, name, descriptor) = member(file, constant)?;
                let kind = field_kind(descriptor)?;
                let place = self.lift.field(class, name, descriptor, opcode <= 0xb3)?;
                let value = if matches!(opcode, 0xb3 | 0xb5) { Some(self.pop()?) } else { None };
                let at: Ir = match place {
                    Place::Global(global) => Ir::Global(global),
                    Place::Offset(at) => {
                        let object = self.pop()?;
                        self.null_check(&object, offset)?;
                        self.field_at(object, at).into()
                    }
                };
                match value {
                    Some(value) => self.b.store(self.cur, ty(kind), value, at),
                    None => {
                        let value = self.b.load(self.cur, ty(kind), at, name);
                        self.push(value, kind);
                    }
                }
            }
            0xb6..=0xb9 => {
                let (class, name, descriptor) = member(file, constant)?;
                let (parameters, returned) = method_kinds(descriptor)?;
                let receiver = opcode != 0xb8;
                let count = parameters.len() + usize::from(receiver);
                if self.stack.len() < count {
                    return error("the stack is empty");
                }
                let args: Vec<Ir> = self.stack.split_off(self.stack.len() - count).into_iter().map(|(value, _)| value).collect();
                let how = if matches!(opcode, 0xb6 | 0xb9) { How::Found } else { How::Named };
                let result = self.lift.call(&mut self.b, self.cur, how, receiver, class, name, descriptor, args)?;
                if let (Some(kind), Some(result)) = (returned, result) {
                    self.push(result, kind);
                }
                self.check(offset)?;
            }
            0xba => self.dynamic(constant, offset)?,
            0xbb => {
                let class = self.text(class_at(file, constant)?);
                let object = self.rt("jvm.new", Type::Ptr, vec![class]);
                self.push(object, Reference);
            }
            0xbc | 0xbd => {
                let count = self.pop()?;
                let array = if opcode == 0xbc {
                    let Operand::NewArray(element) = instruction.operand else { return error("no element type") };
                    self.rt("jvm.newarray", Type::Ptr, vec![(Type::I32, Ir::Int(element.into())), (Type::I32, count)])
                } else {
                    let class = self.text(class_at(file, constant)?);
                    self.rt("jvm.anewarray", Type::Ptr, vec![class, (Type::I32, count)])
                };
                self.push(array, Reference);
                self.check(offset)?;
            }
            0xbe => {
                let array = self.pop()?;
                self.null_check(&array, offset)?;
                let at = self.field_at(array, HEADER);
                let length = self.b.load(self.cur, Type::I32, at, "length");
                self.push(length, Int);
            }
            0xbf => {
                let thrown = self.pop()?;
                self.rt("jvm.throw", Type::Void, vec![(Type::Ptr, thrown)]);
                let block = self.dispatch(offset)?;
                self.b.terminate(self.cur, Terminator::Br(block));
                self.live = false;
            }
            0xc0 => {
                let class = self.text(class_at(file, constant)?);
                let object = self.stack.last().map(|(value, _)| value.clone()).ok_or_else(|| Error("the stack is empty".to_owned()))?;
                self.rt("jvm.checkcast", Type::Void, vec![(Type::Ptr, object), class]);
                self.check(offset)?;
            }
            0xc1 => {
                let class = self.text(class_at(file, constant)?);
                let object = self.pop()?;
                let is = self.rt("jvm.instanceof", Type::I32, vec![(Type::Ptr, object), class]);
                self.push(is, Int);
            }
            0xc2 | 0xc3 => {
                let object = self.pop()?;
                self.null_check(&object, offset)?;
                let name = if opcode == 0xc2 { "jvm.monitorenter" } else { "jvm.monitorexit" };
                self.rt(name, Type::Void, vec![(Type::Ptr, object)]);
            }
            0xc5 => {
                let Operand::MultiANewArray { dimensions, .. } = instruction.operand else { return error("no dimensions") };
                let class = self.text(class_at(file, constant)?);
                let counts = self.b.alloca(self.entry, Type::array(u64::from(dimensions), Type::I32), "counts");
                for n in (0..u64::from(dimensions)).rev() {
                    let count = self.pop()?;
                    let at = self.b.gep(self.cur, Type::I8, counts, vec![(Type::I64, Ir::Int((4 * n).into()))], "count.at");
                    self.b.store(self.cur, Type::I32, count, at);
                }
                let array = self.rt(
                    "jvm.multianewarray",
                    Type::Ptr,
                    vec![class, (Type::I32, Ir::Int(dimensions.into())), (Type::Ptr, counts.into())],
                );
                self.push(array, Reference);
                self.check(offset)?;
            }
            _ => return error("an instruction the lift does not know"),
        }
        Ok(())
    }

    /// A call site of `invokedynamic`, by what its bootstrap method makes of it.
    fn dynamic(&mut self, constant: u16, offset: u32) -> Result<(), Error> {
        let file = self.file;
        let Some(Constant::InvokeDynamic { bootstrap, name_and_type: named }) = file.constants.get(usize::from(constant)) else {
            return error(format!("constant {constant} is no call site"));
        };
        let (name, descriptor) = name_and_type(file, *named)?;
        let bootstraps = file.bootstrap_methods()?;
        let Some((handle, arguments)) = bootstraps.get(usize::from(*bootstrap)) else {
            return error(format!("the class has no bootstrap method {bootstrap}"));
        };
        let Some(Constant::MethodHandle { reference, .. }) = file.constants.get(usize::from(*handle)) else {
            return error(format!("bootstrap method {bootstrap} is no method handle"));
        };
        let (class, method, _) = member(file, *reference)?;
        let (parameters, _) = split(descriptor)?;
        if self.stack.len() < parameters.len() {
            return error("the stack is empty");
        }
        let args = self.stack.split_off(self.stack.len() - parameters.len());
        match (class, method) {
            ("java/lang/invoke/LambdaMetafactory", "metafactory" | "altMetafactory") => {
                let made = self.lift.lambda(file, name, descriptor, arguments)?;
                let class = self.text(&made);
                let object = self.rt("jvm.new", Type::Ptr, vec![class]);
                for (n, (value, kind)) in args.into_iter().enumerate() {
                    let at = self.field_at(object.clone(), HEADER + 8 * n as u64);
                    self.b.store(self.cur, ty(kind), value, at);
                }
                self.push(object, Kind::Reference);
            }
            ("java/lang/invoke/StringConcatFactory", "makeConcatWithConstants" | "makeConcat") => {
                // The recipe: \u{1} where an argument goes, \u{2} where a constant does, text between.
                let recipe = match arguments.first().map(|&a| file.constants.get(usize::from(a))) {
                    Some(Some(Constant::String(text))) if method == "makeConcatWithConstants" => text_at(file, *text)?.to_owned(),
                    _ => "\u{1}".repeat(parameters.len()),
                };
                let builder_class = "java/lang/StringBuilder";
                let class = self.text(builder_class);
                let builder = self.rt("jvm.new", Type::Ptr, vec![class]);
                self.lift.call(&mut self.b, self.cur, How::Named, true, builder_class, "<init>", "()V", vec![builder.clone()])?;
                let mut args = args.into_iter().zip(&parameters);
                let mut constants = arguments.iter().skip(1);
                let mut literal = String::new();
                let append = |body: &mut Self, value: Ir, descriptor: &str| {
                    let takes = match descriptor {
                        "I" | "S" | "B" => "I",
                        "C" | "Z" | "J" | "F" | "D" | "Ljava/lang/String;" => descriptor,
                        _ => "Ljava/lang/Object;",
                    };
                    let descriptor = format!("({takes})Ljava/lang/StringBuilder;");
                    body.lift.call(
                        &mut body.b,
                        body.cur,
                        How::Named,
                        true,
                        builder_class,
                        "append",
                        &descriptor,
                        vec![builder.clone(), value],
                    )
                };
                let text = |body: &mut Self, text: &str| {
                    let (global, units) = body.lift.string(text);
                    body.rt("jvm.string", Type::Ptr, vec![(Type::Ptr, Ir::Global(global)), (Type::I32, Ir::Int(units.into()))])
                };
                // The end of the recipe ends the text before it as a mark does.
                for c in recipe.chars().chain(['\0']) {
                    if !matches!(c, '\u{1}' | '\u{2}' | '\0') {
                        literal.push(c);
                        continue;
                    }
                    if !literal.is_empty() {
                        let string = text(self, &literal);
                        append(self, string, "Ljava/lang/String;")?;
                        literal.clear();
                    }
                    if c == '\u{1}' {
                        if let Some(((value, _), descriptor)) = args.next() {
                            append(self, value, descriptor)?;
                        }
                    } else if c == '\u{2}' {
                        let string = match constants.next().map(|&a| file.constants.get(usize::from(a))) {
                            Some(Some(Constant::String(index))) => text_at(file, *index)?.to_owned(),
                            Some(Some(Constant::Integer(v))) => v.to_string(),
                            _ => return error("a constant of a concatenation the lift does not write yet"),
                        };
                        let string = text(self, &string);
                        append(self, string, "Ljava/lang/String;")?;
                    }
                }
                let string = self.lift.call(
                    &mut self.b,
                    self.cur,
                    How::Named,
                    true,
                    builder_class,
                    "toString",
                    "()Ljava/lang/String;",
                    vec![builder.clone()],
                )?;
                self.push(string.expect("a string"), Kind::Reference);
                self.check(offset)?;
            }
            _ => return error(format!("a call site of {class}.{method}, a bootstrap method the lift does not know")),
        }
        Ok(())
    }
}
