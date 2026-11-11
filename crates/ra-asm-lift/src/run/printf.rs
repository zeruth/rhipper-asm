//! The `printf` family's formatting, for the stdio functions the interpreter answers itself so a
//! program's output is captured rather than written by the host's C runtime. The conversions of
//! C99 and MSVC's lengths (`I64`, `I32`, `I`, `w`), with the arguments pulled from wherever the
//! caller keeps them: the integer slots of a Win64 call, or a `va_list` in the program's memory.

/// Where the conversions' arguments come from.
pub(super) trait Args {
    /// The next integer argument; `wide` when the conversion takes 64 bits (two slots of a 32-bit
    /// `va_list`).
    fn int(&mut self, wide: bool) -> u64;
    fn double(&mut self) -> f64;
    /// The bytes of the C string at `address`, without its NUL.
    fn string(&mut self, address: u64) -> Vec<u8>;
    /// The code units of the UTF-16 string at `address`, without its NUL.
    fn wstring(&mut self, address: u64) -> Vec<u16>;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Length {
    None,
    Char,
    Short,
    Long,
    LongLong,
    /// `size_t` and `ptrdiff_t`: the program's pointer width.
    Size,
    /// MSVC's `w`: a wide character or string.
    Wide,
}

#[derive(Default)]
struct Spec {
    left: bool,
    plus: bool,
    space: bool,
    alt: bool,
    zero: bool,
    width: Option<usize>,
    precision: Option<usize>,
}

/// Formats `spec` with `args`; `bits` is the program's pointer width, for `%p` and `%zu`.
pub(super) fn format(spec: &[u8], bits: u32, args: &mut dyn Args) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < spec.len() {
        let c = spec[i];
        i += 1;
        if c != b'%' {
            out.push(c);
            continue;
        }
        let mut s = Spec::default();
        while let Some(&f) = spec.get(i) {
            match f {
                b'-' => s.left = true,
                b'+' => s.plus = true,
                b' ' => s.space = true,
                b'#' => s.alt = true,
                b'0' => s.zero = true,
                _ => break,
            }
            i += 1;
        }
        if spec.get(i) == Some(&b'*') {
            i += 1;
            let w = args.int(false) as i32;
            s.left |= w < 0;
            s.width = Some(w.unsigned_abs() as usize);
        } else {
            s.width = digits(spec, &mut i);
        }
        if spec.get(i) == Some(&b'.') {
            i += 1;
            if spec.get(i) == Some(&b'*') {
                i += 1;
                let p = args.int(false) as i32;
                s.precision = (p >= 0).then_some(p as usize);
            } else {
                s.precision = Some(digits(spec, &mut i).unwrap_or(0));
            }
        }
        let length = match (spec.get(i), spec.get(i + 1)) {
            (Some(b'h'), Some(b'h')) => {
                i += 2;
                Length::Char
            }
            (Some(b'h'), _) => {
                i += 1;
                Length::Short
            }
            (Some(b'l'), Some(b'l')) => {
                i += 2;
                Length::LongLong
            }
            (Some(b'l'), _) => {
                i += 1;
                Length::Long
            }
            (Some(b'j' | b'L'), _) => {
                i += 1;
                Length::LongLong
            }
            (Some(b'I'), Some(b'6')) if spec.get(i + 2) == Some(&b'4') => {
                i += 3;
                Length::LongLong
            }
            (Some(b'I'), Some(b'3')) if spec.get(i + 2) == Some(&b'2') => {
                i += 3;
                Length::Long
            }
            (Some(b'z' | b't' | b'I'), _) => {
                i += 1;
                Length::Size
            }
            (Some(b'w'), _) => {
                i += 1;
                Length::Wide
            }
            _ => Length::None,
        };
        let Some(&conv) = spec.get(i) else { break };
        i += 1;
        let int_bits = match length {
            Length::Char => 8,
            Length::Short => 16,
            Length::LongLong => 64,
            Length::Size => bits,
            _ => 32,
        };
        match conv {
            b'%' => out.push(b'%'),
            b'd' | b'i' => {
                let raw = args.int(int_bits == 64);
                let v = ra_asm_ir::interp::sext(u128::from(raw), int_bits) as i64;
                let sign = if v < 0 {
                    "-"
                } else if s.plus {
                    "+"
                } else if s.space {
                    " "
                } else {
                    ""
                };
                number(&mut out, &s, sign, &v.unsigned_abs().to_string());
            }
            b'u' | b'o' | b'x' | b'X' => {
                let raw = args.int(int_bits == 64) & ra_asm_ir::interp::mask(int_bits) as u64;
                let (digits, prefix) = match conv {
                    b'o' => (format!("{raw:o}"), if s.alt && raw != 0 { "0" } else { "" }),
                    b'x' => (format!("{raw:x}"), if s.alt && raw != 0 { "0x" } else { "" }),
                    b'X' => (format!("{raw:X}"), if s.alt && raw != 0 { "0X" } else { "" }),
                    _ => (raw.to_string(), ""),
                };
                number(&mut out, &s, prefix, &digits);
            }
            b'p' => {
                let raw = args.int(bits == 64);
                let hex = format!("{raw:0width$X}", width = (bits / 4) as usize);
                pad(&mut out, &s, hex.as_bytes());
            }
            b'c' | b'C' => {
                let raw = args.int(false);
                let text = if length == Length::Long || length == Length::Wide || conv == b'C' {
                    String::from_utf16_lossy(&[raw as u16]).into_bytes()
                } else {
                    vec![raw as u8]
                };
                pad(&mut out, &s, &text);
            }
            b's' | b'S' => {
                let p = args.int(bits == 64);
                let mut text = if p == 0 {
                    b"(null)".to_vec()
                } else if length == Length::Long || length == Length::Wide || conv == b'S' {
                    String::from_utf16_lossy(&args.wstring(p)).into_bytes()
                } else {
                    args.string(p)
                };
                if let Some(p) = s.precision {
                    text.truncate(p);
                }
                pad(&mut out, &s, &text);
            }
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' | b'a' | b'A' => {
                let v = args.double();
                let sign = if v.is_sign_negative() && !v.is_nan() {
                    "-"
                } else if s.plus {
                    "+"
                } else if s.space {
                    " "
                } else {
                    ""
                };
                let magnitude = v.abs();
                let mut text = if magnitude.is_nan() {
                    "nan".to_owned()
                } else if magnitude.is_infinite() {
                    "inf".to_owned()
                } else {
                    float(magnitude, conv.to_ascii_lowercase(), s.precision, s.alt)
                };
                if conv.is_ascii_uppercase() {
                    text.make_ascii_uppercase();
                }
                let zero = s.zero && magnitude.is_finite();
                number(&mut out, &Spec { zero, precision: None, ..s }, sign, &text);
            }
            b'n' => {
                args.int(bits == 64);
            }
            other => {
                out.push(b'%');
                out.push(other);
            }
        }
    }
    out
}

fn digits(spec: &[u8], i: &mut usize) -> Option<usize> {
    let start = *i;
    while spec.get(*i).is_some_and(u8::is_ascii_digit) {
        *i += 1;
    }
    (start < *i).then(|| std::str::from_utf8(&spec[start..*i]).ok().and_then(|d| d.parse().ok()).unwrap_or(0))
}

/// A number with its sign or prefix, the precision's zeros, and the width's padding (zeros after
/// the sign when the flag asks and no precision is given).
fn number(out: &mut Vec<u8>, s: &Spec, prefix: &str, digits: &str) {
    let mut body = digits.to_owned();
    if let Some(p) = s.precision {
        if p == 0 && digits == "0" {
            body.clear();
        }
        while body.len() < p {
            body.insert(0, '0');
        }
    }
    let width = s.width.unwrap_or(0);
    let len = prefix.len() + body.len();
    if s.left {
        out.extend_from_slice(prefix.as_bytes());
        out.extend_from_slice(body.as_bytes());
        out.extend(std::iter::repeat_n(b' ', width.saturating_sub(len)));
    } else if s.zero && s.precision.is_none() {
        out.extend_from_slice(prefix.as_bytes());
        out.extend(std::iter::repeat_n(b'0', width.saturating_sub(len)));
        out.extend_from_slice(body.as_bytes());
    } else {
        out.extend(std::iter::repeat_n(b' ', width.saturating_sub(len)));
        out.extend_from_slice(prefix.as_bytes());
        out.extend_from_slice(body.as_bytes());
    }
}

/// Text padded to the width with spaces.
fn pad(out: &mut Vec<u8>, s: &Spec, text: &[u8]) {
    let fill = s.width.unwrap_or(0).saturating_sub(text.len());
    if s.left {
        out.extend_from_slice(text);
        out.extend(std::iter::repeat_n(b' ', fill));
    } else {
        out.extend(std::iter::repeat_n(if s.zero { b'0' } else { b' ' }, fill));
        out.extend_from_slice(text);
    }
}

/// A finite, non-negative double in the style of `conv` (`f`, `e`, `g` or `a`).
fn float(v: f64, conv: u8, precision: Option<usize>, alt: bool) -> String {
    match conv {
        b'f' => fixed(v, precision.unwrap_or(6), alt),
        b'e' | b'a' => exponential(v, precision.unwrap_or(6), alt),
        _ => {
            // `%g`: the shorter of the two styles by the exponent, without trailing zeros.
            let p = match precision.unwrap_or(6) {
                0 => 1,
                p => p,
            };
            let x = if v == 0.0 { 0 } else { exponent_of(v, p - 1) };
            let text = if x >= -4 && (x as usize) < p { fixed(v, p - 1 - x.max(0) as usize, alt) } else { exponential(v, p - 1, alt) };
            if alt { text } else { strip_zeros(&text) }
        }
    }
}

fn fixed(v: f64, precision: usize, alt: bool) -> String {
    let mut text = format!("{v:.precision$}");
    if alt && precision == 0 {
        text.push('.');
    }
    text
}

fn exponential(v: f64, precision: usize, alt: bool) -> String {
    let text = format!("{v:.precision$e}");
    let (mantissa, exponent) = text.split_once('e').unwrap_or((&text, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let dot = if alt && precision == 0 { "." } else { "" };
    format!("{mantissa}{dot}e{}{:02}", if exponent < 0 { '-' } else { '+' }, exponent.abs())
}

/// The decimal exponent of `v` once rounded to `precision` digits after the point of its
/// exponential form.
fn exponent_of(v: f64, precision: usize) -> i32 {
    let text = format!("{v:.precision$e}");
    text.split_once('e').and_then(|(_, e)| e.parse().ok()).unwrap_or(0)
}

/// The trailing zeros of the fraction removed, and the point with them when nothing follows.
fn strip_zeros(text: &str) -> String {
    let (mantissa, exponent) = match text.find('e') {
        Some(at) => (&text[..at], &text[at..]),
        None => (text, ""),
    };
    let mantissa = if mantissa.contains('.') { mantissa.trim_end_matches('0').trim_end_matches('.') } else { mantissa };
    format!("{mantissa}{exponent}")
}
