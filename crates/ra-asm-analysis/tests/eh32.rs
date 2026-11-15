//! The C++ exception frames of a 32-bit program, over `eh-clang.exe`: clang links each frame in
//! place, under `ebp`, or under `esi` where it realigns the stack, with a `__ehhandler$` stub that
//! hands `__CxxFrameHandler3` the function's `FuncInfo`. The addresses are those of the linker map
//! beside the image.

use std::collections::BTreeMap;
use std::path::Path;

use ra_asm_analysis::{Analysis, CxxFrame32, analyze};
use ra_asm_pe::Image;

const THROWER: u64 = 0x0040_1020;
const BY_TYPE: u64 = 0x0040_1260;
const DEEP: u64 = 0x0040_13a0;
const RETHROWS: u64 = 0x0040_1470;
const NESTED: u64 = 0x0040_1540;
const CTOR: u64 = 0x0040_16b0;
/// `seh` and `finally_param` register `_except_handler3`'s scope table, which is no C++ frame.
const SEH: u64 = 0x0040_17e0;
const FINALLY_PARAM: u64 = 0x0040_1900;

fn load() -> Image {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/rhtest/eh-clang.exe");
    ra_asm_pe::load(&path).unwrap_or_else(|e| panic!("eh-clang.exe: {e:#}"))
}

fn frame(analysis: &Analysis<'_>, entry: u64) -> CxxFrame32 {
    analysis.cxx_frames32.get(&entry).cloned().unwrap_or_else(|| panic!("no frame at {entry:#x}: {:?}", analysis.cxx_unread32.get(&entry)))
}

#[test]
fn the_functions_under_the_cxx_handler_are_found() {
    let image = load();
    let analysis = analyze(&image);
    assert!(analysis.cxx_unread32.is_empty(), "{:?}", analysis.cxx_unread32);
    let stubs: BTreeMap<u64, (u64, u64)> = analysis.cxx_frames32.values().map(|f| (f.function, (f.handler, f.info.address))).collect();
    let expected = BTreeMap::from([
        (THROWER, (0x0040_1bd0, 0x0040_4b8c)),
        (BY_TYPE, (0x0040_1be0, 0x0040_4bb8)),
        (DEEP, (0x0040_1bf0, 0x0040_4c40)),
        (RETHROWS, (0x0040_1c00, 0x0040_4c6c)),
        (NESTED, (0x0040_1c10, 0x0040_4cf8)),
        (CTOR, (0x0040_1c20, 0x0040_4d9c)),
    ]);
    assert_eq!(stubs, expected, "{stubs:#x?}");
    assert!(!analysis.cxx_frames32.contains_key(&SEH) && !analysis.cxx_frames32.contains_key(&FINALLY_PARAM));
    // The registration sits under the frame pointer, or over the realigned stack `esi` holds.
    let by_type = frame(&analysis, BY_TYPE);
    assert_eq!((by_type.base, by_type.registration), ("ebp", -0x18));
    let thrower = frame(&analysis, THROWER);
    assert_eq!((thrower.base, thrower.registration), ("esi", 0x30));
}

#[test]
fn the_tables_are_read_as_the_x64_ones_are() {
    let image = load();
    let analysis = analyze(&image);

    // One try block over state 0 with four catches: by value, two by reference, and `...`.
    let by_type = frame(&analysis, BY_TYPE).info;
    assert_eq!(by_type.unwind_map.len(), 2);
    assert_eq!(by_type.try_blocks.len(), 1);
    let try_block = &by_type.try_blocks[0];
    assert_eq!((try_block.low, try_block.high, try_block.catch_high), (0, 0, 1));
    let handlers: Vec<u64> = try_block.handlers.iter().map(|h| h.handler).collect();
    assert_eq!(handlers, [0x0040_12f0, 0x0040_1310, 0x0040_1350, 0x0040_1380]);
    assert_eq!(try_block.handlers.iter().map(|h| h.catch_object).collect::<Vec<_>>(), [-0x18, -0x1c, -0x20, 0]);
    assert!(try_block.handlers[3].is_catch_all() && !try_block.handlers[0].is_catch_all());
    assert!(by_type.ip_to_state.is_empty());

    // Two try blocks, the inner first, and the destructors of the three guards.
    let nested = frame(&analysis, NESTED).info;
    let tries: Vec<(i32, i32, i32, u64)> = nested.try_blocks.iter().map(|t| (t.low, t.high, t.catch_high, t.handlers[0].handler)).collect();
    assert_eq!(tries, [(2, 3, 5, 0x0040_1670), (0, 5, 6, 0x0040_1640)]);
    let unwind: Vec<(i32, Option<u64>)> = nested.unwind_map.iter().map(|e| (e.to_state, e.action)).collect();
    assert_eq!(
        unwind,
        [(-1, None), (0, Some(0x0040_1620)), (1, None), (2, Some(0x0040_15e0)), (1, None), (4, Some(0x0040_1600)), (-1, None)]
    );
}

#[test]
fn the_state_comes_from_the_stores_of_the_registration() {
    let image = load();
    let analysis = analyze(&image);

    // The call to `thrower` runs in the try body's state; the store itself still outside it.
    let by_type = frame(&analysis, BY_TYPE);
    assert_eq!(by_type.states_at(0x0040_126f), [-1]);
    assert_eq!(by_type.states_at(0x0040_1291), [-1]);
    assert_eq!(by_type.states_at(0x0040_12a0), [0]);
    // A catch runs past its try block's body, and its continuation in the state it left; the
    // way out joins the try body's.
    assert_eq!(by_type.states_at(0x0040_12f8), [1]);
    assert_eq!(by_type.states_at(0x0040_132d), [1]);
    assert_eq!(by_type.states_at(0x0040_12bc), [1]);
    assert_eq!(by_type.states_at(0x0040_12a8), [0, 1]);

    // A throw from inside a catch, after the catch steps the state; an unwind action runs in the
    // state its entry leaves to.
    let nested = frame(&analysis, NESTED);
    assert_eq!(nested.states_at(0x0040_15c9), [3]);
    assert_eq!(nested.states_at(0x0040_1682), [4]);
    assert_eq!(nested.states_at(0x0040_16a2), [5]);
    assert_eq!(nested.states_at(0x0040_1627), [0]);
    assert_eq!(nested.states_at(0x0040_15e7), [2]);
    assert_eq!(nested.states_at(0x0040_15d1), [6]);
    assert_eq!(nested.states_at(0x0040_15a4), [-1, 6]);

    // The state stored through `esi` over the realigned stack, which the unwind action finds again
    // from the `ebp` the handler gives it.
    let thrower = frame(&analysis, THROWER);
    assert_eq!(thrower.states_at(0x0040_10a3), [-1]);
    assert_eq!(thrower.states_at(0x0040_10b3), [0]);
    assert_eq!(thrower.states_at(0x0040_1163), [-1]);

    // An inner catch that rethrows, an outer one that returns.
    let rethrows = frame(&analysis, RETHROWS);
    assert_eq!(rethrows.states_at(0x0040_14ba), [1]);
    assert_eq!(rethrows.states_at(0x0040_1534), [2]);
    assert_eq!(rethrows.states_at(0x0040_14ea), [3]);
    assert_eq!(rethrows.states_at(0x0040_14c2), [1, 3]);

    // Code nothing reaches has no state.
    assert!(by_type.states_at(0x0040_12e3).is_empty());
}
