//! The MIPS III rows replayed against `testdata/vectors/mips64el`, the vectors `write_mips3_vectors`
//! takes from the harness (`testdata/vectors/harness/guest.c` with `mips64el.S`, built by `run.sh`
//! with clang in WSL) under `qemu-mips64el-static`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ra_asm_machine::{Cases, Guest, Input, Output, Vectors, generate, replay};
use ra_asm_mips::rows::mips3;

/// The case's code and the data window, where guest.c maps them.
const CASE_IP: u64 = 0x3000_2000;
const DATA_BASE: u32 = 0x4000_0000;
const WINDOW: usize = 64;

/// The cases of a row, and the seed they are drawn at.
const SAMPLES: usize = 40;
const SEED: u64 = 0x5eed_0349;

/// The n64 `struct case_in` and `struct case_out` of guest.c.
const IN_LEN: usize = 360;
const OUT_LEN: usize = 360;
const OUT_WINDOW: usize = 296;

const SIGTRAP: u64 = 5;
const SIGFPE: u64 = 8;
const SIGBUS: u64 = 10;
const SIGSEGV: u64 = 11;

fn vectors_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/vectors")
}

fn wsl_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    match text.as_bytes() {
        [drive, b':', b'/', ..] if drive.is_ascii_alphabetic() => format!("/mnt/{}{}", drive.to_ascii_lowercase() as char, &text[2..]),
        _ => text,
    }
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> String + '_ {
    move |e| format!("{}: {e}", path.display())
}

/// The machine: the harness built for mips64el and run under qemu-user, one run a group.
struct Qemu {
    wsl: PathBuf,
    scratch: ra_asm_testing::Scratch,
}

impl Qemu {
    /// The results the harness wrote for the cases of `blob`.
    fn harness(&self, group: &str, blob: &[u8]) -> Result<Vec<u8>, String> {
        let dir = self.scratch.path();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/vectors/harness");
        let copy = dir.join("harness");
        fs::create_dir_all(&copy).map_err(io(&copy))?;
        // A checkout with CRLF line ends gives bash a script it cannot read: the copy has LF.
        for name in ["guest.c", "mips64el.S", "run.sh"] {
            let from = source.join(name);
            let text = fs::read_to_string(&from).map_err(io(&from))?;
            let to = copy.join(name);
            fs::write(&to, text.replace("\r\n", "\n")).map_err(io(&to))?;
        }
        let input = dir.join(format!("{group}.bin"));
        let output = dir.join(format!("{group}.out"));
        fs::write(&input, blob).map_err(io(&input))?;
        let run = Command::new(&self.wsl)
            .args(["-d", "Ubuntu-24.04", "--", "bash"])
            .arg(wsl_path(&copy.join("run.sh")))
            .arg("mips64el")
            .args([wsl_path(dir), wsl_path(&input), wsl_path(&output)])
            .output()
            .map_err(|e| format!("wsl: {e}"))?;
        if !run.status.success() {
            return Err(format!(
                "run.sh mips64el failed: {}\n{}{}",
                run.status,
                String::from_utf8_lossy(&run.stdout),
                String::from_utf8_lossy(&run.stderr)
            ));
        }
        fs::read(&output).map_err(io(&output))
    }
}

impl Guest for Qemu {
    fn run(&mut self, group: &str, inputs: &[Input]) -> Result<Vec<Output>, String> {
        let mut blob = Vec::with_capacity(inputs.len() * IN_LEN);
        for input in inputs {
            pack(&mut blob, input)?;
        }
        let results = self.harness(group, &blob)?;
        if results.len() != inputs.len() * OUT_LEN {
            return Err(format!("the harness answered {} bytes for {} cases", results.len(), inputs.len()));
        }
        results.chunks_exact(OUT_LEN).map(unpack).collect()
    }
}

/// `input` as guest.c reads a case: the registers, hi and lo, the code's length, the window's
/// address, the code and the window.
fn pack(blob: &mut Vec<u8>, input: &Input) -> Result<(), String> {
    if input.registers.len() != mips3::REGISTERS as usize || input.window.len() != WINDOW || input.code.len() > 4 {
        return Err(format!(
            "a case of {} registers, {} window bytes and {} words",
            input.registers.len(),
            input.window.len(),
            input.code.len()
        ));
    }
    for &value in &input.registers {
        blob.extend(value.to_le_bytes());
    }
    let len = u32::try_from(4 * input.code.len()).map_err(|e| e.to_string())?;
    blob.extend(len.to_le_bytes());
    blob.extend(DATA_BASE.to_le_bytes());
    let mut code = [0u8; 16];
    for (bytes, word) in code.chunks_exact_mut(4).zip(&input.code) {
        bytes.copy_from_slice(&word.to_le_bytes());
    }
    blob.extend(code);
    blob.extend(&input.window);
    Ok(())
}

/// A case's result as guest.c writes it: the `break` after the code ends it, the overflow trap is
/// SIGFPE and a bad access SIGSEGV or SIGBUS.
fn unpack(record: &[u8]) -> Result<Output, String> {
    let word = |i: usize| u64::from_le_bytes(record[8 * i..8 * i + 8].try_into().expect("eight bytes"));
    let stray = word(36);
    if stray != 0 {
        return Err(format!("a store reached {:#x}, outside the window", stray - 1));
    }
    let fault = match word(0) {
        SIGTRAP => None,
        SIGFPE => Some("trap".to_owned()),
        SIGBUS | SIGSEGV => Some("segv".to_owned()),
        sig => return Err(format!("the case stopped with signal {sig} at {:#x}", word(1))),
    };
    Ok(Output { fault, pc: word(1), registers: (2..36).map(word).collect(), window: record[OUT_WINDOW..].to_vec() })
}

/// The doubleword rows' vectors, every row in the one `mips3` group, written to
/// `testdata/vectors/mips64el/mips3.vec`.
fn write_vectors() {
    let wsl = ra_asm_testing::tool("wsl").expect("the vectors are taken under qemu in WSL");
    let rows: Vec<Cases> = mips3::ROWS.iter().map(|&(row, _)| Cases { row, group: "mips3", undefined: &[] }).collect();
    let table = Vectors {
        machine: mips3::machine(),
        spec: mips3::SPEC,
        rows: &rows,
        samples: SAMPLES,
        at: CASE_IP,
        window: u64::from(DATA_BASE),
        window_len: WINDOW,
        slot: 0,
    };
    let mut guest = Qemu { wsl, scratch: ra_asm_testing::Scratch::new("ra-asm-mips-tests", "vectors-mips64el") };
    let written = generate(&table, &mut guest, SEED, &vectors_dir()).unwrap_or_else(|e| panic!("{e}"));
    println!("mips64el vectors: {} cases into {}", rows.len() * SAMPLES, written[0].display());
}

/// Regenerates the MIPS III vectors from the machine.
#[test]
#[ignore = "builds the harness for mips64el in WSL and runs it under qemu-mips64el-static, writing testdata/vectors/mips64el"]
fn write_mips3_vectors() {
    write_vectors();
}

/// Every case of the 22 doubleword rows lifts and runs as the machine ran it, the overflow traps
/// of `dadd`, `daddi` and `dsub` included.
#[test]
fn mips3() {
    let directory = vectors_dir().join("mips64el");
    let replayed = replay(mips3::machine(), &mips3::SPEC, &directory).unwrap_or_else(|e| panic!("{e}"));
    println!("{replayed}");
    assert!(replayed.passed(), "{replayed}");
    assert_eq!(replayed.forms, mips3::ROWS.len(), "{replayed}");
    assert_eq!(mips3::ROWS.len(), 22);
    assert!(replayed.cases >= 22 * SAMPLES, "{replayed}");
}
