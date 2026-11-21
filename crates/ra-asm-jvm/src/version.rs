//! The releases of Java by the major version of the class files they write, and what each
//! brought to the class file or took from it.

/// The major version of the first release's class files (Java 1.0 and 1.1).
pub const FIRST: u16 = 45;
/// The major version of the newest release read (Java 25).
pub const LAST: u16 = 69;

/// The number of the release that writes class files of `major`: 1 for Java 1.0 and 1.1, 2 to 4
/// for Java 1.2 to 1.4, and the release's own number from Java 5 on. `None` for a version no
/// release read here writes.
#[must_use]
pub fn release(major: u16) -> Option<u16> {
    (FIRST..=LAST).contains(&major).then(|| major - 44)
}

/// The major version the release numbered `release` writes ([`release`]'s inverse).
#[must_use]
pub fn major(release: u16) -> Option<u16> {
    (1..=LAST - 44).contains(&release).then(|| release + 44)
}

/// The release's name as it was sold: `1.1`, `1.4`, `5`, `21`.
#[must_use]
pub fn name(major: u16) -> Option<String> {
    let release = release(major)?;
    Some(if release < 5 { format!("1.{release}") } else { release.to_string() })
}

/// The first major version whose constant pool may hold a constant of `tag`: the method handles,
/// the method types and `invokedynamic`'s call sites came with Java 7, modules and packages with
/// Java 9, and the constants computed at run time with Java 11.
#[must_use]
pub fn constant_since(tag: u8) -> u16 {
    match tag {
        15 | 16 | 18 => 51,
        19 | 20 => 53,
        17 => 55,
        _ => FIRST,
    }
}

/// Whether a method of a class of `major` may hold the instruction `opcode`: `invokedynamic`
/// came with Java 7, whose class files lost the subroutines (`jsr`, `ret`, `jsr_w`) that
/// compilers had made `finally` of.
#[must_use]
pub fn has_opcode(major: u16, opcode: u8) -> bool {
    match opcode {
        0xba => major >= 51,
        0xa8 | 0xa9 | 0xc9 => major < 51,
        _ => opcode <= 0xc9,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_is_its_major_version() {
        assert_eq!(release(45), Some(1));
        assert_eq!(release(52), Some(8));
        assert_eq!(release(69), Some(25));
        assert_eq!(release(44), None);
        assert_eq!(release(70), None);
        assert_eq!(major(8), Some(52));
        assert_eq!(major(0), None);
        assert_eq!(name(48).as_deref(), Some("1.4"));
        assert_eq!(name(49).as_deref(), Some("5"));
        assert_eq!(name(65).as_deref(), Some("21"));
    }

    #[test]
    fn a_release_has_what_it_had() {
        assert!(!has_opcode(50, 0xba));
        assert!(has_opcode(51, 0xba));
        assert!(has_opcode(50, 0xa8));
        assert!(!has_opcode(51, 0xa8));
        assert!(!has_opcode(69, 0xca));
        assert_eq!(constant_since(18), 51);
        assert_eq!(constant_since(17), 55);
        assert_eq!(constant_since(19), 53);
        assert_eq!(constant_since(1), 45);
    }
}
