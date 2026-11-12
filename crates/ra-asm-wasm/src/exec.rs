//! The interpreter: one function body's ops run over the instance's value stack, a call of
//! another lifted body is a nested `invoke`, a call of an import goes to its host function.
// The conversions and the comparisons of floats are the instructions', exactly as the format
// defines them.
#![allow(clippy::cast_precision_loss, clippy::float_cmp, clippy::many_single_char_names)]

use std::sync::Arc;

use crate::decode::Op;
use crate::{Error, FuncType, Instance, Module, PAGE};

/// The deepest calls may nest: a bound on the host's own stack, which each nested call uses.
const MAX_DEPTH: u32 = 400;

impl Module {
    /// The type of function `index`, imported or defined.
    pub(crate) fn func_type(&self, index: u32) -> Result<&FuncType, Error> {
        let index = index as usize;
        let ty = match self.imports.get(index) {
            Some(import) => import.ty,
            None => *self.funcs.get(index - self.imports.len()).ok_or_else(|| Error::new(format!("a call of no function ({index})")))?,
        };
        Ok(&self.types[ty as usize])
    }
}

/// An open block: where its values start on the stack, how many a branch to it carries, and the
/// op a branch to it goes to (a block's or an if's `End`, which closes it; a loop's first op).
#[derive(Clone, Copy)]
struct Label {
    height: usize,
    arity: usize,
    target: usize,
}

fn trap(what: &str) -> Error {
    Error::new(what.to_owned())
}

fn pop(stack: &mut Vec<u64>) -> Result<u64, Error> {
    stack.pop().ok_or_else(|| trap("the value stack underflows"))
}

/// Keeps the top `arity` values of `stack` and drops the rest down to `height`.
fn unwind(stack: &mut Vec<u64>, height: usize, arity: usize) -> Result<(), Error> {
    if stack.len() < height + arity {
        return Err(trap("the value stack underflows"));
    }
    let from = stack.len() - arity;
    stack.copy_within(from.., height);
    stack.truncate(height + arity);
    Ok(())
}

/// What a branch does: go to an op of the body, or return from the function.
enum Branch {
    To(usize),
    Return,
}

/// Branches out of the `depth`th open block, the function's own past the last.
fn branch(stack: &mut Vec<u64>, labels: &mut Vec<Label>, depth: u32, base: usize, results: usize) -> Result<Branch, Error> {
    let depth = depth as usize;
    if depth == labels.len() {
        unwind(stack, base, results)?;
        return Ok(Branch::Return);
    }
    let index = labels.len().checked_sub(depth + 1).ok_or_else(|| trap("a branch out of no block"))?;
    let label = labels[index];
    unwind(stack, label.height, label.arity)?;
    labels.truncate(index + 1);
    Ok(Branch::To(label.target))
}

impl Instance {
    /// Runs function `func` over the arguments on top of the stack, which it replaces with its
    /// results.
    pub(crate) fn invoke(&mut self, func: u32, depth: u32) -> Result<(), Error> {
        let module = Arc::clone(&self.module);
        let ty = module.func_type(func)?;
        let params = ty.params.len();
        let results = ty.results.len();
        if self.stack.len() < params {
            return Err(trap("the value stack underflows"));
        }
        let imported = module.imports.len();
        if (func as usize) < imported {
            let args = self.stack.split_off(self.stack.len() - params);
            let values = (self.hosts[func as usize])(&mut self.memory, &args)?;
            if values.len() != results {
                let import = &module.imports[func as usize];
                return Err(Error::new(format!(
                    "the host's {}.{} returned {} values, not {results}",
                    import.module,
                    import.name,
                    values.len()
                )));
            }
            self.stack.extend(values);
            return Ok(());
        }
        if depth >= MAX_DEPTH {
            return Err(Error::new(format!("calls nest deeper than {MAX_DEPTH}")));
        }
        let body = &module.bodies[func as usize - imported];
        let mut locals = self.stack.split_off(self.stack.len() - params);
        locals.resize(params + body.locals.len(), 0);
        let base = self.stack.len();
        let mut labels: Vec<Label> = Vec::new();
        let mut pc = 0usize;
        loop {
            self.fuel = self.fuel.checked_sub(1).ok_or_else(|| trap("the plugin ran out of its instructions"))?;
            let op = body.ops.get(pc).ok_or_else(|| trap("the code runs past its end"))?;
            pc += 1;
            let stack = &mut self.stack;
            match op {
                Op::Unreachable => return Err(trap("an unreachable instruction")),
                Op::Nop => {}
                Op::Block { params, results, end } => {
                    let height = stack.len().checked_sub(*params as usize).ok_or_else(|| trap("the value stack underflows"))?;
                    labels.push(Label { height, arity: *results as usize, target: *end as usize });
                }
                Op::Loop { params } => {
                    let height = stack.len().checked_sub(*params as usize).ok_or_else(|| trap("the value stack underflows"))?;
                    labels.push(Label { height, arity: *params as usize, target: pc });
                }
                Op::If { params, results, else_at, end } => {
                    let condition = pop(stack)? as u32;
                    let height = stack.len().checked_sub(*params as usize).ok_or_else(|| trap("the value stack underflows"))?;
                    labels.push(Label { height, arity: *results as usize, target: *end as usize });
                    if condition == 0 {
                        pc = if else_at == end { *end as usize } else { *else_at as usize + 1 };
                    }
                }
                // The end of the taken arm: on to the `End`, which closes the if.
                Op::Else { end } => pc = *end as usize,
                Op::End => {
                    if labels.pop().is_none() {
                        unwind(stack, base, results)?;
                        return Ok(());
                    }
                }
                Op::Br(depth) => match branch(stack, &mut labels, *depth, base, results)? {
                    Branch::To(target) => pc = target,
                    Branch::Return => return Ok(()),
                },
                Op::BrIf(depth) => {
                    if pop(stack)? as u32 != 0 {
                        match branch(stack, &mut labels, *depth, base, results)? {
                            Branch::To(target) => pc = target,
                            Branch::Return => return Ok(()),
                        }
                    }
                }
                Op::BrTable(targets, default) => {
                    let i = pop(stack)? as u32 as usize;
                    let depth = targets.get(i).copied().unwrap_or(*default);
                    match branch(stack, &mut labels, depth, base, results)? {
                        Branch::To(target) => pc = target,
                        Branch::Return => return Ok(()),
                    }
                }
                Op::Return => {
                    unwind(stack, base, results)?;
                    return Ok(());
                }
                Op::Call(f) => self.invoke(*f, depth + 1)?,
                Op::CallIndirect(expected) => {
                    let i = pop(stack)? as u32 as usize;
                    let f = self
                        .table
                        .get(i)
                        .copied()
                        .flatten()
                        .ok_or_else(|| Error::new(format!("an indirect call of no function (slot {i})")))?;
                    let wanted = module.types.get(*expected as usize).ok_or_else(|| trap("an indirect call of no type"))?;
                    if module.func_type(f)? != wanted {
                        return Err(trap("an indirect call of a function of another type"));
                    }
                    self.invoke(f, depth + 1)?;
                }
                Op::Drop => {
                    pop(stack)?;
                }
                Op::Select => {
                    let condition = pop(stack)? as u32;
                    let b = pop(stack)?;
                    let a = pop(stack)?;
                    stack.push(if condition != 0 { a } else { b });
                }
                Op::LocalGet(i) => stack.push(*locals.get(*i as usize).ok_or_else(|| trap("a local of no index"))?),
                Op::LocalSet(i) => {
                    let value = pop(stack)?;
                    *locals.get_mut(*i as usize).ok_or_else(|| trap("a local of no index"))? = value;
                }
                Op::LocalTee(i) => {
                    let value = *stack.last().ok_or_else(|| trap("the value stack underflows"))?;
                    *locals.get_mut(*i as usize).ok_or_else(|| trap("a local of no index"))? = value;
                }
                Op::GlobalGet(i) => stack.push(*self.globals.get(*i as usize).ok_or_else(|| trap("a global of no index"))?),
                Op::GlobalSet(i) => {
                    let value = pop(stack)?;
                    if !module.globals.get(*i as usize).is_some_and(|g| g.mutable) {
                        return Err(trap("a write of an immutable global"));
                    }
                    self.globals[*i as usize] = value;
                }
                Op::Memory(code, offset) => memory_op(stack, &mut self.memory, *code, *offset)?,
                Op::MemorySize => stack.push((self.memory.len() / PAGE) as u64),
                Op::MemoryGrow => {
                    let delta = pop(stack)? as u32;
                    let pages = (self.memory.len() / PAGE) as u32;
                    match pages.checked_add(delta).filter(|&p| p <= self.max_pages) {
                        Some(p) => {
                            self.memory.resize(p as usize * PAGE, 0);
                            stack.push(u64::from(pages));
                        }
                        None => stack.push(u64::from(u32::MAX)),
                    }
                }
                Op::MemoryCopy => {
                    let n = pop(stack)? as u32 as usize;
                    let source = pop(stack)? as u32 as usize;
                    let destination = pop(stack)? as u32 as usize;
                    let size = self.memory.len();
                    if source + n > size || destination + n > size {
                        return Err(trap("a memory.copy out of bounds"));
                    }
                    self.memory.copy_within(source..source + n, destination);
                }
                Op::MemoryFill => {
                    let n = pop(stack)? as u32 as usize;
                    let value = pop(stack)? as u8;
                    let destination = pop(stack)? as u32 as usize;
                    let region = self.memory.get_mut(destination..destination + n).ok_or_else(|| trap("a memory.fill out of bounds"))?;
                    region.fill(value);
                }
                Op::Const(bits) => stack.push(*bits),
                Op::Numeric(code) => numeric(stack, *code)?,
                Op::TruncSat(sub) => {
                    let a = pop(stack)?;
                    // The f32 sources are 0, 1, 4 and 5; the f64 ones 2, 3, 6 and 7.
                    let x = if sub & 2 == 0 { f64::from(f32::from_bits(a as u32)) } else { f64::from_bits(a) };
                    stack.push(match sub {
                        0 | 2 => u64::from(x as i32 as u32),
                        1 | 3 => u64::from(x as u32),
                        4 | 6 => x as i64 as u64,
                        _ => x as u64,
                    });
                }
            }
        }
    }
}

/// A load or a store: the address is the `i32` under the value (for a store) plus `offset`.
fn memory_op(stack: &mut Vec<u64>, memory: &mut [u8], code: u8, offset: u32) -> Result<(), Error> {
    let (size, store) = match code {
        0x28 | 0x2a | 0x34 | 0x35 => (4, false),
        0x29 | 0x2b => (8, false),
        0x2c | 0x2d | 0x30 | 0x31 => (1, false),
        0x2e | 0x2f | 0x32 | 0x33 => (2, false),
        0x36 | 0x38 | 0x3e => (4, true),
        0x37 | 0x39 => (8, true),
        0x3a | 0x3c => (1, true),
        _ => (2, true),
    };
    let value = if store { pop(stack)? } else { 0 };
    let at = u64::from(pop(stack)? as u32) + u64::from(offset);
    let start = usize::try_from(at).map_err(|_| trap("a memory access out of bounds"))?;
    let bytes = memory.get_mut(start..start + size).ok_or_else(|| Error::new(format!("a memory access out of bounds at 0x{at:x}")))?;
    if store {
        bytes.copy_from_slice(&value.to_le_bytes()[..size]);
        return Ok(());
    }
    let mut raw = [0u8; 8];
    raw[..size].copy_from_slice(bytes);
    let raw = u64::from_le_bytes(raw);
    let bits = size as u32 * 8;
    let signed = matches!(code, 0x2c | 0x2e | 0x30 | 0x32 | 0x34);
    let value = if signed && bits < 64 { (((raw << (64 - bits)) as i64) >> (64 - bits)) as u64 } else { raw };
    // An `i32` result keeps its low half.
    let narrow = matches!(code, 0x28 | 0x2a | 0x2c..=0x2f);
    stack.push(if narrow { value & 0xffff_ffff } else { value });
    Ok(())
}

/// Whether the numeric instruction `code` takes two operands.
fn is_binary(code: u8) -> bool {
    matches!(code, 0x46..=0x4f | 0x51..=0x66 | 0x6a..=0x78 | 0x7c..=0x8a | 0x92..=0x98 | 0xa0..=0xa6)
}

/// The float in range for a trapping truncation to an integer of `(lo, hi)`, both exclusive.
fn truncated(x: f64, lo: f64, hi: f64) -> Result<f64, Error> {
    if x.is_nan() {
        return Err(trap("a conversion of NaN to an integer"));
    }
    if !(x > lo && x < hi) {
        return Err(trap("an integer overflow in a conversion"));
    }
    Ok(x.trunc())
}

fn min64(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b {
        if a.is_sign_negative() { a } else { b }
    } else {
        a.min(b)
    }
}

fn max64(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == b {
        if a.is_sign_positive() { a } else { b }
    } else {
        a.max(b)
    }
}

// The result of either is one of its operands, which the f32 holds exactly.
fn min32(a: f32, b: f32) -> f32 {
    min64(f64::from(a), f64::from(b)) as f32
}

fn max32(a: f32, b: f32) -> f32 {
    max64(f64::from(a), f64::from(b)) as f32
}

fn numeric(stack: &mut Vec<u64>, code: u8) -> Result<(), Error> {
    let b = if is_binary(code) { pop(stack)? } else { 0 };
    let a = pop(stack)?;
    stack.push(compute(code, a, b)?);
    Ok(())
}

/// The instruction `code` over the bits of its operands.
fn compute(code: u8, a: u64, b: u64) -> Result<u64, Error> {
    let (x, y) = (a as u32, b as u32);
    let (sx, sy) = (x as i32, y as i32);
    let (sa, sb) = (a as i64, b as i64);
    let (fa, fb) = (f32::from_bits(x), f32::from_bits(y));
    let (da, db) = (f64::from_bits(a), f64::from_bits(b));
    let flag = u64::from;
    let w = u64::from;
    let f32_bits = |v: f32| u64::from(v.to_bits());
    let f64_bits = f64::to_bits;
    let zero = || trap("an integer division by zero");
    Ok(match code {
        0x45 => flag(x == 0),
        0x46 => flag(x == y),
        0x47 => flag(x != y),
        0x48 => flag(sx < sy),
        0x49 => flag(x < y),
        0x4a => flag(sx > sy),
        0x4b => flag(x > y),
        0x4c => flag(sx <= sy),
        0x4d => flag(x <= y),
        0x4e => flag(sx >= sy),
        0x4f => flag(x >= y),
        0x50 => flag(a == 0),
        0x51 => flag(a == b),
        0x52 => flag(a != b),
        0x53 => flag(sa < sb),
        0x54 => flag(a < b),
        0x55 => flag(sa > sb),
        0x56 => flag(a > b),
        0x57 => flag(sa <= sb),
        0x58 => flag(a <= b),
        0x59 => flag(sa >= sb),
        0x5a => flag(a >= b),
        0x5b => flag(fa == fb),
        0x5c => flag(fa != fb),
        0x5d => flag(fa < fb),
        0x5e => flag(fa > fb),
        0x5f => flag(fa <= fb),
        0x60 => flag(fa >= fb),
        0x61 => flag(da == db),
        0x62 => flag(da != db),
        0x63 => flag(da < db),
        0x64 => flag(da > db),
        0x65 => flag(da <= db),
        0x66 => flag(da >= db),
        0x67 => w(x.leading_zeros()),
        0x68 => w(x.trailing_zeros()),
        0x69 => w(x.count_ones()),
        0x6a => w(x.wrapping_add(y)),
        0x6b => w(x.wrapping_sub(y)),
        0x6c => w(x.wrapping_mul(y)),
        0x6d => {
            if y == 0 {
                return Err(zero());
            }
            w(sx.checked_div(sy).ok_or_else(|| trap("an integer overflow in a division"))? as u32)
        }
        0x6e => w(x.checked_div(y).ok_or_else(zero)?),
        0x6f => {
            if y == 0 {
                return Err(zero());
            }
            w(sx.wrapping_rem(sy) as u32)
        }
        0x70 => w(x.checked_rem(y).ok_or_else(zero)?),
        0x71 => w(x & y),
        0x72 => w(x | y),
        0x73 => w(x ^ y),
        0x74 => w(x.wrapping_shl(y)),
        0x75 => w(sx.wrapping_shr(y) as u32),
        0x76 => w(x.wrapping_shr(y)),
        0x77 => w(x.rotate_left(y & 31)),
        0x78 => w(x.rotate_right(y & 31)),
        0x79 => u64::from(a.leading_zeros()),
        0x7a => u64::from(a.trailing_zeros()),
        0x7b => u64::from(a.count_ones()),
        0x7c => a.wrapping_add(b),
        0x7d => a.wrapping_sub(b),
        0x7e => a.wrapping_mul(b),
        0x7f => {
            if b == 0 {
                return Err(zero());
            }
            sa.checked_div(sb).ok_or_else(|| trap("an integer overflow in a division"))? as u64
        }
        0x80 => a.checked_div(b).ok_or_else(zero)?,
        0x81 => {
            if b == 0 {
                return Err(zero());
            }
            sa.wrapping_rem(sb) as u64
        }
        0x82 => a.checked_rem(b).ok_or_else(zero)?,
        0x83 => a & b,
        0x84 => a | b,
        0x85 => a ^ b,
        0x86 => a.wrapping_shl(b as u32),
        0x87 => sa.wrapping_shr(b as u32) as u64,
        0x88 => a.wrapping_shr(b as u32),
        0x89 => a.rotate_left((b & 63) as u32),
        0x8a => a.rotate_right((b & 63) as u32),
        0x8b => f32_bits(fa.abs()),
        0x8c => f32_bits(-fa),
        0x8d => f32_bits(fa.ceil()),
        0x8e => f32_bits(fa.floor()),
        0x8f => f32_bits(fa.trunc()),
        0x90 => f32_bits(fa.round_ties_even()),
        0x91 => f32_bits(fa.sqrt()),
        0x92 => f32_bits(fa + fb),
        0x93 => f32_bits(fa - fb),
        0x94 => f32_bits(fa * fb),
        0x95 => f32_bits(fa / fb),
        0x96 => f32_bits(min32(fa, fb)),
        0x97 => f32_bits(max32(fa, fb)),
        0x98 => f32_bits(fa.copysign(fb)),
        0x99 => f64_bits(da.abs()),
        0x9a => f64_bits(-da),
        0x9b => f64_bits(da.ceil()),
        0x9c => f64_bits(da.floor()),
        0x9d => f64_bits(da.trunc()),
        0x9e => f64_bits(da.round_ties_even()),
        0x9f => f64_bits(da.sqrt()),
        0xa0 => f64_bits(da + db),
        0xa1 => f64_bits(da - db),
        0xa2 => f64_bits(da * db),
        0xa3 => f64_bits(da / db),
        0xa4 => f64_bits(min64(da, db)),
        0xa5 => f64_bits(max64(da, db)),
        0xa6 => f64_bits(da.copysign(db)),
        0xa7 | 0xbc | 0xbe => a & 0xffff_ffff,
        0xa8 => w(truncated(f64::from(fa), -2_147_483_649.0, 2_147_483_648.0)? as i32 as u32),
        0xa9 => w(truncated(f64::from(fa), -1.0, 4_294_967_296.0)? as u32),
        0xaa => w(truncated(da, -2_147_483_649.0, 2_147_483_648.0)? as i32 as u32),
        0xab => w(truncated(da, -1.0, 4_294_967_296.0)? as u32),
        0xac => i64::from(sx) as u64,
        0xad => u64::from(x),
        0xae => truncated(f64::from(fa), -9_223_372_036_854_777_856.0, 9_223_372_036_854_775_808.0)? as i64 as u64,
        0xaf => truncated(f64::from(fa), -1.0, 18_446_744_073_709_551_616.0)? as u64,
        0xb0 => truncated(da, -9_223_372_036_854_777_856.0, 9_223_372_036_854_775_808.0)? as i64 as u64,
        0xb1 => truncated(da, -1.0, 18_446_744_073_709_551_616.0)? as u64,
        0xb2 => f32_bits(sx as f32),
        0xb3 => f32_bits(x as f32),
        0xb4 => f32_bits(sa as f32),
        0xb5 => f32_bits(a as f32),
        0xb6 => f32_bits(da as f32),
        0xb7 => f64_bits(f64::from(sx)),
        0xb8 => f64_bits(f64::from(x)),
        0xb9 => f64_bits(sa as f64),
        0xba => f64_bits(a as f64),
        0xbb => f64_bits(f64::from(fa)),
        0xbd | 0xbf => a,
        0xc0 => w(i32::from(x as i8) as u32),
        0xc1 => w(i32::from(x as i16) as u32),
        0xc2 => i64::from(a as i8) as u64,
        0xc3 => i64::from(a as i16) as u64,
        0xc4 => i64::from(a as i32) as u64,
        _ => return Err(Error::new(format!("the instruction 0x{code:02x}, not one the interpreter runs"))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_numeric_instructions_keep_the_formats_edges() {
        assert_eq!(compute(0x6d, (-7i32) as u32 as u64, 2).unwrap(), (-3i32) as u32 as u64);
        assert!(compute(0x6d, i32::MIN as u32 as u64, u64::from(u32::MAX)).is_err());
        assert_eq!(compute(0x6f, i32::MIN as u32 as u64, u64::from(u32::MAX)).unwrap(), 0);
        assert!(compute(0x70, 1, 0).is_err());
        assert_eq!(compute(0x74, 1, 33).unwrap(), 2);
        assert_eq!(compute(0x75, 0x8000_0000, 31).unwrap(), 0xffff_ffff);
        assert_eq!(compute(0xc0, 0x80, 0).unwrap(), 0xffff_ff80);
        assert!(compute(0xa9, u64::from((-1.0f32).to_bits()), 0).is_err());
        assert_eq!(compute(0xa9, u64::from((-0.5f32).to_bits()), 0).unwrap(), 0);
        assert!(compute(0xb0, f64::NAN.to_bits(), 0).is_err());
        assert_eq!(compute(0xb0, (-9_223_372_036_854_775_808.0f64).to_bits(), 0).unwrap(), i64::MIN as u64);
        assert_eq!(f64::from_bits(compute(0xa4, 0f64.to_bits(), (-0f64).to_bits()).unwrap()).to_bits(), (-0f64).to_bits());
        assert_eq!(f32::from_bits(compute(0x90, u64::from(2.5f32.to_bits()), 0).unwrap() as u32), 2.0);
    }
}
