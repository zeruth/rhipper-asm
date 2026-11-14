//! The manifest of `testdata`: `testdata/MANIFEST.toml` lists a committed binary with its SHA-256,
//! the source and the command that made it, the tools' versions, its expected output and its
//! evidence, so that a binary with no record of how it was made cannot slip in. The file is a
//! list of `[[file]]` tables whose values are all strings, read here without a TOML library:
//! a line is a table header, a `key = "string"` pair, a comment or blank, and anything else is
//! refused with its line number.

use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};

/// Where a listed file's expected output comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    /// The program ran on this machine.
    Native,
    /// The program ran in an emulator of its machine.
    Emulator,
    /// Derived from the source and the platform's documentation; no machine ran it.
    Source,
}

impl Evidence {
    /// The word the manifest writes.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Emulator => "emulator",
            Self::Source => "source",
        }
    }

    fn parse(word: &str) -> Option<Self> {
        match word {
            "native" => Some(Self::Native),
            "emulator" => Some(Self::Emulator),
            "source" => Some(Self::Source),
            _ => None,
        }
    }
}

/// One `[[file]]` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The file, relative to the workspace root, with `/` separators.
    pub path: String,
    /// The lowercase hex SHA-256 of its bytes.
    pub sha256: String,
    pub evidence: Evidence,
    /// The expected output the file is compared with, relative to the workspace root.
    pub expected: Option<String>,
    /// The source it was built from.
    pub source: Option<String>,
    /// The command that made it.
    pub command: Option<String>,
    /// The tools and versions the command ran with.
    pub tools: Option<String>,
}

/// What stops a manifest from being read: the line and what is wrong with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MANIFEST.toml:{}: {}", self.line, self.message)
    }
}

impl std::error::Error for Error {}

/// The workspace's `testdata/MANIFEST.toml`.
#[must_use]
pub fn path() -> PathBuf {
    workspace().join("testdata/MANIFEST.toml")
}

/// The workspace root, which the manifest's paths are relative to.
#[must_use]
pub fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The entries of the manifest text, in order.
///
/// # Errors
/// The first line that is not a `[[file]]` header, a string pair, a comment or blank; a key that
/// is not known or repeats; a table missing `path`, `sha256` or `evidence`; an evidence word that
/// is not one of the three; a hash that is not 64 hex digits.
pub fn parse(text: &str) -> Result<Vec<Entry>, Error> {
    let mut entries = Vec::new();
    let mut table: Option<(usize, Vec<(String, String)>)> = None;
    for (index, raw) in text.lines().enumerate() {
        let number = index + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "[[file]]" {
            if let Some((start, pairs)) = table.take() {
                entries.push(entry(start, &pairs)?);
            }
            table = Some((number, Vec::new()));
            continue;
        }
        let (key, value) = pair(number, line)?;
        let Some((_, pairs)) = table.as_mut() else {
            return Err(Error { line: number, message: format!("`{key}` before the first [[file]]") });
        };
        if pairs.iter().any(|(k, _)| *k == key) {
            return Err(Error { line: number, message: format!("`{key}` is given twice") });
        }
        pairs.push((key, value));
    }
    if let Some((start, pairs)) = table {
        entries.push(entry(start, &pairs)?);
    }
    Ok(entries)
}

/// `key = "value"`: a bare key and a basic string, whose escapes are `\"` and `\\`.
fn pair(number: usize, line: &str) -> Result<(String, String), Error> {
    let error = |message: String| Error { line: number, message };
    let (key, rest) = line.split_once('=').ok_or_else(|| error(format!("not a `key = \"value\"` line: {line}")))?;
    let key = key.trim();
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(error(format!("not a bare key: `{key}`")));
    }
    let rest = rest.trim();
    let Some(body) = rest.strip_prefix('"') else {
        return Err(error(format!("`{key}` is not a string: {rest}")));
    };
    let mut value = String::new();
    let mut chars = body.chars();
    loop {
        match chars.next() {
            None => return Err(error(format!("`{key}` has no closing quote"))),
            Some('"') => break,
            Some('\\') => match chars.next() {
                Some('"') => value.push('"'),
                Some('\\') => value.push('\\'),
                other => return Err(error(format!("`{key}` has an escape this reader does not take: \\{}", other.unwrap_or(' ')))),
            },
            Some(c) => value.push(c),
        }
    }
    let tail = chars.as_str().trim();
    if !tail.is_empty() && !tail.starts_with('#') {
        return Err(error(format!("text after the string of `{key}`: {tail}")));
    }
    Ok((key.to_owned(), value))
}

fn entry(line: usize, pairs: &[(String, String)]) -> Result<Entry, Error> {
    let error = |message: String| Error { line, message };
    let get = |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    let required = |key: &str| get(key).ok_or_else(|| error(format!("[[file]] without `{key}`")));
    for (key, _) in pairs {
        if !matches!(key.as_str(), "path" | "sha256" | "evidence" | "expected" | "source" | "command" | "tools") {
            return Err(error(format!("`{key}` is not a key of a [[file]]")));
        }
    }
    let path = required("path")?;
    let sha256 = required("sha256")?;
    if sha256.len() != 64 || !sha256.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
        return Err(error(format!("`sha256` of {path} is not 64 lowercase hex digits")));
    }
    let evidence = required("evidence")?;
    let evidence =
        Evidence::parse(&evidence).ok_or_else(|| error(format!("`evidence` of {path} is `{evidence}`, not native, emulator or source")))?;
    Ok(Entry { path, sha256, evidence, expected: get("expected"), source: get("source"), command: get("command"), tools: get("tools") })
}

/// The lowercase hex SHA-256 of `bytes` (FIPS 180-4).
#[must_use]
pub fn sha256(bytes: &[u8]) -> String {
    #[rustfmt::skip]
    const K: [u32; 64] = [
        0x428a_2f98, 0x7137_4491, 0xb5c0_fbcf, 0xe9b5_dba5, 0x3956_c25b, 0x59f1_11f1, 0x923f_82a4, 0xab1c_5ed5, 0xd807_aa98, 0x1283_5b01,
        0x2431_85be, 0x550c_7dc3, 0x72be_5d74, 0x80de_b1fe, 0x9bdc_06a7, 0xc19b_f174, 0xe49b_69c1, 0xefbe_4786, 0x0fc1_9dc6, 0x240c_a1cc,
        0x2de9_2c6f, 0x4a74_84aa, 0x5cb0_a9dc, 0x76f9_88da, 0x983e_5152, 0xa831_c66d, 0xb003_27c8, 0xbf59_7fc7, 0xc6e0_0bf3, 0xd5a7_9147,
        0x06ca_6351, 0x1429_2967, 0x27b7_0a85, 0x2e1b_2138, 0x4d2c_6dfc, 0x5338_0d13, 0x650a_7354, 0x766a_0abb, 0x81c2_c92e, 0x9272_2c85,
        0xa2bf_e8a1, 0xa81a_664b, 0xc24b_8b70, 0xc76c_51a3, 0xd192_e819, 0xd699_0624, 0xf40e_3585, 0x106a_a070, 0x19a4_c116, 0x1e37_6c08,
        0x2748_774c, 0x34b0_bcb5, 0x391c_0cb3, 0x4ed8_aa4a, 0x5b9c_ca4f, 0x682e_6ff3, 0x748f_82ee, 0x78a5_636f, 0x84c8_7814, 0x8cc7_0208,
        0x90be_fffa, 0xa450_6ceb, 0xbef9_a3f7, 0xc671_78f2,
    ];
    let mut state: [u32; 8] = [0x6a09_e667, 0xbb67_ae85, 0x3c6e_f372, 0xa54f_f53a, 0x510e_527f, 0x9b05_688c, 0x1f83_d9ab, 0x5be0_cd19];
    // The padding: a 1 bit, zeros to 56 mod 64, the bit length in 8 big-endian bytes.
    let mut message = bytes.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((bytes.len() as u64) * 8).to_be_bytes());
    for block in message.chunks_exact(64) {
        let mut schedule = [0u32; 64];
        for (slot, word) in schedule.iter_mut().zip(block.chunks_exact(4)) {
            *slot = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = schedule[i - 15].rotate_right(7) ^ schedule[i - 15].rotate_right(18) ^ (schedule[i - 15] >> 3);
            let s1 = schedule[i - 2].rotate_right(17) ^ schedule[i - 2].rotate_right(19) ^ (schedule[i - 2] >> 10);
            schedule[i] = schedule[i - 16].wrapping_add(s0).wrapping_add(schedule[i - 7]).wrapping_add(s1);
        }
        // The working variables a..h of the standard are `working[0..8]`: each round shifts them
        // down one and puts the two new values at the top and the middle.
        let mut working = state;
        for (constant, word) in K.iter().zip(schedule) {
            let (top, fifth) = (working[0], working[4]);
            let s1 = fifth.rotate_right(6) ^ fifth.rotate_right(11) ^ fifth.rotate_right(25);
            let choice = (fifth & working[5]) ^ (!fifth & working[6]);
            let t1 = working[7].wrapping_add(s1).wrapping_add(choice).wrapping_add(*constant).wrapping_add(word);
            let s0 = top.rotate_right(2) ^ top.rotate_right(13) ^ top.rotate_right(22);
            let majority = (top & working[1]) ^ (top & working[2]) ^ (working[1] & working[2]);
            working = [
                t1.wrapping_add(s0.wrapping_add(majority)),
                working[0],
                working[1],
                working[2],
                working[3].wrapping_add(t1),
                working[4],
                working[5],
                working[6],
            ];
        }
        for (sum, value) in state.iter_mut().zip(working) {
            *sum = sum.wrapping_add(value);
        }
    }
    let mut hex = String::with_capacity(64);
    for v in state {
        write!(hex, "{v:08x}").expect("a String takes every write");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::{Evidence, parse, sha256};

    /// The two test vectors of FIPS 180-4 and the empty message.
    #[test]
    fn sha256_matches_the_standard_vectors() {
        assert_eq!(sha256(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        // Two blocks with the length in the second: 56 bytes of padding boundary.
        assert_eq!(sha256(&[b'a'; 56]), "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a");
    }

    #[test]
    fn a_table_reads_and_a_bad_line_names_itself() {
        let text = "# the manifest\n[[file]]\npath = \"testdata/bin/x\"\nsha256 = \"";
        let text = format!("{text}{}\"\nevidence = \"emulator\" # ran in qemu\ncommand = \"a \\\"quoted\\\" word\"\n", "0".repeat(64));
        let entries = parse(&text).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].evidence, Evidence::Emulator);
        assert_eq!(entries[0].command.as_deref(), Some("a \"quoted\" word"));
        assert_eq!(entries[0].expected, None);
        let bad = parse("[[file]]\npath = \"x\"\nsha256 = \"00\"\nevidence = \"native\"\n").unwrap_err();
        assert!(bad.message.contains("sha256"), "{bad}");
        let bad = parse("[[file]]\npath = \"x\"\nsize = 3\n").unwrap_err();
        assert_eq!(bad.line, 3, "{bad}");
        let bad = parse("path = \"x\"\n").unwrap_err();
        assert_eq!(bad.line, 1, "{bad}");
    }
}
