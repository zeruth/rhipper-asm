//! The facts of a traced run: what the interpreter saw a program do that no reading of its image
//! shows, kept as JSON (`build/facts.json` in a project) with the input that produced them, and
//! taken by [`crate::analyze_with`] on the next pass. An indirect call the run resolved names its
//! targets, which the analysis adds to the caller's calls and the lift makes direct (with the
//! indirect call kept for any target the run did not see); a target no seed reached becomes a
//! function; the functions entered are marked as seen at run time.
//!
//! ```json
//! {"input": {"exe": "game.exe", "size": 1234, "args": []},
//!  "calls": [{"site": "0x1400011f3", "targets": ["0x140001110", "0x140001120"]}],
//!  "entered": ["0x140001000", "0x140001110"]}
//! ```

use ra_asm_pe::Image;
use serde::{Deserialize, Serialize};

/// The facts of one run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Facts {
    pub input: Input,
    /// The indirect calls and jumps the run resolved, by the address of the instruction.
    pub calls: Vec<SeenCall>,
    /// The entries of the functions the run entered.
    #[serde(with = "hex_list")]
    pub entered: Vec<u64>,
}

/// The program and the arguments a run was of: facts of another input are not taken.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Input {
    /// The file name of the image.
    pub exe: String,
    /// The size of the image file.
    pub size: u64,
    pub args: Vec<String>,
}

/// An indirect call or jump and the functions it reached.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeenCall {
    #[serde(with = "hex")]
    pub site: u64,
    #[serde(with = "hex_list")]
    pub targets: Vec<u64>,
}

impl Input {
    /// The input of a run of `image` with `args`.
    pub fn of(image: &Image, args: &[String]) -> Self {
        let exe = image.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Self { exe, size: file_size(image), args: args.to_vec() }
    }

    /// Whether the facts of this input apply to `image`: the same file name and size.
    pub fn is_of(&self, image: &Image) -> bool {
        let same_name = image.path.as_ref().and_then(|p| p.file_name()).is_some_and(|n| n.to_string_lossy() == self.exe);
        same_name && self.size == file_size(image)
    }
}

/// The size of the image's file, or of its mapped bytes when it has no file.
fn file_size(image: &Image) -> u64 {
    image.path.as_ref().and_then(|p| std::fs::metadata(p).ok()).map_or(image.mapped().len() as u64, |m| m.len())
}

impl Facts {
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty() && self.entered.is_empty()
    }

    /// The targets the run saw the call at `site` reach.
    pub fn targets_of(&self, site: u64) -> &[u64] {
        self.calls.iter().find(|c| c.site == site).map_or(&[], |c| c.targets.as_slice())
    }
}

/// An address as `0x...` in the JSON, as the workbench writes addresses.
mod hex {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};

    // Serde's `with` hands the field by reference.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub(super) fn serialize<S: Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("{v:#x}"))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        parse(&String::deserialize(d)?).map_err(D::Error::custom)
    }

    pub(super) fn parse(text: &str) -> Result<u64, String> {
        let digits = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")).unwrap_or(text);
        u64::from_str_radix(digits, 16).map_err(|_| format!("`{text}` is not an address"))
    }
}

mod hex_list {
    use serde::{Deserialize, Deserializer, Serializer, de::Error, ser::SerializeSeq};

    pub(super) fn serialize<S: Serializer>(v: &[u64], s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(v.len()))?;
        for a in v {
            seq.serialize_element(&format!("{a:#x}"))?;
        }
        seq.end()
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u64>, D::Error> {
        Vec::<String>::deserialize(d)?.iter().map(|t| super::hex::parse(t).map_err(D::Error::custom)).collect()
    }
}
