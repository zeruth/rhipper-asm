//! The exception directory: one `RUNTIME_FUNCTION` per function or out-of-line piece, with the
//! unwind information that says how the prolog built the frame and whether the entry chains to a
//! parent. The function extents of an x64 or ARM64 image come from here. On x64 the entry names
//! its range and its `UNWIND_INFO`; on ARM64 it names its start and either packs a canonical
//! prolog into its second word or names an `.xdata` record with the function's length, its epilogs
//! and its unwind codes ([`Arm64Unwind`]). The tables of the C++ frame handler hang off it too:
//! the `FuncInfo` a handler's data names (the try blocks, the catch handlers, the unwind actions
//! and the state of each range of code) and the `ThrowInfo` a throw site passes (the types a
//! thrown object can be caught as).

use std::collections::BTreeSet;

use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;

use crate::headers::{Directory, Machine};
use crate::image::Image;

/// One entry of the exception table: a range of code and its unwind information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RuntimeFunction {
    pub begin: u64,
    /// The first address after the range.
    pub end: u64,
    /// The address of the unwind data: x64's `UNWIND_INFO`, or ARM64's `.xdata` record; zero for
    /// an ARM64 entry that packs its unwind data into `packed`.
    pub unwind: u64,
    /// The second word of an ARM64 entry whose unwind data is packed into it.
    pub packed: Option<u32>,
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

    /// Reads the x64 unwind information of the entry.
    pub fn unwind_info(&self, image: &Image) -> Result<UnwindInfo> {
        ensure!(image.machine() == Machine::Amd64, "the entry at {:#x} is not an x64 entry: see arm64_unwind", self.begin);
        UnwindInfo::parse(image, self.unwind)
    }

    /// Reads the ARM64 unwind data of the entry.
    pub fn arm64_unwind(&self, image: &Image) -> Result<Arm64Unwind> {
        ensure!(image.machine() == Machine::Arm64, "the entry at {:#x} is not an ARM64 entry: see unwind_info", self.begin);
        match self.packed {
            Some(word) => Ok(Arm64Unwind::packed(word)),
            None => Arm64Unwind::parse(image, self.unwind).map(|(_, unwind)| unwind),
        }
    }
}

/// The unwind data of an ARM64 entry: a canonical prolog packed into the entry, or the `.xdata`
/// record the entry names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum Arm64Unwind {
    Packed {
        /// A fragment: the range has no prolog of its own (flag 2).
        fragment: bool,
        /// The count of `d8..` registers saved, as the field has it (0..=7 for `d8..d15`).
        regf: u8,
        /// The count of `x19..` registers saved (0..=10, for `x19..x28`).
        regi: u8,
        /// Whether the prolog homes the integer argument registers.
        homes_arguments: bool,
        /// How the frame chain is kept: 0 unchained, 1 with the link register signed, 2
        /// reserved, 3 `x29`/`x30` saved and `x29` set as the frame pointer.
        cr: u8,
        /// The bytes the prolog allocates.
        frame_size: u32,
    },
    Full {
        version: u8,
        /// The epilog scopes: the offset of each epilog from the start, in bytes, and the index
        /// of its first unwind code. Empty when the one epilog shares the prolog's codes.
        epilogs: Vec<Arm64Epilog>,
        /// The unwind codes, as encoded: see [`Arm64Unwind::operations`].
        codes: Vec<u8>,
        /// The exception handler and its data, when the record names one.
        handler: Option<u64>,
        handler_data: Option<u64>,
    },
}

/// One epilog scope of an ARM64 `.xdata` record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Arm64Epilog {
    pub start_offset: u32,
    pub code_index: u32,
}

/// One ARM64 unwind code: what the prolog did, in prolog order; an epilog's codes undo them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Arm64UnwindOp {
    /// `sub sp, sp, #bytes`.
    Alloc(u32),
    /// `stp x29, x30, [sp, #offset]`, or the pre-indexed form that also allocates.
    SaveFpLr {
        offset: u32,
        pre_indexed: bool,
    },
    /// `stp x19, x20, [sp, #-offset]!`.
    SaveR19R20X(u32),
    /// `str x<reg>, [sp, #offset]`, pre-indexed or not.
    SaveReg {
        reg: u8,
        offset: u32,
        pre_indexed: bool,
    },
    /// `stp x<reg>, x<reg + 1>, [sp, #offset]`, pre-indexed or not.
    SaveRegPair {
        reg: u8,
        offset: u32,
        pre_indexed: bool,
    },
    /// `stp x<reg>, lr, [sp, #offset]`.
    SaveLrPair {
        reg: u8,
        offset: u32,
    },
    /// `str d<reg>, [sp, #offset]`, pre-indexed or not.
    SaveFReg {
        reg: u8,
        offset: u32,
        pre_indexed: bool,
    },
    /// `stp d<reg>, d<reg + 1>, [sp, #offset]`, pre-indexed or not.
    SaveFRegPair {
        reg: u8,
        offset: u32,
        pre_indexed: bool,
    },
    /// `mov x29, sp`.
    SetFp,
    /// `add x29, sp, #bytes`.
    AddFp(u32),
    /// The register pair or register after the last saved one, at the next slot.
    SaveNext,
    /// A save of any register through the three-byte form (ARM64EC and SVE).
    SaveAnyReg,
    Nop,
    End,
    /// The end of the prolog shared with a chained fragment.
    EndC,
    TrapFrame,
    MachineFrame,
    Context,
    EcContext,
    ClearUnwoundToCall,
    PacSignLr,
    /// A code the format reserves, with its first byte.
    Reserved(u8),
}

impl Arm64Unwind {
    fn packed(word: u32) -> Self {
        Self::Packed {
            fragment: word & 3 == 2,
            regf: ((word >> 13) & 7) as u8,
            regi: ((word >> 16) & 0xf) as u8,
            homes_arguments: (word >> 20) & 1 != 0,
            cr: ((word >> 21) & 3) as u8,
            frame_size: ((word >> 23) & 0x1ff) * 16,
        }
    }

    /// Reads the `.xdata` record at `va`: the record and the entry's extent as the record gives it
    /// (`begin` is zero: the caller knows it).
    pub fn parse(image: &Image, va: u64) -> Result<(u64, Self)> {
        let head = image.u32(va).with_context(|| format!("the ARM64 unwind record at {va:#x}"))?;
        let length = u64::from(head & 0x3ffff) * 4;
        let version = ((head >> 18) & 3) as u8;
        let has_handler = (head >> 20) & 1 != 0;
        let single_epilog = (head >> 21) & 1 != 0;
        let mut epilog_count = (head >> 22) & 0x1f;
        let mut code_words = head >> 27;
        let mut at = va + 4;
        if epilog_count == 0 && code_words == 0 {
            let extended = image.u32(at).with_context(|| format!("the extended header of the ARM64 unwind record at {va:#x}"))?;
            epilog_count = extended & 0xffff;
            code_words = (extended >> 16) & 0xff;
            at += 4;
        }
        let mut epilogs = Vec::new();
        if !single_epilog {
            for n in 0..epilog_count {
                let scope = image.u32(at).with_context(|| format!("epilog scope {n} of the ARM64 unwind record at {va:#x}"))?;
                epilogs.push(Arm64Epilog { start_offset: (scope & 0x3ffff) * 4, code_index: scope >> 22 });
                at += 4;
            }
        }
        let codes = image
            .read(at, code_words as usize * 4)
            .with_context(|| format!("the {code_words} code words of the ARM64 unwind record at {va:#x}"))?
            .to_vec();
        at += u64::from(code_words) * 4;
        let (mut handler, mut handler_data) = (None, None);
        if has_handler {
            handler = Some(image.va(image.u32(at).with_context(|| format!("the handler of the ARM64 unwind record at {va:#x}"))?));
            handler_data = Some(at + 4);
        }
        Ok((length, Self::Full { version, epilogs, codes, handler, handler_data }))
    }

    /// The operations of the prolog, decoded from the codes of a full record; a packed entry has
    /// none to decode.
    pub fn operations(&self) -> Result<Vec<Arm64UnwindOp>> {
        let Self::Full { codes, .. } = self else { return Ok(Vec::new()) };
        let mut ops = Vec::new();
        let mut i = 0;
        while i < codes.len() {
            let b = codes[i];
            let next = |n: usize| {
                codes.get(i + n).copied().map(u32::from).with_context(|| format!("unwind code {i} ({b:#04x}) lacks its byte {n}"))
            };
            let (op, used) = match b {
                0x00..=0x1f => (Arm64UnwindOp::Alloc(u32::from(b & 0x1f) * 16), 1),
                0x20..=0x3f => (Arm64UnwindOp::SaveR19R20X(u32::from(b & 0x1f) * 8), 1),
                0x40..=0x7f => (Arm64UnwindOp::SaveFpLr { offset: u32::from(b & 0x3f) * 8, pre_indexed: false }, 1),
                0x80..=0xbf => (Arm64UnwindOp::SaveFpLr { offset: (u32::from(b & 0x3f) + 1) * 8, pre_indexed: true }, 1),
                0xc0..=0xc7 => (Arm64UnwindOp::Alloc((u32::from(b & 7) << 8 | next(1)?) * 16), 2),
                0xc8..=0xcf => {
                    let w = next(1)?;
                    let reg = 19 + ((u32::from(b & 3) << 2) | w >> 6) as u8;
                    let pre = b >= 0xcc;
                    (Arm64UnwindOp::SaveRegPair { reg, offset: ((w & 0x3f) + u32::from(pre)) * 8, pre_indexed: pre }, 2)
                }
                // save_reg is `110100xx xxzzzzzz`; the pre-indexed form has one more register bit
                // and one fewer offset bit: `1101010x xxxzzzzz`.
                0xd0..=0xd3 => {
                    let w = next(1)?;
                    let reg = 19 + ((u32::from(b & 3) << 2) | w >> 6) as u8;
                    (Arm64UnwindOp::SaveReg { reg, offset: (w & 0x3f) * 8, pre_indexed: false }, 2)
                }
                0xd4..=0xd5 => {
                    let w = next(1)?;
                    let reg = 19 + ((u32::from(b & 1) << 3) | w >> 5) as u8;
                    (Arm64UnwindOp::SaveReg { reg, offset: ((w & 0x1f) + 1) * 8, pre_indexed: true }, 2)
                }
                0xd6..=0xd7 => {
                    let w = next(1)?;
                    (Arm64UnwindOp::SaveLrPair { reg: 19 + 2 * ((u32::from(b & 1) << 2 | w >> 6) as u8), offset: (w & 0x3f) * 8 }, 2)
                }
                0xd8..=0xdb => {
                    let w = next(1)?;
                    let pre = b >= 0xda;
                    let reg = 8 + ((u32::from(b & 1) << 2) | w >> 6) as u8;
                    (Arm64UnwindOp::SaveFRegPair { reg, offset: ((w & 0x3f) + u32::from(pre)) * 8, pre_indexed: pre }, 2)
                }
                // save_freg is `1101110x xxzzzzzz`; the pre-indexed form `11011110 xxxzzzzz`.
                0xdc..=0xdd => {
                    let w = next(1)?;
                    let reg = 8 + ((u32::from(b & 1) << 2) | w >> 6) as u8;
                    (Arm64UnwindOp::SaveFReg { reg, offset: (w & 0x3f) * 8, pre_indexed: false }, 2)
                }
                0xde => {
                    let w = next(1)?;
                    (Arm64UnwindOp::SaveFReg { reg: 8 + (w >> 5) as u8, offset: ((w & 0x1f) + 1) * 8, pre_indexed: true }, 2)
                }
                0xe0 => (Arm64UnwindOp::Alloc((next(1)? << 16 | next(2)? << 8 | next(3)?) * 16), 4),
                0xe1 => (Arm64UnwindOp::SetFp, 1),
                0xe2 => (Arm64UnwindOp::AddFp(next(1)? * 8), 2),
                0xe3 => (Arm64UnwindOp::Nop, 1),
                0xe4 => (Arm64UnwindOp::End, 1),
                0xe5 => (Arm64UnwindOp::EndC, 1),
                0xe6 => (Arm64UnwindOp::SaveNext, 1),
                0xe7 => (Arm64UnwindOp::SaveAnyReg, 3),
                0xe8 => (Arm64UnwindOp::TrapFrame, 1),
                0xe9 => (Arm64UnwindOp::MachineFrame, 1),
                0xea => (Arm64UnwindOp::Context, 1),
                0xeb => (Arm64UnwindOp::EcContext, 1),
                0xec => (Arm64UnwindOp::ClearUnwoundToCall, 1),
                0xfc => (Arm64UnwindOp::PacSignLr, 1),
                0xf8 => (Arm64UnwindOp::Reserved(b), 2),
                0xf9 => (Arm64UnwindOp::Reserved(b), 3),
                0xfa => (Arm64UnwindOp::Reserved(b), 4),
                0xfb => (Arm64UnwindOp::Reserved(b), 5),
                other => (Arm64UnwindOp::Reserved(other), 1),
            };
            ensure!(i + used <= codes.len(), "unwind code {i} ({b:#04x}) runs past the {} code bytes", codes.len());
            ops.push(op);
            i += used;
        }
        Ok(ops)
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
    /// The address of the handler's own data, which follows the handler: the `FuncInfo` of the C++
    /// frame handler, the scope table of `__C_specific_handler`.
    pub handler_data: Option<u64>,
    /// The parent entry of a chained entry: this range is a piece of that function.
    pub chained: Option<RuntimeFunction>,
}

/// One operation of a prolog, as the unwind codes record it, in unwind order (the last operation
/// of the prolog first). An unwinder undoes them in this order to leave a frame; the lift does the
/// same when an exception passes a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum UnwindOp {
    /// `push reg`, by x64 register number.
    PushNonvol(u8),
    /// `sub rsp, n`.
    Alloc(u32),
    /// `lea fp, [rsp + frame_offset]`: the frame pointer of the `UNWIND_INFO` from here on.
    SetFpReg,
    /// `mov [rsp + offset], reg`.
    SaveNonvol(u8, u32),
    /// `movaps [rsp + offset], xmm`.
    SaveXmm128(u8, u32),
    /// A machine frame pushed by an interrupt or a trap, with or without an error code.
    PushMachFrame(bool),
    /// A version 2 epilog marker: no operation of the prolog.
    Epilog,
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
        let mut handler_data = None;
        let mut chained = None;
        if flags & Self::CHAININFO != 0 {
            chained = Some(read_entry(image, after).with_context(|| format!("the chained entry of UNWIND_INFO at {va:#x}"))?);
        } else if flags & (Self::EHANDLER | Self::UHANDLER) != 0 {
            handler = Some(image.va(image.u32(after).with_context(|| format!("the handler of UNWIND_INFO at {va:#x}"))?));
            handler_data = Some(after + 4);
        }
        Ok(Self { version, flags, prolog_size, frame_register, frame_offset, codes, handler, handler_data, chained })
    }

    pub fn is_chained(&self) -> bool {
        self.chained.is_some()
    }

    /// The operations of the prolog, decoded from the codes: an operation with a size or an
    /// offset takes the slots after its own.
    pub fn operations(&self) -> Result<Vec<UnwindOp>> {
        let raw = |c: &UnwindCode| u32::from(c.prolog_offset) | u32::from(c.operation) << 8 | u32::from(c.info) << 12;
        let mut ops = Vec::new();
        let mut i = 0;
        while i < self.codes.len() {
            let c = self.codes[i];
            let slot = |n: usize| self.codes.get(i + n).map(raw).with_context(|| format!("unwind code {i} lacks its slot {n}"));
            let (op, used) = match c.operation {
                0 => (UnwindOp::PushNonvol(c.info), 1),
                1 if c.info == 0 => (UnwindOp::Alloc(slot(1)? * 8), 2),
                1 => (UnwindOp::Alloc(slot(1)? | slot(2)? << 16), 3),
                2 => (UnwindOp::Alloc(u32::from(c.info) * 8 + 8), 1),
                3 => (UnwindOp::SetFpReg, 1),
                4 => (UnwindOp::SaveNonvol(c.info, slot(1)? * 8), 2),
                5 => (UnwindOp::SaveNonvol(c.info, slot(1)? | slot(2)? << 16), 3),
                6 => (UnwindOp::Epilog, if self.version >= 2 { 1 } else { 2 }),
                8 => (UnwindOp::SaveXmm128(c.info, slot(1)? * 16), 2),
                9 => (UnwindOp::SaveXmm128(c.info, slot(1)? | slot(2)? << 16), 3),
                10 => (UnwindOp::PushMachFrame(c.info != 0), 1),
                other => bail!("unwind code {i} has the unknown operation {other}"),
            };
            ops.push(op);
            i += used;
        }
        Ok(ops)
    }

    /// The bytes the prolog moves `rsp` by: the allocations and the pushes.
    pub fn frame_size(&self) -> Result<u64> {
        Ok(self
            .operations()?
            .iter()
            .map(|op| match op {
                UnwindOp::PushNonvol(_) => 8,
                UnwindOp::Alloc(n) => u64::from(*n),
                UnwindOp::PushMachFrame(code) => {
                    if *code {
                        48
                    } else {
                        40
                    }
                }
                _ => 0,
            })
            .sum())
    }
}

/// One entry of a `FuncInfo`'s unwind map: leaving the state runs `action` (a funclet that
/// destroys what the state constructed) and lands in `to_state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct UnwindMapEntry {
    pub to_state: i32,
    pub action: Option<u64>,
}

/// One catch handler of a try block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CatchHandler {
    /// `HT_IsConst`, `HT_IsVolatile`, `HT_IsReference` (8), `HT_IsStdDotDot` (0x40, `catch (...)`).
    pub adjectives: u32,
    /// The `TypeDescriptor` of the type caught, or `None` for `catch (...)`.
    pub type_descriptor: Option<u64>,
    /// The frame offset of the catch's object, relative to the establisher frame on x64 and to
    /// `ebp` on x86; zero when the catch names no object.
    pub catch_object: i32,
    /// The funclet (x64) or the code (x86) of the catch body; it returns the continuation address.
    pub handler: u64,
    /// x64: the offset in the funclet's frame of the slot that holds the parent's frame.
    pub frame: Option<u32>,
}

impl CatchHandler {
    pub const REFERENCE: u32 = 0x8;

    pub fn is_catch_all(&self) -> bool {
        self.type_descriptor.is_none()
    }
}

/// One try block: the states its body spans, the states of its catch bodies, and its handlers in
/// source order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TryBlock {
    pub low: i32,
    pub high: i32,
    pub catch_high: i32,
    pub handlers: Vec<CatchHandler>,
}

impl TryBlock {
    /// Whether `state` lies in the try body.
    pub fn covers(&self, state: i32) -> bool {
        state >= self.low && state <= self.high
    }

    /// Whether `state` lies in one of the catch bodies.
    pub fn catches_at(&self, state: i32) -> bool {
        state > self.high && state <= self.catch_high
    }
}

/// The `FuncInfo` of a function under the C++ frame handler (`__CxxFrameHandler3`, or the x86
/// handler through its `__ehhandler$` stub): the states of the function, the actions that leave
/// each, the try blocks with their catch handlers, and on x64 the state of each range of code.
/// Fields are image-relative on x64 and absolute on x86.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FuncInfo {
    pub address: u64,
    pub magic: u32,
    pub unwind_map: Vec<UnwindMapEntry>,
    pub try_blocks: Vec<TryBlock>,
    /// The state of the code from each address on, in address order (x64 only).
    pub ip_to_state: Vec<(u64, i32)>,
    /// x64: the frame offset of the slot the runtime keeps its unwind progress in.
    pub unwind_help: Option<i32>,
    /// `FI_EHS_FLAG` (1, `/EHs`), `FI_DYNSTKALIGN_FLAG` (2), `FI_EHNOEXCEPT_FLAG` (4).
    pub flags: u32,
}

impl FuncInfo {
    /// Whether the word at `va` begins a `FuncInfo`: the magic of the three layouts.
    pub fn magic_at(image: &Image, va: u64) -> bool {
        image.u32(va).is_ok_and(|magic| magic & 0xffff_ff00 == 0x1993_0500)
    }

    pub fn parse(image: &Image, va: u64) -> Result<Self> {
        let wide = image.is_64();
        let at = |field: u64| image.u32(va + field * 4).with_context(|| format!("field {field} of the FuncInfo at {va:#x}"));
        let resolve = |v: u32| if wide { image.va(v) } else { u64::from(v) };
        let magic = at(0)?;
        ensure!(magic & 0xffff_ff00 == 0x1993_0500, "no FuncInfo magic at {va:#x} ({magic:#x})");
        let states = at(1)?.min(4096);
        let unwind_at = resolve(at(2)?);
        let tries = at(3)?.min(4096);
        let try_at = resolve(at(4)?);
        let ips = at(5)?.min(65536);
        let ip_at = resolve(at(6)?);
        let unwind_help = if wide { Some(at(7)? as i32) } else { None };
        let flags = if magic >= 0x1993_0522 { at(if wide { 9 } else { 8 })? } else { 0 };
        let mut unwind_map = Vec::with_capacity(states as usize);
        for n in 0..u64::from(states) {
            let to_state = image.u32(unwind_at + n * 8)? as i32;
            let action = image.u32(unwind_at + n * 8 + 4)?;
            unwind_map.push(UnwindMapEntry { to_state, action: (action != 0).then(|| resolve(action)) });
        }
        let handler_size = if wide { 20 } else { 16 };
        let mut try_blocks = Vec::with_capacity(tries as usize);
        for n in 0..u64::from(tries) {
            let entry = try_at + n * 20;
            let (low, high, catch_high) = (image.u32(entry)? as i32, image.u32(entry + 4)? as i32, image.u32(entry + 8)? as i32);
            let count = image.u32(entry + 12)?.min(256);
            let handlers_at = resolve(image.u32(entry + 16)?);
            let mut handlers = Vec::with_capacity(count as usize);
            for h in 0..u64::from(count) {
                let base = handlers_at + h * handler_size;
                let type_descriptor = image.u32(base + 4)?;
                handlers.push(CatchHandler {
                    adjectives: image.u32(base)?,
                    type_descriptor: (type_descriptor != 0).then(|| resolve(type_descriptor)),
                    catch_object: image.u32(base + 8)? as i32,
                    handler: resolve(image.u32(base + 12)?),
                    frame: if wide { Some(image.u32(base + 16)?) } else { None },
                });
            }
            try_blocks.push(TryBlock { low, high, catch_high, handlers });
        }
        let mut ip_to_state = Vec::with_capacity(ips as usize);
        if wide {
            for n in 0..u64::from(ips) {
                ip_to_state.push((image.va(image.u32(ip_at + n * 8)?), image.u32(ip_at + n * 8 + 4)? as i32));
            }
            ip_to_state.sort_by_key(|(ip, _)| *ip);
        }
        Ok(Self { address: va, magic, unwind_map, try_blocks, ip_to_state, unwind_help, flags })
    }

    /// The state of the code at `ip`: that of the last range starting at or before it, -1 before
    /// the first.
    pub fn state_at(&self, ip: u64) -> i32 {
        let i = self.ip_to_state.partition_point(|(start, _)| *start <= ip);
        if i == 0 { -1 } else { self.ip_to_state[i - 1].1 }
    }

    /// The funclets the tables name: the unwind actions and the catch handlers.
    pub fn funclets(&self) -> BTreeSet<u64> {
        let actions = self.unwind_map.iter().filter_map(|e| e.action);
        let handlers = self.try_blocks.iter().flat_map(|t| t.handlers.iter().map(|h| h.handler));
        actions.chain(handlers).collect()
    }
}

/// One type a thrown object can be caught as: the object's own type and each of its bases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CatchableType {
    /// `CT_IsSimpleType` (1), `CT_ByReferenceOnly` (2), `CT_HasVirtualBase` (4).
    pub properties: u32,
    pub type_descriptor: u64,
    /// The displacement of this subobject in the thrown object; `pdisp` is -1 without a virtual base.
    pub mdisp: i32,
    pub pdisp: i32,
    pub vdisp: i32,
    /// The size of the subobject, which a catch by value copies.
    pub size: u32,
    /// The copy constructor a catch by value calls, when a bitwise copy would not do.
    pub copy: Option<u64>,
}

/// The `ThrowInfo` a throw site hands `_CxxThrowException` with the object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThrowInfo {
    /// `TI_IsConst` (1), `TI_IsVolatile` (2), `TI_IsUnaligned` (4), `TI_IsPure` (8).
    pub attributes: u32,
    /// The destructor of the thrown object, run when its catch ends.
    pub destructor: Option<u64>,
    pub catchable: Vec<CatchableType>,
}

impl ThrowInfo {
    pub fn parse(image: &Image, va: u64) -> Result<Self> {
        let wide = image.is_64();
        let resolve = |v: u32| if wide { image.va(v) } else { u64::from(v) };
        let attributes = image.u32(va).with_context(|| format!("the ThrowInfo at {va:#x}"))?;
        let destructor = image.u32(va + 4)?;
        let array = resolve(image.u32(va + 12)?);
        let count = image.u32(array).with_context(|| format!("the catchable types of the ThrowInfo at {va:#x}"))?.min(256);
        let mut catchable = Vec::with_capacity(count as usize);
        for n in 0..u64::from(count) {
            let at = resolve(image.u32(array + 4 + n * 4)?);
            let copy = image.u32(at + 24)?;
            catchable.push(CatchableType {
                properties: image.u32(at)?,
                type_descriptor: resolve(image.u32(at + 4)?),
                mdisp: image.u32(at + 8)? as i32,
                pdisp: image.u32(at + 12)? as i32,
                vdisp: image.u32(at + 16)? as i32,
                size: image.u32(at + 20)?,
                copy: (copy != 0).then(|| resolve(copy)),
            });
        }
        Ok(Self { attributes, destructor: (destructor != 0).then(|| resolve(destructor)), catchable })
    }
}

/// The decorated name of a `TypeDescriptor` (`.?AUError@@`, `.H`): the text after its vtable
/// pointer and spare pointer.
pub fn type_descriptor_name(image: &Image, descriptor: u64) -> Result<String> {
    let at = descriptor + 2 * u64::from(image.pointer_size());
    let bytes = image.read_to_section_end(at).with_context(|| format!("the name of the TypeDescriptor at {descriptor:#x}"))?;
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len()).min(1024);
    Ok(String::from_utf8_lossy(&bytes[..len]).into_owned())
}

/// A function under the C++ frame handler on x64: its own entry, its tables, and the funclets the
/// tables name, each an entry of the exception table of its own. The funclets are code of the
/// function: the handler calls them with the function's frame.
#[derive(Debug, Clone, Serialize)]
pub struct CxxFrame {
    pub entry: RuntimeFunction,
    pub info: FuncInfo,
    pub funclets: Vec<RuntimeFunction>,
}

/// Every x64 function under the C++ frame handler, by the `FuncInfo` its handler data names. A
/// funclet carries the same handler and tables as its parent; the parent is the entry the tables
/// do not name.
pub fn cxx_frames(image: &Image) -> Vec<CxxFrame> {
    let mut by_info: std::collections::BTreeMap<u64, (FuncInfo, Vec<RuntimeFunction>)> = std::collections::BTreeMap::new();
    for f in &image.runtime_functions {
        let Ok(unwind) = f.unwind_info(image) else { continue };
        let Some(data) = unwind.handler_data else { continue };
        let Ok(rva) = image.u32(data) else { continue };
        let va = image.va(rva);
        if !FuncInfo::magic_at(image, va) {
            continue;
        }
        let slot = match by_info.entry(va) {
            std::collections::btree_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::btree_map::Entry::Vacant(e) => match FuncInfo::parse(image, va) {
                Ok(info) => e.insert((info, Vec::new())),
                Err(_) => continue,
            },
        };
        slot.1.push(*f);
    }
    let mut frames = Vec::new();
    for (info, entries) in by_info.into_values() {
        let named = info.funclets();
        let Some(entry) = entries.iter().copied().find(|e| !named.contains(&e.begin)) else { continue };
        let funclets = named.iter().filter_map(|&va| image.runtime_functions.iter().find(|r| r.begin == va).copied()).collect();
        frames.push(CxxFrame { entry, info, funclets });
    }
    frames
}

fn read_entry(image: &Image, va: u64) -> Result<RuntimeFunction> {
    let begin = image.u32(va)?;
    let end = image.u32(va + 4)?;
    let unwind = image.u32(va + 8)?;
    ensure!(end >= begin, "RUNTIME_FUNCTION at {va:#x} ends ({end:#x}) before it begins ({begin:#x})");
    Ok(RuntimeFunction { begin: image.va(begin), end: image.va(end), unwind: image.va(unwind), packed: None })
}

/// An ARM64 entry: the start and a word that packs the unwind data (flag 1 or 2, with the length
/// in units of four bytes) or names the `.xdata` record (flag 0) that holds the length.
fn read_arm64_entry(image: &Image, va: u64) -> Result<RuntimeFunction> {
    let begin = image.va(image.u32(va)?);
    let word = image.u32(va + 4)?;
    match word & 3 {
        0 => {
            let unwind = image.va(word);
            let (length, _) = Arm64Unwind::parse(image, unwind)?;
            Ok(RuntimeFunction { begin, end: begin + length, unwind, packed: None })
        }
        3 => bail!("the ARM64 entry at {va:#x} has the reserved flag 3"),
        _ => Ok(RuntimeFunction { begin, end: begin + u64::from((word >> 2) & 0x7ff) * 4, unwind: 0, packed: Some(word) }),
    }
}

/// The reader of one entry of a machine's exception table.
type EntryReader = fn(&Image, u64) -> Result<RuntimeFunction>;

pub(crate) fn parse(image: &Image) -> Result<Vec<RuntimeFunction>> {
    let (stride, read): (u64, EntryReader) = match image.machine() {
        Machine::Amd64 => (12, read_entry),
        Machine::Arm64 => (8, read_arm64_entry),
        _ => return Ok(Vec::new()),
    };
    let Some((va, size)) = image.directory_va(Directory::Exception) else { return Ok(Vec::new()) };
    let count = u64::from(size) / stride;
    let mut functions = Vec::with_capacity(count as usize);
    for i in 0..count {
        let entry = read(image, va + i * stride).with_context(|| format!("exception table entry {i}"))?;
        if !entry.is_empty() {
            functions.push(entry);
        }
    }
    functions.sort_by_key(|f| f.begin);
    Ok(functions)
}

#[cfg(test)]
mod tests {
    use super::{Arm64Unwind, Arm64UnwindOp as Op};

    fn ops(codes: &[u8]) -> Vec<Op> {
        Arm64Unwind::Full { version: 0, epilogs: Vec::new(), codes: codes.to_vec(), handler: None, handler_data: None }
            .operations()
            .unwrap()
    }

    /// The single-register saves: the pre-indexed forms spend one more bit on the register and
    /// one fewer on the offset than the plain ones, and their offset counts from one.
    #[test]
    fn the_register_saves_decode_both_forms() {
        // str x19, [sp, #16]; str x28, [sp, #504]; str x19, [sp, #-16]!; str x28, [sp, #-256]!
        assert_eq!(
            ops(&[0xd0, 0x02, 0xd2, 0x7f, 0xd4, 0x01, 0xd5, 0x3f]),
            [
                Op::SaveReg { reg: 19, offset: 16, pre_indexed: false },
                Op::SaveReg { reg: 28, offset: 504, pre_indexed: false },
                Op::SaveReg { reg: 19, offset: 16, pre_indexed: true },
                Op::SaveReg { reg: 28, offset: 256, pre_indexed: true },
            ]
        );
        // str d8, [sp, #8]; str d15, [sp, #504]; str d8, [sp, #-16]!; str d15, [sp, #-256]!
        assert_eq!(
            ops(&[0xdc, 0x01, 0xdd, 0xff, 0xde, 0x01, 0xde, 0xff]),
            [
                Op::SaveFReg { reg: 8, offset: 8, pre_indexed: false },
                Op::SaveFReg { reg: 15, offset: 504, pre_indexed: false },
                Op::SaveFReg { reg: 8, offset: 16, pre_indexed: true },
                Op::SaveFReg { reg: 15, offset: 256, pre_indexed: true },
            ]
        );
    }

    /// The pair saves and the frame codes, as a canonical prolog spells them.
    #[test]
    fn a_canonical_prolog_decodes() {
        // stp x29, x30, [sp, #-48]!; stp x19, x20, [sp, #16]; stp d8, d9, [sp, #-32]!; mov x29, sp;
        // sub sp, sp, #4096; end
        assert_eq!(
            ops(&[0x85, 0xc8, 0x02, 0xda, 0x03, 0xe1, 0xc1, 0x00, 0xe4]),
            [
                Op::SaveFpLr { offset: 48, pre_indexed: true },
                Op::SaveRegPair { reg: 19, offset: 16, pre_indexed: false },
                Op::SaveFRegPair { reg: 8, offset: 32, pre_indexed: true },
                Op::SetFp,
                Op::Alloc(4096),
                Op::End,
            ]
        );
    }

    #[test]
    fn a_truncated_code_is_refused() {
        let unwind = Arm64Unwind::Full { version: 0, epilogs: Vec::new(), codes: vec![0xe0, 0x00], handler: None, handler_data: None };
        assert!(unwind.operations().is_err());
    }
}
