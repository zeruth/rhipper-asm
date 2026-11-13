//! The `.ll` reader: the text [`crate::printer`] writes, back into a [`Module`]. It reads the
//! printer's subset and its layout, not LLVM's whole grammar: a global and a declaration each sit
//! on one line, and the address the printer leaves in a trailing comment is read back with it.
//! What it does not read it refuses, with the line and the token it stopped at.
//!
//! The header is covered: the module's name, layout and triple, the named struct types, the
//! globals with their initializers, and the declarations with their attributes. A function with a
//! body is refused.
//!
//! Three things the printer writes do not come back as they were, because the text is the same
//! for two modules: an internal global without an initializer is printed with `zeroinitializer`
//! and reads as one that has it; the address of a global at offset zero, in a pointer field, is
//! printed as the global and reads as [`Const::Global`]; and a float constant comes back as the
//! value of its type, so one that the type cannot hold exactly comes back rounded.

use std::collections::HashMap;
use std::fmt;

use crate::module::{CallConv, Const, Declaration, Global, Linkage, Module};
use crate::types::{FnType, Type};

/// A refusal: the line, the token the parser stopped at, and what it wanted there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// The line of the token, counted from one.
    pub line: usize,
    /// The token as the text spells it, or `end of file`.
    pub token: String,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: `{}`: {}", self.line, self.token, self.message)
    }
}

impl std::error::Error for ParseError {}

/// The module of a text the printer wrote.
pub fn parse(text: &str) -> Result<Module, ParseError> {
    let mut p = lex(text)?;
    let mut module = Module::default();
    while let Some(tok) = p.peek() {
        let line = p.here();
        match tok {
            Tok::Word("source_filename") => {
                p.at += 1;
                p.expect(b'=')?;
                p.string()?.clone_into(&mut module.name);
                p.end_of_line(line)?;
            }
            Tok::Word("target") => {
                p.at += 1;
                let field = match p.word() {
                    Some("datalayout") => &mut module.data_layout,
                    Some("triple") => &mut module.triple,
                    _ => return Err(p.refuse("`datalayout` or `triple` follows `target`")),
                };
                p.at += 1;
                p.expect(b'=')?;
                p.string()?.clone_into(field);
                p.end_of_line(line)?;
            }
            Tok::Local(_) => module.types.push(p.named_type()?),
            Tok::Global(_) => module.globals.push(p.global()?),
            Tok::Word("declare") => module.declarations.push(p.declaration()?),
            Tok::Word("define") => return Err(p.refuse("a function with a body is outside what the parser reads")),
            _ => return Err(p.refuse("does not start a type, a global, a declaration or a function")),
        }
    }
    Ok(module)
}

#[derive(Debug, Clone, PartialEq)]
enum Tok<'a> {
    /// A keyword, a primitive type or a bare attribute.
    Word(&'a str),
    Int(i128),
    /// The digits of a `0x` constant, with the letter that names its format when it has one.
    Hex(&'a str),
    /// A quoted string, as the text holds it between the quotes.
    Str(&'a str),
    /// A `c"…"` string, unescaped.
    Bytes(Vec<u8>),
    /// `@name`, unquoted.
    Global(String),
    /// `%name`, unquoted.
    Local(String),
    Punct(u8),
    Ellipsis,
}

#[derive(Debug)]
struct Token<'a> {
    tok: Tok<'a>,
    line: usize,
    start: usize,
    end: usize,
}

fn refusal(line: usize, token: &str, message: &str) -> ParseError {
    ParseError { line, token: token.to_owned(), message: message.to_owned() }
}

/// A character of a bare name, which is the set the printer leaves unquoted.
fn is_name(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'$' | b'-')
}

/// The number of bytes from `at` on that `keep` holds for.
fn run(bytes: &[u8], at: usize, keep: impl Fn(u8) -> bool) -> usize {
    bytes[at.min(bytes.len())..].iter().take_while(|b| keep(**b)).count()
}

/// The text between the quote at `at` and the next one of its line.
fn quoted(src: &str, at: usize, line: usize) -> Result<&str, ParseError> {
    let rest = &src[at + 1..];
    let rest = &rest[..rest.find('\n').unwrap_or(rest.len())];
    rest.find('"').map(|end| &rest[..end]).ok_or_else(|| refusal(line, "\"", "the string does not close on its line"))
}

/// The bytes of a string's text: `\\` is a backslash and `\XX` the byte of two hex digits.
fn unescape(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] != b'\\' {
            out.push(bytes[at]);
            at += 1;
        } else if bytes.get(at + 1) == Some(&b'\\') {
            out.push(b'\\');
            at += 2;
        } else {
            let hex = text.get(at + 1..at + 3).filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()))?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            at += 3;
        }
    }
    Some(out)
}

/// The tokens of a text, and the address in the trailing comment of each line that has one.
fn lex(src: &str) -> Result<Parser<'_>, ParseError> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::new();
    let mut addresses = HashMap::new();
    let (mut at, mut line) = (0, 1);
    while at < bytes.len() {
        let start = at;
        let c = bytes[at];
        let tok = match c {
            b'\n' => {
                line += 1;
                at += 1;
                continue;
            }
            b' ' | b'\t' | b'\r' => {
                at += 1;
                continue;
            }
            b';' => {
                let end = src[at..].find('\n').map_or(bytes.len(), |n| at + n);
                if let Some(address) = src[at + 1..end].trim().strip_prefix("0x").and_then(|hex| u64::from_str_radix(hex, 16).ok()) {
                    addresses.insert(line, address);
                }
                at = end;
                continue;
            }
            b'"' => {
                let text = quoted(src, at, line)?;
                at += text.len() + 2;
                Tok::Str(text)
            }
            b'c' if bytes.get(at + 1) == Some(&b'"') => {
                let text = quoted(src, at + 1, line)?;
                at += text.len() + 3;
                Tok::Bytes(unescape(text).ok_or_else(|| refusal(line, &src[start..at], "an escape of the string is not two hex digits"))?)
            }
            b'@' | b'%' => {
                let name = if bytes.get(at + 1) == Some(&b'"') {
                    let text = quoted(src, at + 1, line)?;
                    at += text.len() + 3;
                    unescape(text)
                        .and_then(|name| String::from_utf8(name).ok())
                        .ok_or_else(|| refusal(line, &src[start..at], "the name does not unescape to text"))?
                } else {
                    at += 1 + run(bytes, at + 1, is_name);
                    if at == start + 1 {
                        return Err(refusal(line, &src[start..at], "a name follows the sigil"));
                    }
                    src[start + 1..at].to_owned()
                };
                if c == b'@' { Tok::Global(name) } else { Tok::Local(name) }
            }
            b'0' if bytes.get(at + 1) == Some(&b'x') => {
                at += 2 + run(bytes, at + 2, |b| b.is_ascii_alphanumeric());
                Tok::Hex(&src[start + 2..at])
            }
            b'-' | b'0'..=b'9' => {
                at += 1 + run(bytes, at + 1, |b| b.is_ascii_digit());
                let text = &src[start..at];
                Tok::Int(text.parse().map_err(|_| refusal(line, text, "is not an integer of 128 bits"))?)
            }
            b'.' if src[at..].starts_with("...") => {
                at += 3;
                Tok::Ellipsis
            }
            b'=' | b',' | b'(' | b')' | b'[' | b']' | b'{' | b'}' | b'<' | b'>' | b'#' | b':' => {
                at += 1;
                Tok::Punct(c)
            }
            _ if c.is_ascii_alphabetic() || c == b'_' => {
                at += run(bytes, at, is_name);
                Tok::Word(&src[start..at])
            }
            _ => {
                let width = src[at..].chars().next().map_or(1, char::len_utf8);
                return Err(refusal(line, &src[at..at + width], "is not a character of the IR's text"));
            }
        };
        tokens.push(Token { tok, line, start, end: at });
    }
    Ok(Parser { src, tokens, at: 0, addresses })
}

/// The value of a float constant's digits in the type's format, as the printer writes it: `half`
/// as `H` and its 16-bit pattern, the rest as the 64-bit pattern of the value as a double.
fn float_of(ty: &Type, digits: &str) -> Option<f64> {
    match ty {
        Type::Half => {
            let digits = digits.strip_prefix('H').filter(|d| d.len() == 4)?;
            Some(half_value(u16::from_str_radix(digits, 16).ok()?))
        }
        Type::Float | Type::Double | Type::Fp80 if digits.len() == 16 => Some(f64::from_bits(u64::from_str_radix(digits, 16).ok()?)),
        _ => None,
    }
}

/// The value of an IEEE half bit pattern.
fn half_value(bits: u16) -> f64 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = i32::from((bits >> 10) & 0x1f);
    let mantissa = bits & 0x3ff;
    let magnitude = match exponent {
        0 => f64::from(mantissa) * 2f64.powi(-24),
        0x1f if mantissa == 0 => f64::INFINITY,
        0x1f => f64::NAN,
        _ => (1.0 + f64::from(mantissa) / 1024.0) * 2f64.powi(exponent - 15),
    };
    sign * magnitude
}

struct Parser<'a> {
    src: &'a str,
    tokens: Vec<Token<'a>>,
    /// The index of the token to read next.
    at: usize,
    /// The address in the trailing comment of a line, by line.
    addresses: HashMap<usize, u64>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&Tok<'a>> {
        self.tokens.get(self.at).map(|t| &t.tok)
    }

    /// The next token when it is a word.
    fn word(&self) -> Option<&'a str> {
        match self.peek() {
            Some(Tok::Word(w)) => Some(*w),
            _ => None,
        }
    }

    /// The line of the next token, or the last line of the text at its end.
    fn here(&self) -> usize {
        self.tokens.get(self.at).or(self.tokens.last()).map_or(1, |t| t.line)
    }

    /// Whether the next token is on `line`.
    fn on_line(&self, line: usize) -> bool {
        self.tokens.get(self.at).is_some_and(|t| t.line == line)
    }

    /// A refusal at the next token.
    fn refuse(&self, message: &str) -> ParseError {
        let token = self.tokens.get(self.at).map_or("end of file", |t| &self.src[t.start..t.end]);
        refusal(self.here(), token, message)
    }

    /// A statement of `line` is over: nothing else stands on its line.
    fn end_of_line(&self, line: usize) -> Result<(), ParseError> {
        if self.on_line(line) { Err(self.refuse("the line ends before this")) } else { Ok(()) }
    }

    fn eat(&mut self, punct: u8) -> bool {
        let found = self.peek() == Some(&Tok::Punct(punct));
        self.at += usize::from(found);
        found
    }

    fn expect(&mut self, punct: u8) -> Result<(), ParseError> {
        if self.eat(punct) { Ok(()) } else { Err(self.refuse(&format!("`{}` is expected", punct as char))) }
    }

    fn eat_word(&mut self, word: &str) -> bool {
        let found = self.word() == Some(word);
        self.at += usize::from(found);
        found
    }

    fn expect_word(&mut self, word: &str) -> Result<(), ParseError> {
        if self.eat_word(word) { Ok(()) } else { Err(self.refuse(&format!("`{word}` is expected"))) }
    }

    /// A quoted string, as the text holds it: the printer writes the module's strings unescaped.
    fn string(&mut self) -> Result<&'a str, ParseError> {
        match self.peek() {
            Some(Tok::Str(text)) => {
                let text = *text;
                self.at += 1;
                Ok(text)
            }
            _ => Err(self.refuse("a quoted string is expected")),
        }
    }

    fn global_name(&mut self) -> Result<String, ParseError> {
        match self.peek() {
            Some(Tok::Global(name)) => {
                let name = name.clone();
                self.at += 1;
                Ok(name)
            }
            _ => Err(self.refuse("the `@` name of a global is expected")),
        }
    }

    /// An integer that fits the type asked for; `what` names it in the refusal.
    fn number<T: TryFrom<i128>>(&mut self, what: &str) -> Result<T, ParseError> {
        let value = match self.peek() {
            Some(Tok::Int(v)) => T::try_from(*v).ok(),
            _ => None,
        };
        let value = value.ok_or_else(|| self.refuse(&format!("{what} is expected")))?;
        self.at += 1;
        Ok(value)
    }

    /// `%name = type { T, U }`.
    fn named_type(&mut self) -> Result<(String, Vec<Type>), ParseError> {
        let line = self.here();
        let name = match self.peek() {
            Some(Tok::Local(name)) => name.clone(),
            _ => return Err(self.refuse("the `%` name of a type is expected")),
        };
        self.at += 1;
        self.expect(b'=')?;
        self.expect_word("type")?;
        self.expect(b'{')?;
        let fields = self.fields()?;
        self.end_of_line(line)?;
        Ok((name, fields))
    }

    /// The fields of a struct, after its `{` and through its `}`.
    fn fields(&mut self) -> Result<Vec<Type>, ParseError> {
        let mut fields = Vec::new();
        if self.eat(b'}') {
            return Ok(fields);
        }
        loop {
            fields.push(self.ty()?);
            if self.eat(b'}') {
                return Ok(fields);
            }
            self.expect(b',')?;
        }
    }

    /// The parameters of a function type, after its `(` and through its `)`, and whether it ends
    /// in `...`.
    fn params(&mut self) -> Result<(Vec<Type>, bool), ParseError> {
        let mut params = Vec::new();
        if self.eat(b')') {
            return Ok((params, false));
        }
        loop {
            if self.peek() == Some(&Tok::Ellipsis) {
                self.at += 1;
                self.expect(b')')?;
                return Ok((params, true));
            }
            params.push(self.ty()?);
            if self.eat(b')') {
                return Ok((params, false));
            }
            self.expect(b',')?;
        }
    }

    /// A type; one followed by a parameter list is the function type that returns it.
    fn ty(&mut self) -> Result<Type, ParseError> {
        let mut ty = self.value_ty()?;
        while self.eat(b'(') {
            let (params, varargs) = self.params()?;
            ty = Type::Func(Box::new(FnType { ret: ty, params, varargs }));
        }
        Ok(ty)
    }

    fn value_ty(&mut self) -> Result<Type, ParseError> {
        let ty = match self.peek() {
            Some(Tok::Word(word)) => match *word {
                "void" => Type::Void,
                "half" => Type::Half,
                "float" => Type::Float,
                "double" => Type::Double,
                "x86_fp80" => Type::Fp80,
                "ptr" => Type::Ptr,
                other => {
                    let digits = other.strip_prefix('i').filter(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
                    match digits.and_then(|d| d.parse::<u16>().ok()) {
                        Some(bits) => Type::Int(bits),
                        None => return Err(self.refuse("is not a type")),
                    }
                }
            },
            Some(Tok::Local(name)) => Type::Named(name.clone()),
            Some(Tok::Punct(b'[')) => {
                self.at += 1;
                let count = self.number("the length of the array")?;
                self.expect_word("x")?;
                let element = self.ty()?;
                self.expect(b']')?;
                return Ok(Type::array(count, element));
            }
            Some(Tok::Punct(b'{')) => {
                self.at += 1;
                return Ok(Type::Struct(self.fields()?));
            }
            Some(Tok::Punct(b'<')) => {
                self.at += 1;
                if self.eat(b'{') {
                    let fields = self.fields()?;
                    self.expect(b'>')?;
                    return Ok(Type::Packed(fields));
                }
                let count = self.number("the lane count of the vector")?;
                self.expect_word("x")?;
                let element = self.ty()?;
                self.expect(b'>')?;
                return Ok(Type::vector(count, element));
            }
            _ => return Err(self.refuse("a type is expected")),
        };
        self.at += 1;
        Ok(ty)
    }

    /// The type written here, which must be `expected`: the printer spells the type of every
    /// element of an aggregate from the aggregate's own.
    fn typed(&mut self, expected: &Type) -> Result<(), ParseError> {
        let start = self.at;
        if self.ty()? == *expected {
            return Ok(());
        }
        self.at = start;
        Err(self.refuse(&format!("the type here is `{expected}`")))
    }

    /// `@name = [internal] constant|global T [init][, align N]`, on one line, with the address of
    /// the line's comment.
    fn global(&mut self) -> Result<Global, ParseError> {
        let line = self.here();
        let name = self.global_name()?;
        self.expect(b'=')?;
        let linkage = if self.eat_word("internal") { Linkage::Internal } else { Linkage::External };
        let constant = if self.eat_word("constant") {
            true
        } else if self.eat_word("global") {
            false
        } else {
            return Err(self.refuse("`constant` or `global` is expected"));
        };
        let ty = self.ty()?;
        // A global defined elsewhere has no initializer, and its line ends or goes on to `align`.
        let init = if self.on_line(line) && self.peek() != Some(&Tok::Punct(b',')) { Some(self.constant(&ty)?) } else { None };
        let align = if self.on_line(line) && self.eat(b',') {
            self.expect_word("align")?;
            self.number("an alignment")?
        } else {
            0
        };
        self.end_of_line(line)?;
        Ok(Global { name, ty, init, constant, linkage, align, address: self.addresses.get(&line).copied() })
    }

    /// A constant of the type `ty`, which decides how a float and an aggregate are read.
    fn constant(&mut self, ty: &Type) -> Result<Const, ParseError> {
        let constant = match self.peek() {
            Some(Tok::Int(v)) => Const::Int(*v),
            Some(Tok::Hex(digits)) => match float_of(ty, digits) {
                Some(v) => Const::Float(v),
                None => return Err(self.refuse(&format!("is not a float constant of `{ty}`"))),
            },
            Some(Tok::Bytes(bytes)) => Const::Bytes(bytes.clone()),
            Some(Tok::Word("zeroinitializer")) => Const::Zero,
            Some(Tok::Word("null")) => Const::Null,
            Some(Tok::Global(name)) => Const::Global(name.clone()),
            Some(Tok::Word("getelementptr")) if *ty == Type::Ptr => {
                let (global, offset) = self.element_address()?;
                return Ok(Const::Address { global, offset });
            }
            Some(Tok::Word("ptrtoint")) if *ty != Type::Ptr => {
                self.at += 1;
                self.expect(b'(')?;
                self.expect_word("ptr")?;
                let (global, offset) =
                    if self.word() == Some("getelementptr") { self.element_address()? } else { (self.global_name()?, 0) };
                self.expect_word("to")?;
                self.typed(ty)?;
                self.expect(b')')?;
                return Ok(Const::Address { global, offset });
            }
            Some(Tok::Punct(b'[')) => return self.array(ty),
            Some(Tok::Punct(b'{' | b'<')) => return self.aggregate(ty),
            Some(_) => return Err(self.refuse("is not a constant")),
            None => return Err(self.refuse("a constant is expected")),
        };
        self.at += 1;
        Ok(constant)
    }

    /// `getelementptr (i8, ptr @name, i64 offset)`: the global and the byte of it.
    fn element_address(&mut self) -> Result<(String, i64), ParseError> {
        self.expect_word("getelementptr")?;
        self.expect(b'(')?;
        self.expect_word("i8")?;
        self.expect(b',')?;
        self.expect_word("ptr")?;
        let global = self.global_name()?;
        self.expect(b',')?;
        self.expect_word("i64")?;
        let offset = self.number("an offset of 64 bits")?;
        self.expect(b')')?;
        Ok((global, offset))
    }

    /// `[T a, T b]`, each element of the array's element type.
    fn array(&mut self, ty: &Type) -> Result<Const, ParseError> {
        let element = match ty {
            Type::Array(_, element) => element.as_ref(),
            other => other,
        };
        self.expect(b'[')?;
        let mut items = Vec::new();
        if self.eat(b']') {
            return Ok(Const::Array(items));
        }
        loop {
            self.typed(element)?;
            items.push(self.constant(element)?);
            if self.eat(b']') {
                return Ok(Const::Array(items));
            }
            self.expect(b',')?;
        }
    }

    /// `{ T a, U b }` or `<{ T a, U b }>`, each field of the struct's field type. The printer
    /// writes the fields of a type that is not a literal struct as bytes.
    fn aggregate(&mut self, ty: &Type) -> Result<Const, ParseError> {
        let (fields, packed) = match ty {
            Type::Struct(fields) => (Some(fields), false),
            Type::Packed(fields) => (Some(fields), true),
            _ => (None, false),
        };
        if packed {
            self.expect(b'<')?;
        }
        self.expect(b'{')?;
        let mut items = Vec::new();
        if !self.eat(b'}') {
            loop {
                let field = match fields {
                    Some(fields) => fields.get(items.len()).cloned().ok_or_else(|| self.refuse("the struct has no field left for this"))?,
                    None => Type::I8,
                };
                self.typed(&field)?;
                items.push(self.constant(&field)?);
                if self.eat(b'}') {
                    break;
                }
                self.expect(b',')?;
            }
        }
        if packed {
            self.expect(b'>')?;
        }
        Ok(Const::Struct(items))
    }

    /// `declare [conv] T @name(T, U[, ...])[ attributes]`, on one line.
    fn declaration(&mut self) -> Result<Declaration, ParseError> {
        let line = self.here();
        self.expect_word("declare")?;
        let conv = self.conv();
        let ret = self.ty()?;
        let name = self.global_name()?;
        self.expect(b'(')?;
        let (params, varargs) = self.params()?;
        let attrs = self.attributes(line);
        Ok(Declaration { name, ty: FnType { ret, params, varargs }, conv, attrs })
    }

    /// The calling convention named here, or C's when none is.
    fn conv(&mut self) -> CallConv {
        let named = [CallConv::Win64, CallConv::X86Stdcall, CallConv::X86Fastcall, CallConv::X86Thiscall];
        let Some(conv) = named.into_iter().find(|c| self.word() == Some(c.keyword())) else { return CallConv::C };
        self.at += 1;
        conv
    }

    /// The attributes that close `line`, each as the text spells it: the tokens that touch are
    /// one attribute, so `"key"="value"` and `memory(none)` stay whole.
    fn attributes(&mut self, line: usize) -> Vec<String> {
        let mut attrs = Vec::new();
        while self.on_line(line) {
            let start = self.tokens[self.at].start;
            let mut end = self.tokens[self.at].end;
            self.at += 1;
            while self.on_line(line) && self.tokens[self.at].start == end {
                end = self.tokens[self.at].end;
                self.at += 1;
            }
            attrs.push(self.src[start..end].to_owned());
        }
        attrs
    }
}
