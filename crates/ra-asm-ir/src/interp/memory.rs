//! The memory of a run: a flat address space over regions of the host's own memory.
//!
//! Every address a lifted program computes is a host address, so the host's DLLs can be called
//! with the program's pointers as they are. The regions (the image at its base, the stacks, the
//! heap, the globals) are the host's allocations, registered here; an access outside them asks the
//! probe, which the host answers from its own knowledge of the process (a page a DLL handed the
//! program), and faults when nothing is there, naming the address.

use std::collections::BTreeMap;

use super::Stop;

/// A range of the address space the program may touch.
#[derive(Debug, Clone)]
pub struct Region {
    pub start: u64,
    pub len: u64,
    pub name: String,
}

impl Region {
    pub fn new(start: u64, len: u64, name: &str) -> Self {
        Self { start, len, name: name.to_owned() }
    }

    fn holds(&self, address: u64, len: u64) -> bool {
        address >= self.start && address.saturating_add(len) <= self.start.saturating_add(self.len)
    }
}

/// What the host answers for an address outside every region: the committed range it lies in.
pub type Probe = Box<dyn FnMut(u64) -> Option<Region>>;

/// What the program did to the memory while it was armed, as runs of adjacent bytes in address
/// order: `reads` holds the bytes it read before writing them itself, as they were at their first
/// read, and `writes` the bytes it wrote, as it left them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Access {
    pub reads: Vec<(u64, Vec<u8>)>,
    pub writes: Vec<(u64, Vec<u8>)>,
}

/// The log of an armed memory, by byte: an access may overlap any earlier one.
#[derive(Default)]
struct Log {
    reads: BTreeMap<u64, u8>,
    writes: BTreeMap<u64, u8>,
}

/// The bytes of a log joined into runs of adjacent addresses.
fn runs(bytes: &BTreeMap<u64, u8>) -> Vec<(u64, Vec<u8>)> {
    let mut out: Vec<(u64, Vec<u8>)> = Vec::new();
    for (&address, &byte) in bytes {
        match out.last_mut() {
            Some((start, run)) if start.checked_add(run.len() as u64) == Some(address) => run.push(byte),
            _ => out.push((address, vec![byte])),
        }
    }
    out
}

#[derive(Default)]
pub struct Memory {
    regions: Vec<Region>,
    probe: Option<Probe>,
    /// The region of the last access: the next one is usually in it.
    last: usize,
    /// The log of the accesses, while armed.
    log: Option<Log>,
}

impl Memory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_probe(&mut self, probe: Probe) {
        self.probe = Some(probe);
    }

    pub fn add_region(&mut self, region: Region) {
        self.regions.push(region);
    }

    pub fn remove_region(&mut self, start: u64) {
        self.regions.retain(|r| r.start != start);
        self.last = 0;
    }

    pub fn regions(&self) -> &[Region] {
        &self.regions
    }

    /// The region holding `address`, by name.
    pub fn region_of(&mut self, address: u64) -> Option<&Region> {
        self.locate(address, 1).ok().map(|i| &self.regions[i])
    }

    /// Starts a log of the accesses: the bytes read before they were written, and the bytes
    /// written. There is one log: arming again starts it over.
    pub fn arm(&mut self) {
        self.log = Some(Log::default());
    }

    /// Ends the log and returns it; empty when the memory was not armed.
    pub fn disarm(&mut self) -> Access {
        self.log.take().map_or_else(Access::default, |log| Access { reads: runs(&log.reads), writes: runs(&log.writes) })
    }

    fn locate(&mut self, address: u64, len: u64) -> Result<usize, Stop> {
        if let Some(r) = self.regions.get(self.last)
            && r.holds(address, len)
        {
            return Ok(self.last);
        }
        if let Some(i) = self.regions.iter().position(|r| r.holds(address, len)) {
            self.last = i;
            return Ok(i);
        }
        if address != 0
            && let Some(probe) = &mut self.probe
            && let Some(region) = probe(address)
            && region.holds(address, len)
        {
            self.regions.push(region);
            self.last = self.regions.len() - 1;
            return Ok(self.last);
        }
        Err(Stop::Fault(format!("an access to {len} bytes at {address:#x}, which is not mapped")))
    }

    /// Reads `buf.len()` bytes at `address`.
    pub fn read(&mut self, address: u64, buf: &mut [u8]) -> Result<(), Stop> {
        if buf.is_empty() {
            return Ok(());
        }
        self.locate(address, buf.len() as u64)?;
        // The region is the host's own memory, mapped and committed: the check above is the
        // whole safety argument.
        unsafe { std::ptr::copy_nonoverlapping(address as usize as *const u8, buf.as_mut_ptr(), buf.len()) };
        if let Some(log) = &mut self.log {
            // `locate` held the whole range, so the addresses do not wrap.
            for (at, &byte) in (address..).zip(buf.iter()) {
                if !log.writes.contains_key(&at) {
                    log.reads.entry(at).or_insert(byte);
                }
            }
        }
        Ok(())
    }

    /// Writes `bytes` at `address`.
    pub fn write(&mut self, address: u64, bytes: &[u8]) -> Result<(), Stop> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.locate(address, bytes.len() as u64)?;
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), address as usize as *mut u8, bytes.len()) };
        if let Some(log) = &mut self.log {
            log.writes.extend((address..).zip(bytes.iter().copied()));
        }
        Ok(())
    }

    pub fn read_vec(&mut self, address: u64, len: u64) -> Result<Vec<u8>, Stop> {
        let mut buf = vec![0u8; len as usize];
        self.read(address, &mut buf)?;
        Ok(buf)
    }

    pub fn fill(&mut self, address: u64, len: u64, byte: u8) -> Result<(), Stop> {
        self.write(address, &vec![byte; len as usize])
    }

    pub fn read_u8(&mut self, address: u64) -> Result<u8, Stop> {
        let mut b = [0u8; 1];
        self.read(address, &mut b)?;
        Ok(b[0])
    }

    pub fn read_u16(&mut self, address: u64) -> Result<u16, Stop> {
        let mut b = [0u8; 2];
        self.read(address, &mut b)?;
        Ok(u16::from_le_bytes(b))
    }

    pub fn read_u32(&mut self, address: u64) -> Result<u32, Stop> {
        let mut b = [0u8; 4];
        self.read(address, &mut b)?;
        Ok(u32::from_le_bytes(b))
    }

    pub fn read_u64(&mut self, address: u64) -> Result<u64, Stop> {
        let mut b = [0u8; 8];
        self.read(address, &mut b)?;
        Ok(u64::from_le_bytes(b))
    }

    /// A word of `bytes` bytes (4 or 8), zero-extended.
    pub fn read_word(&mut self, address: u64, bytes: u64) -> Result<u64, Stop> {
        if bytes == 4 { self.read_u32(address).map(u64::from) } else { self.read_u64(address) }
    }

    pub fn write_u8(&mut self, address: u64, v: u8) -> Result<(), Stop> {
        self.write(address, &[v])
    }

    pub fn write_u16(&mut self, address: u64, v: u16) -> Result<(), Stop> {
        self.write(address, &v.to_le_bytes())
    }

    pub fn write_u32(&mut self, address: u64, v: u32) -> Result<(), Stop> {
        self.write(address, &v.to_le_bytes())
    }

    pub fn write_u64(&mut self, address: u64, v: u64) -> Result<(), Stop> {
        self.write(address, &v.to_le_bytes())
    }

    pub fn write_word(&mut self, address: u64, bytes: u64, v: u64) -> Result<(), Stop> {
        if bytes == 4 { self.write_u32(address, v as u32) } else { self.write_u64(address, v) }
    }

    /// The bytes of a NUL-terminated string at `address`, without the NUL; at most `limit` bytes.
    pub fn read_cstr(&mut self, address: u64, limit: usize) -> Result<Vec<u8>, Stop> {
        let mut out = Vec::new();
        let mut at = address;
        while out.len() < limit {
            let b = self.read_u8(at)?;
            if b == 0 {
                break;
            }
            out.push(b);
            at += 1;
        }
        Ok(out)
    }

    /// A NUL-terminated UTF-16 string at `address`, as its code units.
    pub fn read_wstr(&mut self, address: u64, limit: usize) -> Result<Vec<u16>, Stop> {
        let mut out = Vec::new();
        let mut at = address;
        while out.len() < limit {
            let u = self.read_u16(at)?;
            if u == 0 {
                break;
            }
            out.push(u);
            at += 2;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod armed {
    use super::{Access, Memory, Region};

    /// A memory over one region of 16 bytes holding 0..16, with the region's address; the buffer
    /// is returned so it outlives the accesses.
    fn memory() -> (Memory, u64, Vec<u8>) {
        let mut bytes: Vec<u8> = (0..16).collect();
        let base = bytes.as_mut_ptr() as u64;
        let mut memory = Memory::new();
        memory.add_region(Region::new(base, 16, "test"));
        (memory, base, bytes)
    }

    #[test]
    fn a_read_is_logged_once() {
        let (mut memory, base, _bytes) = memory();
        memory.arm();
        assert_eq!(memory.read_u32(base + 4).unwrap(), 0x0706_0504);
        assert_eq!(memory.read_u32(base + 4).unwrap(), 0x0706_0504);
        let access = memory.disarm();
        assert_eq!(access.reads, [(base + 4, vec![4, 5, 6, 7])]);
        assert!(access.writes.is_empty());
    }

    #[test]
    fn a_read_after_a_write_is_not_logged() {
        let (mut memory, base, _bytes) = memory();
        memory.arm();
        memory.write_u16(base + 2, 0xbbaa).unwrap();
        assert_eq!(memory.read_u16(base + 2).unwrap(), 0xbbaa);
        // Only the two bytes the write did not cover are the function's input.
        assert_eq!(memory.read_u32(base).unwrap(), 0xbbaa_0100);
        let access = memory.disarm();
        assert_eq!(access.reads, [(base, vec![0, 1])]);
        assert_eq!(access.writes, [(base + 2, vec![0xaa, 0xbb])]);
    }

    #[test]
    fn a_first_read_keeps_its_bytes_over_a_later_write() {
        let (mut memory, base, _bytes) = memory();
        memory.arm();
        assert_eq!(memory.read_u8(base + 8).unwrap(), 8);
        memory.write_u8(base + 8, 0xff).unwrap();
        memory.write_u8(base + 8, 0xee).unwrap();
        assert_eq!(memory.read_u8(base + 8).unwrap(), 0xee);
        let access = memory.disarm();
        assert_eq!(access.reads, [(base + 8, vec![8])]);
        assert_eq!(access.writes, [(base + 8, vec![0xee])]);
    }

    #[test]
    fn overlapping_reads_are_merged() {
        let (mut memory, base, _bytes) = memory();
        memory.arm();
        memory.read_u32(base + 2).unwrap();
        memory.read_u32(base).unwrap();
        // Adjacent to the run above, and one apart from it.
        memory.read_u16(base + 6).unwrap();
        memory.read_u8(base + 12).unwrap();
        let access = memory.disarm();
        assert_eq!(access.reads, [(base, vec![0, 1, 2, 3, 4, 5, 6, 7]), (base + 12, vec![12])]);
    }

    #[test]
    fn the_log_is_empty_when_not_armed() {
        let (mut memory, base, _bytes) = memory();
        memory.read_u32(base).unwrap();
        memory.write_u32(base + 4, 1).unwrap();
        assert_eq!(memory.disarm(), Access::default());
        // A log ends with its disarm: what follows is not in the next one.
        memory.arm();
        memory.disarm();
        memory.read_u32(base).unwrap();
        assert_eq!(memory.disarm(), Access::default());
    }
}
