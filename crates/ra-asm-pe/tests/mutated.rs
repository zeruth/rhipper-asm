//! The loader over files that are almost programs: each ELF fixture, and the flat firmware
//! image, with a few bytes of its headers changed, many times over. A changed file may be
//! refused, or read as what it now says; it may not stop the loader with a panic (a sum of two
//! of the file's own numbers that overflows, an index past a table). The changes come from a
//! fixed seed, so a run is the same run each time; the fuzzer of `fuzz/` looks further, and what
//! it finds is fixed here first when it can be shown here.

use std::path::PathBuf;

use ra_asm_pe::Image;

/// A small generator of numbers from a seed: the same seed gives the same file changes.
struct Numbers(u64);

impl Numbers {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Where the headers of an ELF file are: the file header, the program headers and the section
/// headers, as the file header says; the whole file's start for a file that is none.
fn header_ranges(file: &[u8]) -> Vec<(usize, usize)> {
    let mut ranges = vec![(0, file.len().min(0x100))];
    let number = |at: usize, len: usize| file.get(at..at + len).map(|b| b.iter().rev().fold(0u64, |v, &x| v << 8 | u64::from(x)) as usize);
    if file.starts_with(b"\x7fELF") && file.get(5) == Some(&1) {
        let wide = file.get(4) == Some(&2);
        let fields = if wide {
            [(32, 8), (40, 8), (54, 2), (56, 2), (58, 2), (60, 2)]
        } else {
            [(28, 4), (32, 4), (42, 2), (44, 2), (46, 2), (48, 2)]
        };
        let values: Vec<usize> = fields.iter().filter_map(|&(at, len)| number(at, len)).collect();
        if let [phoff, shoff, phentsize, phnum, shentsize, shnum] = values[..] {
            ranges.push((phoff, phentsize * phnum));
            ranges.push((shoff, shentsize * shnum));
        }
    }
    ranges.into_iter().filter(|&(at, len)| len != 0 && at < file.len()).map(|(at, len)| (at, len.min(file.len() - at))).collect()
}

#[test]
fn a_changed_file_never_panics_the_loader() {
    let bin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/bin");
    let mut fixtures: Vec<PathBuf> = std::fs::read_dir(&bin).unwrap().map(|entry| entry.unwrap().path()).collect();
    fixtures.sort();
    let mut numbers = Numbers(0x9e37_79b9_7f4a_7c15);
    let (mut files, mut panics) = (0, Vec::new());
    // The loader's own messages are enough: a panic's would bury them.
    let hook = std::panic::take_hook();
    static WHERE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
    std::panic::set_hook(Box::new(|info| {
        let place = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        *WHERE.lock().unwrap() = format!("{place}: {}", info.payload().downcast_ref::<&str>().copied().unwrap_or("a panic"));
    }));
    for path in fixtures {
        let Ok(file) = std::fs::read(&path) else { continue };
        let flat = path.file_name().is_some_and(|name| name == "cortexm.bin");
        // The ELF fixtures small enough to change many times over, and the flat image.
        if !(flat || file.starts_with(b"\x7fELF") && file.len() < 200_000) {
            continue;
        }
        files += 1;
        let ranges = header_ranges(&file);
        for round in 0..3000 {
            let mut changed = file.clone();
            for _ in 0..=numbers.below(4) {
                let (at, len) = ranges[numbers.below(ranges.len())];
                let at = at + numbers.below(len);
                // A whole field of ones, four bytes or eight where the headers align them, is
                // the number whose sum with any other overflows.
                if numbers.below(3) == 0 {
                    let width = if numbers.below(2) == 0 { 4 } else { 8 };
                    let start = at - at % width;
                    for byte in changed.iter_mut().skip(start).take(width) {
                        *byte = 0xff;
                    }
                    continue;
                }
                // A byte of all ones or none makes a number huge or nothing; any other byte is any other number.
                changed[at] = match numbers.below(4) {
                    0 => 0xff,
                    1 => 0,
                    2 => 0x80,
                    _ => numbers.next() as u8,
                };
            }
            let kept = changed.clone();
            if std::panic::catch_unwind(move || drop(Image::parse(changed))).is_err() && panics.len() < 5 {
                let differing: Vec<String> = kept
                    .iter()
                    .zip(&file)
                    .enumerate()
                    .filter(|(_, (a, b))| a != b)
                    .map(|(at, (a, _))| format!("{at:#x}={a:#04x}"))
                    .collect();
                let place = WHERE.lock().unwrap().clone();
                panics.push(format!("{} round {round} at {place}: {}", path.file_name().unwrap().to_string_lossy(), differing.join(" ")));
            }
        }
    }
    std::panic::set_hook(hook);
    assert!(files >= 5, "{files} fixtures were changed");
    assert!(panics.is_empty(), "the loader panicked on changed files:\n{}", panics.join("\n"));
}
