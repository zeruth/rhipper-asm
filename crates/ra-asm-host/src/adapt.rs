//! The adapters of the Win32 structures a 32-bit program and the 64-bit host hand each other: a
//! program's `WNDCLASSEXA` is laid out for 4-byte pointers, the host's call takes one laid out for
//! 8. An adapter is generated from the structure's two layouts in `ra-asm-protos`: an integer of
//! one width at both is copied, a pointer or an integer that widens is zero- or sign-extended on
//! the way to the host and narrowed on the way back only when the host's value fits in 32 bits,
//! and a nested structure is adapted field by field. A union has none, since its layouts do not
//! say which member it holds.

use ra_asm_protos::{Kind, Record, layout};

/// The structures the host converts for its calls: `RegisterClassExA`'s.
const ADAPTED: [&str; 1] = ["WNDCLASSEXA"];

/// How a field crosses between the widths.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum How {
    /// The same bytes at both.
    Copy,
    /// A pointer or an unsigned integer, zero-extended.
    Zero,
    /// A signed integer, sign-extended.
    Sign,
}

/// One field of the structure, nested ones flattened, at both widths.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Step {
    /// The field's path from the structure, an array element's by its index.
    field: String,
    at32: usize,
    at64: usize,
    narrow: usize,
    wide: usize,
    how: How,
}

/// The conversion of one structure between the program's layout and the host's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Adapter {
    record: &'static Record,
    steps: Vec<Step>,
}

/// The adapter the host has for the structure `name`, by its tag or a typedef name.
///
/// # Errors
/// A structure the layout table does not have, or one the host does not convert.
pub fn adapter(name: &str) -> Result<Adapter, String> {
    let record = layout(name).ok_or_else(|| format!("the layout table does not have {name}"))?;
    if !ADAPTED.iter().any(|adapted| layout(adapted).is_some_and(|r| r.name == record.name)) {
        return Err(format!("the host has no adapter for {}", record.name));
    }
    generate(record)
}

/// The adapter of `record`, generated from its two layouts.
///
/// # Errors
/// A structure whose fields share storage, a field that narrows from 32 to 64 bits or a float
/// or bit field that changes width, or a nested structure the layout table does not have.
pub fn generate(record: &'static Record) -> Result<Adapter, String> {
    let mut steps = Vec::new();
    plan(record, 0, 0, "", &mut steps)?;
    Ok(Adapter { record, steps })
}

fn plan(record: &Record, base32: usize, base64: usize, prefix: &str, steps: &mut Vec<Step>) -> Result<(), String> {
    if record.overlapping {
        return Err(format!("{}: its fields share storage, and their layouts do not say which one it holds", record.name));
    }
    for field in record.fields {
        let path = format!("{prefix}{}", field.name);
        let unequal = || {
            format!(
                "{}.{path}: a {:?} field of {} bytes at 32 bits and {} at 64",
                record.name, field.kind, field.at32.size, field.at64.size
            )
        };
        let (at32, at64) = (base32 + field.at32.offset as usize, base64 + field.at64.offset as usize);
        match field.kind {
            Kind::Record => {
                let inner = layout(field.record)
                    .ok_or_else(|| format!("{}.{path}: the layout table does not have {}", record.name, field.record))?;
                for element in 0..field.count as usize {
                    let prefix = if field.count == 1 { format!("{path}.") } else { format!("{path}[{element}].") };
                    plan(inner, at32 + element * inner.size32 as usize, at64 + element * inner.size64 as usize, &prefix, steps)?;
                }
            }
            Kind::Float | Kind::Bits => {
                if field.at32.size != field.at64.size {
                    return Err(unequal());
                }
                let size = field.at32.size as usize;
                steps.push(Step { field: path, at32, at64, narrow: size, wide: size, how: How::Copy });
            }
            Kind::Pointer | Kind::Signed | Kind::Unsigned => {
                let count = field.count.max(1) as usize;
                let (narrow, wide) = (field.at32.size as usize / count, field.at64.size as usize / count);
                if wide < narrow {
                    return Err(unequal());
                }
                let how = match field.kind {
                    _ if wide == narrow => How::Copy,
                    Kind::Signed => How::Sign,
                    _ => How::Zero,
                };
                for element in 0..count {
                    let field = if count == 1 { path.clone() } else { format!("{path}[{element}]") };
                    steps.push(Step { field, at32: at32 + element * narrow, at64: at64 + element * wide, narrow, wide, how });
                }
            }
        }
    }
    Ok(())
}

impl Adapter {
    /// The structure it converts.
    #[must_use]
    pub fn record(&self) -> &'static Record {
        self.record
    }

    /// The host's form of the program's structure `guest`; padding is zero.
    ///
    /// # Errors
    /// `guest` is not the structure's size at 32 bits.
    pub fn to_host(&self, guest: &[u8]) -> Result<Vec<u8>, String> {
        let size = self.record.size32 as usize;
        if guest.len() != size {
            return Err(format!("{}: {} bytes at 32 bits, not {size}", self.record.name, guest.len()));
        }
        let mut host = vec![0; self.record.size64 as usize];
        for step in &self.steps {
            let source = &guest[step.at32..][..step.narrow];
            let target = &mut host[step.at64..][..step.wide];
            let negative = step.how == How::Sign && source.last().is_some_and(|top| top & 0x80 != 0);
            target[..step.narrow].copy_from_slice(source);
            target[step.narrow..].fill(if negative { 0xFF } else { 0 });
        }
        Ok(host)
    }

    /// The program's form of the host's structure `host`; padding is zero.
    ///
    /// # Errors
    /// `host` is not the structure's size at 64 bits, or a pointer or an integer of it holds a
    /// value that does not fit in its 32-bit field.
    pub fn to_guest(&self, host: &[u8]) -> Result<Vec<u8>, String> {
        let size = self.record.size64 as usize;
        if host.len() != size {
            return Err(format!("{}: {} bytes at 64 bits, not {size}", self.record.name, host.len()));
        }
        let mut guest = vec![0; self.record.size32 as usize];
        for step in &self.steps {
            let source = &host[step.at64..][..step.wide];
            let (low, high) = source.split_at(step.narrow);
            let extension = match step.how {
                How::Sign if low.last().is_some_and(|top| top & 0x80 != 0) => 0xFF,
                _ => 0,
            };
            if step.how != How::Copy && high.iter().any(|&byte| byte != extension) {
                let mut value = [0; 8];
                value[..source.len().min(8)].copy_from_slice(&source[..source.len().min(8)]);
                return Err(format!(
                    "{}.{}: the host's {:#x} does not fit in {} bytes",
                    self.record.name,
                    step.field,
                    u64::from_le_bytes(value),
                    step.narrow
                ));
            }
            guest[step.at32..][..step.narrow].copy_from_slice(low);
        }
        Ok(guest)
    }
}

#[cfg(test)]
mod tests;
