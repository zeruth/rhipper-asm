//! The MSVC frame helpers a 32-bit function calls to build and take down its frame
//! (`__SEH_prolog4`, `__SEH_epilog4`): each moves the caller's `esp` and `ebp`, which a call to a
//! function of its own cannot. The lift copies them into the caller, and the caller reads its
//! arguments and locals through the `ebp` the prolog set.

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
fn call(at: u64, to: u64) -> Vec<u8> {
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
    main.extend(call(MAIN + 2, F));
    main.extend([0x83, 0xc4, 0x04, 0xc3]);
    put(MAIN, &main);
    // f: push 12; push scopes; call prolog; mov eax, [ebp+8]; inc eax; mov [ebp-0x1c], eax;
    // mov eax, [ebp-0x1c]; call epilog; ret
    let mut f = vec![0x6a, 0x0c, 0x68];
    f.extend(le(SCOPES));
    f.extend(call(F + 7, PROLOG));
    f.extend([0x8b, 0x45, 0x08, 0x40, 0x89, 0x45, 0xe4, 0x8b, 0x45, 0xe4]);
    let at = F + f.len() as u64;
    f.extend(call(at, EPILOG));
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

/// A PE32 with the one section, read, written and run, and `main` its entry.
fn image() -> Vec<u8> {
    let mut f = vec![0u8; 0x400];
    let w16 = |f: &mut Vec<u8>, at: usize, v: u16| f[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let w32 = |f: &mut Vec<u8>, at: usize, v: u32| f[at..at + 4].copy_from_slice(&v.to_le_bytes());
    f[0] = b'M';
    f[1] = b'Z';
    w32(&mut f, 0x3c, 0x40);
    f[0x40..0x44].copy_from_slice(b"PE\0\0");
    let coff = 0x44;
    w16(&mut f, coff, 0x14c);
    w16(&mut f, coff + 2, 1);
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
    w32(&mut f, opt + 56, 0x2000);
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
    f[0x200..0x400].copy_from_slice(&section());
    f
}

/// `f(41)` through the interpreter at the ABI level: 42, read through the frame the prolog built.
#[test]
fn a_function_framed_by_the_seh_helpers_reads_its_argument_through_ebp() {
    let image = ra_asm_pe::Image::parse(image()).expect("the image parses");
    let analysis = ra_asm_analysis::analyze(&image);
    for level in [ra_asm_lift::RaiseLevel::Abi] {
        let options = ra_asm_lift::LiftOptions { standalone: level == ra_asm_lift::RaiseLevel::Abi, x87: ra_asm_lift::X87Unit::Double };
        let mut lifted = ra_asm_lift::lift_with(&image, &analysis, options);
        let options = lifted.raise_options(level);
        let mut signatures = ra_asm_lift::raise(&mut lifted.module, &lifted.roots, options);
        lifted.apply_names(&analysis, Some(&mut signatures));
        let function = lifted.functions.get(&F).cloned().expect("f is lifted");
        let call = ra_asm_lift::run::Call { function, args: vec![41] };
        let (value, outcome) = ra_asm_lift::run::call(&image, &lifted, &signatures, &call, &ra_asm_lift::run::Options::default()).unwrap();
        assert_eq!(outcome.stop, None, "{level:?}: f(41) stopped");
        assert_eq!(value as u32, 42, "{level:?}: f(41)\n{}", lifted.module.to_ll());
    }
}
