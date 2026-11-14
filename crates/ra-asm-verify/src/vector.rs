//! The function vector and its file.
//!
//! A vector is one activation of one function, seen from outside: the registers it was entered
//! with, the memory it read that it had not written itself, the calls it made in order, the
//! registers and the memory it left, and the way it left. The calls are part of the vector so a
//! replay can answer them without running the callees.
//!
//! The file is text, one item a line, so a vector can be read, compared and written by hand:
//!
//! ```text
//! ra-asm-verify vectors 1
//!
//! vector F_140001000 at 0x140001000
//! entry
//!   rcx = 0x5
//! memory in
//!   0x14000a000: 01 02 03
//! call kernel32!GetTickCount
//!   arg 0x1
//!   result rax = 0x2a
//!   effect 0x14000b000: ff
//! exit
//!   rax = 0x2f
//! memory out
//!   0x14000a000: 04
//! left return
//! ```
//!
//! Every section is present, in this order, even when it is empty. A run of bytes longer than a
//! line goes on over lines that hold bytes alone. A blank line and a line that starts with `#`
//! say nothing; the indentation is for the reader.

use std::fmt::{self, Write as _};

/// The line every vector file starts with; the number is the format's version.
const HEADER: &str = "ra-asm-verify vectors 1";

/// The bytes a line of a run holds before the run goes on over the next line.
const BYTES_PER_LINE: usize = 16;

/// A register and its value. The name is the architecture's, one word without `=`; the value is
/// wide enough for a 128-bit vector register.
pub type Register = (String, u128);

/// Adjacent bytes of the memory and the address of the first, the shape the interpreter's
/// armed memory reports its accesses in.
pub type Run = (u64, Vec<u8>);

/// One activation of a function: what it was given, what it asked and what it left.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vector {
    /// The function's name, one word.
    pub function: String,
    /// The function's address in the image.
    pub address: u64,
    /// The registers at the entry.
    pub entry: Vec<Register>,
    /// The bytes the function read before writing them itself, as they were when first read.
    pub memory_in: Vec<Run>,
    /// The calls the function made, in the order it made them.
    pub calls: Vec<Call>,
    /// The registers at the exit.
    pub exit: Vec<Register>,
    /// The bytes the function wrote, as it left them.
    pub memory_out: Vec<Run>,
    /// The way the activation ended.
    pub left: Left,
}

/// One call a function made, with everything a replay needs to answer it in the callee's place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Call {
    /// The callee's name, one word: a function of the image or an import.
    pub callee: String,
    /// The arguments, in the order of the callee's convention.
    pub args: Vec<u64>,
    /// The registers the callee returned its results in.
    pub results: Vec<Register>,
    /// The bytes the callee wrote, as it left them.
    pub effects: Vec<Run>,
}

/// The way an activation ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Left {
    /// The function returned to its caller.
    #[default]
    Return,
    /// The process exited inside the activation, with this code.
    Exit(u32),
    /// The lift stopped at an instruction it does not cover; the reason is one line of text.
    Trap { address: u64, reason: String },
    /// The processor raised an exception that the activation did not handle.
    Exception { code: u32, address: u64 },
}

/// Why a vector file could not be read, or a vector could not be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// The line of the text at fault, counted from one; none when writing.
    pub line: Option<usize>,
    pub message: String,
}

impl Error {
    fn at(line: usize, message: impl Into<String>) -> Self {
        Self { line: Some(line), message: message.into() }
    }

    fn unwritable(message: String) -> Self {
        Self { line: None, message }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: {}", self.message),
            None => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for Error {}

/// Writes vectors as the text of a vector file. A vector whose text would not read back as
/// itself is refused: a name that is not one word, a trap's reason that is not one line.
pub fn write(vectors: &[Vector]) -> Result<String, Error> {
    let mut out = String::from(HEADER);
    out.push('\n');
    for vector in vectors {
        out.push('\n');
        write_vector(&mut out, vector)?;
    }
    Ok(out)
}

fn write_vector(out: &mut String, vector: &Vector) -> Result<(), Error> {
    let function = word(&vector.function, "function")?;
    let _ = writeln!(out, "vector {function} at {:#x}", vector.address);
    out.push_str("entry\n");
    write_registers(out, "  ", &vector.entry)?;
    out.push_str("memory in\n");
    write_runs(out, "  ", &vector.memory_in);
    for call in &vector.calls {
        let _ = writeln!(out, "call {}", word(&call.callee, "callee")?);
        for arg in &call.args {
            let _ = writeln!(out, "  arg {arg:#x}");
        }
        write_registers(out, "  result ", &call.results)?;
        write_runs(out, "  effect ", &call.effects);
    }
    out.push_str("exit\n");
    write_registers(out, "  ", &vector.exit)?;
    out.push_str("memory out\n");
    write_runs(out, "  ", &vector.memory_out);
    match &vector.left {
        Left::Return => out.push_str("left return\n"),
        Left::Exit(code) => {
            let _ = writeln!(out, "left exit {code:#x}");
        }
        Left::Trap { address, reason } => {
            if reason.trim() != reason || reason.contains(['\n', '\r']) {
                return Err(Error::unwritable(format!("the trap's reason {reason:?} is not one trimmed line")));
            }
            let _ = write!(out, "left trap {address:#x}");
            if !reason.is_empty() {
                out.push(' ');
                out.push_str(reason);
            }
            out.push('\n');
        }
        Left::Exception { code, address } => {
            let _ = writeln!(out, "left exception {code:#x} at {address:#x}");
        }
    }
    Ok(())
}

fn write_registers(out: &mut String, prefix: &str, registers: &[Register]) -> Result<(), Error> {
    for (name, value) in registers {
        let name =
            register_name(name).ok_or_else(|| Error::unwritable(format!("the register name {name:?} is not one word without `=`")))?;
        let _ = writeln!(out, "{prefix}{name} = {value:#x}");
    }
    Ok(())
}

fn write_runs(out: &mut String, prefix: &str, runs: &[Run]) {
    for (address, bytes) in runs {
        let _ = write!(out, "{prefix}{address:#x}:");
        for (index, chunk) in bytes.chunks(BYTES_PER_LINE).enumerate() {
            if index > 0 {
                out.push_str("\n   ");
            }
            for byte in chunk {
                let _ = write!(out, " {byte:02x}");
            }
        }
        out.push('\n');
    }
}

/// The name when it is one word, the only shape a `vector` or a `call` line can carry.
fn word<'a>(name: &'a str, what: &str) -> Result<&'a str, Error> {
    if name.is_empty() || name.contains(char::is_whitespace) {
        return Err(Error::unwritable(format!("the {what} name {name:?} is not one word")));
    }
    Ok(name)
}

/// The name when a register line can carry it: one word, without the `=` that ends it and
/// without the `#` that would make the line a comment.
fn register_name(name: &str) -> Option<&str> {
    let fits = !name.is_empty() && !name.starts_with('#') && !name.contains(|c: char| c.is_whitespace() || c == '=');
    fits.then_some(name)
}

/// Reads the text of a vector file. The reading is strict: a line the format does not know, a
/// section missing or out of order, and a number that is not hexadecimal are errors that name
/// their line.
pub fn read(text: &str) -> Result<Vec<Vector>, Error> {
    let mut lines = Lines::new(text);
    let (line, header) = lines.take("the header")?;
    if header != HEADER {
        return Err(Error::at(line, format!("the header is `{HEADER}`, not `{header}`")));
    }
    let mut vectors = Vec::new();
    while lines.peek().is_some() {
        vectors.push(read_vector(&mut lines)?);
    }
    Ok(vectors)
}

/// The lines that say something, each trimmed and with its number in the text.
struct Lines<'a> {
    kept: Vec<(usize, &'a str)>,
    at: usize,
    /// The number of the text's last line, where a text that ends too early is at fault.
    end: usize,
}

impl<'a> Lines<'a> {
    fn new(text: &'a str) -> Self {
        let end = text.lines().count().max(1);
        let kept = text
            .lines()
            .enumerate()
            .map(|(index, line)| (index + 1, line.trim()))
            .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'));
        Self { kept: kept.collect(), at: 0, end }
    }

    fn peek(&self) -> Option<(usize, &'a str)> {
        self.kept.get(self.at).copied()
    }

    fn take(&mut self, what: &str) -> Result<(usize, &'a str), Error> {
        let line = self.peek().ok_or_else(|| Error::at(self.end, format!("the text ends where {what} is expected")))?;
        self.at += 1;
        Ok(line)
    }

    /// Takes the line that must be exactly this section's.
    fn section(&mut self, name: &str) -> Result<(), Error> {
        let what = format!("`{name}`");
        let (line, text) = self.take(&what)?;
        if text == name { Ok(()) } else { Err(Error::at(line, format!("{what} is expected, not `{text}`"))) }
    }
}

fn read_vector(lines: &mut Lines<'_>) -> Result<Vector, Error> {
    let (line, text) = lines.take("a vector")?;
    let words: Vec<&str> = text.split_whitespace().collect();
    let ["vector", function, "at", address] = words[..] else {
        return Err(Error::at(line, format!("`vector <name> at <address>` is expected, not `{text}`")));
    };
    let mut vector = Vector { function: function.to_owned(), address: hex64(line, address)?, ..Vector::default() };

    lines.section("entry")?;
    vector.entry = read_registers(lines, "memory in")?;
    lines.section("memory in")?;
    vector.memory_in = read_runs(lines, |text| first_word(text) == "call" || text == "exit")?;
    while let Some((_, text)) = lines.peek()
        && first_word(text) == "call"
    {
        vector.calls.push(read_call(lines)?);
    }
    lines.section("exit")?;
    vector.exit = read_registers(lines, "memory out")?;
    lines.section("memory out")?;
    vector.memory_out = read_runs(lines, |text| first_word(text) == "left")?;
    let (line, text) = lines.take("`left`")?;
    vector.left = read_left(line, text)?;
    Ok(vector)
}

/// Reads register lines up to the section that follows them, which is left in place.
fn read_registers(lines: &mut Lines<'_>, next: &str) -> Result<Vec<Register>, Error> {
    let mut registers = Vec::new();
    while let Some((line, text)) = lines.peek()
        && text != next
    {
        lines.at += 1;
        registers.push(read_register(line, text)?);
    }
    Ok(registers)
}

fn read_register(line: usize, text: &str) -> Result<Register, Error> {
    let fault = || Error::at(line, format!("`<register> = <value>` is expected, not `{text}`"));
    let (name, value) = text.split_once('=').ok_or_else(fault)?;
    let name = register_name(name.trim()).ok_or_else(fault)?;
    Ok((name.to_owned(), hex(line, value.trim())?))
}

/// Reads runs of bytes up to the line `ends` knows, which is left in place.
fn read_runs(lines: &mut Lines<'_>, ends: impl Fn(&str) -> bool) -> Result<Vec<Run>, Error> {
    let mut runs: Vec<Run> = Vec::new();
    while let Some((line, text)) = lines.peek()
        && !ends(text)
    {
        lines.at += 1;
        if text.contains(':') {
            runs.push(read_run(line, text)?);
        } else {
            let (_, bytes) = runs.last_mut().ok_or_else(|| Error::at(line, format!("`<address>: <bytes>` is expected, not `{text}`")))?;
            read_bytes(line, text, bytes)?;
        }
    }
    Ok(runs)
}

/// Reads the first line of a run: its address and the bytes that fit beside it.
fn read_run(line: usize, text: &str) -> Result<Run, Error> {
    let (address, rest) = text.split_once(':').ok_or_else(|| Error::at(line, format!("`<address>: <bytes>` is expected, not `{text}`")))?;
    let mut bytes = Vec::new();
    read_bytes(line, rest, &mut bytes)?;
    Ok((hex64(line, address.trim())?, bytes))
}

fn read_bytes(line: usize, text: &str, bytes: &mut Vec<u8>) -> Result<(), Error> {
    for pair in text.split_whitespace() {
        let byte = if pair.len() == 2 && pair.bytes().all(|b| b.is_ascii_hexdigit()) { u8::from_str_radix(pair, 16).ok() } else { None };
        bytes.push(byte.ok_or_else(|| Error::at(line, format!("`{pair}` is not a byte of two hexadecimal digits")))?);
    }
    Ok(())
}

fn read_call(lines: &mut Lines<'_>) -> Result<Call, Error> {
    let (line, text) = lines.take("a call")?;
    let words: Vec<&str> = text.split_whitespace().collect();
    let ["call", callee] = words[..] else {
        return Err(Error::at(line, format!("`call <name>` is expected, not `{text}`")));
    };
    let mut call = Call { callee: callee.to_owned(), ..Call::default() };
    // Bytes alone go on with a run only on the lines right after its `effect`.
    let mut in_effect = false;
    while let Some((line, text)) = lines.peek() {
        let (kind, rest) = split_word(text);
        match kind {
            "call" => break,
            "exit" if rest.is_empty() => break,
            "arg" => call.args.push(hex64(line, rest)?),
            "result" => call.results.push(read_register(line, rest)?),
            "effect" => call.effects.push(read_run(line, rest)?),
            _ => match call.effects.last_mut() {
                Some((_, bytes)) if in_effect => read_bytes(line, text, bytes)?,
                _ => return Err(Error::at(line, format!("`arg`, `result`, `effect`, `call` or `exit` is expected, not `{text}`"))),
            },
        }
        in_effect = kind != "arg" && kind != "result";
        lines.at += 1;
    }
    Ok(call)
}

fn read_left(line: usize, text: &str) -> Result<Left, Error> {
    let fault = || {
        Error::at(
            line,
            format!("`left return`, `exit <code>`, `trap <address> <reason>` or `exception <code> at <address>` is expected, not `{text}`"),
        )
    };
    let ("left", rest) = split_word(text) else {
        return Err(fault());
    };
    let (kind, rest) = split_word(rest);
    match kind {
        "return" if rest.is_empty() => Ok(Left::Return),
        "exit" => Ok(Left::Exit(hex32(line, rest)?)),
        "trap" => {
            let (address, reason) = split_word(rest);
            Ok(Left::Trap { address: hex64(line, address)?, reason: reason.to_owned() })
        }
        "exception" => {
            let words: Vec<&str> = rest.split_whitespace().collect();
            let [code, "at", address] = words[..] else {
                return Err(fault());
            };
            Ok(Left::Exception { code: hex32(line, code)?, address: hex64(line, address)? })
        }
        _ => Err(fault()),
    }
}

fn first_word(text: &str) -> &str {
    split_word(text).0
}

/// The first word of a trimmed line and what follows it, without the space between.
fn split_word(text: &str) -> (&str, &str) {
    match text.split_once(char::is_whitespace) {
        Some((word, rest)) => (word, rest.trim_start()),
        None => (text, ""),
    }
}

/// Reads a number as the writer prints one: `0x` and hexadecimal digits, nothing else.
fn hex(line: usize, text: &str) -> Result<u128, Error> {
    text.strip_prefix("0x")
        .filter(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_hexdigit()))
        .and_then(|digits| u128::from_str_radix(digits, 16).ok())
        .ok_or_else(|| Error::at(line, format!("`{text}` is not a hexadecimal number of at most 128 bits")))
}

fn hex64(line: usize, text: &str) -> Result<u64, Error> {
    u64::try_from(hex(line, text)?).map_err(|_| Error::at(line, format!("`{text}` does not fit 64 bits")))
}

fn hex32(line: usize, text: &str) -> Result<u32, Error> {
    u32::try_from(hex(line, text)?).map_err(|_| Error::at(line, format!("`{text}` does not fit 32 bits")))
}
