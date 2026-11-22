//! The text of a PowerPC instruction, as `llvm-objdump` writes it: the registers by number, the
//! immediates in decimal, a branch's target in hexadecimal, and the simplified mnemonics the
//! disassembler prefers (`li` for an `addi` from nothing, `mr` for an `or` of a register with
//! itself, `slwi` and its kin for the rotates that are shifts, `sub` for `subf` with its
//! operands the other way round).
//!
//! It covers what the lift does: the integer machine. A word of anything else is `None`.

fn field(word: u32, from: u32, to: u32) -> u32 {
    (word >> (31 - to)) & ((1u64 << (to - from + 1)) - 1) as u32
}

/// The instruction `word` at `address`, as text.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn disassemble(word: u32, address: u64) -> Option<String> {
    let w = word;
    let (d, a, b) = (field(w, 6, 10), field(w, 11, 15), field(w, 16, 20));
    let simm = i64::from(w as u16 as i16);
    let uimm = w & 0xffff;
    let dot = if w & 1 != 0 { "." } else { "" };
    let relative =
        |offset: i64| if w & 2 != 0 { offset.cast_unsigned() & 0xffff_ffff } else { address.wrapping_add_signed(offset) & 0xffff_ffff };
    let link = if w & 1 != 0 { "l" } else { "" };
    let absolute = if w & 2 != 0 { "a" } else { "" };
    Some(match w >> 26 {
        7 => format!("mulli {d}, {a}, {simm}"),
        8 => format!("subfic {d}, {a}, {simm}"),
        10 | 11 => {
            // The compare is of the doubleword where the instruction's L bit is set.
            let name = match (w >> 26, field(w, 10, 10)) {
                (10, 0) => "cmplwi",
                (10, _) => "cmpldi",
                (_, 0) => "cmpwi",
                _ => "cmpdi",
            };
            let value = if w >> 26 == 10 { i64::from(uimm) } else { simm };
            match field(w, 6, 8) {
                0 => format!("{name} {a}, {value}"),
                which => format!("{name} {which}, {a}, {value}"),
            }
        }
        12 => format!("addic {d}, {a}, {simm}"),
        13 => format!("addic. {d}, {a}, {simm}"),
        14 if a == 0 => format!("li {d}, {simm}"),
        14 => format!("addi {d}, {a}, {simm}"),
        15 if a == 0 => format!("lis {d}, {simm}"),
        15 => format!("addis {d}, {a}, {simm}"),
        16 => {
            // A conditional branch, by what its options test: a bit of the condition register,
            // the count register, or both.
            let (options, bit) = (d, a);
            let target = relative(i64::from(w as u16 as i16 & !3));
            let name = match options & 0b11110 {
                0b01100 => "bt",
                0b00100 => "bf",
                0b10000 => return Some(format!("bdnz{link}{absolute} 0x{target:x}")),
                0b10010 => return Some(format!("bdz{link}{absolute} 0x{target:x}")),
                _ => return Some(format!("bc{link}{absolute} {options}, {bit}, 0x{target:x}")),
            };
            format!("{name}{link}{absolute} {bit}, 0x{target:x}")
        }
        17 => "sc".to_owned(),
        18 => format!("b{link}{absolute} 0x{:x}", relative(i64::from(((w & 0x03ff_fffc) << 6).cast_signed() >> 6))),
        19 => match ((w >> 1) & 0x3ff, d & 0b11110) {
            (16, 0b10100) => format!("blr{link}"),
            (528, 0b10100) => format!("bctr{link}"),
            // A conditional one, by its options and the bit it tests.
            (16, _) => format!("bclr{link} {d}, {a}"),
            (528, _) => format!("bcctr{link} {d}, {a}"),
            _ => return None,
        },
        20 => format!("rlwimi{dot} {a}, {d}, {b}, {}, {}", field(w, 21, 25), field(w, 26, 30)),
        21 => {
            // The rotates that are shifts and clears go by the simpler name.
            let (shift, begin, end) = (b, field(w, 21, 25), field(w, 26, 30));
            if begin == 0 && end == 31 {
                format!("rotlwi{dot} {a}, {d}, {shift}")
            } else if begin == 0 && end == 31 - shift {
                format!("slwi{dot} {a}, {d}, {shift}")
            } else if end == 31 && shift == 32 - begin {
                format!("srwi{dot} {a}, {d}, {begin}")
            } else if shift == 0 && end == 31 {
                format!("clrlwi{dot} {a}, {d}, {begin}")
            } else if shift == 0 && begin == 0 {
                format!("clrrwi{dot} {a}, {d}, {}", 31 - end)
            } else {
                format!("rlwinm{dot} {a}, {d}, {shift}, {begin}, {end}")
            }
        }
        23 => format!("rlwnm{dot} {a}, {d}, {b}, {}, {}", field(w, 21, 25), field(w, 26, 30)),
        24 if w == 0x6000_0000 => "nop".to_owned(),
        24..=29 => {
            let name = ["ori", "oris", "xori", "xoris", "andi.", "andis."][(w >> 26) as usize - 24];
            format!("{name} {a}, {d}, {uimm}")
        }
        31 => return extended(w),
        32..=55 => {
            let name = [
                "lwz", "lwzu", "lbz", "lbzu", "stw", "stwu", "stb", "stbu", "lhz", "lhzu", "lha", "lhau", "sth", "sthu", "lmw", "stmw",
                "lfs", "lfsu", "lfd", "lfdu", "stfs", "stfsu", "stfd", "stfdu",
            ][(w >> 26) as usize - 32];
            format!("{name} {d}, {simm}({a})")
        }
        59 | 63 => return float(w),
        30 => {
            // The doubleword rotates, by the simpler name where one fits.
            let (shift, end) = (b | ((w >> 1) & 1) << 5, field(w, 21, 25) | field(w, 26, 26) << 5);
            match ((w >> 1) & 0xf, (w >> 2) & 7) {
                (8, _) => format!("rldcl{dot} {a}, {d}, {b}, {end}"),
                (9, _) => format!("rldcr{dot} {a}, {d}, {b}, {end}"),
                (_, 0) if shift == 0 => format!("clrldi{dot} {a}, {d}, {end}"),
                (_, 0) if end == 0 => format!("rotldi{dot} {a}, {d}, {shift}"),
                (_, 0) if shift == 64 - end => format!("srdi{dot} {a}, {d}, {end}"),
                (_, 0) => format!("rldicl{dot} {a}, {d}, {shift}, {end}"),
                (_, 1) if end == 63 - shift => format!("sldi{dot} {a}, {d}, {shift}"),
                (_, 1) => format!("rldicr{dot} {a}, {d}, {shift}, {end}"),
                (_, 2) => format!("rldic{dot} {a}, {d}, {shift}, {end}"),
                (_, 3) => format!("rldimi{dot} {a}, {d}, {shift}, {end}"),
                _ => return None,
            }
        }
        58 | 62 => {
            let name = match (w >> 26, w & 3) {
                (58, 0) => "ld",
                (58, 1) => "ldu",
                (58, 2) => "lwa",
                (62, 0) => "std",
                (62, 1) => "stdu",
                _ => return None,
            };
            format!("{name} {d}, {}({a})", simm & !3)
        }
        _ => return None,
    })
}

/// The instructions of the floating-point unit: primary opcode 59, the singles, and 63.
fn float(w: u32) -> Option<String> {
    let (d, a, b, c) = (field(w, 6, 10), field(w, 11, 15), field(w, 16, 20), field(w, 21, 25));
    let dot = if w & 1 != 0 { "." } else { "" };
    let single = w >> 26 == 59;
    if !single {
        let name = match (w >> 1) & 0x3ff {
            0 => return Some(format!("fcmpu {}, {a}, {b}", field(w, 6, 8))),
            32 => return Some(format!("fcmpo {}, {a}, {b}", field(w, 6, 8))),
            12 => Some("frsp"),
            14 => Some("fctiw"),
            15 => Some("fctiwz"),
            40 => Some("fneg"),
            72 => Some("fmr"),
            136 => Some("fnabs"),
            264 => Some("fabs"),
            814 => Some("fctid"),
            815 => Some("fctidz"),
            846 => Some("fcfid"),
            _ => None,
        };
        if let Some(name) = name {
            return Some(format!("{name}{dot} {d}, {b}"));
        }
    }
    let s = if single { "s" } else { "" };
    Some(match (w >> 1) & 0x1f {
        18 => format!("fdiv{s}{dot} {d}, {a}, {b}"),
        20 => format!("fsub{s}{dot} {d}, {a}, {b}"),
        21 => format!("fadd{s}{dot} {d}, {a}, {b}"),
        25 => format!("fmul{s}{dot} {d}, {a}, {c}"),
        23 if !single => format!("fsel{dot} {d}, {a}, {c}, {b}"),
        xo @ 28..=31 => {
            let name = ["fmsub", "fmadd", "fnmsub", "fnmadd"][xo as usize - 28];
            format!("{name}{s}{dot} {d}, {a}, {c}, {b}")
        }
        _ => return None,
    })
}

/// The instructions of primary opcode 31.
fn extended(w: u32) -> Option<String> {
    let (d, a, b) = (field(w, 6, 10), field(w, 11, 15), field(w, 16, 20));
    let dot = if w & 1 != 0 { "." } else { "" };
    let xo = (w >> 1) & 0x3ff;
    // The loads and stores by an index, and the logic and the shifts, whose first operand is the target.
    let indexed = [
        (23, "lwzx"),
        (55, "lwzux"),
        (87, "lbzx"),
        (119, "lbzux"),
        (279, "lhzx"),
        (311, "lhzux"),
        (343, "lhax"),
        (375, "lhaux"),
        (151, "stwx"),
        (183, "stwux"),
        (215, "stbx"),
        (247, "stbux"),
        (407, "sthx"),
        (439, "sthux"),
        (21, "ldx"),
        (53, "ldux"),
        (341, "lwax"),
        (373, "lwaux"),
        (149, "stdx"),
        (181, "stdux"),
        (535, "lfsx"),
        (567, "lfsux"),
        (599, "lfdx"),
        (631, "lfdux"),
        (663, "stfsx"),
        (695, "stfsux"),
        (727, "stfdx"),
        (759, "stfdux"),
        (983, "stfiwx"),
    ];
    if let Some((_, name)) = indexed.iter().find(|(code, _)| *code == xo) {
        return Some(format!("{name} {d}, {a}, {b}"));
    }
    let logic = [
        (28, "and"),
        (60, "andc"),
        (124, "nor"),
        (284, "eqv"),
        (316, "xor"),
        (412, "orc"),
        (444, "or"),
        (476, "nand"),
        (24, "slw"),
        (536, "srw"),
        (792, "sraw"),
        (27, "sld"),
        (539, "srd"),
        (794, "srad"),
    ];
    if let Some((_, name)) = logic.iter().find(|(code, _)| *code == xo) {
        return Some(match (xo, d == b) {
            (444, true) => format!("mr{dot} {a}, {d}"),
            (124, true) => format!("not{dot} {a}, {d}"),
            _ => format!("{name}{dot} {a}, {d}, {b}"),
        });
    }
    Some(match xo {
        0 | 32 => {
            let name = match (xo, field(w, 10, 10)) {
                (0, 0) => "cmpw",
                (0, _) => "cmpd",
                (_, 0) => "cmplw",
                _ => "cmpld",
            };
            match field(w, 6, 8) {
                0 => format!("{name} {a}, {b}"),
                which => format!("{name} {which}, {a}, {b}"),
            }
        }
        824 => format!("srawi{dot} {a}, {d}, {b}"),
        826 | 827 => format!("sradi{dot} {a}, {d}, {}", b | (xo & 1) << 5),
        58 => format!("cntlzd{dot} {a}, {d}"),
        986 => format!("extsw{dot} {a}, {d}"),
        26 => format!("cntlzw{dot} {a}, {d}"),
        922 => format!("extsh{dot} {a}, {d}"),
        954 => format!("extsb{dot} {a}, {d}"),
        // With bit eleven set the instruction asks for one field, which its mask names.
        19 if field(w, 11, 11) == 1 => format!("mfocrf {d}, {}", field(w, 12, 19)),
        19 => format!("mfcr {d}"),
        339 | 467 => {
            let register = match field(w, 16, 20) << 5 | field(w, 11, 15) {
                1 => "xer",
                8 => "lr",
                9 => "ctr",
                _ => return None,
            };
            format!("{}{register} {d}", if xo == 339 { "mf" } else { "mt" })
        }
        _ => {
            // The arithmetic, whose extended opcode's top bit asks for the overflow.
            let overflow = if xo & 0x200 != 0 { "o" } else { "" };
            match xo & 0x1ff {
                40 => format!("sub{overflow}{dot} {d}, {b}, {a}"),
                8 => format!("subc{overflow}{dot} {d}, {b}, {a}"),
                104 | 202 | 234 | 200 | 232 => {
                    let name = match xo & 0x1ff {
                        104 => "neg",
                        202 => "addze",
                        234 => "addme",
                        200 => "subfze",
                        _ => "subfme",
                    };
                    format!("{name}{overflow}{dot} {d}, {a}")
                }
                code => {
                    let name = match code {
                        266 => "add",
                        10 => "addc",
                        138 => "adde",
                        136 => "subfe",
                        233 => "mulld",
                        73 => "mulhd",
                        9 => "mulhdu",
                        489 => "divd",
                        457 => "divdu",
                        235 => "mullw",
                        75 => "mulhw",
                        11 => "mulhwu",
                        491 => "divw",
                        459 => "divwu",
                        _ => return None,
                    };
                    format!("{name}{overflow}{dot} {d}, {a}, {b}")
                }
            }
        }
    })
}
