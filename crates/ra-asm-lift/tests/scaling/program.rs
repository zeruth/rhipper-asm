//! The synthetic program the scaling guards grow, and the guard itself: one x86-64 function of a
//! chain of `units` diamonds, each with a stack slot written and read back, a compare and a
//! branch, and a join whose registers become phis, and every fourth with a call. Every join dominates the next, so the
//! dominator tree is as deep as the function is long: a pass that walks up the dominators from
//! each instruction or block, or rescans the function per value, grows with the square of it. The
//! shared file of `ra-asm-lift`'s and `ra-asm-emit`'s `scaling` tests.

use std::time::Duration;

use ra_asm_pe::Image;

/// The diamonds of the smaller function; the larger has four times as many.
pub(crate) const UNITS: usize = 300;
/// How much longer the larger function may take than the smaller: four times the work, with the
/// slack of a debug build's allocator and caches.
pub(crate) const GROWTH: f64 = 6.0;

const IMAGE_BASE: u64 = 0x1_4000_0000;
const TEXT_RVA: u64 = 0x1000;

/// A stage of a run and how long it took.
pub(crate) type Stage = (&'static str, Duration);

/// The code of the function of `units` diamonds at the text's start, and the leaf it calls after
/// it.
fn code(units: usize) -> Vec<u8> {
    // push rbx; push rsi; sub rsp, 0x38; mov ebx, ecx; xor esi, esi
    let mut c = vec![0x53, 0x56, 0x48, 0x83, 0xEC, 0x38, 0x89, 0xCB, 0x31, 0xF6];
    let mut calls = Vec::new();
    for k in 0..units {
        // A call in every fourth diamond: the raise's work around a call is most of a diamond's,
        // and fewer of them let the chain be deeper in the same time.
        if k % 4 == 0 {
            // mov ecx, ebx; call leaf; add esi, eax
            c.extend([0x89, 0xD9, 0xE8]);
            calls.push(c.len());
            c.extend([0; 4]);
            c.extend([0x01, 0xC6]);
        }
        // mov [rsp+0x20], esi; cmp ebx, k; jle join
        c.extend([0x89, 0x74, 0x24, 0x20, 0x83, 0xFB, (k % 97) as u8, 0x7E, 10]);
        // imul ebx, ebx, 3; xor esi, k; add ebx, [rsp+0x20]
        c.extend([0x6B, 0xDB, 0x03, 0x83, 0xF6, (k % 61) as u8 + 1, 0x03, 0x5C, 0x24, 0x20]);
        // join: lea ebx, [rbx+rsi+k]
        c.extend([0x8D, 0x5C, 0x33, (k % 13) as u8 + 1]);
    }
    // mov eax, ebx; add eax, esi; add rsp, 0x38; pop rsi; pop rbx; ret
    c.extend([0x89, 0xD8, 0x01, 0xF0, 0x48, 0x83, 0xC4, 0x38, 0x5E, 0x5B, 0xC3]);
    c.resize(c.len().next_multiple_of(16), 0xCC);
    let leaf = c.len();
    // lea eax, [rcx+1]; ret
    c.extend([0x8D, 0x41, 0x01, 0xC3]);
    for at in calls {
        let rel = (leaf as i64 - (at as i64 + 4)) as i32;
        c[at..at + 4].copy_from_slice(&rel.to_le_bytes());
    }
    c
}

/// A PE32+ with one `.text` section that holds `text` at `TEXT_RVA`, the entry point its start.
fn pe_with_text(text: &[u8]) -> Vec<u8> {
    let len = text.len().next_multiple_of(0x1000);
    let mut f = vec![0u8; 0x400 + len];
    let w16 = |f: &mut Vec<u8>, at: usize, v: u16| f[at..at + 2].copy_from_slice(&v.to_le_bytes());
    let w32 = |f: &mut Vec<u8>, at: usize, v: u32| f[at..at + 4].copy_from_slice(&v.to_le_bytes());
    let w64 = |f: &mut Vec<u8>, at: usize, v: u64| f[at..at + 8].copy_from_slice(&v.to_le_bytes());
    f[0] = b'M';
    f[1] = b'Z';
    w32(&mut f, 0x3C, 0x40);
    f[0x40..0x44].copy_from_slice(b"PE\0\0");
    let coff = 0x44;
    w16(&mut f, coff, 0x8664);
    w16(&mut f, coff + 2, 1);
    w16(&mut f, coff + 16, 0xF0);
    w16(&mut f, coff + 18, 0x22);
    let opt = coff + 20;
    w16(&mut f, opt, 0x20B);
    w32(&mut f, opt + 4, len as u32);
    w32(&mut f, opt + 16, TEXT_RVA as u32);
    w32(&mut f, opt + 20, TEXT_RVA as u32);
    w64(&mut f, opt + 24, IMAGE_BASE);
    w32(&mut f, opt + 32, 0x1000);
    w32(&mut f, opt + 36, 0x200);
    w16(&mut f, opt + 40, 6);
    w16(&mut f, opt + 48, 6);
    w32(&mut f, opt + 56, TEXT_RVA as u32 + len as u32);
    w32(&mut f, opt + 60, 0x400);
    w16(&mut f, opt + 68, 3);
    w64(&mut f, opt + 72, 0x10_0000);
    w64(&mut f, opt + 80, 0x1000);
    w64(&mut f, opt + 88, 0x10_0000);
    w64(&mut f, opt + 96, 0x1000);
    w32(&mut f, opt + 108, 16);
    let sec = opt + 0xF0;
    f[sec..sec + 5].copy_from_slice(b".text");
    w32(&mut f, sec + 8, len as u32);
    w32(&mut f, sec + 12, TEXT_RVA as u32);
    w32(&mut f, sec + 16, len as u32);
    w32(&mut f, sec + 20, 0x400);
    w32(&mut f, sec + 36, 0x6000_0020);
    f[0x400..].fill(0xCC);
    f[0x400..0x400 + text.len()].copy_from_slice(text);
    f
}

/// The program of one function of `units` diamonds.
pub(crate) fn image(units: usize) -> Image {
    Image::parse(pe_with_text(&code(units))).expect("the synthetic image parses")
}

fn total(stages: &[Stage]) -> Duration {
    stages.iter().map(|s| s.1).sum()
}

fn millis(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// The least a stage of the larger function takes to be judged on its own: a shorter one is the
/// timer's and the scheduler's noise, and the total still holds it.
const JUDGED: Duration = Duration::from_millis(50);

/// How much longer the larger function's `small` and `large` took.
fn growth_of(small: Duration, large: Duration) -> f64 {
    large.as_secs_f64() / small.as_secs_f64().max(1e-3)
}

/// Runs `stages` over the function of [`UNITS`] diamonds and of four times as many, and fails
/// when the larger took more than [`GROWTH`] times as long, in all or in any stage long enough to
/// time ([`JUDGED`]): a stage that grows with the square hides in the total while the others are
/// longer. The failure names the stage that grew the most beyond linear. The smaller runs twice
/// and keeps each stage's quicker time: the first run of a process pays for its pages and the
/// allocator's growth.
///
/// It times only on CI (`CI` set, as GitHub Actions sets it): a local run shares the machine with
/// the walk's workers and builds, and a loaded run measured 6.3x to 9.8x on an unchanged raise,
/// sending back cards whose work had nothing to do with it (asm #205, #512 and #558, 2026-11-17).
pub(crate) fn guard(stages: impl Fn(&Image, usize) -> Vec<Stage>) {
    if std::env::var_os("CI").is_none() {
        eprintln!("the scaling guard times only on CI (CI unset): skipped");
        return;
    }
    let (small_image, large_image) = (image(UNITS), image(4 * UNITS));
    let first = stages(&small_image, UNITS);
    let second = stages(&small_image, UNITS);
    let small: Vec<Stage> = first.iter().zip(&second).map(|(a, b)| (a.0, a.1.min(b.1))).collect();
    let large = stages(&large_image, 4 * UNITS);
    let growth = growth_of(total(&small), total(&large));
    let stage_grew = small.iter().zip(&large).any(|(s, l)| l.1 >= JUDGED && growth_of(s.1, l.1) > GROWTH);
    if growth <= GROWTH && !stage_grew {
        return;
    }
    let beyond = |(s, l): (&Stage, &Stage)| l.1.as_secs_f64() - 4.0 * s.1.as_secs_f64();
    let worst = small.iter().zip(&large).max_by(|a, b| beyond(*a).total_cmp(&beyond(*b))).map_or("none", |(s, _)| s.0);
    let table: Vec<String> = small
        .iter()
        .zip(&large)
        .map(|(s, l)| format!("  {}: {:.0} ms to {:.0} ms ({:.1}x)", s.0, millis(s.1), millis(l.1), growth_of(s.1, l.1)))
        .collect();
    panic!(
        "{} diamonds took {growth:.1} times as long as {UNITS} ({:.0} ms to {:.0} ms), in all or in a stage more than {GROWTH}: the {worst} stage grew the most beyond linear\n{}",
        4 * UNITS,
        millis(total(&small)),
        millis(total(&large)),
        table.join("\n")
    );
}
