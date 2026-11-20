//! The PlayStation's CD-ROM controller: four registers behind an index, a command with its
//! parameters in, and the answers out as numbered interrupts, each with its bytes, one at a time
//! as the program acknowledges them. A read delivers a sector per interrupt, which the program
//! takes from the data port or by DMA: its user data, or the whole sector past its sync when the
//! mode asks. The disc is its sectors as the drive reads them and its tracks; the drive's timing
//! is a fixed delay per answer and the sector rate of its speed.

use std::collections::VecDeque;

use crate::disc::{Track, address};

/// The cycles from a command to its first answer, from that to its second, and from one sector of
/// a read to the next at single speed.
const FIRST: u64 = 20_000;
const SECOND: u64 = 200_000;
const SECTOR: u64 = 33_868_800 / 75;

/// The answers by their interrupt's number.
const DATA_READY: u8 = 1;
const COMPLETE: u8 = 2;
const ACKNOWLEDGE: u8 = 3;
const ERROR: u8 = 5;

/// The drive's status byte: the motor runs, the lid is open (no disc), a read is going, sound
/// plays.
const MOTOR: u8 = 0x02;
const OPEN: u8 = 0x10;
const READING: u8 = 0x20;
const PLAYING: u8 = 0x80;

/// The mode byte's bits: sound tracks play, a play ends at its track's end, a play reports where
/// it is, only the sound sectors of the filter's file and channel play, the whole sector past
/// its sync is the data, the sound sectors of a data track play, the drive turns at double speed.
const MODE_FILTER: u8 = 0x08;
const MODE_WHOLE: u8 = 0x20;
const MODE_XA: u8 = 0x40;
const MODE_DOUBLE: u8 = 0x80;

/// A sector's subheader: its submode says it holds sound, and is one of a stream in real time.
const SUB_AUDIO: u8 = 0x04;

#[derive(Default)]
pub(super) struct Cdrom {
    index: u8,
    parameters: Vec<u8>,
    response: VecDeque<u8>,
    data: VecDeque<u8>,
    /// The sector the drive read last, which the program asks into the data port.
    sector: Vec<u8>,
    /// The address, mode and subheader of the data sector read last.
    header: [u8; 8],
    enable: u8,
    flag: u8,
    /// The answers still to come: the cycles after the one before, the interrupt, the bytes.
    pending: VecDeque<(u64, u8, Vec<u8>)>,
    /// The cycles the head of `pending` still waits.
    wait: u64,
    /// The cycles to the next sector of a read or a play, while one goes.
    next: Option<u64>,
    /// Where the next sector is read, where a `Setloc` said the next read or seek starts, and
    /// whether the drive is reading or playing.
    position: u32,
    target: Option<u32>,
    reading: bool,
    playing: bool,
    mode: u8,
    /// The file and the channel of the sound sectors that play, when the mode filters them.
    filter: (u8, u8),
    /// Whether the host has a disc: decided when the drive is first asked, with its tracks and
    /// the sectors of its program area.
    pub(super) disc: bool,
    pub(super) tracks: Vec<Track>,
    pub(super) sectors: u32,
    /// The sound the drive read and the sound unit has not taken yet, at 44100 samples a
    /// second; the last two samples of each side of a data track's sound, which the next
    /// sector's go on from; and how far the next sample is into the mixer's.
    pub(super) audio: Vec<[i16; 2]>,
    xa_history: [(i32, i32); 2],
    xa_phase: u64,
    muted: bool,
}

/// What starts a data sector: a sound track's sector has samples there.
const SYNC: [u8; 12] = [0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0];

fn bcd(v: u8) -> u32 {
    u32::from(v >> 4) * 10 + u32::from(v & 0xf)
}

fn to_bcd(v: u32) -> u8 {
    ((v / 10) << 4 | v % 10) as u8
}

impl Cdrom {
    fn stat(&self) -> u8 {
        if !self.disc {
            return OPEN;
        }
        MOTOR | if self.reading { READING } else { 0 } | if self.playing { PLAYING } else { 0 }
    }

    /// The cycles from one sector to the next at the mode's speed.
    fn rate(&self) -> u64 {
        if self.mode & MODE_DOUBLE != 0 { SECTOR / 2 } else { SECTOR }
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

    /// Ends a read or a play, with the sectors still to come.
    fn halt(&mut self) {
        (self.reading, self.playing) = (false, false);
        self.next = None;
    }

    /// The head goes where the last `Setloc` said, when one did since the last read or seek.
    fn seek(&mut self) {
        if let Some(target) = self.target.take() {
            self.position = target;
        }
    }

    fn command(&mut self, command: u8) {
        let parameters = std::mem::take(&mut self.parameters);
        let stat = self.stat();
        if crate::tracing() {
            eprintln!("cdrom: command {command:#04x} {parameters:02x?}");
        }
        let needs_disc = matches!(command, 0x02 | 0x03 | 0x06 | 0x09 | 0x13 | 0x14 | 0x15 | 0x16 | 0x1b | 0x1e);
        if needs_disc && !self.disc {
            self.answer(FIRST, ERROR, vec![stat | 1, 0x80]);
            return;
        }
        match command {
            // Getstat, Mute, Demute.
            0x01 | 0x0b | 0x0c => {
                if command != 0x01 {
                    self.muted = command == 0x0b;
                }
                self.answer(FIRST, ACKNOWLEDGE, vec![stat]);
            }
            // Setloc: minutes, seconds and frames in BCD, from the two seconds of the lead-in.
            0x02 => {
                let field = |i: usize| parameters.get(i).copied().map_or(0, bcd);
                self.target = Some(((field(0) * 60 + field(1)) * 75 + field(2)).saturating_sub(150));
                self.answer(FIRST, ACKNOWLEDGE, vec![stat]);
            }
            // Play: from the track named, or from where the head is; a sector's time apart.
            0x03 => {
                self.halt();
                match parameters.first().map(|t| bcd(*t)).filter(|t| *t != 0) {
                    Some(number) => {
                        self.target = None;
                        self.position =
                            self.tracks.iter().find(|t| u32::from(t.number) == number).or(self.tracks.last()).map_or(0, |t| t.start);
                    }
                    None => self.seek(),
                }
                self.playing = true;
                self.answer(FIRST, ACKNOWLEDGE, vec![self.stat()]);
                self.next = Some(FIRST + SECTOR);
            }
            // ReadN, ReadS: an acknowledge, then a sector an interrupt until a pause.
            0x06 | 0x1b => {
                self.halt();
                self.seek();
                self.reading = true;
                self.answer(FIRST, ACKNOWLEDGE, vec![self.stat()]);
                self.next = Some(FIRST + self.rate());
            }
            // MotorOn, Stop, Pause, Init, SeekL, SeekP, ReadTOC: an acknowledge, then the end.
            0x07 | 0x08 | 0x09 | 0x0a | 0x15 | 0x16 | 0x1e => {
                if matches!(command, 0x08 | 0x09 | 0x0a) {
                    self.halt();
                }
                if matches!(command, 0x15 | 0x16) {
                    self.halt();
                    self.seek();
                }
                if command == 0x0a {
                    self.mode = MODE_WHOLE;
                }
                self.answer(FIRST, ACKNOWLEDGE, vec![stat]);
                self.answer(SECOND, COMPLETE, vec![self.stat()]);
            }
            // Setfilter: the file and the channel of the sound sectors that play.
            0x0d => {
                self.filter = (parameters.first().copied().unwrap_or(0), parameters.get(1).copied().unwrap_or(0));
                self.answer(FIRST, ACKNOWLEDGE, vec![stat]);
            }
            0x0e => {
                self.mode = parameters.first().copied().unwrap_or(0);
                self.answer(FIRST, ACKNOWLEDGE, vec![stat]);
            }
            // Getparam: the mode and the filter.
            0x0f => self.answer(FIRST, ACKNOWLEDGE, vec![stat, self.mode, 0, self.filter.0, self.filter.1]),
            // GetlocL: the header of the data sector read last.
            0x10 => self.answer(FIRST, ACKNOWLEDGE, self.header.to_vec()),
            // GetlocP: the track, its index, and the head's place in the track and on the disc.
            0x11 => {
                let track = self.tracks.iter().rev().find(|t| t.start <= self.position).or(self.tracks.first()).copied();
                let (number, start) = track.map_or((1, 0), |t| (u32::from(t.number), t.start));
                let within = self.position.saturating_sub(start);
                let relative = [to_bcd(within / 4500), to_bcd(within / 75 % 60), to_bcd(within % 75)];
                let absolute = address(self.position);
                self.answer(FIRST, ACKNOWLEDGE, [&[to_bcd(number), 1][..], &relative, &absolute].concat());
            }
            // GetTN: the first track and the last.
            0x13 => {
                let (first, last) = (self.tracks.first().map_or(1, |t| t.number), self.tracks.last().map_or(1, |t| t.number));
                self.answer(FIRST, ACKNOWLEDGE, vec![stat, to_bcd(first.into()), to_bcd(last.into())]);
            }
            // GetTD: where a track starts, or the lead-out for track zero.
            0x14 => {
                let number = parameters.first().map_or(0, |t| bcd(*t));
                let start = match number {
                    0 => Some(self.sectors),
                    _ => self.tracks.iter().find(|t| u32::from(t.number) == number).map(|t| t.start),
                };
                match start {
                    Some(start) => {
                        let [minutes, seconds, _] = address(start);
                        self.answer(FIRST, ACKNOWLEDGE, vec![stat, minutes, seconds]);
                    }
                    None => self.answer(FIRST, ERROR, vec![stat | 1, 0x10]),
                }
            }
            // Test, by its first parameter. 20: the controller's version. 04: count the licence
            // strings the drive reads from here on; 05: how many it tried and how many it read,
            // none where the head is in the program area, which has no such string (a program
            // that finds one there has found a chip that feeds it).
            0x19 => match parameters.first() {
                Some(0x20) => self.answer(FIRST, ACKNOWLEDGE, vec![0x94, 0x09, 0x19, 0xc0]),
                Some(0x04) => self.answer(FIRST, ACKNOWLEDGE, vec![stat]),
                Some(0x05) => self.answer(FIRST, ACKNOWLEDGE, vec![stat, 0, 0]),
                _ => self.answer(FIRST, ERROR, vec![stat | 1, 0x10]),
            },
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
    /// before, it is raised, a sector's with what `sector` reads off the disc for its number,
    /// whole; the result says whether the controller's interrupt line rose.
    pub(super) fn advance(&mut self, cycles: u64, mut sector: impl FnMut(u32) -> Option<Vec<u8>>) -> bool {
        self.wait = self.wait.saturating_sub(cycles);
        if let Some(due) = &mut self.next {
            *due = due.saturating_sub(cycles);
        }
        if self.flag != 0 {
            return false;
        }
        // A command's answer comes ahead of the next sector.
        if self.wait == 0
            && let Some((_, interrupt, bytes)) = self.pending.pop_front()
        {
            return self.raise(interrupt, bytes);
        }
        if self.next != Some(0) {
            return false;
        }
        let Some(raw) = sector(self.position).filter(|raw| raw.len() >= 2352) else {
            self.halt();
            return self.raise(ERROR, vec![self.stat() | 1, 0x04]);
        };
        self.position += 1;
        self.next = Some(self.rate());
        if self.playing {
            // A sound track's sector is its samples, and nothing for the program to take; a
            // data track played as sound is noise the drive keeps to itself.
            if raw[..12] != SYNC && !self.muted {
                self.audio.extend(raw.chunks_exact(4).map(|s| [i16::from_le_bytes([s[0], s[1]]), i16::from_le_bytes([s[2], s[3]])]));
            }
            return false;
        }
        let whole = raw[15] != 1;
        if whole {
            self.header.copy_from_slice(&raw[12..20]);
        } else {
            self.header = [raw[12], raw[13], raw[14], raw[15], 0, 0, 0, 0];
        }
        // A sound sector of a data track plays when the mode says so, and is none of the
        // program's data: the one of the filter's file and channel, or any.
        if whole && self.mode & MODE_XA != 0 && raw[18] & SUB_AUDIO != 0 {
            if (self.mode & MODE_FILTER == 0 || (raw[16], raw[17]) == self.filter) && !self.muted {
                self.xa(&raw);
            }
            return false;
        }
        let user = if whole { 24 } else { 16 };
        self.sector = if self.mode & MODE_WHOLE != 0 { raw[12..].to_vec() } else { raw[user..user + 2048].to_vec() };
        // What the program left of the sector before is gone with it.
        self.data.clear();
        self.raise(DATA_READY, vec![self.stat()])
    }

    /// The sound of a data track's sound sector: eighteen groups of eight units of twenty-eight
    /// samples, four bits each, by the coding its subheader says (two sides or one, 37800 or
    /// 18900 samples a second), brought to the 44100 a second the sound unit mixes at.
    fn xa(&mut self, raw: &[u8]) {
        let (stereo, half_rate, wide) = (raw[19] & 1 != 0, raw[19] & 4 != 0, raw[19] & 0x10 != 0);
        if wide {
            // Eight bits a sample: no program of the console's is known to use it.
            return;
        }
        let mut sides: [Vec<i16>; 2] = [Vec::new(), Vec::new()];
        for group in raw[24..24 + 18 * 128].chunks_exact(128) {
            for unit in 0..8 {
                let side = if stereo { unit & 1 } else { 0 };
                let parameter = group[4 + unit];
                let shift = match parameter & 0xf {
                    s @ 0..=12 => u32::from(s),
                    _ => 9,
                };
                let (k0, k1) = [(0, 0), (60, 0), (115, -52), (98, -55)][usize::from((parameter >> 4) & 3)];
                let history = &mut self.xa_history[side];
                for sample in 0..28 {
                    let nibble = (group[16 + sample * 4 + unit / 2] >> (4 * (unit & 1))) & 0xf;
                    let correction = (i32::from(nibble) << 28 >> 28 << 12) >> shift;
                    let value = (correction + (history.0 * k0 + history.1 * k1 + 32) / 64).clamp(-0x8000, 0x7fff);
                    *history = (value, history.0);
                    sides[side].push(value as i16);
                }
            }
        }
        let rate: u64 = if half_rate { 18_900 } else { 37_800 };
        let count = sides[0].len();
        for n in 0..count {
            let pair = if stereo { [sides[0][n], sides[1].get(n).copied().unwrap_or(0)] } else { [sides[0][n]; 2] };
            // Each sample is held for as many of the mixer's as its rate leaves room for.
            self.xa_phase += 44_100;
            while self.xa_phase >= rate {
                self.xa_phase -= rate;
                self.audio.push(pair);
            }
        }
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
        let mut cd = Cdrom { disc: true, sectors: 1000, tracks: vec![Track { number: 1, audio: false, start: 0 }], ..Cdrom::default() };
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

    /// A whole mode 2 sector: its address, a subheader of `submode`, and `byte` as its data.
    fn raw(lba: u32, submode: u8, byte: u8) -> Option<Vec<u8>> {
        let mut out = vec![0u8; 2352];
        out[12..15].copy_from_slice(&address(lba));
        out[15] = 2;
        out[16..20].copy_from_slice(&[1, 2, submode, 0]);
        out[24..24 + 2048].fill(byte);
        Some(out)
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
            raw(lba, 8, lba as u8)
        }));
        assert_eq!(asked, [16], "00:02:16 is the sixteenth sector");
        assert_eq!(cd.read(1), MOTOR | READING);
        assert_eq!(cd.pull(4), [16, 16, 16, 16]);
        acknowledge(&mut cd);
        assert!(cd.advance(SECTOR, |lba| raw(lba, 8, lba as u8)));
        assert_eq!(cd.pull(1), [17]);
        // The header of the sector read last is what GetlocL answers.
        acknowledge(&mut cd);
        cd.write(1, 0x10);
        assert!(cd.advance(FIRST, |_| None));
        let header: Vec<u8> = (0..8).map(|_| cd.read(1)).collect();
        assert_eq!(header, [0x00, 0x02, 0x17, 2, 1, 2, 8, 0]);
    }

    #[test]
    fn the_mode_gives_the_whole_sector_and_keeps_a_sound_sector_from_the_data() {
        let mut cd = drive();
        // The whole sector, the sound sectors played, at double speed.
        cd.write(2, MODE_WHOLE | MODE_XA | MODE_DOUBLE);
        cd.write(1, 0x0e);
        assert!(cd.advance(FIRST, |_| None));
        acknowledge(&mut cd);
        cd.write(1, 0x06);
        assert!(cd.advance(FIRST, |_| None));
        acknowledge(&mut cd);
        assert!(!cd.advance(SECTOR / 2, |lba| raw(lba, 0x64, 9)), "a sound sector is none of the program's");
        assert!(cd.advance(SECTOR / 2, |lba| raw(lba, 8, 9)));
        let sector = cd.pull(2340);
        assert_eq!((&sector[..8], sector[12]), (&[0x00, 0x02, 0x01, 2, 1, 2, 8, 0][..], 9), "from the address on");
    }

    #[test]
    fn the_tracks_answer_the_contents_commands() {
        let mut cd = drive();
        cd.tracks.push(Track { number: 2, audio: true, start: 4500 - 150 });
        cd.write(1, 0x13);
        assert!(cd.advance(FIRST, |_| None));
        assert_eq!((cd.read(1), cd.read(1), cd.read(1)), (MOTOR, 1, 2));
        acknowledge(&mut cd);
        cd.write(2, 0x02);
        cd.write(1, 0x14);
        assert!(cd.advance(FIRST, |_| None));
        assert_eq!((cd.read(1), cd.read(1), cd.read(1)), (MOTOR, 0x01, 0x00), "track 2 starts at 01:00");
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
