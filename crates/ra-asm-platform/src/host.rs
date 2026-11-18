//! What a platform runtime asks of this machine: a picture, sound, the controllers, files and a
//! console. Each is a trait a window, an audio device or a directory implements, and each has a
//! headless form that keeps what it was given, which is what a test and the oracle run against.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use crate::pad;

/// A frame of the console's picture: `width * height` pixels, a row after a row, each `0x00RRGGBB`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

impl Frame {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, pixels: vec![0; width as usize * height as usize] }
    }

    /// A checksum of the picture and its size (FNV-1a over the pixels), which names a frame in a
    /// test's expectation.
    pub fn checksum(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        let words = [self.width, self.height].into_iter().chain(self.pixels.iter().copied());
        for byte in words.flat_map(u32::to_le_bytes) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    /// The pixels that differ from `other`'s by more than `tolerance` in a channel; every pixel
    /// when the sizes differ.
    pub fn differing(&self, other: &Self, tolerance: u8) -> usize {
        if (self.width, self.height) != (other.width, other.height) {
            return self.pixels.len().max(other.pixels.len());
        }
        let far = |a: u32, b: u32| (0..3).any(|c| ((a >> (8 * c)) as u8).abs_diff((b >> (8 * c)) as u8) > tolerance);
        self.pixels.iter().zip(&other.pixels).filter(|(a, b)| far(**a, **b)).count()
    }
}

/// Where the console's picture goes at each vertical blank.
pub trait Video {
    /// Shows `frame`; `false` once the person closed the picture, which ends the run.
    fn present(&mut self, frame: &Frame) -> bool;

    /// Called often between two frames, for a picture that has to answer the system meanwhile (a
    /// window's messages); `false` once the person closed it.
    fn idle(&mut self) -> bool {
        true
    }
}

/// Where the console's sound goes: stereo samples at `rate`, as the sound processor mixes them.
pub trait Audio {
    fn play(&mut self, samples: &[[i16; 2]], rate: u32);
}

/// The controllers, read when the console polls them.
pub trait Input {
    /// The controller in `port`, counted from zero; `None` for an empty port.
    fn pad(&mut self, port: usize) -> Option<pad::State>;
}

/// The program's console: the BIOS's TTY, the kernel's serial port.
pub trait Console {
    fn write(&mut self, bytes: &[u8]);
}

/// An open file of the [`Storage`].
pub trait File {
    fn read(&mut self, buf: &mut [u8]) -> usize;
    fn write(&mut self, bytes: &[u8]) -> usize;
    /// Moves to `offset` from the start and gives the position; `None` past what the file takes.
    fn seek(&mut self, offset: u64) -> Option<u64>;
    fn len(&mut self) -> u64;
}

/// The console's files, by the names the program opens them under (`cdrom:\\FILE.DAT;1`,
/// `host:file`, `mc0:save`): the device before the colon picks the place.
pub trait Storage {
    fn open(&mut self, name: &str, write: bool) -> Option<Box<dyn File>>;
    /// The sector `lba` of the disc, 2048 bytes of user data; `None` without a disc or past its end.
    fn sector(&mut self, lba: u32) -> Option<Vec<u8>>;
}

/// The host's side of a runtime.
pub struct Host {
    pub video: Box<dyn Video>,
    pub audio: Box<dyn Audio>,
    pub input: Box<dyn Input>,
    pub storage: Box<dyn Storage>,
    pub console: Box<dyn Console>,
}

impl Host {
    /// The headless host over `capture`: nothing is shown or played, everything is kept.
    pub fn headless(capture: &Capture) -> Self {
        Self {
            video: Box::new(capture.clone()),
            audio: Box::new(capture.clone()),
            input: Box::new(capture.clone()),
            storage: Box::new(Directory::default()),
            console: Box::new(capture.clone()),
        }
    }
}

/// What a headless run kept, and the input it plays: shared, since the run that fills it and the
/// test that reads it are on two threads.
#[derive(Clone, Default)]
pub struct Capture(Arc<Mutex<Captured>>);

#[derive(Default)]
pub struct Captured {
    /// The frames presented so far.
    pub frames: u64,
    /// The last frame presented.
    pub last: Frame,
    /// The frames kept by their number, counted from one: those [`Capture::keep`] named.
    pub kept: BTreeMap<u64, Frame>,
    keep: Vec<u64>,
    /// The run ends when this many frames were presented; zero for no end.
    pub limit: u64,
    /// The directory each presented frame is written to as `frame-<n>.png`, when set.
    pub dump: Option<PathBuf>,
    /// Every sample played, and the rate of the last ones.
    pub samples: Vec<[i16; 2]>,
    pub rate: u32,
    /// What the program wrote on its console.
    pub console: Vec<u8>,
    /// Write the console through to stdout as it comes.
    pub live: bool,
    /// The input script: from the frame of each entry on, the pad of its port holds its state.
    pub script: Vec<(u64, usize, pad::State)>,
}

impl Capture {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, Captured> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Ends the run after `frames` frames.
    #[must_use]
    pub fn limit(self, frames: u64) -> Self {
        self.lock().limit = frames;
        self
    }

    /// Keeps the frames of these numbers, counted from one.
    #[must_use]
    pub fn keep(self, frames: &[u64]) -> Self {
        self.lock().keep.extend_from_slice(frames);
        self
    }

    /// From `frame` on, the pad of `port` holds `state`.
    #[must_use]
    pub fn press(self, frame: u64, port: usize, state: pad::State) -> Self {
        self.lock().script.push((frame, port, state));
        self
    }
}

impl Video for Capture {
    fn present(&mut self, frame: &Frame) -> bool {
        let mut kept = self.lock();
        kept.frames += 1;
        let number = kept.frames;
        if kept.keep.contains(&number) {
            kept.kept.insert(number, frame.clone());
        }
        if let Some(dir) = &kept.dump {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(dir.join(format!("frame-{number:05}.png")), crate::png::encode(frame));
        }
        kept.last.clone_from(frame);
        kept.limit == 0 || number < kept.limit
    }
}

impl Audio for Capture {
    fn play(&mut self, samples: &[[i16; 2]], rate: u32) {
        let mut kept = self.lock();
        kept.samples.extend_from_slice(samples);
        kept.rate = rate;
    }
}

impl Input for Capture {
    fn pad(&mut self, port: usize) -> Option<pad::State> {
        let kept = self.lock();
        let now = kept.frames;
        // The first port holds a pad from the start; the second, once the script names it.
        let scripted = kept.script.iter().filter(|(from, p, _)| *p == port && *from <= now).max_by_key(|(from, ..)| *from);
        match scripted {
            Some((.., state)) => Some(*state),
            None => (port == 0).then(pad::State::default),
        }
    }
}

impl Console for Capture {
    fn write(&mut self, bytes: &[u8]) {
        use std::io::Write;
        let mut kept = self.lock();
        kept.console.extend_from_slice(bytes);
        if kept.live {
            let mut out = std::io::stdout().lock();
            let _ = out.write_all(bytes);
            let _ = out.flush();
        }
    }
}

/// The console on this process's standard output.
pub struct Stdout;

impl Console for Stdout {
    fn write(&mut self, bytes: &[u8]) {
        use std::io::Write;
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(bytes);
        let _ = out.flush();
    }
}

/// Sound nobody hears.
pub struct Silence;

impl Audio for Silence {
    fn play(&mut self, _samples: &[[i16; 2]], _rate: u32) {}
}

/// The console's files as files of this machine: a disc image or the directory of its files for
/// the disc's names (`cdrom:`, `cdrom0:`), and a directory for the others (`host:`, `mc0:`),
/// each device a folder of its name. Without either, nothing opens.
#[derive(Default)]
pub struct Directory {
    /// The disc: an ISO 9660 image, or the directory its files were copied to.
    pub disc: Option<PathBuf>,
    /// Where the files of the other devices are.
    pub root: Option<PathBuf>,
    image: Option<crate::iso::Iso<std::fs::File>>,
}

impl Directory {
    pub fn new(disc: Option<PathBuf>, root: Option<PathBuf>) -> Self {
        Self { disc, root, image: None }
    }

    /// The device and the path of a console file name: `cdrom:\\DIR\\FILE.DAT;1` gives `cdrom`
    /// and `DIR/FILE.DAT`, its version and its leading separators dropped.
    pub fn split(name: &str) -> (&str, String) {
        let (device, path) = name.split_once(':').unwrap_or(("", name));
        let path = path.split(';').next().unwrap_or(path).replace('\\', "/");
        (device, path.trim_start_matches('/').to_owned())
    }

    fn image(&mut self) -> Option<&mut crate::iso::Iso<std::fs::File>> {
        if self.image.is_none() {
            let path = self.disc.as_ref().filter(|p| p.is_file())?;
            self.image = crate::iso::Iso::open(std::fs::File::open(path).ok()?);
        }
        self.image.as_mut()
    }
}

/// A file of this machine as a console's file.
struct HostFile(std::fs::File);

impl File for HostFile {
    fn read(&mut self, buf: &mut [u8]) -> usize {
        use std::io::Read;
        let mut done = 0;
        while done < buf.len() {
            match self.0.read(&mut buf[done..]) {
                Ok(0) | Err(_) => break,
                Ok(n) => done += n,
            }
        }
        done
    }

    fn write(&mut self, bytes: &[u8]) -> usize {
        use std::io::Write;
        if self.0.write_all(bytes).is_ok() { bytes.len() } else { 0 }
    }

    fn seek(&mut self, offset: u64) -> Option<u64> {
        use std::io::Seek;
        self.0.seek(std::io::SeekFrom::Start(offset)).ok()
    }

    fn len(&mut self) -> u64 {
        self.0.metadata().map_or(0, |m| m.len())
    }
}

/// A file's bytes in memory: one read off a disc image.
pub struct Bytes {
    pub bytes: Vec<u8>,
    pub at: usize,
}

impl File for Bytes {
    fn read(&mut self, buf: &mut [u8]) -> usize {
        let n = buf.len().min(self.bytes.len().saturating_sub(self.at));
        buf[..n].copy_from_slice(&self.bytes[self.at..self.at + n]);
        self.at += n;
        n
    }

    fn write(&mut self, _bytes: &[u8]) -> usize {
        0
    }

    fn seek(&mut self, offset: u64) -> Option<u64> {
        self.at = usize::try_from(offset).ok().filter(|at| *at <= self.bytes.len())?;
        Some(offset)
    }

    fn len(&mut self) -> u64 {
        self.bytes.len() as u64
    }
}

/// The file `path` under `root`, its name matched without regard to case, as a disc's are.
fn find(root: &Path, path: &str) -> Option<PathBuf> {
    let mut at = root.to_path_buf();
    for part in path.split('/').filter(|p| !p.is_empty() && *p != ".") {
        if part == ".." {
            return None;
        }
        let exact = at.join(part);
        at = if exact.exists() {
            exact
        } else {
            std::fs::read_dir(&at).ok()?.filter_map(Result::ok).find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?.path()
        };
    }
    Some(at)
}

impl Storage for Directory {
    fn open(&mut self, name: &str, write: bool) -> Option<Box<dyn File>> {
        let (device, path) = Self::split(name);
        if device.starts_with("cdrom") || device.starts_with("cdfs") {
            if write {
                return None;
            }
            if let Some(image) = self.image() {
                return image.file(&path).map(|bytes| Box::new(Bytes { bytes, at: 0 }) as Box<dyn File>);
            }
            let found = find(self.disc.as_ref().filter(|p| p.is_dir())?, &path)?;
            return std::fs::File::open(found).ok().map(|f| Box::new(HostFile(f)) as Box<dyn File>);
        }
        let folder = self.root.as_ref()?.join(device.trim_end_matches(char::is_numeric));
        if write {
            let _ = std::fs::create_dir_all(&folder);
            let target = folder.join(path.replace("..", "_"));
            let file = std::fs::OpenOptions::new().read(true).write(true).create(true).truncate(false).open(target).ok()?;
            return Some(Box::new(HostFile(file)));
        }
        std::fs::File::open(find(&folder, &path)?).ok().map(|f| Box::new(HostFile(f)) as Box<dyn File>)
    }

    fn sector(&mut self, lba: u32) -> Option<Vec<u8>> {
        self.image()?.sector(lba)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_console_file_name_splits_into_its_device_and_its_path() {
        assert_eq!(Directory::split("cdrom:\\DATA\\FILE.DAT;1"), ("cdrom", "DATA/FILE.DAT".to_owned()));
        assert_eq!(Directory::split("host:file.txt"), ("host", "file.txt".to_owned()));
        assert_eq!(Directory::split("plain"), ("", "plain".to_owned()));
    }

    #[test]
    fn the_headless_host_keeps_the_frames_named_and_ends_at_its_limit() {
        let capture = Capture::new().limit(3).keep(&[2]);
        let mut video: Box<dyn Video> = Box::new(capture.clone());
        let mut frame = Frame::new(2, 1);
        assert!(video.present(&frame));
        frame.pixels[0] = 0x00ff_0000;
        assert!(video.present(&frame));
        frame.pixels[0] = 0;
        assert!(!video.present(&frame), "the third frame is the last");
        let kept = capture.lock();
        assert_eq!(kept.frames, 3);
        assert_eq!(kept.kept[&2].pixels, [0x00ff_0000, 0]);
    }

    #[test]
    fn the_script_holds_a_pad_from_its_frame_on() {
        let pressed = pad::State { buttons: pad::CROSS, ..pad::State::default() };
        let capture = Capture::new().press(2, 0, pressed);
        let mut input: Box<dyn Input> = Box::new(capture.clone());
        assert_eq!(input.pad(0), Some(pad::State::default()));
        assert_eq!(input.pad(1), None, "the second port is empty");
        capture.lock().frames = 2;
        assert_eq!(input.pad(0), Some(pressed));
    }

    #[test]
    fn frames_compare_within_a_tolerance() {
        let a = Frame { width: 2, height: 1, pixels: vec![0x0010_2030, 0] };
        let b = Frame { width: 2, height: 1, pixels: vec![0x0012_2030, 0x0000_0009] };
        assert_eq!(a.differing(&b, 1), 2);
        assert_eq!(a.differing(&b, 8), 1);
        assert_eq!(a.differing(&b, 9), 0);
        assert_ne!(a.checksum(), b.checksum());
    }
}
