//! A [`Reader`] reads fixed-width values at absolute offsets of a byte slice, and a [`Cursor`]
//! reads them in sequence, both in the reader's [`ByteOrder`]. A read past the end fails with the
//! name of the field, the offset and the length of the slice, so a malformed file points at its
//! own defect.

use anyhow::{Result, bail, ensure};

/// The order of the bytes of a multi-byte value. Little is the default, since the PE files the
/// engine reads first are little-endian; a loader of a big-endian container states it once.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ByteOrder {
    #[default]
    Little,
    Big,
}

/// Random access to a byte slice.
#[derive(Debug, Clone, Copy)]
pub struct Reader<'a> {
    bytes: &'a [u8],
    order: ByteOrder,
}

impl<'a> Reader<'a> {
    /// A little-endian reader.
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, order: ByteOrder::Little }
    }

    /// The same bytes read in `order`, by this reader and by every cursor it makes.
    #[must_use]
    pub fn with_order(self, order: ByteOrder) -> Self {
        Self { order, ..self }
    }

    pub fn order(&self) -> ByteOrder {
        self.order
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// `len` bytes at `offset`.
    pub fn slice(&self, offset: usize, len: usize, what: &str) -> Result<&'a [u8]> {
        match offset.checked_add(len) {
            Some(end) if end <= self.bytes.len() => Ok(&self.bytes[offset..end]),
            _ => bail!("{what} at offset {offset:#x} ({len:#x} bytes) is past the end of the data ({:#x} bytes)", self.bytes.len()),
        }
    }

    pub fn u8(&self, offset: usize, what: &str) -> Result<u8> {
        Ok(self.slice(offset, 1, what)?[0])
    }

    pub fn u16(&self, offset: usize, what: &str) -> Result<u16> {
        let b = self.slice(offset, 2, what)?;
        let b = [b[0], b[1]];
        Ok(match self.order {
            ByteOrder::Little => u16::from_le_bytes(b),
            ByteOrder::Big => u16::from_be_bytes(b),
        })
    }

    pub fn u32(&self, offset: usize, what: &str) -> Result<u32> {
        let b = self.slice(offset, 4, what)?;
        let b = [b[0], b[1], b[2], b[3]];
        Ok(match self.order {
            ByteOrder::Little => u32::from_le_bytes(b),
            ByteOrder::Big => u32::from_be_bytes(b),
        })
    }

    pub fn u64(&self, offset: usize, what: &str) -> Result<u64> {
        let b = self.slice(offset, 8, what)?;
        let b = [b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]];
        Ok(match self.order {
            ByteOrder::Little => u64::from_le_bytes(b),
            ByteOrder::Big => u64::from_be_bytes(b),
        })
    }

    pub fn i32(&self, offset: usize, what: &str) -> Result<i32> {
        Ok(self.u32(offset, what)? as i32)
    }

    /// A NUL-terminated string at `offset`, at most `max` bytes long. It fails when no NUL comes
    /// within `max` bytes or the end of the data, or when the bytes are not UTF-8.
    pub fn cstr(&self, offset: usize, max: usize, what: &str) -> Result<&'a str> {
        ensure!(offset <= self.bytes.len(), "{what} at offset {offset:#x} is past the end of the data ({:#x} bytes)", self.bytes.len());
        let window = &self.bytes[offset..self.bytes.len().min(offset.saturating_add(max))];
        let Some(end) = window.iter().position(|&b| b == 0) else {
            bail!("{what} at offset {offset:#x} has no terminator within {max} bytes");
        };
        match std::str::from_utf8(&window[..end]) {
            Ok(s) => Ok(s),
            Err(_) => bail!("{what} at offset {offset:#x} is not UTF-8"),
        }
    }

    /// A string in a fixed field of `len` bytes, cut at the first NUL. Non-UTF-8 bytes become
    /// U+FFFD, since a section name is a label, not data.
    pub fn fixed_str(&self, offset: usize, len: usize, what: &str) -> Result<String> {
        let b = self.slice(offset, len, what)?;
        let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
        Ok(String::from_utf8_lossy(&b[..end]).into_owned())
    }

    /// A cursor that starts at `offset`.
    pub fn at(&self, offset: usize) -> Cursor<'a> {
        Cursor { reader: *self, pos: offset }
    }
}

/// Sequential access to a byte slice. Each read advances the position.
#[derive(Debug, Clone, Copy)]
pub struct Cursor<'a> {
    reader: Reader<'a>,
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub fn position(&self) -> usize {
        self.pos
    }

    /// The order of the reader the cursor came from.
    pub fn order(&self) -> ByteOrder {
        self.reader.order
    }

    /// Moves past `len` bytes. A skip past the end is not a refusal itself: the next read is, and
    /// it names the field it wanted.
    pub fn skip(&mut self, len: usize) -> &mut Self {
        self.pos = self.pos.saturating_add(len);
        self
    }

    pub fn slice(&mut self, len: usize, what: &str) -> Result<&'a [u8]> {
        let s = self.reader.slice(self.pos, len, what)?;
        self.pos += len;
        Ok(s)
    }

    pub fn u8(&mut self, what: &str) -> Result<u8> {
        let v = self.reader.u8(self.pos, what)?;
        self.pos += 1;
        Ok(v)
    }

    pub fn u16(&mut self, what: &str) -> Result<u16> {
        let v = self.reader.u16(self.pos, what)?;
        self.pos += 2;
        Ok(v)
    }

    pub fn u32(&mut self, what: &str) -> Result<u32> {
        let v = self.reader.u32(self.pos, what)?;
        self.pos += 4;
        Ok(v)
    }

    pub fn u64(&mut self, what: &str) -> Result<u64> {
        let v = self.reader.u64(self.pos, what)?;
        self.pos += 8;
        Ok(v)
    }

    pub fn i32(&mut self, what: &str) -> Result<i32> {
        Ok(self.u32(what)? as i32)
    }

    pub fn fixed_str(&mut self, len: usize, what: &str) -> Result<String> {
        let s = self.reader.fixed_str(self.pos, len, what)?;
        self.pos += len;
        Ok(s)
    }
}

#[cfg(test)]
mod tests;
