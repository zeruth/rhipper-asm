//! The decoder against the assembler. `dvp-as` assembled `testdata/ps2/vu1.vsm` into `vu1.o` and
//! wrote its listing, `vu1.lst`, the bytes of each pair beside the source line they came from;
//! `mips64r5900el-ps2-elf-objdump` printed the object's words in `vu1.objdump` (as MIPS: no
//! binutils of today disassembles the micro mode). Every pair of the listing is the object's two
//! words, and decodes back to its source line, a label read as its address.

use std::collections::HashMap;
use std::path::Path;

use ra_asm_arch::{Access, Arch, Delay, Flow, Instr, Target};
use ra_asm_vu::{I, Q, VF0, Vu, register};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/ps2").join(name)).unwrap()
}

/// One pair of the listing: `  10 0000 BC060280 \t        nop    xtop vi02`, its upper word on
/// the next line with no address.
struct Row {
    address: u64,
    bytes: Vec<u8>,
    source: String,
}

fn hex(text: &str) -> Vec<u8> {
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

/// The pairs of the listing, and the address of each label.
fn listing(text: &str) -> (Vec<Row>, HashMap<String, u64>) {
    let mut rows: Vec<Row> = Vec::new();
    let mut pending = Vec::new();
    let mut labels = HashMap::new();
    for line in text.lines() {
        let (left, source) = line.split_once('\t').unwrap_or((line, ""));
        let tokens: Vec<&str> = left.split_whitespace().collect();
        match tokens.as_slice() {
            [_, address, bytes] => {
                let address = u64::from_str_radix(address, 16).unwrap();
                for label in pending.drain(..) {
                    labels.insert(label, address);
                }
                rows.push(Row { address, bytes: hex(bytes), source: source.to_owned() });
            }
            [_, bytes] => rows.last_mut().expect("a continuation follows a row").bytes.extend(hex(bytes)),
            _ => {
                let source = source.trim();
                if let Some(label) = source.strip_suffix(':').filter(|l| !l.starts_with(';') && !l.contains(char::is_whitespace)) {
                    pending.push(label.to_owned());
                }
            }
        }
    }
    (rows, labels)
}

/// The words objdump printed, by address.
fn words(text: &str) -> HashMap<u64, u32> {
    text.lines()
        .filter_map(|line| {
            let (address, rest) = line.trim_start().split_once(":\t")?;
            let word = rest.split_whitespace().next()?;
            Some((u64::from_str_radix(address, 16).ok()?, u32::from_str_radix(word, 16).ok()?))
        })
        .collect()
}

/// The source line in the decoder's spelling: one space between tokens, a label as its address.
fn expected(source: &str, labels: &HashMap<String, u64>) -> String {
    let source = source.split(';').next().unwrap_or(source);
    source
        .split_whitespace()
        .map(|token| labels.get(token).map_or_else(|| token.to_owned(), |a| format!("{a:#x}")))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn agrees_with_the_assembler_on_every_pair() {
    let (rows, labels) = listing(&fixture("vu1.lst"));
    let words = words(&fixture("vu1.objdump"));
    assert!(rows.len() > 40, "the listing has only {} pairs", rows.len());
    assert_eq!(words.len(), rows.len() * 2, "the objdump has words the listing does not");
    let vu = Vu::default();
    let mut mismatches = Vec::new();
    for row in &rows {
        let bytes: [u8; 8] =
            row.bytes.as_slice().try_into().unwrap_or_else(|_| panic!("{:#x}: not one pair: {:?}", row.address, row.bytes));
        let lower = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        let upper = u32::from_le_bytes(bytes[4..].try_into().unwrap());
        assert_eq!(
            (words.get(&row.address), words.get(&(row.address + 4))),
            (Some(&lower), Some(&upper)),
            "{:#x}: the listing's words are not the object's",
            row.address
        );
        let want = expected(&row.source, &labels);
        match vu.decode(&bytes, row.address) {
            None => mismatches.push(format!("{:#x}: ours nothing, the source {want:?}", row.address)),
            Some(instr) => {
                let ours = instr.text.split_whitespace().collect::<Vec<_>>().join(" ");
                if ours != want {
                    mismatches.push(format!("{:#x}: ours {ours:?}, the source {want:?}", row.address));
                }
            }
        }
    }
    assert!(mismatches.is_empty(), "{} differences from dvp-as:\n{}", mismatches.len(), mismatches.join("\n"));
}

fn decode(upper: u32, lower: u32, address: u64) -> Instr {
    let mut bytes = lower.to_le_bytes().to_vec();
    bytes.extend(upper.to_le_bytes());
    Vu::default().decode(&bytes, address).unwrap_or_else(|| panic!("{upper:#010x} {lower:#010x} decodes"))
}

const NOP: u32 = 0x0000_02ff;

#[test]
fn a_branch_takes_effect_past_its_slot() {
    // `ibne vi03, vi00, loop` closes the vertex loop.
    let ibne = decode(NOP, 0x5203_07f3, 0xd8);
    assert_eq!(ibne.flow, Flow::CondJump(Target::Direct(0x78)));
    assert_eq!(Vu::delay(&ibne), Delay::Slot);
    assert_eq!(Vu::successors(&ibne), [0x78, 0xe8]);

    let bal = decode(NOP, 0x420f_0003, 0x148);
    assert_eq!(bal.flow, Flow::Call(Target::Direct(0x168)));
    assert_eq!(Vu::successors(&bal), [0x158]);

    let b = decode(NOP, 0x4000_0003, 0x158);
    assert_eq!(b.flow, Flow::Jump(Target::Direct(0x178)));
    assert_eq!(Vu::successors(&b), [0x178]);

    let jr = decode(NOP, 0x4800_7800, 0x168);
    assert_eq!((jr.text.as_str(), jr.flow), ("nop\tjr vi15", Flow::Return));
    assert_eq!(Vu::delay(&jr), Delay::Slot);
    assert!(Vu::successors(&jr).is_empty());

    // A target wraps within the unit's micro memory.
    assert_eq!(decode(NOP, 0x5203_07f3, 0).flow, Flow::CondJump(Target::Direct(0x3fa0)));
    let vu0 = Vu { vu0: true };
    assert_eq!(Arch::name(&vu0), "vu0");
    let mut bytes = 0x5203_07f3_u32.to_le_bytes().to_vec();
    bytes.extend(NOP.to_le_bytes());
    assert_eq!(vu0.decode(&bytes, 0).unwrap().flow, Flow::CondJump(Target::Direct(0xfa0)));
}

#[test]
fn the_e_bit_ends_the_program_after_the_next_pair() {
    let end = decode(0x4000_02ff, 0x8000_033c, 0x178);
    assert_eq!((end.text.as_str(), end.flow), ("nop[e]\tnop", Flow::Halt));
    assert_eq!(Vu::delay(&end), Delay::Slot);
    assert!(Vu::successors(&end).is_empty());
    // The unit forbids an `e` bit beside a branch.
    let mut bytes = 0x4000_0003_u32.to_le_bytes().to_vec();
    bytes.extend(0x4000_02ff_u32.to_le_bytes());
    assert!(Vu::default().decode(&bytes, 0).is_none());

    // `xgkick` under an `m` bit sends the packet and goes on.
    let kick = decode(0x2000_02ff, 0x8000_2efc, 0x140);
    assert_eq!(kick.flow, Flow::Next);
    assert_eq!(Vu::delay(&kick), Delay::None);
    assert_eq!(Vu::successors(&kick), [0x148]);
    assert!(kick.reads_memory);
    let pair = Vu::default().pair(0x2000_02ff_8000_2efc, 0x140).unwrap();
    assert!(pair.bits.m && !pair.bits.e);
}

#[test]
fn the_dest_mask_and_the_i_bit_reach_the_registers() {
    // `mulq.xyz vf07, vf07, q` keeps `w`, so it reads `vf07` as well as writing it.
    let mulq = decode(0x01c0_39dc, 0x2400_003f, 0xb0);
    assert!(mulq.reads(register(Q)));
    assert!(mulq.registers.contains(&(register(VF0 + 7), Access::ReadWrite)));
    // `ftoi4.xyzw vf08, vf07` writes every lane.
    let ftoi = decode(0x01e8_397d, 0x1203_1801, 0xc8);
    assert!(ftoi.registers.contains(&(register(VF0 + 8), Access::Write)));

    // With the `i` bit the lower word is the float `loi` loads.
    let pair = Vu::default().pair(0x81e0_026c_3f00_0000, 0x60).unwrap();
    assert!(pair.bits.i);
    assert_eq!(pair.lower.mnemonic, "loi");
    assert!(pair.instr(0x60).writes(register(I)));
}
