//! The structured exception frames of a 32-bit program, over `seh-clang.exe`: clang's own
//! functions register `_except_handler3`'s table in place, the C runtime's startup linked beside
//! them pushes `_except_handler4`'s. The addresses are those of the linker map beside the image.

use std::collections::BTreeMap;
use std::path::Path;

use ra_asm_analysis::seh32::Layout;
use ra_asm_analysis::{Analysis, SehFrame, analyze};
use ra_asm_pe::Image;

const ACCESS_VIOLATIONS: u64 = 0x0040_1030;
const ARITHMETIC: u64 = 0x0040_1110;
const CONTINUE_EXECUTION: u64 = 0x0040_12a0;
const RAISED_PARAMS: u64 = 0x0040_1380;
const VECTORED_HANDLER: u64 = 0x0040_1490;
const FINALLY_NESTED: u64 = 0x0040_1a90;
const NAME_THREAD: u64 = 0x0040_20f0;
/// `__scrt_common_main_seh` and `__scrt_is_nonwritable_in_current_image` of `msvcrt.lib`.
const COMMON_MAIN: u64 = 0x0040_2bd0;
const IS_NONWRITABLE: u64 = 0x0040_2e20;

fn load() -> Image {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/rhtest/seh-clang.exe");
    ra_asm_pe::load(&path).unwrap_or_else(|e| panic!("seh-clang.exe: {e:#}"))
}

fn frame(analysis: &Analysis<'_>, entry: u64) -> SehFrame {
    analysis.seh_frames.get(&entry).cloned().unwrap_or_else(|| panic!("no frame at {entry:#x}: {:?}", analysis.seh_unread.get(&entry)))
}

#[test]
fn the_functions_with_a_scope_table_are_found() {
    let image = load();
    let analysis = analyze(&image);
    assert!(analysis.seh_unread.is_empty(), "{:?}", analysis.seh_unread);
    let tables: BTreeMap<u64, (u64, Layout)> = analysis.seh_frames.values().map(|f| (f.function, (f.table, f.layout))).collect();
    let expected = BTreeMap::from([
        (ACCESS_VIOLATIONS, (0x0040_4aa4, Layout::Handler3)),
        (ARITHMETIC, (0x0040_4ab0, Layout::Handler3)),
        (CONTINUE_EXECUTION, (0x0040_4ac8, Layout::Handler3)),
        (RAISED_PARAMS, (0x0040_4ad4, Layout::Handler3)),
        (VECTORED_HANDLER, (0x0040_4ae0, Layout::Handler3)),
        (FINALLY_NESTED, (0x0040_4af8, Layout::Handler3)),
        (NAME_THREAD, (0x0040_4b10, Layout::Handler3)),
        (COMMON_MAIN, (0x0040_4b20, Layout::Handler4)),
        (IS_NONWRITABLE, (0x0040_4b40, Layout::Handler4)),
    ]);
    assert_eq!(tables, expected, "{tables:#x?}");
    // clang keeps the try level four slots under the frame pointer, MSVC's prologue in the first.
    assert_eq!(frame(&analysis, ARITHMETIC).level_slot, -0x10);
    assert_eq!(frame(&analysis, IS_NONWRITABLE).level_slot, -4);
}

#[test]
fn a_scope_protects_the_code_between_its_level_stores() {
    let image = load();
    let analysis = analyze(&image);

    // One `__try` around the read: from the call after `mov [level], 0` through the store of -1
    // the `__except` block joins. The second `__try` holds no call, and clang stores no level.
    let reads = frame(&analysis, ACCESS_VIOLATIONS);
    assert_eq!(reads.scopes.len(), 1);
    assert_eq!((reads.scopes[0].filter, reads.scopes[0].handler), (Some(0x0040_1780), 0x0040_10ff));
    assert_eq!(reads.scopes[0].ranges, [(0x0040_1079, 0x0040_1088)]);
    assert!(reads.scopes[0].protects(0x0040_107e) && !reads.scopes[0].protects(0x0040_10b6));

    // Two `__try` side by side, each with its filter and its `__except` block.
    let divides = frame(&analysis, ARITHMETIC);
    assert_eq!(divides.scopes.len(), 2);
    assert_eq!((divides.scopes[0].filter, divides.scopes[0].handler), (Some(0x0040_17e0), 0x0040_1218));
    assert_eq!((divides.scopes[1].filter, divides.scopes[1].handler), (Some(0x0040_1810), 0x0040_120b));
    assert_eq!(divides.scopes[0].ranges, [(0x0040_1161, 0x0040_1184)]);
    assert_eq!(divides.scopes[1].ranges, [(0x0040_11b8, 0x0040_11dc)]);
    assert!(divides.scopes.iter().all(|s| s.parent.is_none()));

    // `_except_handler4`'s table: the entry follows the cookie offsets, the outermost level is -2.
    let probes = frame(&analysis, IS_NONWRITABLE);
    assert_eq!(probes.handler, 0x0040_33c0);
    assert_eq!(probes.scopes.len(), 1);
    assert_eq!((probes.scopes[0].parent, probes.scopes[0].filter, probes.scopes[0].handler), (None, Some(0x0040_2ec2), 0x0040_2ed5));
    assert!(probes.scopes[0].protects(0x0040_2e5a) && probes.scopes[0].protects(0x0040_2e95));
    assert!(!probes.scopes[0].protects(0x0040_2e53) && !probes.scopes[0].protects(0x0040_2eae));
}

#[test]
fn a_nested_scope_names_its_parent() {
    let image = load();
    let analysis = analyze(&image);
    let nested = frame(&analysis, FINALLY_NESTED);
    assert_eq!(nested.scopes.len(), 2);
    let (outer, inner) = (&nested.scopes[0], &nested.scopes[1]);
    // The `__except` around both `__finally`: clang folds the innermost into the code.
    assert_eq!((outer.parent, outer.filter, outer.handler), (None, Some(0x0040_1b30), 0x0040_1b08));
    // The `__finally` inside it has no filter, and its handler is the funclet that runs the block.
    assert_eq!((inner.parent, inner.filter, inner.handler), (Some(0), None, 0x0040_1b20));
    assert_eq!(inner.ranges, [(0x0040_1ae6, 0x0040_1af2)]);
    // The parent protects what the nested scope does, and the call after it besides.
    assert!(inner.ranges.iter().all(|&(begin, end)| outer.ranges.iter().any(|&(b, e)| b <= begin && end <= e)));
    assert!(outer.protects(0x0040_1af2) && !inner.protects(0x0040_1af2));
    assert!(!outer.protects(0x0040_1ad5));
}
