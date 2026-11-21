//! The runtime of the lifted Java machine: what a module [`crate::lift`] made calls and does not
//! hold, answered over the IR's interpreter.
//!
//! [`run`] gives the module a memory, reads its classes from their globals, initializes them, and
//! calls a class's `main`. The module's calls of the `jvm.` functions (an object made, an element
//! found, a method found by the object's class, a throw) and of the methods of the classes that
//! were not lifted come to the [`Machine`], which is the machine's own part: the heap, the
//! exception pending, and the few classes of the library a program cannot be without (strings and
//! their builder, the boxes, a list, the exceptions the machine itself throws, the output
//! stream). A class the program names that is none of these stops the run by its name.
//!
//! An object of a library class holds no fields in memory: what it is (a string's text, a list's
//! elements) the machine keeps beside the heap, by the object's address. Nothing is collected.

use std::collections::HashMap;

use ra_asm_ir::interp::{Host, Memory, Region, Stop, Value, Vm};
use ra_asm_ir::{FnType, Module, Type};

use crate::lift::{CLASSES, ELEMENTS, HEADER, once, symbol};

const STATICS: (u64, u64) = (0x1000_0000, 0x0080_0000);
const FRAMES: (u64, u64) = (0x2000_0000, 0x0080_0000);
const HEAP: (u64, u64) = (0x4000_0000, 0x0200_0000);

/// The library's classes the machine knows: each with its superclass and its interfaces.
const LIBRARY: &[(&str, &str, &[&str])] = &[
    ("java/lang/Object", "", &[]),
    ("java/lang/String", "java/lang/Object", &["java/lang/CharSequence", "java/lang/Comparable"]),
    ("java/lang/StringBuilder", "java/lang/Object", &["java/lang/CharSequence"]),
    ("java/lang/Class", "java/lang/Object", &[]),
    ("java/lang/Enum", "java/lang/Object", &["java/lang/Comparable"]),
    ("java/lang/Record", "java/lang/Object", &[]),
    ("java/lang/Number", "java/lang/Object", &[]),
    ("java/lang/Integer", "java/lang/Number", &["java/lang/Comparable"]),
    ("java/lang/Long", "java/lang/Number", &["java/lang/Comparable"]),
    ("java/lang/Short", "java/lang/Number", &["java/lang/Comparable"]),
    ("java/lang/Byte", "java/lang/Number", &["java/lang/Comparable"]),
    ("java/lang/Float", "java/lang/Number", &["java/lang/Comparable"]),
    ("java/lang/Double", "java/lang/Number", &["java/lang/Comparable"]),
    ("java/lang/Boolean", "java/lang/Object", &["java/lang/Comparable"]),
    ("java/lang/Character", "java/lang/Object", &["java/lang/Comparable"]),
    ("java/io/PrintStream", "java/lang/Object", &[]),
    ("java/util/ArrayList", "java/lang/Object", &["java/util/List", "java/util/Collection", "java/lang/Iterable"]),
    ("java/util/ArrayList$Itr", "java/lang/Object", &["java/util/Iterator"]),
    ("java/lang/Throwable", "java/lang/Object", &[]),
    ("java/lang/Error", "java/lang/Throwable", &[]),
    ("java/lang/Exception", "java/lang/Throwable", &[]),
    ("java/lang/RuntimeException", "java/lang/Exception", &[]),
    ("java/lang/IllegalStateException", "java/lang/RuntimeException", &[]),
    ("java/lang/MatchException", "java/lang/RuntimeException", &[]),
    ("java/lang/IllegalArgumentException", "java/lang/RuntimeException", &[]),
    ("java/lang/NumberFormatException", "java/lang/IllegalArgumentException", &[]),
    ("java/lang/UnsupportedOperationException", "java/lang/RuntimeException", &[]),
    ("java/lang/ArithmeticException", "java/lang/RuntimeException", &[]),
    ("java/lang/NullPointerException", "java/lang/RuntimeException", &[]),
    ("java/lang/ClassCastException", "java/lang/RuntimeException", &[]),
    ("java/lang/NegativeArraySizeException", "java/lang/RuntimeException", &[]),
    ("java/lang/IndexOutOfBoundsException", "java/lang/RuntimeException", &[]),
    ("java/lang/ArrayIndexOutOfBoundsException", "java/lang/IndexOutOfBoundsException", &[]),
    ("java/lang/StringIndexOutOfBoundsException", "java/lang/IndexOutOfBoundsException", &[]),
];

/// A class as the machine knows it.
#[derive(Debug, Clone, Default)]
struct Class {
    name: String,
    above: Option<String>,
    interfaces: Vec<String>,
    /// The bytes of the class's own fields: none for a class of the library.
    fields: u64,
    /// The functions of a lifted class's methods, by `name:descriptor`.
    methods: HashMap<String, u64>,
}

/// What an object of a library class is.
#[derive(Debug, Clone)]
enum Native {
    /// A string's UTF-16 units; a builder's.
    Text(Vec<u16>),
    /// A list's elements.
    List(Vec<u64>),
    /// Where an iterator of a list is.
    Iterator {
        list: u64,
        at: usize,
    },
    Throwable {
        message: u64,
    },
    /// A box's value, as its bits.
    Box(u64),
    /// The stream of a file number.
    Stream(u32),
    /// The class a `Class` object is of.
    Class(String),
    /// A constant of an enum: its name, a string, and its place among the constants.
    Enum {
        name: u64,
        ordinal: i32,
    },
}

/// What a run printed, and how it ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// What the program wrote to its output.
    pub out: Vec<u8>,
    /// What it wrote to its error stream.
    pub err: Vec<u8>,
    /// The exception no handler caught, as the machine names it: its class and its message.
    pub thrown: Option<String>,
    /// The IR instructions the run took.
    pub steps: u64,
}

/// The machine's own part of a run.
pub struct Machine {
    classes: Vec<Class>,
    by_name: HashMap<String, usize>,
    natives: HashMap<u64, Native>,
    /// The string object of each constant's text: a constant is one object wherever it is named.
    interned: HashMap<Vec<u16>, u64>,
    mirrors: HashMap<String, u64>,
    /// Where each static field of the library the program names is.
    statics: HashMap<String, u64>,
    pending: u64,
    heap: u64,
    out: Vec<u8>,
    err: Vec<u8>,
}

fn int(v: i32) -> Value {
    Value::int(u128::from(v.cast_unsigned()), 32)
}

fn long(v: i64) -> Value {
    Value::int(u128::from(v.cast_unsigned()), 64)
}

fn pointer(v: u64) -> Value {
    Value::Int(u128::from(v))
}

fn truth(v: bool) -> Value {
    int(i32::from(v))
}

fn as_int(v: &Value) -> i32 {
    (v.as_u64() as u32).cast_signed()
}

fn as_long(v: &Value) -> i64 {
    v.as_u64().cast_signed()
}

const NOTHING: Value = Value::Int(0);

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// A number's shortest digits that read back as it, and the power of ten of the first.
fn digits(scientific: &str) -> (String, i32) {
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((scientific, "0"));
    (mantissa.chars().filter(char::is_ascii_digit).collect(), exponent.parse().unwrap_or(0))
}

/// A `double` or a `float` as the library writes it: its shortest digits, with a point and at
/// least one digit after it, and in the scientific form outside a thousandth to ten million.
fn real(negative: bool, special: Option<&str>, scientific: &str) -> String {
    let sign = if negative { "-" } else { "" };
    if let Some(special) = special {
        return special.to_owned();
    }
    let (digits, exponent) = digits(scientific.trim_start_matches('-'));
    if digits.bytes().all(|b| b == b'0') {
        return format!("{sign}0.0");
    }
    if (-3..7).contains(&exponent) {
        if exponent < 0 {
            return format!("{sign}0.{}{digits}", "0".repeat((-exponent - 1) as usize));
        }
        let whole = exponent as usize + 1;
        let padded = format!("{digits:0<whole$}");
        let (before, after) = padded.split_at(whole);
        return format!("{sign}{before}.{}", if after.is_empty() { "0" } else { after });
    }
    let (first, rest) = digits.split_at(1);
    format!("{sign}{first}.{}E{exponent}", if rest.is_empty() { "0" } else { rest })
}

/// A `double` as `Double.toString` writes it.
#[must_use]
pub fn double_text(v: f64) -> String {
    let special = if v.is_nan() {
        Some("NaN")
    } else if v.is_infinite() {
        Some(if v > 0.0 { "Infinity" } else { "-Infinity" })
    } else {
        None
    };
    real(v.is_sign_negative(), special, &format!("{v:e}"))
}

/// A `float` as `Float.toString` writes it.
#[must_use]
pub fn float_text(v: f32) -> String {
    let special = if v.is_nan() {
        Some("NaN")
    } else if v.is_infinite() {
        Some(if v > 0.0 { "Infinity" } else { "-Infinity" })
    } else {
        None
    };
    real(v.is_sign_negative(), special, &format!("{v:e}"))
}

/// The bytes of an element of an array of `descriptor` (`[I`, `[[D`, `[Ljava/lang/String;`).
fn element_size(descriptor: &str) -> u64 {
    match descriptor.as_bytes().get(1) {
        Some(b'Z' | b'B') => 1,
        Some(b'C' | b'S') => 2,
        Some(b'I' | b'F') => 4,
        _ => 8,
    }
}

impl Machine {
    /// A machine with the library's classes; the module tells it of its own when its
    /// [`CLASSES`] runs.
    fn new() -> Self {
        let mut machine = Machine {
            classes: Vec::new(),
            by_name: HashMap::new(),
            natives: HashMap::new(),
            interned: HashMap::new(),
            mirrors: HashMap::new(),
            statics: HashMap::new(),
            pending: 0,
            heap: HEAP.0,
            out: Vec::new(),
            err: Vec::new(),
        };
        for &(name, above, interfaces) in LIBRARY {
            machine.add(Class {
                name: name.to_owned(),
                above: (!above.is_empty()).then(|| above.to_owned()),
                interfaces: interfaces.iter().map(|&i| i.to_owned()).collect(),
                ..Class::default()
            });
        }
        machine
    }

    /// Adds a class, or replaces the one of its name: a program's own class of a library's name
    /// is the class.
    fn add(&mut self, class: Class) -> usize {
        if let Some(&n) = self.by_name.get(&class.name) {
            self.classes[n] = class;
            return n;
        }
        self.by_name.insert(class.name.clone(), self.classes.len());
        self.classes.push(class);
        self.classes.len() - 1
    }

    /// Where the static field `named` of a class of the library is: eight bytes of its own, which
    /// hold the stream for the fields of `System` that are streams.
    fn library_static(&mut self, vm: &mut Vm<'_>, named: &str) -> Result<u64, Stop> {
        if let Some(&at) = self.statics.get(named) {
            return Ok(at);
        }
        let at = self.allocate(vm, 8)?;
        let number = match named {
            "java/lang/System.out:Ljava/io/PrintStream;" => Some(1),
            "java/lang/System.err:Ljava/io/PrintStream;" => Some(2),
            _ => None,
        };
        if let Some(number) = number {
            let stream = self.object(vm, "java/io/PrintStream")?;
            self.natives.insert(stream, Native::Stream(number));
            vm.memory.write_u64(at, stream)?;
        }
        self.statics.insert(named.to_owned(), at);
        Ok(at)
    }

    /// The number of the class `name`; an array's class is made when it is first named.
    fn class(&mut self, name: &str) -> Result<usize, Stop> {
        if let Some(&n) = self.by_name.get(name) {
            return Ok(n);
        }
        if name.starts_with('[') {
            return Ok(self.add(Class { name: name.to_owned(), above: Some("java/lang/Object".to_owned()), ..Class::default() }));
        }
        Err(Stop::fault(format!("the class {name} is neither lifted nor one of the runtime's")))
    }

    /// The bytes of an object of the class: the number and the fields of the lifted classes it is of.
    fn size(&self, name: &str) -> u64 {
        let (mut size, mut at) = (HEADER, Some(name));
        while let Some(class) = at.and_then(|name| self.by_name.get(name)).map(|&n| &self.classes[n]) {
            size += class.fields;
            at = class.above.as_deref();
        }
        size
    }

    fn allocate(&mut self, vm: &mut Vm<'_>, bytes: u64) -> Result<u64, Stop> {
        let at = self.heap;
        let end = at + bytes.div_ceil(8) * 8;
        if end > HEAP.0 + HEAP.1 {
            return Err(Stop::fault("the heap is full: the runtime collects nothing yet"));
        }
        self.heap = end;
        let _ = vm;
        Ok(at)
    }

    /// A new object of the class `name`, its fields zero.
    fn object(&mut self, vm: &mut Vm<'_>, name: &str) -> Result<u64, Stop> {
        let class = self.class(name)?;
        let at = self.allocate(vm, self.size(name))?;
        vm.memory.write_u64(at, class as u64 + 1)?;
        let native = if self.is(name, "java/lang/StringBuilder") {
            Some(Native::Text(Vec::new()))
        } else if self.is(name, "java/util/ArrayList") {
            Some(Native::List(Vec::new()))
        } else if self.is(name, "java/lang/Throwable") {
            Some(Native::Throwable { message: 0 })
        } else {
            None
        };
        if let Some(native) = native {
            self.natives.insert(at, native);
        }
        Ok(at)
    }

    fn array(&mut self, vm: &mut Vm<'_>, descriptor: &str, count: i32) -> Result<u64, Stop> {
        if count < 0 {
            self.throw(vm, "java/lang/NegativeArraySizeException", &count.to_string())?;
            return Ok(0);
        }
        let class = self.class(descriptor)?;
        let at = self.allocate(vm, ELEMENTS + element_size(descriptor) * u64::from(count.cast_unsigned()))?;
        vm.memory.write_u64(at, class as u64 + 1)?;
        vm.memory.write_u32(at + HEADER, count.cast_unsigned())?;
        Ok(at)
    }

    fn arrays(&mut self, vm: &mut Vm<'_>, descriptor: &str, counts: &[i32]) -> Result<u64, Stop> {
        let array = self.array(vm, descriptor, counts[0])?;
        if array != 0 && counts.len() > 1 {
            for n in 0..u64::from(counts[0].cast_unsigned()) {
                let inner = self.arrays(vm, &descriptor[1..], &counts[1..])?;
                if inner == 0 {
                    return Ok(0);
                }
                vm.memory.write_u64(array + ELEMENTS + 8 * n, inner)?;
            }
        }
        Ok(array)
    }

    fn string(&mut self, vm: &mut Vm<'_>, text: Vec<u16>) -> Result<u64, Stop> {
        let at = self.object(vm, "java/lang/String")?;
        self.natives.insert(at, Native::Text(text));
        Ok(at)
    }

    fn boxed(&mut self, vm: &mut Vm<'_>, class: &str, bits: u64) -> Result<Value, Stop> {
        let at = self.object(vm, class)?;
        self.natives.insert(at, Native::Box(bits));
        Ok(pointer(at))
    }

    /// The name of the class of the object at `at`.
    fn class_of(&self, vm: &mut Vm<'_>, at: u64) -> Result<String, Stop> {
        let number = vm.memory.read_u64(at)?;
        match self.classes.get((number as usize).wrapping_sub(1)) {
            Some(class) => Ok(class.name.clone()),
            None => Err(Stop::fault(format!("the object at {at:#x} is of no class: its number is {number}"))),
        }
    }

    /// Whether the class `at` is `name`, or under it, or implements it.
    fn is(&self, at: &str, name: &str) -> bool {
        if at == name || name == "java/lang/Object" {
            return true;
        }
        let Some(class) = self.by_name.get(at).map(|&n| &self.classes[n]) else { return false };
        class.interfaces.iter().any(|i| self.is(i, name)) || class.above.as_deref().is_some_and(|above| self.is(above, name))
    }

    /// The classes from `name` up.
    fn chain(&self, name: &str) -> Vec<String> {
        let mut chain = vec![name.to_owned()];
        let mut at = name;
        while let Some(above) = self.by_name.get(at).and_then(|&n| self.classes[n].above.as_deref()) {
            chain.push(above.to_owned());
            at = above;
        }
        chain
    }

    /// The function of the lifted method `named` an object of the class `name` has: its class's,
    /// a superclass's, or the default of an interface.
    fn find(&self, name: &str, named: &str) -> Option<u64> {
        let chain = self.chain(name);
        let method = |class: &str| self.by_name.get(class).and_then(|&n| self.classes[n].methods.get(named)).copied();
        if let Some(found) = chain.iter().find_map(|class| method(class)) {
            return Some(found);
        }
        let mut interfaces: Vec<String> = Vec::new();
        let mut look: Vec<String> = chain;
        while let Some(class) = look.pop() {
            for interface in self.by_name.get(&class).map(|&n| self.classes[n].interfaces.clone()).unwrap_or_default() {
                if !interfaces.contains(&interface) {
                    interfaces.push(interface.clone());
                    look.push(interface);
                }
            }
        }
        interfaces.iter().find_map(|interface| method(interface))
    }

    /// Leaves an exception of the library's class `class` pending.
    fn throw(&mut self, vm: &mut Vm<'_>, class: &str, message: &str) -> Result<(), Stop> {
        let thrown = self.object(vm, class)?;
        let message = if message.is_empty() { 0 } else { self.string(vm, units(message))? };
        self.natives.insert(thrown, Native::Throwable { message });
        self.pending = thrown;
        Ok(())
    }

    fn text(&self, at: u64) -> Option<&Vec<u16>> {
        match self.natives.get(&at) {
            Some(Native::Text(text)) => Some(text),
            _ => None,
        }
    }

    /// Calls the method `named` the object `receiver` has, lifted or the library's.
    fn invoke(&mut self, vm: &mut Vm<'_>, receiver: u64, named: &str, mut args: Vec<Value>, ret: Type) -> Result<Value, Stop> {
        let class = self.class_of(vm, receiver)?;
        args.insert(0, pointer(receiver));
        match self.find(&class, named) {
            Some(function) => {
                let ty = FnType::new(ret, Vec::new());
                vm.call_address(self, function, &ty, args)
            }
            None => self.library(vm, &class, named, &args),
        }
    }

    /// What `String.valueOf` makes of an object.
    fn describe(&mut self, vm: &mut Vm<'_>, at: u64) -> Result<Vec<u16>, Stop> {
        if at == 0 {
            return Ok(units("null"));
        }
        if let Some(text) = self.text(at) {
            return Ok(text.clone());
        }
        let string = self.invoke(vm, at, "toString:()Ljava/lang/String;", Vec::new(), Type::Ptr)?.as_u64();
        Ok(self.text(string).cloned().unwrap_or_else(|| units("null")))
    }

    /// What is thrown as the machine names it at the end of a thread: `class: message`.
    fn thrown(&mut self, vm: &mut Vm<'_>, at: u64) -> Result<String, Stop> {
        let class = self.class_of(vm, at)?.replace('/', ".");
        let message = match self.natives.get(&at) {
            Some(Native::Throwable { message }) if *message != 0 => self.text(*message).map(|text| String::from_utf16_lossy(text)),
            _ => None,
        };
        Ok(match message {
            Some(message) => format!("{class}: {message}"),
            None => class,
        })
    }

    /// The machine's own functions.
    #[allow(clippy::too_many_lines)]
    fn machine(&mut self, vm: &mut Vm<'_>, name: &str, args: &[Value]) -> Result<Value, Stop> {
        let text = |vm: &mut Vm<'_>, n: usize| -> Result<String, Stop> {
            Ok(String::from_utf8_lossy(&vm.memory.read_cstr(args[n].as_u64(), 4096)?).into_owned())
        };
        Ok(match name {
            "jvm.pending" => pointer(self.pending),
            "jvm.caught" => pointer(std::mem::take(&mut self.pending)),
            "jvm.throw" => {
                match args[0].as_u64() {
                    0 => self.throw(vm, "java/lang/NullPointerException", "")?,
                    thrown => self.pending = thrown,
                }
                NOTHING
            }
            "jvm.null" => {
                self.throw(vm, "java/lang/NullPointerException", "")?;
                NOTHING
            }
            "jvm.nothing" | "jvm.monitorenter" | "jvm.monitorexit" => NOTHING,
            "jvm.define" => {
                let above = if args[1].as_u64() == 0 { None } else { Some(text(vm, 1)?) };
                self.add(Class { name: text(vm, 0)?, above, fields: args[2].as_u64(), ..Class::default() });
                NOTHING
            }
            "jvm.implement" | "jvm.method" => {
                let (class, named) = (text(vm, 0)?, text(vm, 1)?);
                let class = self.class(&class)?;
                if name == "jvm.method" {
                    self.classes[class].methods.insert(named, args[2].as_u64());
                } else {
                    self.classes[class].interfaces.push(named);
                }
                NOTHING
            }
            "jvm.static" => {
                let named = text(vm, 0)?;
                pointer(self.library_static(vm, &named)?)
            }
            "jvm.new" => {
                let class = text(vm, 0)?;
                pointer(self.object(vm, &class)?)
            }
            "jvm.string" => {
                let (at, count) = (args[0].as_u64(), u64::from(args[1].as_u64() as u32));
                let mut text = Vec::with_capacity(count as usize);
                for n in 0..count {
                    text.push(vm.memory.read_u16(at + 2 * n)?);
                }
                if let Some(&string) = self.interned.get(&text) {
                    return Ok(pointer(string));
                }
                let string = self.string(vm, text.clone())?;
                self.interned.insert(text, string);
                pointer(string)
            }
            "jvm.class" => {
                let class = text(vm, 0)?;
                if let Some(&mirror) = self.mirrors.get(&class) {
                    return Ok(pointer(mirror));
                }
                let mirror = self.object(vm, "java/lang/Class")?;
                self.natives.insert(mirror, Native::Class(class.clone()));
                self.mirrors.insert(class, mirror);
                pointer(mirror)
            }
            "jvm.virtual" => {
                let receiver = args[0].as_u64();
                let named = text(vm, 1)?;
                if receiver == 0 {
                    self.throw(vm, "java/lang/NullPointerException", "")?;
                    return Ok(pointer(vm.bind_external("jvm.nothing")?));
                }
                let class = self.class_of(vm, receiver)?;
                match self.find(&class, &named) {
                    Some(function) => pointer(function),
                    None => pointer(vm.bind_external(&format!("{class}.{named}"))?),
                }
            }
            "jvm.instanceof" => {
                let (object, class) = (args[0].as_u64(), text(vm, 1)?);
                truth(object != 0 && self.is(&self.class_of(vm, object)?, &class))
            }
            "jvm.checkcast" => {
                let (object, class) = (args[0].as_u64(), text(vm, 1)?);
                if object != 0 {
                    let of = self.class_of(vm, object)?;
                    if !self.is(&of, &class) {
                        let message = format!("class {} cannot be cast to class {}", of.replace('/', "."), class.replace('/', "."));
                        self.throw(vm, "java/lang/ClassCastException", &message)?;
                    }
                }
                NOTHING
            }
            "jvm.element" => {
                let (array, index, size) = (args[0].as_u64(), as_int(&args[1]), args[2].as_u64());
                if array == 0 {
                    self.throw(vm, "java/lang/NullPointerException", "")?;
                    return Ok(NOTHING);
                }
                let length = vm.memory.read_u32(array + HEADER)?.cast_signed();
                if index < 0 || index >= length {
                    let message = format!("Index {index} out of bounds for length {length}");
                    self.throw(vm, "java/lang/ArrayIndexOutOfBoundsException", &message)?;
                    return Ok(NOTHING);
                }
                pointer(array + ELEMENTS + size * u64::from(index.cast_unsigned()))
            }
            "jvm.newarray" => {
                let descriptor = match args[0].as_u64() {
                    4 => "[Z",
                    5 => "[C",
                    6 => "[F",
                    7 => "[D",
                    8 => "[B",
                    9 => "[S",
                    10 => "[I",
                    _ => "[J",
                };
                pointer(self.array(vm, descriptor, as_int(&args[1]))?)
            }
            "jvm.anewarray" => {
                let class = text(vm, 0)?;
                let descriptor = if class.starts_with('[') { format!("[{class}") } else { format!("[L{class};") };
                pointer(self.array(vm, &descriptor, as_int(&args[1]))?)
            }
            "jvm.multianewarray" => {
                let (descriptor, dimensions, at) = (text(vm, 0)?, args[1].as_u64(), args[2].as_u64());
                let mut counts = Vec::new();
                for n in 0..dimensions {
                    counts.push(vm.memory.read_u32(at + 4 * n)?.cast_signed());
                }
                if let Some(&negative) = counts.iter().find(|&&count| count < 0) {
                    self.throw(vm, "java/lang/NegativeArraySizeException", &negative.to_string())?;
                    return Ok(NOTHING);
                }
                pointer(self.arrays(vm, &descriptor, &counts)?)
            }
            "jvm.idiv" | "jvm.irem" | "jvm.ldiv" | "jvm.lrem" => {
                let wide = name.starts_with("jvm.l");
                let (a, b) =
                    if wide { (as_long(&args[0]), as_long(&args[1])) } else { (i64::from(as_int(&args[0])), i64::from(as_int(&args[1]))) };
                if b == 0 {
                    self.throw(vm, "java/lang/ArithmeticException", "/ by zero")?;
                    return Ok(NOTHING);
                }
                let result = if name.ends_with("div") { a.wrapping_div(b) } else { a.wrapping_rem(b) };
                if wide { long(result) } else { int(result as i32) }
            }
            // A float that is no number is zero; one out of range is the nearest in it.
            "jvm.f2i" => int(args[0].as_f32() as i32),
            "jvm.f2l" => long(args[0].as_f32() as i64),
            "jvm.d2i" => int(args[0].as_f64() as i32),
            "jvm.d2l" => long(args[0].as_f64() as i64),
            _ => return Err(Stop::fault(format!("the runtime has no function {name}"))),
        })
    }

    /// A method of the library: of `class` or of the nearest class above it that has it.
    fn library(&mut self, vm: &mut Vm<'_>, class: &str, named: &str, args: &[Value]) -> Result<Value, Stop> {
        for at in self.chain(class) {
            if let Some(value) = self.method(vm, &at, named, args)? {
                return Ok(value);
            }
        }
        Err(Stop::fault(format!("the runtime has no method {class}.{named}")))
    }

    /// The method `named` of the library's class `class`, when the machine has it.
    #[allow(clippy::too_many_lines)]
    fn method(&mut self, vm: &mut Vm<'_>, class: &str, named: &str, args: &[Value]) -> Result<Option<Value>, Stop> {
        let arg = |n: usize| args.get(n).map_or(0, Value::as_u64);
        let this = arg(0);
        let (name, descriptor) = named.split_once(':').unwrap_or((named, ""));
        // The library's classes by their short names; a program's class of the same name is none.
        let short = ["java/lang/", "java/util/", "java/io/"].iter().find_map(|package| class.strip_prefix(package)).unwrap_or("");
        Ok(Some(match (short, name, descriptor) {
            ("Object", "<init>", "()V") => NOTHING,
            ("Object", "hashCode", "()I") => int((this >> 3) as i32),
            ("Object", "equals", _) => truth(this == arg(1)),
            ("Object", "toString", _) => {
                let text = match self.natives.get(&this) {
                    Some(Native::Text(text)) => text.clone(),
                    Some(Native::Throwable { .. }) => units(&self.thrown(vm, this)?),
                    Some(Native::List(items)) => {
                        let mut text = units("[");
                        for (n, item) in items.clone().into_iter().enumerate() {
                            if n > 0 {
                                text.extend(units(", "));
                            }
                            text.extend(self.describe(vm, item)?);
                        }
                        text.extend(units("]"));
                        text
                    }
                    Some(Native::Class(of)) => units(&format!("class {}", of.replace('/', "."))),
                    _ => units(&format!("{}@{:x}", self.class_of(vm, this)?.replace('/', "."), this >> 3)),
                };
                pointer(self.string(vm, text)?)
            }
            ("Object", "clone", _) => {
                // An array's copy: the same class, the same length, the same elements.
                let of = self.class_of(vm, this)?;
                if !of.starts_with('[') {
                    self.throw(vm, "java/lang/CloneNotSupportedException", &of.replace('/', "."))?;
                    return Ok(Some(NOTHING));
                }
                let length = vm.memory.read_u32(this + HEADER)?;
                let copy = self.array(vm, &of, length.cast_signed())?;
                let bytes = vm.memory.read_vec(this + ELEMENTS, element_size(&of) * u64::from(length))?;
                vm.memory.write(copy + ELEMENTS, &bytes)?;
                pointer(copy)
            }
            ("Objects", "equals", _) => {
                let (a, b) = (this, arg(1));
                if a == b {
                    truth(true)
                } else if a == 0 {
                    truth(false)
                } else {
                    self.invoke(vm, a, "equals:(Ljava/lang/Object;)Z", vec![pointer(b)], Type::I32)?
                }
            }
            ("Objects", "hashCode", _) => match this {
                0 => int(0),
                object => self.invoke(vm, object, "hashCode:()I", Vec::new(), Type::I32)?,
            },
            ("Enum", "<init>", "(Ljava/lang/String;I)V") => {
                self.natives.insert(this, Native::Enum { name: arg(1), ordinal: as_int(&args[2]) });
                NOTHING
            }
            ("Enum", "name" | "toString" | "ordinal" | "hashCode" | "compareTo", _) => {
                let constant = |machine: &Self, at: u64| match machine.natives.get(&at) {
                    Some(&Native::Enum { name, ordinal }) => (name, ordinal),
                    _ => (0, 0),
                };
                let (text, ordinal) = constant(self, this);
                match name {
                    "name" | "toString" => pointer(text),
                    "compareTo" => int(ordinal - constant(self, arg(1)).1),
                    _ => int(ordinal),
                }
            }
            ("Enum", "valueOf", "(Ljava/lang/Class;Ljava/lang/String;)Ljava/lang/Enum;") => {
                // The constant of the class of that name, among the constants made so far: the
                // class's own `valueOf` initialized it before it asked.
                let of = match self.natives.get(&this) {
                    Some(Native::Class(of)) => of.clone(),
                    _ => String::new(),
                };
                let wanted = self.text(arg(1)).cloned().unwrap_or_default();
                let mut found = 0;
                let constants: Vec<(u64, u64)> = self
                    .natives
                    .iter()
                    .filter_map(|(&at, native)| match native {
                        Native::Enum { name, .. } => Some((at, *name)),
                        _ => None,
                    })
                    .collect();
                for (at, text) in constants {
                    if self.text(text) == Some(&wanted) && self.class_of(vm, at)? == of {
                        found = at;
                    }
                }
                if found == 0 {
                    let message = format!("No enum constant {}.{}", of.replace(['/', '$'], "."), String::from_utf16_lossy(&wanted));
                    self.throw(vm, "java/lang/IllegalArgumentException", &message)?;
                }
                pointer(found)
            }
            ("ArrayList", "contains", _) => {
                let items = match self.natives.get(&this) {
                    Some(Native::List(items)) => items.clone(),
                    _ => Vec::new(),
                };
                let mut found = false;
                for item in items {
                    let equal = match (arg(1), item) {
                        (a, b) if a == b => true,
                        (0, _) => false,
                        (a, b) => self.invoke(vm, a, "equals:(Ljava/lang/Object;)Z", vec![pointer(b)], Type::I32)?.as_u64() & 1 != 0,
                    };
                    if equal {
                        found = true;
                        break;
                    }
                }
                truth(found)
            }
            ("Objects", "requireNonNull", _) => {
                if this == 0 {
                    self.throw(vm, "java/lang/NullPointerException", "")?;
                }
                pointer(this)
            }
            ("String" | "StringBuilder" | "CharSequence", "length", "()I") => int(self.text(this).map_or(0, Vec::len) as i32),
            ("String" | "StringBuilder" | "CharSequence", "charAt", "(I)C") => {
                let index = as_int(&args[1]);
                match self.text(this).and_then(|text| text.get(usize::try_from(index).ok()?)).copied() {
                    Some(unit) => int(i32::from(unit)),
                    None => {
                        self.throw(vm, "java/lang/StringIndexOutOfBoundsException", &format!("index {index}"))?;
                        NOTHING
                    }
                }
            }
            ("String", "isEmpty", "()Z") => truth(self.text(this).is_none_or(Vec::is_empty)),
            ("String", "hashCode", "()I") => int(self
                .text(this)
                .map_or(0, |text| text.iter().fold(0i32, |hash, &unit| hash.wrapping_mul(31).wrapping_add(i32::from(unit))))),
            ("String", "equals", _) => truth(arg(1) != 0 && self.text(this).is_some() && self.text(this) == self.text(arg(1))),
            ("String", "toString" | "intern", _) => pointer(this),
            ("String", "concat", _) => {
                let mut text = self.text(this).cloned().unwrap_or_default();
                text.extend(self.text(arg(1)).cloned().unwrap_or_default());
                pointer(self.string(vm, text)?)
            }
            ("String", "substring", "(I)Ljava/lang/String;" | "(II)Ljava/lang/String;") => {
                let text = self.text(this).cloned().unwrap_or_default();
                let (from, to) = (as_int(&args[1]), args.get(2).map_or(text.len() as i32, as_int));
                if from < 0 || to > text.len() as i32 || from > to {
                    self.throw(vm, "java/lang/StringIndexOutOfBoundsException", &format!("begin {from}, end {to}, length {}", text.len()))?;
                    return Ok(Some(NOTHING));
                }
                pointer(self.string(vm, text[from as usize..to as usize].to_vec())?)
            }
            ("String", "compareTo", _) => {
                let (a, b) = (self.text(this).cloned().unwrap_or_default(), self.text(arg(1)).cloned().unwrap_or_default());
                let differ = a.iter().zip(&b).find(|(x, y)| x != y).map(|(&x, &y)| i32::from(x) - i32::from(y));
                int(differ.unwrap_or(a.len() as i32 - b.len() as i32))
            }
            ("String", "valueOf", "(Ljava/lang/Object;)Ljava/lang/String;") => {
                let text = self.describe(vm, this)?;
                pointer(self.string(vm, text)?)
            }
            ("String" | "Integer", "valueOf" | "toString", "(I)Ljava/lang/String;") => {
                pointer(self.string(vm, units(&as_int(&args[0]).to_string()))?)
            }
            ("String" | "Long", "valueOf" | "toString", "(J)Ljava/lang/String;") => {
                pointer(self.string(vm, units(&as_long(&args[0]).to_string()))?)
            }
            ("String" | "Double", "valueOf" | "toString", "(D)Ljava/lang/String;") => {
                pointer(self.string(vm, units(&double_text(args[0].as_f64())))?)
            }
            ("String" | "Character", "valueOf" | "toString", "(C)Ljava/lang/String;") => pointer(self.string(vm, vec![arg(0) as u16])?),
            ("String" | "Boolean", "valueOf" | "toString", "(Z)Ljava/lang/String;") => {
                pointer(self.string(vm, units(if arg(0) & 1 != 0 { "true" } else { "false" }))?)
            }
            ("StringBuilder", "<init>", "()V" | "(I)V") => NOTHING,
            ("StringBuilder", "<init>", "(Ljava/lang/String;)V") => {
                let text = self.text(arg(1)).cloned().unwrap_or_default();
                self.natives.insert(this, Native::Text(text));
                NOTHING
            }
            ("StringBuilder", "append", _) => {
                let added = match descriptor.split_once(')').map_or("", |(takes, _)| &takes[1..]) {
                    "I" | "S" | "B" => units(&as_int(&args[1]).to_string()),
                    "J" => units(&as_long(&args[1]).to_string()),
                    "C" => vec![arg(1) as u16],
                    "Z" => units(if arg(1) & 1 != 0 { "true" } else { "false" }),
                    "F" => units(&float_text(args[1].as_f32())),
                    "D" => units(&double_text(args[1].as_f64())),
                    _ => self.describe(vm, arg(1))?,
                };
                if let Some(Native::Text(text)) = self.natives.get_mut(&this) {
                    text.extend(added);
                }
                pointer(this)
            }
            ("StringBuilder", "toString", _) => {
                let text = self.text(this).cloned().unwrap_or_default();
                pointer(self.string(vm, text)?)
            }
            ("PrintStream", "println" | "print", _) => {
                let mut text = match descriptor {
                    "()V" => Vec::new(),
                    "(I)V" | "(S)V" | "(B)V" => units(&as_int(&args[1]).to_string()),
                    "(J)V" => units(&as_long(&args[1]).to_string()),
                    "(C)V" => vec![arg(1) as u16],
                    "(Z)V" => units(if arg(1) & 1 != 0 { "true" } else { "false" }),
                    "(F)V" => units(&float_text(args[1].as_f32())),
                    "(D)V" => units(&double_text(args[1].as_f64())),
                    _ => self.describe(vm, arg(1))?,
                };
                if name == "println" {
                    text.push(u16::from(b'\n'));
                }
                let bytes = String::from_utf16_lossy(&text).into_bytes();
                match self.natives.get(&this) {
                    Some(Native::Stream(2)) => self.err.extend(bytes),
                    _ => self.out.extend(bytes),
                }
                NOTHING
            }
            ("ArrayList", "<init>", "()V" | "(I)V") => NOTHING,
            ("ArrayList", "add", "(Ljava/lang/Object;)Z") => {
                if let Some(Native::List(items)) = self.natives.get_mut(&this) {
                    items.push(arg(1));
                }
                truth(true)
            }
            ("ArrayList", "size", "()I") | ("ArrayList", "isEmpty", "()Z") => {
                let count = match self.natives.get(&this) {
                    Some(Native::List(items)) => items.len(),
                    _ => 0,
                };
                if name == "size" { int(count as i32) } else { truth(count == 0) }
            }
            ("ArrayList", "get" | "set", _) => {
                let index = as_int(&args[1]);
                let replacement = arg(2);
                let Some(Native::List(items)) = self.natives.get_mut(&this) else { return Ok(Some(NOTHING)) };
                let count = items.len();
                match usize::try_from(index).ok().and_then(|n| items.get_mut(n)) {
                    Some(item) => pointer(if name == "set" { std::mem::replace(item, replacement) } else { *item }),
                    None => {
                        self.throw(vm, "java/lang/IndexOutOfBoundsException", &format!("Index {index} out of bounds for length {count}"))?;
                        NOTHING
                    }
                }
            }
            ("ArrayList", "clear", "()V") => {
                self.natives.insert(this, Native::List(Vec::new()));
                NOTHING
            }
            ("ArrayList", "iterator", _) => {
                let iterator = self.object(vm, "java/util/ArrayList$Itr")?;
                self.natives.insert(iterator, Native::Iterator { list: this, at: 0 });
                pointer(iterator)
            }
            ("ArrayList", "forEach", _) => {
                let items = match self.natives.get(&this) {
                    Some(Native::List(items)) => items.clone(),
                    _ => Vec::new(),
                };
                for item in items {
                    self.invoke(vm, arg(1), "accept:(Ljava/lang/Object;)V", vec![pointer(item)], Type::Void)?;
                    if self.pending != 0 {
                        break;
                    }
                }
                NOTHING
            }
            ("ArrayList$Itr", "hasNext" | "next", _) => {
                let Some(&Native::Iterator { list, at }) = self.natives.get(&this) else { return Ok(Some(NOTHING)) };
                let item = match self.natives.get(&list) {
                    Some(Native::List(items)) => items.get(at).copied(),
                    _ => None,
                };
                if name == "hasNext" {
                    truth(item.is_some())
                } else {
                    self.natives.insert(this, Native::Iterator { list, at: at + 1 });
                    pointer(item.unwrap_or(0))
                }
            }
            ("Throwable", "<init>", _) => {
                // The message is the first argument when it is a string.
                let message = if descriptor.starts_with("(Ljava/lang/String;") { arg(1) } else { 0 };
                self.natives.insert(this, Native::Throwable { message });
                NOTHING
            }
            ("Throwable", "getMessage" | "getLocalizedMessage", _) => match self.natives.get(&this) {
                Some(Native::Throwable { message }) => pointer(*message),
                _ => NOTHING,
            },
            (
                "Integer" | "Short" | "Byte" | "Character" | "Boolean",
                "valueOf",
                "(I)Ljava/lang/Integer;"
                | "(S)Ljava/lang/Short;"
                | "(B)Ljava/lang/Byte;"
                | "(C)Ljava/lang/Character;"
                | "(Z)Ljava/lang/Boolean;",
            ) => self.boxed(vm, class, u64::from(arg(0) as u32))?,
            ("Long", "valueOf", "(J)Ljava/lang/Long;") => self.boxed(vm, class, arg(0))?,
            ("Float", "valueOf", "(F)Ljava/lang/Float;") => self.boxed(vm, class, u64::from(args[0].as_f32().to_bits()))?,
            ("Double", "valueOf", "(D)Ljava/lang/Double;") => self.boxed(vm, class, args[0].as_f64().to_bits())?,
            (
                "Integer" | "Short" | "Byte" | "Character" | "Boolean" | "Long" | "Float" | "Double" | "Number",
                _,
                "()I" | "()S" | "()B" | "()C" | "()Z" | "()J" | "()F" | "()D",
            ) if name.ends_with("Value") || name == "hashCode" => {
                let Some(&Native::Box(bits)) = self.natives.get(&this) else { return Ok(Some(NOTHING)) };
                // The box's own value, as the kind the caller asks for.
                let of = self.class_of(vm, this)?;
                let held = match of.as_str() {
                    "java/lang/Float" => f64::from(f32::from_bits(bits as u32)),
                    "java/lang/Double" => f64::from_bits(bits),
                    "java/lang/Long" => bits.cast_signed() as f64,
                    _ => f64::from((bits as u32).cast_signed()),
                };
                let whole = match of.as_str() {
                    "java/lang/Float" | "java/lang/Double" => held as i64,
                    "java/lang/Long" => bits.cast_signed(),
                    _ => i64::from((bits as u32).cast_signed()),
                };
                match descriptor {
                    "()J" => long(whole),
                    "()F" => Value::F32(held as f32),
                    "()D" => Value::F64(held),
                    _ => int(whole as i32),
                }
            }
            ("Integer" | "Short" | "Byte" | "Character" | "Boolean" | "Long" | "Float" | "Double", "toString", "()Ljava/lang/String;") => {
                let Some(&Native::Box(bits)) = self.natives.get(&this) else { return Ok(Some(NOTHING)) };
                let text = match short {
                    "Float" => float_text(f32::from_bits(bits as u32)),
                    "Double" => double_text(f64::from_bits(bits)),
                    "Long" => bits.cast_signed().to_string(),
                    "Character" => String::from_utf16_lossy(&[bits as u16]),
                    "Boolean" => (bits & 1 != 0).to_string(),
                    _ => (bits as u32).cast_signed().to_string(),
                };
                pointer(self.string(vm, units(&text))?)
            }
            ("Integer" | "Short" | "Byte" | "Character" | "Boolean" | "Long" | "Float" | "Double", "equals", _) => {
                let same = arg(1) != 0 && self.class_of(vm, arg(1))? == self.class_of(vm, this)?;
                truth(
                    same && matches!((self.natives.get(&this), self.natives.get(&arg(1))), (Some(Native::Box(a)), Some(Native::Box(b))) if a == b),
                )
            }
            ("Integer", "parseInt", "(Ljava/lang/String;)I") => {
                let text = self.text(this).map(|text| String::from_utf16_lossy(text)).unwrap_or_default();
                match text.parse::<i32>() {
                    Ok(value) => int(value),
                    Err(_) => {
                        self.throw(vm, "java/lang/NumberFormatException", &format!("For input string: \"{text}\""))?;
                        NOTHING
                    }
                }
            }
            ("Math", "abs", "(I)I") => int(as_int(&args[0]).wrapping_abs()),
            ("Math", "abs", "(J)J") => long(as_long(&args[0]).wrapping_abs()),
            ("Math", "abs", "(D)D") => Value::F64(args[0].as_f64().abs()),
            ("Math", "abs", "(F)F") => Value::F32(args[0].as_f32().abs()),
            ("Math", "max", "(II)I") => int(as_int(&args[0]).max(as_int(&args[1]))),
            ("Math", "min", "(II)I") => int(as_int(&args[0]).min(as_int(&args[1]))),
            ("Math", "max", "(JJ)J") => long(as_long(&args[0]).max(as_long(&args[1]))),
            ("Math", "min", "(JJ)J") => long(as_long(&args[0]).min(as_long(&args[1]))),
            ("Math", "sqrt", "(D)D") => Value::F64(args[0].as_f64().sqrt()),
            ("Math", "floor", "(D)D") => Value::F64(args[0].as_f64().floor()),
            ("Math", "ceil", "(D)D") => Value::F64(args[0].as_f64().ceil()),
            ("System", "exit", "(I)V") => return Err(Stop::Exit(as_int(&args[0]))),
            ("System", "arraycopy", _) => {
                let (from, from_at, to, to_at, count) = (arg(0), as_int(&args[1]), arg(2), as_int(&args[3]), as_int(&args[4]));
                if from == 0 || to == 0 {
                    self.throw(vm, "java/lang/NullPointerException", "")?;
                    return Ok(Some(NOTHING));
                }
                let size = element_size(&self.class_of(vm, from)?);
                let (have, room) = (vm.memory.read_u32(from + HEADER)?.cast_signed(), vm.memory.read_u32(to + HEADER)?.cast_signed());
                if from_at < 0 || to_at < 0 || count < 0 || from_at > have - count || to_at > room - count {
                    self.throw(vm, "java/lang/ArrayIndexOutOfBoundsException", "arraycopy: out of bounds")?;
                    return Ok(Some(NOTHING));
                }
                let at = |array: u64, index: i32| array + ELEMENTS + size * u64::from(index.cast_unsigned());
                let bytes = vm.memory.read_vec(at(from, from_at), size * u64::from(count.cast_unsigned()))?;
                vm.memory.write(at(to, to_at), &bytes)?;
                NOTHING
            }
            _ => return Ok(None),
        }))
    }
}

impl Host for Machine {
    fn call(&mut self, vm: &mut Vm<'_>, name: &str, _ty: &FnType, args: &[Value]) -> Result<Value, Stop> {
        if name.starts_with("jvm.") {
            return self.machine(vm, name, args);
        }
        match name.split_once('.') {
            Some((class, named)) => self.library(vm, class, named, args),
            None => Err(Stop::fault(format!("the runtime has no function {name}"))),
        }
    }
}

/// Runs the `main` of the class `main` of a lifted module with `arguments`, after the static
/// initializers of the module's classes in the module's order.
///
/// # Errors
/// When the run stops other than by `main` returning or throwing: a class or a method the
/// runtime has not, a fault of the lifted code, the limit of `limit` instructions (zero for none).
pub fn run(module: &Module, main: &str, arguments: &[&str], limit: u64) -> Result<Outcome, Stop> {
    enter(module, limit, |vm, machine| {
        // The class is initialized before its `main` runs; the others when they are first used.
        let mut entries: Vec<(String, Vec<Value>)> = Vec::new();
        if module.function(&once(main)).is_some() {
            entries.push((once(main), Vec::new()));
        }
        let array = machine.array(vm, "[Ljava/lang/String;", arguments.len() as i32)?;
        for (n, argument) in arguments.iter().enumerate() {
            let string = machine.string(vm, units(argument))?;
            vm.memory.write_u64(array + ELEMENTS + 8 * n as u64, string)?;
        }
        entries.push((symbol(main, "main", "([Ljava/lang/String;)V"), vec![pointer(array)]));
        Ok(entries)
    })
}

/// Runs one static method of a lifted module that takes nothing, by its function's name
/// (`Class.name:()V`), with no class initialized first.
///
/// # Errors
/// As [`run`].
pub fn call(module: &Module, function: &str, limit: u64) -> Result<Outcome, Stop> {
    enter(module, limit, |_, _| Ok(vec![(function.to_owned(), Vec::new())]))
}

/// Gives `module` its memory and its machine, and calls the functions `entries` names in their
/// order, until one leaves an exception pending.
fn enter(
    module: &Module,
    limit: u64,
    entries: impl FnOnce(&mut Vm<'_>, &mut Machine) -> Result<Vec<(String, Vec<Value>)>, Stop>,
) -> Result<Outcome, Stop> {
    let mut memory = Memory::new();
    for (name, (start, len)) in [("statics", STATICS), ("frames", FRAMES), ("heap", HEAP)] {
        memory.add_region(Region::owned(start, len, name));
    }
    let mut vm = Vm::new(module, memory);
    vm.limit = limit;
    vm.set_static_region(STATICS.0, STATICS.1);
    vm.set_frame_region(FRAMES.0, FRAMES.1);
    vm.place()?;
    let mut machine = Machine::new();
    if vm.function(CLASSES).is_some() {
        vm.call_named(&mut machine, CLASSES, Vec::new())?;
    }
    let entries = entries(&mut vm, &mut machine)?;
    let mut thrown = None;
    for (function, args) in entries {
        match vm.call_named(&mut machine, &function, args) {
            Ok(_) if machine.pending == 0 => {}
            Err(Stop::Exit(_)) => break,
            Ok(_) => {
                let pending = std::mem::take(&mut machine.pending);
                let named = machine.thrown(&mut vm, pending)?;
                machine.err.extend(format!("Exception in thread \"main\" {named}\n").into_bytes());
                thrown = Some(named);
                break;
            }
            Err(stop) => return Err(stop),
        }
    }
    Ok(Outcome { out: machine.out, err: machine.err, thrown, steps: vm.steps })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::class::ClassFile;
    use crate::class::tests::class;

    #[test]
    fn a_real_number_is_written_as_the_library_writes_it() {
        for (value, text) in [
            (0.0, "0.0"),
            (-0.0, "-0.0"),
            (1.0, "1.0"),
            (100.0, "100.0"),
            (786_012.333_333_333_4, "786012.3333333334"),
            (0.001, "0.001"),
            (0.000_1, "1.0E-4"),
            (9_999_999.0, "9999999.0"),
            (1.0e7, "1.0E7"),
            (1.5e300, "1.5E300"),
            (-2.5e-10, "-2.5E-10"),
            (f64::NAN, "NaN"),
            (f64::NEG_INFINITY, "-Infinity"),
        ] {
            assert_eq!(double_text(value), text);
        }
        assert_eq!(float_text(1.0e7), "1.0E7");
        assert_eq!(float_text(0.1), "0.1");
        assert_eq!(float_text(3.4028235e38), "3.4028235E38");
    }

    /// A subroutine of the releases before Java 7, lifted: each `jsr` goes in, and `ret` comes
    /// back to the instruction after the one that went.
    #[test]
    fn a_subroutine_returns_to_each_of_its_callers() {
        // 0: aconst_null; 1: pop; 2: iconst_0; 3: istore_2; 4: jsr 17; 7: jsr 17;
        // 10: iconst_1; 11: iconst_2; 12: iload_2; 13: isub; 14: idiv; 15: pop; 16: return;
        // 17: astore_1; 18: iinc 2 1; 21: ret 1
        // The division is by zero exactly when the subroutine ran twice.
        let code = [
            0x01, 0x57, 0x03, 0x3d, 0xa8, 0x00, 0x0d, 0xa8, 0x00, 0x0a, 0x04, 0x05, 0x1c, 0x64, 0x6c, 0x57, 0xb1, 0x4c, 0x84, 0x02, 0x01,
            0xa9, 0x01,
        ];
        let file = ClassFile::parse(&class(48, 0, &code, &[])).unwrap();
        let module = crate::lift::lift(std::slice::from_ref(&file)).unwrap();
        let outcome = call(&module, "T.m:()V", 100_000).unwrap();
        assert_eq!(outcome.thrown.as_deref(), Some("java.lang.ArithmeticException: / by zero"));
        // The same code is no program of Java 7: its class files have no subroutines.
        assert!(crate::lift::lift(std::slice::from_ref(&ClassFile::parse(&class(51, 0, &code, &[])).unwrap())).is_err());
    }
}
