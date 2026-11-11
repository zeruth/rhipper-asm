//! The memory of a run: a flat address space over regions of the host's own memory.
//!
//! Every address a lifted program computes is a host address, so the host's DLLs can be called
//! with the program's pointers as they are. The regions (the image at its base, the stacks, the
//! heap, the globals) are the host's allocations, registered here; an access outside them asks the
//! probe, which the host answers from its own knowledge of the process (a page a DLL handed the
//! program), and faults when nothing is there, naming the address.

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

#[derive(Default)]
pub struct Memory {
    regions: Vec<Region>,
    probe: Option<Probe>,
    /// The region of the last access: the next one is usually in it.
    last: usize,
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
        Ok(())
    }

    /// Writes `bytes` at `address`.
    pub fn write(&mut self, address: u64, bytes: &[u8]) -> Result<(), Stop> {
        if bytes.is_empty() {
            return Ok(());
        }
        self.locate(address, bytes.len() as u64)?;
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), address as usize as *mut u8, bytes.len()) };
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
