//! The reference vectors of a described machine, from its table: the cases of each row, run on
//! the machine itself by a [`Guest`] in one batch per group, written as
//! `<out>/<machine>/<group>.vec` in the format of [`ra_asm_testing::vectors`].
//!
//! A case is a word the row wins, its fields drawn at the seed, with the registers its semantics
//! reads drawn too (the rest zero) and, for a row that touches memory, a window of data its base
//! register points into: the base is drawn so that base plus displacement lands in the window at
//! the access's alignment, whatever the displacement. Its outcome is what the guest says changed,
//! less the registers the row leaves undefined, which `undefined` names by a mask of their numbers
//! as the x86 files name their undefined flags.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use ra_asm_testing::vectors::{self, Case, Outcome, State, Value};

use crate::instr::Machine;
use crate::sem::StateSpec;
use crate::table::{Kind, Row};

/// The bytes of one memory cell of a file; the window is written in cells, each in address order.
pub const CELL: usize = 16;
/// The register the files give the program counter.
pub const PC: &str = "pc";
/// The register of an output that holds the mask of the register numbers the row leaves undefined.
pub const UNDEFINED: &str = "undefined";

/// How many words a case draws before it gives up finding one its row wins.
const DRAWS: usize = 32;

/// The cases of one row of the machine's table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cases {
    /// The row's name in the table.
    pub row: &'static str,
    /// The file the row's cases go to.
    pub group: &'static str,
    /// The registers whose value the machine leaves undefined after the row, by their names in the
    /// machine's file: what the guest leaves in them is not part of the outcome.
    pub undefined: &'static [&'static str],
}

/// What a machine's vectors are drawn from: its rows, the state its cases run over, and where the
/// guest puts the code and the data.
#[derive(Debug, Clone, Copy)]
pub struct Vectors<'a> {
    pub machine: &'a Machine,
    /// The width of the registers and the program counter, and the register that reads as zero.
    pub spec: StateSpec,
    pub rows: &'a [Cases],
    /// How many cases a row has, each a word and a state of its own.
    pub samples: usize,
    /// The address of a case's code.
    pub at: u64,
    /// The address of the data window, and its length in bytes.
    pub window: u64,
    pub window_len: usize,
    /// The word in a delayed row's slot: one that changes nothing.
    pub slot: u32,
}

/// One case as the guest takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    /// The words at [`Vectors::at`]: the row's, and after a delayed row the slot's.
    pub code: Vec<u32>,
    /// Every register of the machine's file, by number.
    pub registers: Vec<u64>,
    /// The bytes of the window at [`Vectors::window`].
    pub window: Vec<u8>,
}

/// What a case left, as the guest answers it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// The fault that stopped the case before its code ran through, named in one word as the files
    /// name it; `None` when the case ran to the end of its code or its branch's target.
    pub fault: Option<String>,
    /// Where control went after the code.
    pub pc: u64,
    /// Every register of the machine's file, by number.
    pub registers: Vec<u64>,
    pub window: Vec<u8>,
}

/// The machine itself, which runs cases: the harness under qemu-user, or a stand-in for it.
pub trait Guest {
    /// The outputs of `inputs`, the cases of `group`, in their order, from one run of the machine.
    fn run(&mut self, group: &str, inputs: &[Input]) -> Result<Vec<Output>, String>;
}

/// Why the vectors were not written.
#[derive(Debug)]
pub enum Error {
    /// A row of the vectors is not one of the machine's table.
    Row(&'static str),
    /// A row declares undefined a register the machine's file does not have.
    Register {
        row: &'static str,
        register: &'static str,
    },
    /// None of the words drawn for the row decodes to it: a more specific row wins them all.
    Word(&'static str),
    /// The guest did not run the group.
    Guest {
        group: String,
        message: String,
    },
    /// The guest answered other than it was asked: another number of cases, or a state of another shape.
    Answer {
        group: String,
        message: String,
    },
    Io {
        path: PathBuf,
        error: std::io::Error,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Row(row) => write!(f, "the machine has no row `{row}`"),
            Error::Register { row, register } => write!(f, "row `{row}` leaves undefined `{register}`, which the machine does not have"),
            Error::Word(row) => write!(f, "no word drawn for row `{row}` decodes to it"),
            Error::Guest { group, message } => write!(f, "the guest did not run the {group} group: {message}"),
            Error::Answer { group, message } => write!(f, "the guest's answer to the {group} group: {message}"),
            Error::Io { path, error } => write!(f, "{}: {error}", path.display()),
        }
    }
}

impl std::error::Error for Error {}

/// A case drawn, with what its outcome needs of its row.
struct Drawn {
    input: Input,
    /// The numbers of the registers the row leaves undefined.
    undefined: Vec<usize>,
    /// Whether the row reads or writes memory, so the window is part of its input.
    memory: bool,
}

/// Writes `<out>/<machine>/<group>.vec` for every group of `table`, its cases drawn at `seed` and
/// answered by `guest` in one batch a group; the files written, in the order of their groups.
pub fn generate(table: &Vectors<'_>, guest: &mut dyn Guest, seed: u64, out: &Path) -> Result<Vec<PathBuf>, Error> {
    let machine = table.machine;
    let mut rng = Rng(seed);
    let mut groups: BTreeMap<&str, Vec<Drawn>> = BTreeMap::new();
    for cases in table.rows {
        let row = machine.table.rows().iter().find(|row| row.name == cases.row).ok_or(Error::Row(cases.row))?;
        let undefined = cases
            .undefined
            .iter()
            .map(|&register| machine.registers.iter().position(|r| r.name == register).ok_or(Error::Register { row: cases.row, register }))
            .collect::<Result<Vec<usize>, Error>>()?;
        let group = groups.entry(cases.group).or_default();
        for _ in 0..table.samples {
            let (input, memory) = draw(table, row, &mut rng).ok_or(Error::Word(cases.row))?;
            group.push(Drawn { input, undefined: undefined.clone(), memory });
        }
    }
    let dir = out.join(machine.name);
    std::fs::create_dir_all(&dir).map_err(|error| Error::Io { path: dir.clone(), error })?;
    let mut written = Vec::with_capacity(groups.len());
    for (group, drawn) in &groups {
        let inputs: Vec<Input> = drawn.iter().map(|d| d.input.clone()).collect();
        let outputs = guest.run(group, &inputs).map_err(|message| Error::Guest { group: (*group).to_owned(), message })?;
        let answer = |message: String| Error::Answer { group: (*group).to_owned(), message };
        if outputs.len() != inputs.len() {
            return Err(answer(format!("{} outputs for {} cases", outputs.len(), inputs.len())));
        }
        let cases = drawn.iter().zip(&outputs).map(|(d, o)| case(table, d, o)).collect::<Result<Vec<Case>, String>>().map_err(answer)?;
        let text = format!(
            "# {machine} vectors of the {group} group: {count} cases drawn at seed {seed:#x} from the rows of\n\
             # the group and run on the machine by its guest, written by ra_asm_machine::generate. Bytes:\n\
             # the row's word, its most significant byte first, then the slot's after a delayed row. In:\n\
             # pc at {at:#x}, the registers the row reads (the rest are zero), and for a row that touches\n\
             # memory the window at {window:#x} in {CELL}-byte cells in address order. Out: what changed; a\n\
             # register the row leaves undefined is left out and `{UNDEFINED}` holds the mask of their numbers.\n{}",
            vectors::write(&cases),
            machine = machine.name,
            count = cases.len(),
            at = table.at,
            window = table.window,
        );
        let path = dir.join(format!("{group}.vec"));
        std::fs::write(&path, text).map_err(|error| Error::Io { path: path.clone(), error })?;
        written.push(path);
    }
    Ok(written)
}

/// A case of `row`: its code and the state it starts from, and whether it touches memory; `None`
/// when no word drawn decodes to the row.
fn draw(table: &Vectors<'_>, row: &Row, rng: &mut Rng) -> Option<(Input, bool)> {
    let machine = table.machine;
    let files = machine.registers.len() as u64;
    let bits = u32::from(table.spec.bits);
    'draw: for _ in 0..DRAWS {
        let mut word = row.bits;
        for field in row.fields {
            let width = u32::from(field.width);
            let value = match field.kind {
                Kind::Reg => rng.below(files.min(1 << width)),
                // A target a few words on, which the guest's fill after the code catches.
                Kind::Rel { .. } => 1 + rng.below(3),
                Kind::Imm | Kind::Mem { .. } => rng.value(width),
            };
            word |= ((value & mask(width)) as u32) << field.lo;
        }
        let Some(decoded) = machine.table.decode(word) else { continue };
        if decoded.row.name != row.name {
            continue;
        }
        let access = machine.access(word, table.at)?;
        let mut registers = vec![0u64; machine.registers.len()];
        for &(n, touch) in &access.registers {
            if touch.reads() && table.spec.zero != Some(n) {
                registers[n as usize] = rng.value(bits);
            }
        }
        for field in row.fields {
            let Kind::Mem { base, bits: size } = field.kind else { continue };
            let Some(base) = decoded.field(base) else { continue 'draw };
            if table.spec.zero == Some(base as u32) {
                continue 'draw;
            }
            let size = usize::from(size.div_ceil(8)).max(1);
            let Some(room) = table.window_len.checked_sub(size) else { continue 'draw };
            let offset = rng.below((room / size + 1) as u64) * size as u64;
            let disp = decoded.field(field.name).unwrap_or(0);
            registers[base as usize] = table.window.wrapping_add(offset).wrapping_sub(disp as u64) & mask(bits);
        }
        let memory = access.reads_memory || access.writes_memory;
        let window = if memory { (0..table.window_len).map(|_| rng.next_u64() as u8).collect() } else { vec![0; table.window_len] };
        let code = if row.flow.delayed() { vec![word, table.slot] } else { vec![word] };
        return Some((Input { code, registers, window }, memory));
    }
    None
}

/// The line of a drawn case and its output.
fn case(table: &Vectors<'_>, drawn: &Drawn, output: &Output) -> Result<Case, String> {
    let files = table.machine.registers.len();
    if output.registers.len() != files || output.window.len() != table.window_len {
        return Err(format!(
            "a state of {} registers and {} window bytes, not {files} and {}",
            output.registers.len(),
            output.window.len(),
            table.window_len
        ));
    }
    let input = &drawn.input;
    let registers = |values: &[u64], skip: &[usize]| -> BTreeMap<String, Value> {
        values
            .iter()
            .enumerate()
            .filter(|(n, _)| !skip.contains(n))
            .map(|(n, &value)| (table.machine.registers[n].name.to_owned(), sized(value, table.spec.bits)))
            .collect()
    };
    let cells = |window: &[u8], keep: &dyn Fn(usize) -> bool| -> BTreeMap<u64, Value> {
        window
            .chunks(CELL)
            .enumerate()
            .filter(|&(i, _)| keep(i))
            .map(|(i, cell)| (table.window + (i * CELL) as u64, cell.to_vec()))
            .collect()
    };
    let mut before = State { registers: registers(&input.registers, &[]), memory: cells(&input.window, &|_| drawn.memory) };
    before.registers.insert(PC.to_owned(), sized(table.at, table.spec.pc_bits));
    let outcome = if let Some(kind) = &output.fault {
        Outcome::Fault(kind.clone())
    } else {
        // A row that does not touch memory has no window in, so a cell it changed anyway is out.
        let differs = |i: usize| drawn.memory || input.window.chunks(CELL).nth(i) != output.window.chunks(CELL).nth(i);
        let mut after = State { registers: registers(&output.registers, &drawn.undefined), memory: cells(&output.window, &differs) };
        after.registers.insert(PC.to_owned(), sized(output.pc, table.spec.pc_bits));
        if !drawn.undefined.is_empty() {
            after.registers.insert(UNDEFINED.to_owned(), undefined_mask(&drawn.undefined, files));
        }
        Outcome::State(vectors::changed(&before, &after))
    };
    let bytes = input.code.iter().flat_map(|word| word.to_be_bytes()).collect();
    Ok(Case { bytes, input: before, outcome })
}

/// The low `bits` bits of `value` as the files write a value: as many bytes as the width takes,
/// the most significant first.
fn sized(value: u64, bits: u16) -> Value {
    let len = usize::from(bits.div_ceil(8)).clamp(1, 8);
    value.to_be_bytes()[8 - len..].to_vec()
}

/// The mask of the register numbers `registers` over a file of `files`, the most significant
/// byte first: bit `n` is register `n`.
fn undefined_mask(registers: &[usize], files: usize) -> Value {
    let mut mask = vec![0u8; files.div_ceil(8).max(1)];
    let last = mask.len() - 1;
    for &n in registers {
        mask[last - n / 8] |= 1 << (n % 8);
    }
    mask
}

/// The low `bits` bits set.
fn mask(bits: u32) -> u64 {
    if bits >= 64 { u64::MAX } else { (1 << bits) - 1 }
}

/// The draws of a seed: splitmix64, so a seed gives the same files on every host.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A draw below `n`, which is not zero.
    fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    /// A value of `bits` bits: an edge of the range (zero, one, all ones, the sign bit and the
    /// largest positive) or a small one half the time, so a carry, an overflow and a sign show.
    fn value(&mut self, bits: u32) -> u64 {
        let sign = 1u64 << (bits.clamp(1, 64) - 1);
        let value = match self.below(8) {
            0 => 0,
            1 => 1,
            2 => u64::MAX,
            3 => sign,
            4 => sign - 1,
            5 => self.below(16),
            _ => self.next_u64(),
        };
        value & mask(bits)
    }
}

#[cfg(test)]
mod tests;
