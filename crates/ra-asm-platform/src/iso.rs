//! A disc as an ISO 9660 image: the sectors by number, and a file by its path through the
//! directories from the primary volume descriptor's root. An image of raw 2352-byte sectors (a
//! `.bin` of mode 2 form 1) reads through to the 2048 bytes of user data in each.

use std::io::{Read, Seek, SeekFrom};

const SECTOR: usize = 2048;

/// A disc image over what it is read from.
pub struct Iso<R> {
    source: R,
    /// The bytes of a sector in the image, and where its user data starts: 2048 and 0, or 2352
    /// and 24 for a raw image.
    stride: u64,
    skip: u64,
    /// The root directory's first sector and its length in bytes.
    root: (u32, u32),
}

impl<R: Read + Seek> Iso<R> {
    /// The image, when its sixteenth sector holds a primary volume descriptor in either form;
    /// what it was to be read from, given back, when it holds none.
    pub fn open(source: R) -> Result<Self, R> {
        let mut iso = Self { source, stride: 2048, skip: 0, root: (0, 0) };
        for (stride, skip) in [(2048, 0), (2352, 24), (2352, 16)] {
            (iso.stride, iso.skip) = (stride, skip);
            if let Some(volume) = iso.sector(16)
                && volume[0] == 1
                && &volume[1..6] == b"CD001"
            {
                let field = |at: usize| u32::from_le_bytes([volume[at], volume[at + 1], volume[at + 2], volume[at + 3]]);
                iso.root = (field(158), field(166));
                return Ok(iso);
            }
        }
        Err(iso.source)
    }

    /// What the image is read from.
    pub fn source_mut(&mut self) -> &mut R {
        &mut self.source
    }

    /// The user data of the sector `lba`.
    pub fn sector(&mut self, lba: u32) -> Option<Vec<u8>> {
        let mut out = vec![0; SECTOR];
        self.source.seek(SeekFrom::Start(u64::from(lba) * self.stride + self.skip)).ok()?;
        self.source.read_exact(&mut out).ok()?;
        Some(out)
    }

    fn extent(&mut self, (lba, len): (u32, u32)) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(len as usize);
        for i in 0..len.div_ceil(SECTOR as u32) {
            out.extend(self.sector(lba + i)?);
        }
        out.truncate(len as usize);
        Some(out)
    }

    /// The entries of the directory at `extent`: each name without its version, whether it is a
    /// directory, and its extent.
    fn entries(&mut self, extent: (u32, u32)) -> Option<Vec<(String, bool, (u32, u32))>> {
        let bytes = self.extent(extent)?;
        let mut out = Vec::new();
        let mut at = 0;
        while at < bytes.len() {
            let len = usize::from(bytes[at]);
            if len == 0 {
                // A record does not cross a sector: the rest of this one is padding.
                at = (at / SECTOR + 1) * SECTOR;
                continue;
            }
            let record = bytes.get(at..at + len)?;
            let name = record.get(33..33 + usize::from(record[32]))?;
            if name != [0] && name != [1] {
                let name = String::from_utf8_lossy(name);
                let name = name.split(';').next().unwrap_or(&name).trim_end_matches('.').to_owned();
                let extent = (u32::from_le_bytes(record[2..6].try_into().ok()?), u32::from_le_bytes(record[10..14].try_into().ok()?));
                out.push((name, record[25] & 2 != 0, extent));
            }
            at += len;
        }
        Some(out)
    }

    /// Where the file at `path` lies (its first sector and its length), its names matched without
    /// regard to case.
    pub fn locate(&mut self, path: &str) -> Option<(u32, u32)> {
        let mut at = self.root;
        let mut parts = path.split(['/', '\\']).filter(|p| !p.is_empty()).peekable();
        while let Some(part) = parts.next() {
            let part = part.split(';').next().unwrap_or(part).trim_end_matches('.');
            let last = parts.peek().is_none();
            let (.., extent) = self.entries(at)?.into_iter().find(|(name, dir, _)| name.eq_ignore_ascii_case(part) && *dir != last)?;
            at = extent;
        }
        Some(at)
    }

    /// The bytes of the file at `path`.
    pub fn file(&mut self, path: &str) -> Option<Vec<u8>> {
        let extent = self.locate(path)?;
        self.extent(extent)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn record(name: &[u8], directory: bool, lba: u32, len: u32) -> Vec<u8> {
        let mut out = vec![0u8; 33];
        out[2..6].copy_from_slice(&lba.to_le_bytes());
        out[10..14].copy_from_slice(&len.to_le_bytes());
        out[25] = if directory { 2 } else { 0 };
        out[32] = name.len() as u8;
        out.extend_from_slice(name);
        if out.len() % 2 == 1 {
            out.push(0);
        }
        out[0] = out.len() as u8;
        out
    }

    /// A disc of one directory `DATA` that holds `HELLO.TXT;1`, and `SYSTEM.CNF;1` in the root.
    pub(crate) fn image() -> Vec<u8> {
        let mut image = vec![0u8; 24 * SECTOR];
        let volume = &mut image[16 * SECTOR..];
        volume[0] = 1;
        volume[1..6].copy_from_slice(b"CD001");
        volume[158..162].copy_from_slice(&18u32.to_le_bytes());
        volume[166..170].copy_from_slice(&(SECTOR as u32).to_le_bytes());
        let root = [
            record(&[0], true, 18, 2048),
            record(&[1], true, 18, 2048),
            record(b"DATA", true, 19, 2048),
            record(b"SYSTEM.CNF;1", false, 21, 5),
        ]
        .concat();
        image[18 * SECTOR..18 * SECTOR + root.len()].copy_from_slice(&root);
        let data = [record(&[0], true, 19, 2048), record(&[1], true, 18, 2048), record(b"HELLO.TXT;1", false, 20, 6)].concat();
        image[19 * SECTOR..19 * SECTOR + data.len()].copy_from_slice(&data);
        image[20 * SECTOR..20 * SECTOR + 6].copy_from_slice(b"hello\n");
        image[21 * SECTOR..21 * SECTOR + 5].copy_from_slice(b"BOOT=");
        image
    }

    #[test]
    fn a_file_is_found_through_its_directories_whatever_its_case() {
        let mut iso = Iso::open(std::io::Cursor::new(image())).expect("a volume descriptor");
        assert_eq!(iso.file("data/hello.txt").as_deref(), Some(&b"hello\n"[..]));
        assert_eq!(iso.file("\\SYSTEM.CNF;1").as_deref(), Some(&b"BOOT="[..]));
        assert_eq!(iso.locate("DATA/HELLO.TXT"), Some((20, 6)));
        assert_eq!(iso.file("DATA"), None, "a directory is no file");
        assert_eq!(iso.file("NONE.TXT"), None);
    }

    #[test]
    fn a_raw_image_reads_through_to_the_user_data() {
        let cooked = image();
        let mut raw = Vec::new();
        for sector in cooked.chunks(SECTOR) {
            raw.extend_from_slice(&[0; 24]);
            raw.extend_from_slice(sector);
            raw.extend_from_slice(&[0; 280]);
        }
        let mut iso = Iso::open(std::io::Cursor::new(raw)).expect("a raw image");
        assert_eq!(iso.file("DATA/HELLO.TXT").as_deref(), Some(&b"hello\n"[..]));
    }

    #[test]
    fn a_file_that_is_no_disc_is_refused() {
        assert!(Iso::open(std::io::Cursor::new(vec![0u8; 40 * SECTOR])).is_err());
    }
}
