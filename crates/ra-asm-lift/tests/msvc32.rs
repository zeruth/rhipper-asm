//! What MSVC's 32-bit code does past the conventions, in small images of its own bytes:
//! - the frame helpers a function calls to build and take down its frame (`__SEH_prolog4`,
//!   `__SEH_epilog4`), each of which moves the caller's `esp` and `ebp`, which a call to a
//!   function of its own cannot: the lift copies them into the caller, and the caller reads its
//!   arguments and locals through the `ebp` the prolog set;
//! - a static function's arguments in registers of the compiler's choosing (`edi`, `esi`), which
//!   its callers set and it reads, while the `push esi` of another function only saves it.

/// The image's base and the addresses of its pieces, all in one section at RVA 0x1000.
const BASE: u64 = 0x40_0000;
const CODE: u64 = BASE + 0x1000;
const MAIN: u64 = CODE;
const F: u64 = CODE + 0x10;
const PROLOG: u64 = CODE + 0x80;
const EPILOG: u64 = CODE + 0xd0;
const HANDLER: u64 = CODE + 0xf0;
const SCOPES: u64 = CODE + 0x100;
const COOKIE: u64 = CODE + 0x180;

fn le(v: u64) -> [u8; 4] {
    (v as u32).to_le_bytes()
}

/// `call to` at `at`, five bytes.
fn call_to(at: u64, to: u64) -> Vec<u8> {
    let mut out = vec![0xe8];
    out.extend_from_slice(&((to as i64 - (at as i64 + 5)) as i32).to_le_bytes());
    out
}

/// The code and the data: `main` calls `f(41)`; `f` builds its frame with `__SEH_prolog4` (twelve
/// bytes of locals), adds one to its argument through a local, and takes the frame down with
/// `__SEH_epilog4`; the helpers are the runtime's own bytes.
fn section() -> Vec<u8> {
    let mut s = vec![0xccu8; 0x200];
    let mut put = |at: u64, bytes: &[u8]| {
        let o = (at - CODE) as usize;
        s[o..o + bytes.len()].copy_from_slice(bytes);
    };
    // main: push 41; call f; add esp, 4; ret
    let mut main = vec![0x6a, 41];
    main.extend(call_to(MAIN + 2, F));
    main.extend([0x83, 0xc4, 0x04, 0xc3]);
    put(MAIN, &main);
    // f: push 12; push scopes; call prolog; mov eax, [ebp+8]; inc eax; mov [ebp-0x1c], eax;
    // mov eax, [ebp-0x1c]; call epilog; ret
    let mut f = vec![0x6a, 0x0c, 0x68];
    f.extend(le(SCOPES));
    f.extend(call_to(F + 7, PROLOG));
    f.extend([0x8b, 0x45, 0x08, 0x40, 0x89, 0x45, 0xe4, 0x8b, 0x45, 0xe4]);
    let at = F + f.len() as u64;
    f.extend(call_to(at, EPILOG));
    f.push(0xc3);
    put(F, &f);
    let mut prolog = vec![0x68];
    prolog.extend(le(HANDLER));
    prolog.extend([0x64, 0xff, 0x35, 0, 0, 0, 0, 0x8b, 0x44, 0x24, 0x10, 0x89, 0x6c, 0x24, 0x10, 0x8d, 0x6c, 0x24, 0x10, 0x2b, 0xe0]);
    prolog.extend([0x53, 0x56, 0x57, 0xa1]);
    prolog.extend(le(COOKIE));
    prolog.extend([0x31, 0x45, 0xfc, 0x33, 0xc5, 0x50, 0x89, 0x65, 0xe8, 0xff, 0x75, 0xf8, 0x8b, 0x45, 0xfc]);
    prolog.extend([0xc7, 0x45, 0xfc, 0xfe, 0xff, 0xff, 0xff, 0x89, 0x45, 0xf8, 0x8d, 0x45, 0xf0, 0x64, 0xa3, 0, 0, 0, 0, 0xc3]);
    put(PROLOG, &prolog);
    put(EPILOG, &[0x8b, 0x4d, 0xf0, 0x64, 0x89, 0x0d, 0, 0, 0, 0, 0x59, 0x5f, 0x5f, 0x5e, 0x5b, 0x8b, 0xe5, 0x5d, 0x51, 0xc3]);
    put(HANDLER, &[0xc3]);
    put(SCOPES, &[0xfe, 0xff, 0xff, 0xff, 0, 0, 0, 0, 0xfe, 0xff, 0xff, 0xff, 0, 0, 0, 0]);
    put(COOKIE, &le(0xbb40_e64e));
    s
}

/// The data section's address.
const DATA: u64 = BASE + 0x2000;

/// A PE32 with the code section, read, written and run, `main` its entry, and an empty data
/// section after it.
fn image(section: &[u8]) -> Vec<u8> {
    image_with_data(section, &[0; 0x200])
}

/// A PE32 with the code section, read, written and run, `main` its entry, and `data` in a data
/// section at [`DATA`].
fn image_with_data(section: &[u8], data: &[u8]) -> Vec<u8> {
    let mut f = vec![0u8; 0x600];
    let w16 = |f: &mut Vec<u8>, at: usize, v: u16| f[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let w32 = |f: &mut Vec<u8>, at: usize, v: u32| f[at..at + 4].copy_from_slice(&v.to_le_bytes());
    f[0] = b'M';
    f[1] = b'Z';
    w32(&mut f, 0x3c, 0x40);
    f[0x40..0x44].copy_from_slice(b"PE\0\0");
    let coff = 0x44;
    w16(&mut f, coff, 0x14c);
    w16(&mut f, coff + 2, 2);
    w16(&mut f, coff + 16, 0xe0);
    w16(&mut f, coff + 18, 0x0102);
    let opt = coff + 20;
    w16(&mut f, opt, 0x10b);
    w32(&mut f, opt + 4, 0x200);
    w32(&mut f, opt + 16, (MAIN - BASE) as u32);
    w32(&mut f, opt + 20, 0x1000);
    w32(&mut f, opt + 24, 0x1000);
    w32(&mut f, opt + 28, BASE as u32);
    w32(&mut f, opt + 32, 0x1000);
    w32(&mut f, opt + 36, 0x200);
    w16(&mut f, opt + 40, 6);
    w16(&mut f, opt + 48, 6);
    w32(&mut f, opt + 56, 0x3000);
    w32(&mut f, opt + 60, 0x200);
    w16(&mut f, opt + 68, 3);
    w32(&mut f, opt + 72, 0x10_0000);
    w32(&mut f, opt + 76, 0x1000);
    w32(&mut f, opt + 80, 0x10_0000);
    w32(&mut f, opt + 84, 0x1000);
    w32(&mut f, opt + 92, 16);
    let sec = opt + 0xe0;
    f[sec..sec + 5].copy_from_slice(b".text");
    w32(&mut f, sec + 8, 0x1000);
    w32(&mut f, sec + 12, 0x1000);
    w32(&mut f, sec + 16, 0x200);
    w32(&mut f, sec + 20, 0x200);
    w32(&mut f, sec + 36, 0xe000_0060);
    f[0x200..0x400].copy_from_slice(section);
    let sec = sec + 40;
    f[sec..sec + 5].copy_from_slice(b".data");
    w32(&mut f, sec + 8, 0x1000);
    w32(&mut f, sec + 12, (DATA - BASE) as u32);
    w32(&mut f, sec + 16, 0x200);
    w32(&mut f, sec + 20, 0x400);
    w32(&mut f, sec + 36, 0xc000_0040);
    f[0x400..0x600].copy_from_slice(data);
    f
}

type Signatures = std::collections::HashMap<String, ra_asm_lift::raise::abi::Signature>;

/// `section` as an image, lifted and raised to the ABI level.
fn raised(section: &[u8]) -> (ra_asm_pe::Image, ra_asm_lift::Lifted, Signatures) {
    let image = ra_asm_pe::Image::parse(image(section)).expect("the image parses");
    let analysis = ra_asm_analysis::analyze(&image);
    let options = ra_asm_lift::LiftOptions { standalone: true, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    (image, lifted, signatures)
}

/// The function at `address` called through the interpreter with `stack` as its stack arguments
/// and zero in each register it takes (none of which these functions mean to read): its result.
fn call(image: &ra_asm_pe::Image, lifted: &ra_asm_lift::Lifted, signatures: &Signatures, address: u64, stack: Vec<u64>) -> u32 {
    let function = lifted.functions.get(&address).cloned().expect("the function is lifted");
    let mut args = vec![0; signatures[&function].params.len()];
    args.extend(stack);
    let call = ra_asm_lift::run::Call { function: function.clone(), args };
    let (value, outcome) = ra_asm_lift::run::call(image, lifted, signatures, &call, &ra_asm_lift::run::Options::default()).unwrap();
    assert_eq!(outcome.stop, None, "{function} stopped:\n{}", lifted.module.to_ll());
    value as u32
}

/// `f(41)` through the interpreter at the ABI level: 42, read through the frame the prolog built.
#[test]
fn a_function_framed_by_the_seh_helpers_reads_its_argument_through_ebp() {
    let (image, lifted, signatures) = raised(&section());
    assert_eq!(call(&image, &lifted, &signatures, F, vec![41]), 42, "f(41)\n{}", lifted.module.to_ll());
}

/// `main` points `edi` at a local and calls `set`, which stores 7 through `edi` (MSVC's private
/// convention for a static function), then calls `five`, which saves `esi`, puts 5 in it and
/// returns it, and puts `esi` back: main returns 7 + 5. `set` takes `edi`; `five` takes nothing,
/// since its `push esi` only saves the register.
#[test]
fn a_static_function_takes_its_argument_in_the_register_its_callers_set() {
    const SET: u64 = CODE + 0x40;
    const FIVE: u64 = CODE + 0x60;
    let mut s = vec![0xccu8; 0x200];
    let mut put = |at: u64, bytes: &[u8]| {
        let o = (at - CODE) as usize;
        s[o..o + bytes.len()].copy_from_slice(bytes);
    };
    // push esi; push edi; sub esp, 4; mov edi, esp; call set; call five; add eax, [edi];
    // add esp, 4; pop edi; pop esi; ret
    let mut main = vec![0x56, 0x57, 0x83, 0xec, 0x04, 0x8b, 0xfc];
    main.extend(call_to(MAIN + 7, SET));
    main.extend(call_to(MAIN + 12, FIVE));
    main.extend([0x03, 0x07, 0x83, 0xc4, 0x04, 0x5f, 0x5e, 0xc3]);
    put(MAIN, &main);
    // mov dword ptr [edi], 7; ret
    put(SET, &[0xc7, 0x07, 0x07, 0, 0, 0, 0xc3]);
    // push esi; mov esi, 5; mov eax, esi; pop esi; ret
    put(FIVE, &[0x56, 0xbe, 0x05, 0, 0, 0, 0x8b, 0xc6, 0x5e, 0xc3]);
    let (image, lifted, signatures) = raised(&s);
    let signature = |address: u64| &signatures[&lifted.functions[&address]];
    // ecx, edx, eax, ebx, esi, then edi.
    assert_eq!(signature(SET).params.len(), 6, "set reads edi:\n{}", lifted.module.to_ll());
    assert!(signature(FIVE).params.is_empty(), "five only saves esi:\n{}", lifted.module.to_ll());
    assert_eq!(call(&image, &lifted, &signatures, MAIN, Vec::new()), 12, "main()\n{}", lifted.module.to_ll());
}

/// main sets `esi` in its first block and calls, from the next, `room` (whose `push eax` only
/// makes room for a local, though main has just set `eax`) and `inc_esi` (which reads the `esi`
/// main set a block before): main returns 3 + 31. `room` takes nothing; `inc_esi` takes `esi`.
#[test]
fn a_register_set_blocks_before_the_call_is_an_argument_and_a_push_that_makes_room_is_none() {
    const ROOM: u64 = CODE + 0x40;
    const INC_ESI: u64 = CODE + 0x60;
    let mut s = vec![0xccu8; 0x200];
    let mut put = |at: u64, bytes: &[u8]| {
        let o = (at - CODE) as usize;
        s[o..o + bytes.len()].copy_from_slice(bytes);
    };
    // push esi; mov esi, 30; jmp L; L: mov eax, 7; call room; mov ecx, eax; call inc_esi;
    // add eax, ecx; pop esi; ret
    let mut main = vec![0x56, 0xbe, 30, 0, 0, 0, 0xeb, 0x00, 0xb8, 7, 0, 0, 0];
    main.extend(call_to(MAIN + main.len() as u64, ROOM));
    main.extend([0x8b, 0xc8]);
    main.extend(call_to(MAIN + main.len() as u64, INC_ESI));
    main.extend([0x03, 0xc1, 0x5e, 0xc3]);
    put(MAIN, &main);
    // push eax; mov dword ptr [esp], 3; mov eax, [esp]; pop ecx; ret
    put(ROOM, &[0x50, 0xc7, 0x04, 0x24, 3, 0, 0, 0, 0x8b, 0x04, 0x24, 0x59, 0xc3]);
    // lea eax, [esi+1]; ret
    put(INC_ESI, &[0x8d, 0x46, 0x01, 0xc3]);
    let (image, lifted, signatures) = raised(&s);
    let signature = |address: u64| &signatures[&lifted.functions[&address]];
    assert!(signature(ROOM).params.is_empty(), "room's push only makes room:\n{}", lifted.module.to_ll());
    // ecx, edx, eax, ebx, then esi.
    assert_eq!(signature(INC_ESI).params.len(), 5, "inc_esi reads esi:\n{}", lifted.module.to_ll());
    assert_eq!(call(&image, &lifted, &signatures, MAIN, Vec::new()), 34, "main()\n{}", lifted.module.to_ll());
}

/// The jump tables of the C runtime's `memcpy`, which no `cmp` bounds from zero: `and eax, 3`
/// before the read (the entry for 0 is no code: that path has ruled it out), `sub ecx, 4` whose
/// borrow jumps to `jmp [ecx*4 + table + 16]` (the index is -4 to -1), and `neg ecx` after the
/// compare (the entries count down from the table's address). main returns a(2) + b(2) + c(1).
#[test]
fn jump_tables_indexed_by_a_mask_a_borrow_and_a_negation_resolve() {
    const FA: u64 = CODE + 0x40;
    const FB: u64 = CODE + 0x80;
    const FC: u64 = CODE + 0xc0;
    const TA: u64 = DATA;
    const TB: u64 = DATA + 0x20;
    const T: u64 = DATA + 0x40;
    let mut s = vec![0xccu8; 0x200];
    let mut data = vec![0u8; 0x200];
    let mut put = |at: u64, bytes: &[u8]| {
        let o = (at - CODE) as usize;
        s[o..o + bytes.len()].copy_from_slice(bytes);
    };
    // push 2; call a; add esp, 4; mov esi, eax; push 2; call b; add esp, 4; add esi, eax;
    // push 1; call c; add esp, 4; add eax, esi; ret
    let mut main = Vec::new();
    for (arg, f, tail) in [(2u8, FA, [0x8b, 0xf0]), (2, FB, [0x03, 0xf0]), (1, FC, [0x03, 0xc6])] {
        main.extend([0x6a, arg]);
        main.extend(call_to(MAIN + main.len() as u64, f));
        main.extend([0x83, 0xc4, 0x04]);
        main.extend(tail);
    }
    main.push(0xc3);
    put(MAIN, &main);
    // Each function's four returns of 0, 10, 20, 30, from 0x20 past its start.
    let returns = |f: u64| -> [u64; 4] { std::array::from_fn(|k| f + 0x20 + 8 * k as u64) };
    for f in [FA, FB, FC] {
        for (k, at) in returns(f).into_iter().enumerate() {
            put(at, &[0xb8, (10 * k) as u8, 0, 0, 0, 0xc3]);
        }
    }
    // a: mov eax, [esp+4]; and eax, 3; jmp [eax*4 + TA]
    let mut a = vec![0x8b, 0x44, 0x24, 0x04, 0x83, 0xe0, 0x03, 0xff, 0x24, 0x85];
    a.extend(le(TA));
    put(FA, &a);
    // b: mov ecx, [esp+4]; sub ecx, 4; jb L; mov eax, 99; ret; L: jmp [ecx*4 + TB + 16]
    let mut b = vec![0x8b, 0x4c, 0x24, 0x04, 0x83, 0xe9, 0x04, 0x72, 0x06, 0xb8, 99, 0, 0, 0, 0xc3, 0xff, 0x24, 0x8d];
    b.extend(le(TB + 16));
    put(FB, &b);
    // c: mov ecx, [esp+4]; cmp ecx, 4; jb L; mov eax, 99; ret; L: neg ecx; jmp [ecx*4 + T + 12]
    let mut c = vec![0x8b, 0x4c, 0x24, 0x04, 0x83, 0xf9, 0x04, 0x72, 0x06, 0xb8, 99, 0, 0, 0, 0xc3, 0xf7, 0xd9, 0xff, 0x24, 0x8d];
    c.extend(le(T + 12));
    put(FC, &c);
    let mut word = |at: u64, value: u64| {
        let o = (at - DATA) as usize;
        data[o..o + 4].copy_from_slice(&le(value));
    };
    let (ra, rb, rc) = (returns(FA), returns(FB), returns(FC));
    for k in 1..4 {
        word(TA + 4 * k as u64, ra[k]);
    }
    for k in 0..4 {
        word(TB + 4 * k as u64, rb[k]);
        word(T + 4 * k as u64, rc[3 - k]);
    }
    let image = ra_asm_pe::Image::parse(image_with_data(&s, &data)).expect("the image parses");
    let analysis = ra_asm_analysis::analyze(&image);
    assert!(analysis.warnings.iter().all(|w| !w.contains("did not resolve")), "{:#?}", analysis.warnings);
    let options = ra_asm_lift::LiftOptions { standalone: true, x87: ra_asm_lift::X87Unit::Double };
    let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
    let options = lifted.raise_options(ra_asm_lift::RaiseLevel::Abi);
    let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
    lifted.apply_names(&analysis, Some(&mut signatures));
    assert_eq!(call(&image, &lifted, &signatures, MAIN, Vec::new()), 20 + 20 + 10, "main()\n{}", lifted.module.to_ll());
}

/// In an image with relocations, `cmp eax, imm` with a constant that falls inside a function
/// (a four-character code, in ffxiv.exe) names no function: only a relocated immediate (`push
/// offset table` here) is an address. The immediates are seeds before the pointers in data, so
/// a function seeded at the constant came first and cut `three` (which only the table names) in
/// two, and the half after the cut ran as a function of its own.
#[test]
fn an_immediate_no_relocation_marks_names_no_function() {
    const THREE: u64 = CODE + 0x40;
    const INSIDE: u64 = THREE + 5;
    const TABLE: u64 = DATA;
    let mut s = vec![0xccu8; 0x200];
    let mut put = |at: u64, bytes: &[u8]| {
        let o = (at - CODE) as usize;
        s[o..o + bytes.len()].copy_from_slice(bytes);
    };
    // cmp eax, INSIDE; push offset TABLE; add esp, 4; ret
    let mut main = vec![0x3d];
    main.extend(le(INSIDE));
    main.push(0x68);
    main.extend(le(TABLE));
    main.extend([0x83, 0xc4, 0x04, 0xc3]);
    put(MAIN, &main);
    // mov eax, 1; add eax, 2; ret
    put(THREE, &[0xb8, 1, 0, 0, 0, 0x83, 0xc0, 0x02, 0xc3]);
    let mut data = vec![0u8; 0x200];
    data[..4].copy_from_slice(&le(THREE));
    let mut image = ra_asm_pe::Image::parse(image_with_data(&s, &data)).expect("the image parses");
    let relocation = |va: u64| ra_asm_pe::relocs::Relocation { va, kind: ra_asm_pe::relocs::RelocKind::HighLow };
    image.relocations = vec![relocation(MAIN + 6), relocation(TABLE)];
    let analysis = ra_asm_analysis::analyze(&image);
    assert!(analysis.function(INSIDE).is_none(), "a function at the constant {INSIDE:#x}");
    let three = analysis.function(THREE).expect("three is a function");
    assert!(three.blocks.iter().any(|b| b.instrs.iter().any(|i| i.address == INSIDE)), "three keeps its add: {:?}", three.blocks);
}
