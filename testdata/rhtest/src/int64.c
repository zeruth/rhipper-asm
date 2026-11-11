/* int64: 64-bit integers on x86. MSVC computes them in register pairs (edx:eax) and calls the
 * CRT's helpers for what x86 has no instruction for: __allmul, __alldiv, __aulldiv, __allrem,
 * __aullrem, __alldvrm, __allshl, __allshr, __aullshr, each with a convention of its own (values
 * in edx:eax and on the stack, a shift count in cl, the callee popping its arguments). */
#include "rhtest.h"

NOINLINE i64 add64(i64 a, i64 b) { return a + b; }
NOINLINE i64 sub64(i64 a, i64 b) { return a - b; }
NOINLINE i64 neg64(i64 a) { return -a; }
NOINLINE i64 mul64(i64 a, i64 b) { return a * b; }                  /* __allmul */
NOINLINE u64 mul32x32(u32 a, u32 b) { return (u64)a * b; }          /* mul, inline */
NOINLINE i64 imul32x32(i32 a, i32 b) { return (i64)a * b; }         /* imul, inline */
NOINLINE i64 sdiv64(i64 a, i64 b) { return a / b; }                 /* __alldiv */
NOINLINE u64 udiv64(u64 a, u64 b) { return a / b; }                 /* __aulldiv */
NOINLINE i64 smod64(i64 a, i64 b) { return a % b; }                 /* __allrem */
NOINLINE u64 umod64(u64 a, u64 b) { return a % b; }                 /* __aullrem */
NOINLINE i64 divmod64(i64 a, i64 b) { return a / b * 1000 + a % b; } /* __alldvrm */
NOINLINE i64 shl64(i64 a, int n) { return a << n; }                 /* __allshl */
NOINLINE i64 sar64(i64 a, int n) { return a >> n; }                 /* __allshr */
NOINLINE u64 shr64(u64 a, int n) { return a >> n; }                 /* __aullshr */
NOINLINE u64 shifts_const(u64 a) { return (a << 13) ^ (a >> 7) ^ (a << 40) ^ (a >> 33); }
NOINLINE i64 sext(i32 a) { return a; }
NOINLINE u64 zext(u32 a) { return a; }
NOINLINE u32 lo32(u64 a) { return (u32)a; }
NOINLINE u32 hi32(u64 a) { return (u32)(a >> 32); }
NOINLINE u64 make64(u32 hi, u32 lo) { return (u64)hi << 32 | lo; }

/* comparisons of pairs: signed and unsigned, equal halves, across the sign */
NOINLINE u32 cmp64(i64 a, i64 b) {
    return (u32)(a < b) | (u32)(a <= b) << 1 | (u32)((u64)a < (u64)b) << 2 | (u32)(a == b) << 3 |
           (u32)(a > b) << 4 | (u32)((u64)a >= (u64)b) << 5;
}

NOINLINE i64 min64(i64 a, i64 b) { return a < b ? a : b; }
NOINLINE u64 abs64(i64 a) { return (u64)(a < 0 ? -a : a); }

NOINLINE u64 fib64(u32 n) {
    u64 a = 0, b = 1;
    while (n--) {
        u64 t = a + b;
        a = b;
        b = t;
    }
    return a;
}

NOINLINE u64 gcd64(u64 a, u64 b) {
    while (b) {
        u64 t = a % b;
        a = b;
        b = t;
    }
    return a;
}

NOINLINE u32 popcount64(u64 v) {
    u32 c = 0;
    while (v) {
        v &= v - 1;
        c++;
    }
    return c;
}

/* a 64-bit LCG, the multiply and the add carrying between halves */
NOINLINE u64 lcg64(u64 *state) {
    *state = *state * 6364136223846793005ull + 1442695040888963407ull;
    return *state;
}

NOINLINE u64 sum_lcg(u32 n) {
    u64 st = seed(), h = 0;
    u32 k;
    for (k = 0; k < n; k++) h ^= lcg64(&st) >> (k & 31);
    return h;
}

/* 64-bit values through memory: an array, a struct */
typedef struct {
    u64 key;
    i32 tag;
    i64 value;
} Rec;

NOINLINE i64 rec_sum(const Rec *r, int n) {
    i64 s = 0;
    int k;
    for (k = 0; k < n; k++) s += r[k].tag < 0 ? -r[k].value : r[k].value + (i64)r[k].key;
    return s;
}

NOINLINE u64 rec_check(void) {
    Rec r[4];
    int k;
    for (k = 0; k < 4; k++) {
        r[k].key = (u64)seed() << (8 * k);
        r[k].tag = k - 2;
        r[k].value = -(i64)seed() * 1000000007 * (k + 1);
    }
    return (u64)rec_sum(r, 4);
}

/* a 64-bit parameter between 32-bit ones: MSVC packs it at [esp + 8], 4-aligned. A library
 * signature Ghidra lays out puts it at 12, aligned to 8, and every caller's arguments read a word
 * off (the CRT's __lseeki64_nolock(fh, 0, SEEK_END) as (fh, 0x200000000, ...), a VS2010 client) */
NOINLINE i64 seek64(i32 fh, i64 offset, i32 origin) { return offset * 3 + fh * 100 + origin; }
NOINLINE i64 seeks(u32 s) {
    return seek64((i32)s, 0, 2) + seek64(1, -(i64)s << 33, 0) + seek64(-(i32)s, 0x123456789ll, (i32)s);
}

void rhtest_run(void) {
    u32 s = seed();   /* 7 */
    i64 big = (i64)s * 0x123456789ll;          /* 0x7f6e5d4c3f */
    i64 neg = -(i64)s * 0x1000000001ll;
    check64("add64", (u64)add64(big, (i64)0xffffffff));
    check64("sub64", (u64)sub64((i64)s, big));
    check64("neg64", (u64)neg64(big));
    check64("mul64", (u64)mul64(big, neg));
    check64("mul32x32", mul32x32(0xfffffff0u + s, 0x87654321u));
    check64("imul32x32", (u64)imul32x32(-(i32)s * 100000, 300000));
    check64("sdiv64", (u64)sdiv64(neg, (i64)s * 3));
    check64("udiv64", udiv64((u64)neg, (u64)big));
    check64("smod64", (u64)smod64(neg, 1000003));
    check64("umod64", umod64((u64)neg, 1000003));
    check64("divmod64", (u64)divmod64(big * 31, -(i64)s * 77));
    check64("shl64", (u64)shl64(big, (int)s * 5));
    check64("sar64", (u64)sar64(neg, (int)s * 5));
    check64("shr64", shr64((u64)neg, (int)s * 5));
    check64("shl64_32", (u64)shl64(big, 32 + (int)s));
    check64("shr64_32", shr64((u64)neg, 32 + (int)s));
    check64("shifts_const", shifts_const((u64)big ^ 0xa5a5a5a5a5a5a5a5ull));
    check64("sext", (u64)sext(-(i32)s));
    check64("zext", zext((u32)-(i32)s));
    check("lo32", lo32((u64)neg));
    check("hi32", hi32((u64)neg));
    check64("make64", make64(s * 0x01010101u, 0xdeadbeefu));
    check("cmp64_a", cmp64(neg, big));
    check("cmp64_b", cmp64(big, big));
    check("cmp64_c", cmp64(make64(1, 0), make64(0, 0xffffffffu)));
    check("cmp64_d", cmp64(-1, 1));
    check64("min64", (u64)min64(big, neg));
    check64("abs64", abs64(neg));
    check64("fib64", fib64(s * 13));
    check64("gcd64", gcd64((u64)s * 0x10000000000ull * 6, (u64)s * 0x10000000000ull * 15 + 0));
    check("popcount64", popcount64((u64)neg));
    check64("sum_lcg", sum_lcg(s * 5));
    check64("rec", rec_check());
    check64("seeks", (u64)seeks(s));
    /* (shift counts of 64 or more: C leaves them undefined, x86's helpers decide, `cmp cl, 64; jae`
       to 0 or the sign, where an x64 shift masks the count to 6 bits; and 32..63, the helpers'
       other half) */
    check64("shl64_big", (u64)shl64((i64)s * 0x10001, (int)s + 58));
    check64("sar64_big", (u64)sar64(-(i64)s * 0x10001, (int)s + 60));
    check64("shr64_big", shr64((u64)s * 0x9e3779b97f4a7c15ull, (int)s + 93));
    check64("shl64_mid", (u64)shl64((i64)s * 0x10001, (int)s + 30));
    check64("shr64_mid", shr64((u64)s * 0x9e3779b97f4a7c15ull, (int)s + 33));
}
