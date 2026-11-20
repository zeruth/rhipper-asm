//! The PlayStation's memory card: 128 KiB in sectors of 128 bytes, behind the serial protocol of
//! the controller port, where it answers the address `81`. A transfer reads a sector, writes one
//! or asks the card what it is; every byte the console sends is answered with the byte the card
//! had ready, and acknowledged while the transfer goes on.
//!
//! The card's bytes are a file of the host's storage, a raw image as the card holds it. A file
//! that is new is a card as it is sold: formatted, with no save on it.

use crate::host::File;

/// The bytes of a sector, the sectors of a card, the sectors of one of its sixteen blocks.
pub const SECTOR: usize = 128;
pub const SECTORS: usize = 1024;
const BLOCK: usize = 64;

/// The flag byte's bits: no sector written since the card went in; the last write failed.
const FRESH: u8 = 0x08;
const FAILED: u8 = 0x04;
/// What ends a transfer: done, a checksum that does not hold, a sector the card has not.
const GOOD: u8 = 0x47;
const BAD_CHECKSUM: u8 = 0x4e;
const BAD_SECTOR: u8 = 0xff;

/// A card as it is sold: the header sector with its `MC`, fifteen directory sectors that say
/// their blocks are free, twenty of the list of broken sectors (none), and the header again as
/// the sector a write test goes to.
pub fn formatted() -> Vec<u8> {
    let mut card = vec![0u8; SECTOR * SECTORS];
    let sector = |n: usize| n * SECTOR..(n + 1) * SECTOR;
    let checked = |bytes: &mut [u8]| bytes[SECTOR - 1] = bytes[..SECTOR - 1].iter().fold(0, |sum, byte| sum ^ byte);
    card[..2].copy_from_slice(b"MC");
    checked(&mut card[sector(0)]);
    for n in 1..16 {
        let entry = &mut card[sector(n)];
        entry[0] = 0xa0;
        entry[8..10].fill(0xff);
        checked(entry);
    }
    for n in 16..36 {
        let entry = &mut card[sector(n)];
        entry[..4].fill(0xff);
        entry[8..10].fill(0xff);
        checked(entry);
    }
    card[36 * SECTOR..(BLOCK - 1) * SECTOR].fill(0xff);
    card.copy_within(sector(0), (BLOCK - 1) * SECTOR);
    card
}

/// One slot's end of the protocol.
#[derive(Debug)]
pub struct Slot {
    /// How many bytes of the transfer the console sent.
    step: usize,
    command: u8,
    /// The byte the console sent last, which the card echoes where it has nothing to say.
    previous: u8,
    sector: u16,
    data: Vec<u8>,
    flag: u8,
}

impl Default for Slot {
    fn default() -> Self {
        Self { step: 0, command: 0, previous: 0, sector: 0, data: Vec::new(), flag: FRESH }
    }
}

impl Slot {
    /// Ends the transfer: the console dropped the port's select line.
    pub fn deselect(&mut self) {
        self.step = 0;
    }

    /// The sector `n` of `card`, zeros where the file is short.
    fn load(card: &mut dyn File, n: u16) -> Vec<u8> {
        let mut bytes = vec![0; SECTOR];
        if card.seek(u64::from(n) * SECTOR as u64).is_some() {
            card.read(&mut bytes);
        }
        bytes
    }

    /// The card's answer to `sent` and whether it acknowledges; `card` is the image of the card
    /// in the slot, which answers nothing when the slot is empty.
    pub fn exchange(&mut self, card: Option<&mut dyn File>, sent: u8) -> (u8, bool) {
        let Some(card) = card else { return (0xff, false) };
        let step = self.step;
        let previous = std::mem::replace(&mut self.previous, sent);
        self.step += 1;
        let done = |slot: &mut Self, byte: u8| {
            slot.step = 0;
            (byte, false)
        };
        match (self.command, step) {
            (_, 0) if sent == 0x81 => (0xff, true),
            (_, 0) => done(self, 0xff),
            (_, 1) => {
                self.command = sent;
                let flag = self.flag;
                if matches!(sent, b'R' | b'W' | b'S') { (flag, true) } else { done(self, flag) }
            }
            // The card's two identifier bytes.
            (_, 2) => (0x5a, true),
            (_, 3) => (0x5d, true),
            // What the card is: the acknowledge, then four bytes that say a card of 128 KiB.
            (b'S', 4) => (0x5c, true),
            (b'S', 5) => (0x5d, true),
            (b'S', 6) => (0x04, true),
            (b'S', 7 | 8) => (0x00, true),
            (b'S', _) => done(self, 0x80),
            // The sector's number, its upper byte first.
            (_, 4) => {
                self.sector = u16::from(sent) << 8;
                (0x00, true)
            }
            (_, 5) => {
                self.sector |= u16::from(sent);
                self.data.clear();
                (previous, true)
            }
            // A read: the acknowledge, the number back, the sector, its checksum, the end.
            (b'R', 6) => (0x5c, true),
            (b'R', 7) => (0x5d, true),
            (b'R', 8) if usize::from(self.sector) >= SECTORS => (0xff, true),
            (b'R', 9) if usize::from(self.sector) >= SECTORS => done(self, 0xff),
            (b'R', 8) => {
                self.data = Self::load(card, self.sector);
                ((self.sector >> 8) as u8, true)
            }
            (b'R', 9) => (self.sector as u8, true),
            (b'R', n) if n < 10 + SECTOR => (self.data[n - 10], true),
            (b'R', n) if n == 10 + SECTOR => {
                let [high, low] = self.sector.to_be_bytes();
                (self.data.iter().fold(high ^ low, |sum, byte| sum ^ byte), true)
            }
            (b'R', _) => done(self, GOOD),
            // A write: the sector, its checksum, the acknowledge, and how it went.
            (b'W', n) if n <= 6 + SECTOR => {
                self.data.push(sent);
                (previous, true)
            }
            (b'W', n) if n == 7 + SECTOR => (0x5c, true),
            (b'W', n) if n == 8 + SECTOR => (0x5d, true),
            (b'W', _) => {
                let [high, low] = self.sector.to_be_bytes();
                let (bytes, checksum) = self.data.split_at(SECTOR);
                let end = if usize::from(self.sector) >= SECTORS {
                    BAD_SECTOR
                } else if bytes.iter().fold(high ^ low, |sum, byte| sum ^ byte) != checksum[0] {
                    BAD_CHECKSUM
                } else if card.seek(u64::from(self.sector) * SECTOR as u64).is_some() && card.write(bytes) == SECTOR {
                    GOOD
                } else {
                    BAD_SECTOR
                };
                self.flag = if end == GOOD { self.flag & !(FRESH | FAILED) } else { self.flag | FAILED };
                done(self, end)
            }
            _ => done(self, 0xff),
        }
    }
}

/// The image of the card in a slot as the host's storage has it: the file, made a formatted
/// card when it is new or shorter than a card.
pub fn open(mut file: Box<dyn File>) -> Box<dyn File> {
    if file.len() < (SECTOR * SECTORS) as u64 && file.seek(0).is_some() {
        file.write(&formatted());
    }
    file
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::Bytes;

    /// A card in memory that takes writes, as a file of the host does.
    struct Image(Vec<u8>, usize);

    impl File for Image {
        fn read(&mut self, buf: &mut [u8]) -> usize {
            let n = buf.len().min(self.0.len() - self.1);
            buf[..n].copy_from_slice(&self.0[self.1..self.1 + n]);
            self.1 += n;
            n
        }

        fn write(&mut self, bytes: &[u8]) -> usize {
            self.0[self.1..self.1 + bytes.len()].copy_from_slice(bytes);
            self.1 += bytes.len();
            bytes.len()
        }

        fn seek(&mut self, offset: u64) -> Option<u64> {
            self.1 = usize::try_from(offset).ok().filter(|at| *at <= self.0.len())?;
            Some(offset)
        }

        fn len(&mut self) -> u64 {
            self.0.len() as u64
        }
    }

    fn transfer(slot: &mut Slot, card: &mut dyn File, sent: &[u8]) -> Vec<u8> {
        let mut answers = Vec::new();
        for (i, byte) in sent.iter().enumerate() {
            let (answer, acknowledged) = slot.exchange(Some(card), *byte);
            answers.push(answer);
            assert_eq!(acknowledged, i + 1 < sent.len(), "byte {i} of {}", sent.len());
        }
        answers
    }

    #[test]
    fn a_new_card_is_formatted() {
        let card = formatted();
        assert_eq!(&card[..2], b"MC");
        assert_eq!(card[127], 0x0e);
        assert_eq!((card[128], card[128 + 8], card[128 + 127]), (0xa0, 0xff, 0xa0), "a free directory entry");
        assert_eq!((card[16 * 128], card[16 * 128 + 127]), (0xff, 0x00), "no broken sector");
        assert_eq!(&card[63 * 128..63 * 128 + 2], b"MC", "the write test's sector");
    }

    #[test]
    fn a_read_answers_the_sector_with_its_checksum() {
        let mut card = Image(formatted(), 0);
        let mut slot = Slot::default();
        let mut sent = vec![0x81, b'R', 0, 0, 0x00, 0x01];
        sent.resize(6 + 4 + SECTOR + 2, 0);
        let got = transfer(&mut slot, &mut card, &sent);
        assert_eq!(got[..10], [0xff, FRESH, 0x5a, 0x5d, 0x00, 0x00, 0x5c, 0x5d, 0x00, 0x01]);
        assert_eq!(got[10], 0xa0, "the first directory entry");
        // The checksum covers the sector's number too: a sector that holds its own sums to it.
        assert_eq!(got[10 + SECTOR], 0x01);
        assert_eq!(got[11 + SECTOR], GOOD);
        // A sector past the card's last ends the transfer where the number comes back.
        let got = transfer(&mut slot, &mut card, &[0x81, b'R', 0, 0, 0x04, 0x00, 0, 0, 0, 0]);
        assert_eq!(got[8..], [0xff, 0xff]);
    }

    #[test]
    fn a_write_lands_when_its_checksum_holds() {
        let mut card = Image(formatted(), 0);
        let mut slot = Slot::default();
        let data = [0x5a; SECTOR];
        let mut sent = vec![0x81, b'W', 0, 0, 0x00, 0x40];
        sent.extend_from_slice(&data);
        sent.extend_from_slice(&[0x40, 0, 0, 0]);
        let got = transfer(&mut slot, &mut card, &sent);
        assert_eq!(got[got.len() - 3..], [0x5c, 0x5d, GOOD]);
        assert_eq!(card.0[0x40 * SECTOR..0x41 * SECTOR], data);
        // The card was written: the flag says so from the next transfer on.
        assert_eq!(slot.exchange(Some(&mut card), 0x81), (0xff, true));
        assert_eq!(slot.exchange(Some(&mut card), b'S'), (0, true));
        slot.deselect();
        // A checksum that does not hold leaves the card as it is.
        let last = sent.len() - 4;
        sent[last] ^= 1;
        sent[6..6 + SECTOR].fill(0x11);
        let got = transfer(&mut slot, &mut card, &sent);
        assert_eq!(got[got.len() - 1], BAD_CHECKSUM);
        assert_eq!(card.0[0x40 * SECTOR], 0x5a);
    }

    #[test]
    fn the_card_says_what_it_is_and_an_empty_slot_nothing() {
        let mut card = Bytes { bytes: formatted(), at: 0 };
        let mut slot = Slot::default();
        let got = transfer(&mut slot, &mut card, &[0x81, b'S', 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(got, [0xff, FRESH, 0x5a, 0x5d, 0x5c, 0x5d, 0x04, 0x00, 0x00, 0x80]);
        assert_eq!(slot.exchange(None, 0x81), (0xff, false));
        assert_eq!(slot.exchange(Some(&mut card), 0x01), (0xff, false), "a pad's address");
    }
}
