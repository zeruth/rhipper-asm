//! The cases of a row that the reference vectors run: words of the row with each field at its
//! boundaries and at random, and the registers they start from. A seed fixes them, so the vectors
//! of one seed are the same on every machine and every run.

use crate::asm::assemble;
use crate::table::{Field, Kind, Row, Table};

/// What the cases of a machine are drawn over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// How many registers a register field may name, at least one; a field names none past them.
    pub registers: usize,
    /// The width of a register, in bits, from 1 to 64.
    pub bits: u16,
    /// How many cases have every field random, beside those that put each field at its boundaries.
    pub random: usize,
}

/// One instruction to run: its word, the values of its row's fields, and the registers before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    pub word: u32,
    /// One value per field of the row, in the row's order, as the decoder gives them.
    pub values: Vec<i64>,
    /// The value of each register by number, [`Plan::registers`] of them, each in [`Plan::bits`].
    pub registers: Vec<u64>,
}

/// The cases of `row` of `table` under `seed`: each field at each of its boundary values with the
/// others random, the register fields all naming one boundary register, and [`Plan::random`] cases
/// of random fields. A register field's boundaries are the first two and the last two registers; a
/// constant's are its extremes, those next to them, and those around zero when it is signed. A word
/// the table gives to another row, a more specific one, is no case of this row.
pub fn cases(table: &Table, row: &Row, plan: Plan, seed: u64) -> Vec<Case> {
    // The row's own bits stir the seed, so two rows of one machine draw different values.
    let mut rng = Rng(seed ^ (u64::from(row.mask) << 32 | u64::from(row.bits)));
    let mut picks: Vec<Vec<i64>> = Vec::new();
    for (at, field) in row.fields.iter().enumerate() {
        for value in boundaries(field, plan) {
            let mut values = random_values(row, plan, &mut rng);
            values[at] = value;
            picks.push(values);
        }
    }
    let registers: Vec<usize> = row.fields.iter().enumerate().filter(|(_, field)| field.kind == Kind::Reg).map(|(at, _)| at).collect();
    if registers.len() > 1 {
        // One register in every register field: a source that is also the destination.
        let shared = registers.iter().map(|&at| boundaries(&row.fields[at], plan)).min_by_key(Vec::len).unwrap_or_default();
        for value in shared {
            let mut values = random_values(row, plan, &mut rng);
            for &at in &registers {
                values[at] = value;
            }
            picks.push(values);
        }
    }
    for _ in 0..plan.random {
        picks.push(random_values(row, plan, &mut rng));
    }
    picks
        .into_iter()
        .filter_map(|values| {
            let word = assemble(row, &values).ok()?;
            let decoded = table.decode(word)?;
            let same = decoded.row.name == row.name && decoded.row.mask == row.mask && decoded.row.bits == row.bits;
            (same && decoded.values == values).then(|| Case { word, values, registers: random_registers(plan, &mut rng) })
        })
        .collect()
}

/// How many registers a register field of `field`'s width may name.
fn named(field: &Field, plan: Plan) -> u64 {
    (1u64 << field.width).min(plan.registers as u64).max(1)
}

/// The boundary values of `field`, each once, lowest first.
fn boundaries(field: &Field, plan: Plan) -> Vec<i64> {
    let mut values = if field.kind == Kind::Reg {
        let last = named(field, plan) as i64 - 1;
        vec![0, 1, last - 1, last]
    } else {
        let (low, high) = field.range();
        let mut values = vec![low, low + 1, high - 1, high];
        if field.signed {
            values.extend([-1, 0, 1]);
        }
        values
    };
    let (low, high) = if field.kind == Kind::Reg { (0, named(field, plan) as i64 - 1) } else { field.range() };
    values.retain(|value| (low..=high).contains(value));
    values.sort_unstable();
    values.dedup();
    values
}

/// A value for each field of `row`: a register the machine has, or any value of the field.
fn random_values(row: &Row, plan: Plan, rng: &mut Rng) -> Vec<i64> {
    row.fields
        .iter()
        .map(|field| {
            if field.kind == Kind::Reg {
                rng.below(named(field, plan)) as i64
            } else {
                let raw = rng.draw() & ((1u64 << field.width) - 1);
                field.extract((raw << field.lo) as u32)
            }
        })
        .collect()
}

/// The registers before a case: each a boundary number of the width, or a random one, by halves.
fn random_registers(plan: Plan, rng: &mut Rng) -> Vec<u64> {
    let bits = plan.bits.clamp(1, 64);
    let ones = u64::MAX >> (64 - bits);
    let top = 1u64 << (bits - 1);
    let edges = [0, 1, ones, top, top - 1];
    (0..plan.registers)
        .map(|_| if rng.draw() & 1 == 0 { edges[rng.below(edges.len() as u64) as usize] } else { rng.draw() & ones })
        .collect()
}

/// `SplitMix64`: a few lines, no dependency, and the same numbers from the same seed everywhere.
struct Rng(u64);

impl Rng {
    fn draw(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.draw() % n
    }
}
