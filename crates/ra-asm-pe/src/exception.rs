//! The x64 exception directory: one `RUNTIME_FUNCTION` per function or out-of-line piece, with the
//! unwind information that says how the prolog built the frame and whether the entry chains to a
//! parent. The function extents of an x64 image come from here.

use anyhow::{Context, Result, ensure};
use serde::Serialize;

use crate::headers::{Directory, Machine};
use crate::image::Image;

/// One entry of the exception table: a range of code and its unwind information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RuntimeFunction {
    pub begin: u64,
    /// The first address after the range.
    pub end: u64,
    pub unwind: u64,
}

impl RuntimeFunction {
    pub fn len(&self) -> u64 {
        self.end - self.begin
    }

    pub fn is_empty(&self) -> bool {
        self.end <= self.begin
    }

    pub fn contains(&self, va: u64) -> bool {
        va >= self.begin && va < self.end
    }

    /// Reads the unwind information of the entry.
    pub fn unwind_info(&self, image: &Image) -> Result<UnwindInfo> {
        UnwindInfo::parse(image, self.unwind)
    }
}

/// One unwind code: an operation of the prolog, two bytes each. Kept raw: the lift reads the prolog
/// itself, and the codes serve as a check.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct UnwindCode {
    pub prolog_offset: u8,
    pub operation: u8,
    pub info: u8,
}

/// The `UNWIND_INFO` of a runtime function.
#[derive(Debug, Clone, Serialize)]
pub struct UnwindInfo {
    pub version: u8,
    pub flags: u8,
    pub prolog_size: u8,
    /// The register that holds the frame pointer, as an x64 register number, or `None`.
    pub frame_register: Option<u8>,
    /// The scaled offset of the frame pointer from `rsp`, in bytes.
    pub frame_offset: u16,
    pub codes: Vec<UnwindCode>,
    /// The address of the exception or termination handler.
    pub handler: Option<u64>,
    /// The parent entry of a chained entry: this range is a piece of that function.
    pub chained: Option<RuntimeFunction>,
}

impl UnwindInfo {
    pub const EHANDLER: u8 = 0x1;
    pub const UHANDLER: u8 = 0x2;
    pub const CHAININFO: u8 = 0x4;

    pub fn parse(image: &Image, va: u64) -> Result<Self> {
        let head = image.read(va, 4).with_context(|| format!("UNWIND_INFO at {va:#x}"))?;
        let version = head[0] & 0x7;
        let flags = head[0] >> 3;
        let prolog_size = head[1];
        let count = head[2] as usize;
        let frame_register = (head[3] & 0xf != 0).then_some(head[3] & 0xf);
        let frame_offset = u16::from(head[3] >> 4) * 16;
        let raw = image.read(va + 4, count * 2).with_context(|| format!("the {count} unwind codes at {va:#x}"))?;
        let codes = raw.chunks_exact(2).map(|c| UnwindCode { prolog_offset: c[0], operation: c[1] & 0xf, info: c[1] >> 4 }).collect();
        // The codes are padded to an even count; the handler or the chained entry follows.
        let after = va + 4 + ((count + 1) & !1) as u64 * 2;
        let mut handler = None;
        let mut chained = None;
        if flags & Self::CHAININFO != 0 {
            chained = Some(read_entry(image, after).with_context(|| format!("the chained entry of UNWIND_INFO at {va:#x}"))?);
        } else if flags & (Self::EHANDLER | Self::UHANDLER) != 0 {
            handler = Some(image.va(image.u32(after).with_context(|| format!("the handler of UNWIND_INFO at {va:#x}"))?));
        }
        Ok(Self { version, flags, prolog_size, frame_register, frame_offset, codes, handler, chained })
    }

    pub fn is_chained(&self) -> bool {
        self.chained.is_some()
    }
}

fn read_entry(image: &Image, va: u64) -> Result<RuntimeFunction> {
    let begin = image.u32(va)?;
    let end = image.u32(va + 4)?;
    let unwind = image.u32(va + 8)?;
    ensure!(end >= begin, "RUNTIME_FUNCTION at {va:#x} ends ({end:#x}) before it begins ({begin:#x})");
    Ok(RuntimeFunction { begin: image.va(begin), end: image.va(end), unwind: image.va(unwind) })
}

pub(crate) fn parse(image: &Image) -> Result<Vec<RuntimeFunction>> {
    if image.machine() != Machine::Amd64 {
        return Ok(Vec::new()); // ARM64 has its own entry layout; it comes with that architecture
    }
    let Some((va, size)) = image.directory_va(Directory::Exception) else { return Ok(Vec::new()) };
    let count = size as u64 / 12;
    let mut functions = Vec::with_capacity(count as usize);
    for i in 0..count {
        let entry = read_entry(image, va + i * 12).with_context(|| format!("exception table entry {i}"))?;
        if !entry.is_empty() {
            functions.push(entry);
        }
    }
    functions.sort_by_key(|f| f.begin);
    Ok(functions)
}
