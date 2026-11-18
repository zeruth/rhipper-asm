//! `printf` as a console's BIOS or kernel formats it: the conversions of integers, characters and
//! strings over 32-bit arguments, which a caller hands in the order the program passed them.

use crate::{Memory, MemoryExt};

/// The text `format` writes over the arguments `next` gives one at a time; a `%s` reads its
/// string from `memory`.
pub fn format(memory: &mut dyn Memory, format: &[u8], mut next: impl FnMut() -> u32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < format.len() {
        let c = format[at];
        at += 1;
        if c != b'%' {
            out.push(c);
            continue;
        }
        let (mut left, mut zero, mut plus, mut space, mut alt) = (false, false, false, false, false);
        while let Some(&flag) = format.get(at) {
            match flag {
                b'-' => left = true,
                b'0' => zero = true,
                b'+' => plus = true,
                b' ' => space = true,
                b'#' => alt = true,
                _ => break,
            }
            at += 1;
        }
        let number = |at: &mut usize, next: &mut dyn FnMut() -> u32| -> Option<usize> {
            if format.get(*at) == Some(&b'*') {
                *at += 1;
                return Some(next() as usize);
            }
            let start = *at;
            while format.get(*at).is_some_and(u8::is_ascii_digit) {
                *at += 1;
            }
            std::str::from_utf8(&format[start..*at]).ok()?.parse().ok()
        };
        let width = number(&mut at, &mut next).unwrap_or(0);
        let precision = if format.get(at) == Some(&b'.') {
            at += 1;
            Some(number(&mut at, &mut next).unwrap_or(0))
        } else {
            None
        };
        while matches!(format.get(at), Some(b'l' | b'h' | b'z')) {
            at += 1;
        }
        let Some(&conversion) = format.get(at) else { break };
        at += 1;
        let digits = |value: u32, radix: u32, upper: bool| -> Vec<u8> {
            let text = match radix {
                8 => format!("{value:o}"),
                16 if upper => format!("{value:X}"),
                16 => format!("{value:x}"),
                _ => value.to_string(),
            };
            let mut text = text.into_bytes();
            if let Some(p) = precision {
                while text.len() < p {
                    text.insert(0, b'0');
                }
            }
            text
        };
        let (mut sign, body): (Vec<u8>, Vec<u8>) = match conversion {
            b'd' | b'i' => {
                let value = next().cast_signed();
                let sign = match (value < 0, plus, space) {
                    (true, ..) => b"-".to_vec(),
                    (false, true, _) => b"+".to_vec(),
                    (false, false, true) => b" ".to_vec(),
                    _ => Vec::new(),
                };
                (sign, digits(value.unsigned_abs(), 10, false))
            }
            b'u' => (Vec::new(), digits(next(), 10, false)),
            b'o' => (Vec::new(), digits(next(), 8, false)),
            b'x' | b'X' | b'p' => {
                let value = next();
                let prefix = if (alt && value != 0) || conversion == b'p' { b"0x".to_vec() } else { Vec::new() };
                (prefix, digits(value, 16, conversion == b'X'))
            }
            b'c' => (Vec::new(), vec![next() as u8]),
            b's' => {
                let address = next();
                let mut text = if address == 0 { b"(null)".to_vec() } else { memory.cstr(u64::from(address), 1 << 16) };
                if let Some(p) = precision {
                    text.truncate(p);
                }
                (Vec::new(), text)
            }
            b'%' => (Vec::new(), b"%".to_vec()),
            other => (Vec::new(), vec![b'%', other]),
        };
        let pad = width.saturating_sub(sign.len() + body.len());
        let numeric = matches!(conversion, b'd' | b'i' | b'u' | b'o' | b'x' | b'X' | b'p');
        if left {
            out.append(&mut sign);
            out.extend_from_slice(&body);
            out.extend(std::iter::repeat_n(b' ', pad));
        } else if zero && numeric && precision.is_none() {
            out.append(&mut sign);
            out.extend(std::iter::repeat_n(b'0', pad));
            out.extend_from_slice(&body);
        } else {
            out.extend(std::iter::repeat_n(b' ', pad));
            out.append(&mut sign);
            out.extend_from_slice(&body);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ram;

    fn formatted(spec: &str, args: &[u32]) -> String {
        let mut ram = Ram::new(0x100, 16);
        ram.bytes[..3].copy_from_slice(b"ab\0");
        let mut args = args.iter().copied();
        String::from_utf8(format(&mut ram, spec.as_bytes(), || args.next().unwrap_or(0))).unwrap()
    }

    #[test]
    fn the_conversions_a_console_program_prints_with() {
        assert_eq!(formatted("%d %i %u", &[(-5i32).cast_unsigned(), 7, 0xffff_ffff]), "-5 7 4294967295");
        assert_eq!(formatted("%08x|%-4d|%4d|%+d", &[0xbeef, 3, 3, 3]), "0000beef|3   |   3|+3");
        assert_eq!(formatted("%s %c %% %.1s", &[0x100, u32::from(b'z'), 0x100]), "ab z % a");
        assert_eq!(formatted("%p %#X %lu", &[0x8001_0000, 0xab, 9]), "0x80010000 0xAB 9");
        assert_eq!(formatted("%*d|%s", &[3, 1, 0]), "  1|(null)");
    }
}
