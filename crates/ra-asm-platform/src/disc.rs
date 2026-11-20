//! A compact disc as the files a dump of one is kept in: its tracks, and each sector as the 2352
//! bytes the drive reads, whatever the dump stores of them.
//!
//! - A cue sheet (`.cue`) names the files and the tracks in them: one file for the disc, or one
//!   a track; data tracks of whole sectors (`MODE1/2352`, `MODE2/2352`) or of their user data
//!   alone (`MODE1/2048`), and audio tracks.
//! - A bare image (`.bin`, `.img`, `.iso`) is one data track: whole sectors when it starts with
//!   a sector's sync pattern, the user data of each otherwise.
//!
//! A sector kept as its user data is given back whole, as a mode 2 form 1 sector with its
//! address: what a program that asks the drive for whole sectors reads.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The bytes of a sector as the drive reads it, and of the user data in a data sector.
pub const RAW: usize = 2352;
pub const DATA: usize = 2048;
/// What starts a data sector.
const SYNC: [u8; 12] = [0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0];

/// A track of the disc: its number, whether it is sound, and where it starts, in sectors from
/// the start of the program area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Track {
    pub number: u8,
    pub audio: bool,
    pub start: u32,
}

/// A run of sectors a file holds.
#[derive(Debug, Clone, Copy)]
struct Span {
    /// The first sector on the disc and how many follow.
    start: u32,
    sectors: u32,
    file: usize,
    /// Where the run starts in the file, and the bytes the file keeps of each sector.
    offset: u64,
    stride: u64,
}

pub struct Disc {
    files: Vec<File>,
    spans: Vec<Span>,
    tracks: Vec<Track>,
    /// The sectors of the program area: where the lead-out starts.
    sectors: u32,
}

/// `mm:ss:ff` as sectors.
fn frames(text: &str) -> Option<u32> {
    let mut fields = text.split(':').map(|f| f.trim().parse::<u32>().ok());
    let (m, s, f) = (fields.next()??, fields.next()??, fields.next()??);
    Some((m * 60 + s) * 75 + f)
}

/// A sector's number as the minutes, seconds and frames of its address, in BCD.
pub fn address(lba: u32) -> [u8; 3] {
    let at = lba + 150;
    let bcd = |v: u32| ((v / 10) << 4 | v % 10) as u8;
    [bcd(at / 4500), bcd(at / 75 % 60), bcd(at % 75)]
}

impl Disc {
    /// The disc the file at `path` is, or is the cue sheet of.
    pub fn open(path: &Path) -> Result<Self, String> {
        let is_cue = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("cue"));
        if is_cue { Self::cue(path) } else { Self::image(path) }
    }

    /// A bare image: one data track.
    fn image(path: &Path) -> Result<Self, String> {
        let mut file = File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
        let len = file.metadata().map_err(|e| e.to_string())?.len();
        let mut head = [0u8; 12];
        let whole = file.read_exact(&mut head).is_ok() && head == SYNC && len % RAW as u64 == 0;
        let stride = if whole { RAW } else { DATA } as u64;
        let sectors = u32::try_from(len / stride).map_err(|_| format!("{} is too large for a disc", path.display()))?;
        if sectors == 0 {
            return Err(format!("{} holds no sector", path.display()));
        }
        let span = Span { start: 0, sectors, file: 0, offset: 0, stride };
        Ok(Self { files: vec![file], spans: vec![span], tracks: vec![Track { number: 1, audio: false, start: 0 }], sectors })
    }

    /// A cue sheet: the files it names, beside it, and the tracks in them.
    fn cue(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let folder = path.parent().unwrap_or(Path::new("."));
        let mut disc = Self { files: Vec::new(), spans: Vec::new(), tracks: Vec::new(), sectors: 0 };
        // The file being read: its length in bytes, and where its first sector is on the disc.
        let mut current: Option<(usize, u64, u32)> = None;
        // The track being read: its number, the bytes a sector takes in the file, whether it is
        // sound, and where its sectors start in the file (its first index).
        let mut track: Option<(u8, u64, bool, Option<u32>)> = None;
        // The track before it in the same file, until this one's first index says where it ends.
        let mut before: Option<(u8, u64, bool, Option<u32>)> = None;
        // The sectors a `PREGAP` says precede a track without being in any file.
        let mut gaps = 0u32;
        let close =
            |disc: &mut Self, track: &mut Option<(u8, u64, bool, Option<u32>)>, current: Option<(usize, u64, u32)>, until: Option<u32>| {
                let (Some((_, stride, audio, Some(first))), Some((file, len, base))) = (track.take(), current) else { return };
                let end = until.unwrap_or((len / stride) as u32);
                if end > first {
                    let _ = audio;
                    disc.spans.push(Span { start: base + first, sectors: end - first, file, offset: u64::from(first) * stride, stride });
                }
            };
        for line in text.lines() {
            let line = line.trim();
            let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            match word.to_ascii_uppercase().as_str() {
                "FILE" => {
                    close(&mut disc, &mut before, current, None);
                    close(&mut disc, &mut track, current, None);
                    // The file's sectors are the disc's after those of the files before it.
                    let base = disc.spans.iter().map(|s| s.start + s.sectors).max().unwrap_or(0) + std::mem::take(&mut gaps);
                    let name = rest
                        .trim()
                        .strip_prefix('"')
                        .and_then(|r| r.split_once('"'))
                        .map_or_else(|| rest.split_whitespace().next().unwrap_or(""), |(name, _)| name);
                    let at = folder.join(name);
                    let file = File::open(&at).map_err(|e| format!("cannot open {}, which {} names: {e}", at.display(), path.display()))?;
                    let len = file.metadata().map_err(|e| e.to_string())?.len();
                    disc.files.push(file);
                    current = Some((disc.files.len() - 1, len, base));
                }
                "TRACK" => {
                    let mut fields = rest.split_whitespace();
                    let number: u8 = fields
                        .next()
                        .and_then(|n| n.parse().ok())
                        .ok_or_else(|| format!("{}: a track without a number", path.display()))?;
                    let kind = fields.next().unwrap_or("").to_ascii_uppercase();
                    let stride = match kind.rsplit('/').next().and_then(|s| s.parse::<u64>().ok()) {
                        Some(stride @ (2048 | 2352)) => stride,
                        None if kind == "AUDIO" => RAW as u64,
                        _ => return Err(format!("{}: track {number} is `{kind}`, a form this reads no sectors of", path.display())),
                    };
                    // The track before ends where this one's first index is, once it is read.
                    before = track.take();
                    track = Some((number, stride, kind == "AUDIO", None));
                }
                "PREGAP" => gaps += frames(rest).unwrap_or(0),
                "INDEX" => {
                    let mut fields = rest.split_whitespace();
                    let (Some(index), Some(at)) = (fields.next().and_then(|n| n.parse::<u32>().ok()), fields.next().and_then(frames))
                    else {
                        continue;
                    };
                    let Some((number, _, audio, first)) = &mut track else { continue };
                    if first.is_none() {
                        // The track's sectors start here: the one before it in the file ends.
                        *first = Some(at);
                        close(&mut disc, &mut before, current, Some(at));
                    }
                    if index == 1 {
                        let base = current.map_or(0, |(_, _, base)| base);
                        disc.tracks.push(Track { number: *number, audio: *audio, start: base + at });
                    }
                }
                _ => {}
            }
        }
        close(&mut disc, &mut track, current, None);
        disc.sectors = disc.spans.iter().map(|s| s.start + s.sectors).max().unwrap_or(0);
        if disc.tracks.is_empty() || disc.sectors == 0 {
            return Err(format!("{} names no track", path.display()));
        }
        Ok(disc)
    }

    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    /// The sectors of the program area: where the lead-out starts.
    pub fn sectors(&self) -> u32 {
        self.sectors
    }

    /// The track the sector `lba` is in.
    pub fn track_at(&self, lba: u32) -> Option<Track> {
        self.tracks.iter().rev().find(|t| t.start <= lba).or(self.tracks.first()).copied()
    }

    /// The sector `lba` as the drive reads it; `None` past the disc's end. A sector no file
    /// holds (a gap the cue sheet only declares) is silence.
    pub fn raw(&mut self, lba: u32) -> Option<Vec<u8>> {
        if lba >= self.sectors {
            return None;
        }
        let mut out = vec![0u8; RAW];
        let Some(span) = self.spans.iter().find(|s| lba.wrapping_sub(s.start) < s.sectors).copied() else { return Some(out) };
        let file = &mut self.files[span.file];
        file.seek(SeekFrom::Start(span.offset + u64::from(lba - span.start) * span.stride)).ok()?;
        if span.stride == RAW as u64 {
            file.read_exact(&mut out).ok()?;
        } else {
            // The user data alone: the sector around it, as mode 2 form 1 has it.
            out[..12].copy_from_slice(&SYNC);
            out[12..15].copy_from_slice(&address(lba));
            out[15] = 2;
            out[16..24].copy_from_slice(&[0, 0, 8, 0, 0, 0, 8, 0]);
            file.read_exact(&mut out[24..24 + DATA]).ok()?;
        }
        Some(out)
    }

    /// The user data of the data sector `lba`.
    pub fn data(&mut self, lba: u32) -> Option<Vec<u8>> {
        let raw = self.raw(lba)?;
        let from = if raw[15] == 1 { 16 } else { 24 };
        Some(raw[from..from + DATA].to_vec())
    }
}

/// The disc's user data as one run of bytes, a sector after a sector: what a file system on
/// it is read from.
pub struct Cooked {
    pub disc: Disc,
    at: u64,
}

impl Cooked {
    pub fn new(disc: Disc) -> Self {
        Self { disc, at: 0 }
    }
}

impl Read for Cooked {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let (lba, within) = ((self.at / DATA as u64) as u32, (self.at % DATA as u64) as usize);
        let Some(sector) = self.disc.data(lba) else { return Ok(0) };
        let n = buf.len().min(DATA - within);
        buf[..n].copy_from_slice(&sector[within..within + n]);
        self.at += n as u64;
        Ok(n)
    }
}

impl Seek for Cooked {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        let len = u64::from(self.disc.sectors) * DATA as u64;
        let at = match to {
            SeekFrom::Start(at) => Some(at),
            SeekFrom::End(by) => len.checked_add_signed(by),
            SeekFrom::Current(by) => self.at.checked_add_signed(by),
        };
        self.at = at.ok_or_else(|| std::io::Error::other("a seek before the disc's first byte"))?;
        Ok(self.at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch folder of this test's own.
    fn folder(name: &str) -> std::path::PathBuf {
        let at = std::env::temp_dir().join(format!("ra-disc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&at);
        std::fs::create_dir_all(&at).unwrap();
        at
    }

    /// A whole mode 2 sector whose user data is all `byte`.
    fn sector(lba: u32, byte: u8) -> Vec<u8> {
        let mut out = vec![0u8; RAW];
        out[..12].copy_from_slice(&SYNC);
        out[12..15].copy_from_slice(&address(lba));
        out[15] = 2;
        out[24..24 + DATA].fill(byte);
        out
    }

    #[test]
    fn a_bare_image_is_whole_sectors_or_their_user_data() {
        let dir = folder("bare");
        std::fs::write(dir.join("whole.bin"), [sector(0, 1), sector(1, 2)].concat()).unwrap();
        std::fs::write(dir.join("data.iso"), [vec![3u8; DATA], vec![4u8; DATA]].concat()).unwrap();
        let mut whole = Disc::open(&dir.join("whole.bin")).unwrap();
        assert_eq!((whole.sectors(), whole.data(1).unwrap()[0], whole.raw(1).unwrap()[15]), (2, 2, 2));
        assert!(whole.raw(2).is_none(), "past the end");
        let mut data = Disc::open(&dir.join("data.iso")).unwrap();
        let raw = data.raw(1).unwrap();
        assert_eq!((&raw[..12], &raw[12..16], raw[24]), (&SYNC[..], &[0, 2, 1, 2][..], 4), "the sector around the data, with its address");
        assert_eq!(data.data(0).unwrap(), vec![3u8; DATA]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_cue_sheet_names_the_tracks_of_one_file_or_of_several() {
        let dir = folder("cue");
        // One file: a data track of three sectors, then sound whose first index is two in.
        std::fs::write(dir.join("one.bin"), [sector(0, 1), sector(1, 1), sector(2, 1), vec![7u8; RAW * 4]].concat()).unwrap();
        std::fs::write(
            dir.join("one.cue"),
            "FILE \"one.bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n  TRACK 02 AUDIO\n    INDEX 00 00:00:03\n    INDEX 01 00:00:05\n",
        )
        .unwrap();
        let mut one = Disc::open(&dir.join("one.cue")).unwrap();
        assert_eq!(one.tracks(), [Track { number: 1, audio: false, start: 0 }, Track { number: 2, audio: true, start: 5 }]);
        assert_eq!((one.sectors(), one.data(2).unwrap()[0], one.raw(4).unwrap()[100]), (7, 1, 7));
        assert_eq!(one.track_at(6).map(|t| t.number), Some(2));
        // A file a track, the second with its gap in it; the first of user data alone.
        std::fs::write(dir.join("a.bin"), vec![9u8; DATA * 2]).unwrap();
        std::fs::write(dir.join("b.bin"), vec![5u8; RAW * 3]).unwrap();
        std::fs::write(
            dir.join("two.cue"),
            "FILE \"a.bin\" BINARY\n TRACK 01 MODE1/2048\n  INDEX 01 00:00:00\nFILE \"b.bin\" BINARY\n TRACK 02 AUDIO\n  INDEX 00 00:00:00\n  INDEX 01 00:00:01\n",
        )
        .unwrap();
        let mut two = Disc::open(&dir.join("two.cue")).unwrap();
        assert_eq!(two.tracks()[1], Track { number: 2, audio: true, start: 3 });
        assert_eq!((two.sectors(), two.data(1).unwrap()[5], two.raw(4).unwrap()[0]), (5, 9, 5));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_user_data_reads_as_one_run_of_bytes() {
        let dir = folder("cooked");
        std::fs::write(dir.join("d.bin"), [sector(0, 1), sector(1, 2)].concat()).unwrap();
        let mut cooked = Cooked::new(Disc::open(&dir.join("d.bin")).unwrap());
        cooked.seek(SeekFrom::Start(DATA as u64 - 1)).unwrap();
        let mut two = [0u8; 2];
        cooked.read_exact(&mut two).unwrap();
        assert_eq!(two, [1, 2]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
