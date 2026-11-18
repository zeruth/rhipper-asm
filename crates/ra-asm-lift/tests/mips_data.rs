//! The static mipsel control program lifted standalone at the ABI level stores through its
//! section globals: an address its code builds from `lui` and an offset is the byte of the global
//! it falls in, as the x86 fixtures' constant addresses are, never the image's number, which no
//! port maps.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ra_asm_ir::{BinOp, Function, Inst, Operand, ValueId};
use ra_asm_lift::RaiseLevel;

/// The control program's image, from its first section to the end of its `.bss`.
const IMAGE: std::ops::Range<i128> = 0x10000..0x31000;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The integer `op` stands for when it is a constant through casts, an `or` with zero and an
/// add of constants: the shapes a constant address takes on its way to a store's pointer.
fn constant(defs: &HashMap<ValueId, &Inst>, op: &Operand) -> Option<i128> {
    match op {
        Operand::Int(n) => Some(*n),
        Operand::Local(v) => match defs.get(v)? {
            Inst::Cast { value, .. } => constant(defs, value),
            Inst::Bin { op: BinOp::Or, lhs, rhs: Operand::Int(0), .. } => constant(defs, lhs),
            Inst::Bin { op: BinOp::Add, lhs, rhs, .. } => Some(constant(defs, lhs)? + constant(defs, rhs)?),
            _ => None,
        },
        _ => None,
    }
}

/// The stores of `f` whose address is a number inside the image, by the machine address of each.
fn image_stores(f: &Function) -> Vec<String> {
    let defs: HashMap<ValueId, &Inst> = f.blocks.iter().flat_map(|b| &b.instrs).filter_map(|i| i.result.map(|r| (r, &i.inst))).collect();
    let mut found = Vec::new();
    for i in f.blocks.iter().flat_map(|b| &b.instrs) {
        let Inst::Store { ptr, .. } = &i.inst else { continue };
        if let Some(address) = constant(&defs, ptr).filter(|a| IMAGE.contains(a)) {
            found.push(format!("{} at {:#x}: a store to {address:#x}", f.name, i.address.unwrap_or(0)));
        }
    }
    found
}

#[test]
fn mips_control_stores_through_its_section_globals() {
    let path = root().join("testdata/mips/control");
    let image = ra_asm_pe::load(&path).unwrap_or_else(|e| panic!("{}: {e:#}", path.display()));
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: true, ..Default::default() };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let options = lifted.raise_options(RaiseLevel::Abi);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    assert!(!lifted.module.functions.is_empty(), "mips control: nothing was lifted");
    let found: Vec<String> = lifted.module.functions.iter().flat_map(image_stores).collect();
    assert!(found.is_empty(), "mips control: stores to a number inside the image, not a section global:\n{}", found.join("\n"));
}
