//! The PlayStation's CD-ROM controller: four registers behind an index, a command with its
//! parameters in, and the answers out as numbered interrupts, each with its bytes, one at a time
//! as the program acknowledges them. A read delivers a sector per interrupt, which the program
//! takes from the data port or by DMA. The drive's timing is a fixed delay per answer.

use std::collections::VecDeque;

/// The cycles from a command to its first answer, from that to its second, and from one sector of
/// a read to the next at double speed.
const FIRST: u64 = 20_000;
const SECOND: u64 = 200_000;
const SECTOR: u64 = 33_868_800 / 150;

/// The answers by their interrupt's number.
const DATA_READY: u8 = 1;
const COMPLETE: u8 = 2;
const ACKNOWLEDGE: u8 = 3;
const ERROR: u8 = 5;

/// The drive's status byte: the motor runs, the lid is open (no disc), a read is going.
const MOTOR: u8 = 0x02;
const OPEN: u8 = 0x10;
const READING: u8 = 0x20;

#[derive(Default)]
pub(super) struct Cdrom {
    index: u8,
    parameters: Vec<u8>,
    response: VecDeque<u8>,
    data: VecDeque<u8>,
    /// The sector the drive read last, which the program asks into the data port.
    sector: Vec<u8>,
    enable: u8,
    flag: u8,
    /// The answers still to come: the cycles after the one before, the interrupt, the bytes.
    pending: VecDeque<(u64, u8, Vec<u8>)>,
    /// The cycles the head of `pending` still waits.
    wait: u64,
    /// Where the next sector is read, and whether the drive is reading.
    position: u32,
    reading: bool,
    mode: u8,
    /// Whether the host has a disc: decided when the drive is first asked.
    pub(super) disc: bool,
}

fn bcd(v: u8) -> u32 {
    u32::from(v >> 4) * 10 + u32::from(v & 0xf)
}

impl Cdrom {
    fn stat(&self) -> u8 {
        if !self.disc {
            return OPEN;
        }
        MOTOR | if self.reading { READING } else { 0 }
    }

    pub(super) fn read(&mut self, register: u64) -> u8 {
        match register & 3 {
            0 => {
                let mut status = self.index | 0x08 | 0x10;
                if !self.parameters.is_empty() {
                    status &= !0x08;
                }
                status |= u8::from(!self.response.is_empty()) << 5;
                status |= u8::from(!self.data.is_empty()) << 6;
                status
            }
            1 => self.response.pop_front().unwrap_or(0),
            2 => self.data.pop_front().unwrap_or(0),
            _ if self.index & 1 == 0 => self.enable | 0xe0,
            _ => self.flag | 0xe0,
        }
    }

    pub(super) fn write(&mut self, register: u64, value: u8) {
        match (register & 3, self.index) {
            (0, _) => self.index = value & 3,
            (1, 0) => self.command(value),
            (2, 0) => self.parameters.push(value),
            (2, 1) => self.enable = value & 0x1f,
            (3, 0) => {
                // The program wants the sector's data in the port, or drops what is there.
                if value & 0x80 != 0 {
                    if self.data.is_empty() {
                        self.data = self.sector.iter().copied().collect();
                    }
                } else {
                    self.data.clear();
                }
            }
            (3, 1) => {
                self.flag &= !(value & 0x1f);
                if value & 0x40 != 0 {
                    self.parameters.clear();
                }
            }
            _ => {}
        }
    }

    fn answer(&mut self, after: u64, interrupt: u8, bytes: Vec<u8>) {
        if self.pending.is_empty() {
            self.wait = after;
        }
        self.pending.push_back((after, interrupt, bytes));
    }

    fn command(&mut self, command: u8) {
        let parameters = std::mem::take(&mut self.parameters);
        let stat = self.stat();
        if crate::tracing() {
            eprintln!("cdrom: command {command:#04x} {parameters:02x?}");
        }
        let needs_disc = matches!(command, 0x02 | 0x06 | 0x09 | 0x15 | 0x16 | 0x1b | 0x1e);
        if needs_disc && !self.disc {
            self.answer(FIRST, ERROR, vec![stat | 1, 0x80]);
            return;
        }
        match command {
            // Getstat, Mute, Demute, Setfilter, MotorOn's first answer.
            0x01 | 0x0b | 0x0c | 0x0d => self.answer(FIRST, ACKNOWLEDGE, vec![stat]),
            // Setloc: minutes, seconds and frames in BCD, from the two seconds of the lead-in.
            0x02 => {
                let field = |i: usize| parameters.get(i).copied().map_or(0, bcd);
                self.position = ((field(0) * 60 + field(1)) * 75 + field(2)).saturating_sub(150);
                self.answer(FIRST, ACKNOWLEDGE, vec![stat]);
            }
            // ReadN, ReadS: an acknowledge, then a sector an interrupt until a pause.
            0x06 | 0x1b => {
                self.reading = true;
                self.pending.retain(|(_, interrupt, _)| *interrupt != DATA_READY);
                self.answer(FIRST, ACKNOWLEDGE, vec![self.stat()]);
                self.answer(SECTOR, DATA_READY, Vec::new());
            }
            // MotorOn, Stop, Pause, Init, SeekL, SeekP, ReadTOC: an acknowledge, then the end.
            0x07 | 0x08 | 0x09 | 0x0a | 0x15 | 0x16 | 0x1e => {
                if matches!(command, 0x08 | 0x09 | 0x0a) {
                    self.reading = false;
                    self.pending.retain(|(_, interrupt, _)| *interrupt != DATA_READY);
                }
                if command == 0x0a {
                    self.mode = 0;
                }
                self.answer(FIRST, ACKNOWLEDGE, vec![stat]);
                self.answer(SECOND, COMPLETE, vec![self.stat()]);
            }
            0x0e => {
                self.mode = parameters.first().copied().unwrap_or(0);
                self.answer(FIRST, ACKNOWLEDGE, vec![stat]);
            }
            // GetlocL: the header of the sector read last. GetlocP: the position on the track.
            0x10 | 0x11 => {
                let at = self.position + 150;
                let to_bcd = |v: u32| ((v / 10) << 4 | v % 10) as u8;
                let (m, s, f) = (to_bcd(at / 4500), to_bcd(at / 75 % 60), to_bcd(at % 75));
                let bytes = if command == 0x10 { vec![m, s, f, 2, 0, 0, 0, 0] } else { vec![1, 1, m, s, f, m, s, f] };
                self.answer(FIRST, ACKNOWLEDGE, bytes);
            }
            // GetTN: one track. GetTD: where it starts.
            0x13 => self.answer(FIRST, ACKNOWLEDGE, vec![stat, 1, 1]),
            0x14 => self.answer(FIRST, ACKNOWLEDGE, vec![stat, 0, 2]),
            // Test 20: the controller's version.
            0x19 => self.answer(FIRST, ACKNOWLEDGE, vec![0x94, 0x09, 0x19, 0xc0]),
            // GetID: a licensed data disc, or none.
            0x1a => {
                if self.disc {
                    self.answer(FIRST, ACKNOWLEDGE, vec![stat]);
                    self.answer(SECOND, COMPLETE, vec![0x02, 0x00, 0x20, 0x00, b'S', b'C', b'E', b'A']);
                } else {
                    self.answer(FIRST, ERROR, vec![0x11, 0x80]);
                }
            }
            _ => self.answer(FIRST, ERROR, vec![stat | 1, 0x40]),
        }
    }

    /// Lets `cycles` pass. When an answer comes due and the program has acknowledged the one
    /// before, it is raised, a sector's with the data `sector` reads off the disc for its
    /// number; the result says whether the controller's interrupt line rose.
    pub(super) fn advance(&mut self, cycles: u64, mut sector: impl FnMut(u32) -> Option<Vec<u8>>) -> bool {
        self.wait = self.wait.saturating_sub(cycles);
        if self.wait > 0 || self.flag != 0 {
            return false;
        }
        let Some((_, interrupt, mut bytes)) = self.pending.pop_front() else { return false };
        if interrupt == DATA_READY {
            match sector(self.position) {
                Some(data) => {
                    // The whole sector's form has its address and mode ahead of the data.
                    self.sector = if self.mode & 0x20 != 0 { [vec![0; 12], data].concat() } else { data };
                    // What the program left of the sector before is gone with it.
                    self.data.clear();
                    self.position += 1;
                    bytes = vec![self.stat()];
                    self.pending.push_back((SECTOR, DATA_READY, Vec::new()));
                }
                None => {
                    self.reading = false;
                    return self.raise(ERROR, vec![self.stat() | 1, 0x04]);
                }
            }
        }
        self.raise(interrupt, bytes)
    }

    fn raise(&mut self, interrupt: u8, bytes: Vec<u8>) -> bool {
        self.wait = self.pending.front().map_or(0, |(after, ..)| *after);
        if crate::tracing() {
            eprintln!("cdrom: interrupt {interrupt} {bytes:02x?}, enabled {:#04x}", self.enable);
        }
        self.response = bytes.into();
        self.flag = interrupt;
        self.enable & interrupt != 0
    }

    /// Bytes of the data port, as a DMA block takes them.
    pub(super) fn pull(&mut self, len: usize) -> Vec<u8> {
        if self.data.is_empty() {
            self.data = self.sector.iter().copied().collect();
        }
        (0..len).map(|_| self.data.pop_front().unwrap_or(0)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive() -> Cdrom {
        let mut cd = Cdrom { disc: true, ..Cdrom::default() };
        cd.write(0, 1);
        cd.write(2, 0x1f);
        cd.write(0, 0);
        cd
    }

    fn acknowledge(cd: &mut Cdrom) {
        cd.write(0, 1);
        cd.write(3, 0x1f);
        cd.write(0, 0);
    }

    #[test]
    fn a_command_answers_with_its_interrupt_and_waits_for_the_acknowledge() {
        let mut cd = drive();
        cd.write(1, 0x0a);
        assert!(!cd.advance(FIRST - 1, |_| None));
        assert!(cd.advance(1, |_| None), "the acknowledge raises the line");
        cd.write(0, 1);
        assert_eq!(cd.read(3) & 7, ACKNOWLEDGE);
        assert_eq!(cd.read(1), MOTOR);
        // The second answer waits for the program to acknowledge the first.
        assert!(!cd.advance(SECOND * 2, |_| None));
        acknowledge(&mut cd);
        assert!(cd.advance(0, |_| None));
        cd.write(0, 1);
        assert_eq!(cd.read(3) & 7, COMPLETE);
    }

    #[test]
    fn a_read_delivers_the_sectors_from_the_location_set() {
        let mut cd = drive();
        for parameter in [0x00, 0x02, 0x16] {
            cd.write(2, parameter);
        }
        cd.write(1, 0x02);
        assert!(cd.advance(FIRST, |_| None));
        acknowledge(&mut cd);
        cd.write(1, 0x06);
        assert!(cd.advance(FIRST, |_| None));
        acknowledge(&mut cd);
        let mut asked = Vec::new();
        assert!(cd.advance(SECTOR, |lba| {
            asked.push(lba);
            Some(vec![lba as u8; 2048])
        }));
        assert_eq!(asked, [16], "00:02:16 is the sixteenth sector");
        assert_eq!(cd.read(1), MOTOR | READING);
        assert_eq!(cd.pull(4), [16, 16, 16, 16]);
        acknowledge(&mut cd);
        assert!(cd.advance(SECTOR, |lba| Some(vec![lba as u8; 2048])));
        assert_eq!(cd.pull(1), [17]);
    }

    #[test]
    fn a_drive_without_a_disc_answers_with_an_error() {
        let mut cd = drive();
        cd.disc = false;
        cd.write(1, 0x06);
        assert!(cd.advance(FIRST, |_| None));
        cd.write(0, 1);
        assert_eq!(cd.read(3) & 7, ERROR);
        assert_eq!(cd.read(1), OPEN | 1);
    }
}
