// The host of a lifted program. It maps the image at its own base, binds the import slots to the
// real functions, and calls the lifted entry point.
//
//   clang -O1 program.ll driver.c -o lifted.exe      (image.h beside program.ll)
//   lifted.exe image.bin
//
// Two modes, by image.h. Without RA_ABI (the machine IR), every lifted function takes a machine
// state: the driver gives it a stack and the runtime resolves indirect calls through the function
// table. With RA_ABI (the raised IR), every lifted function has its own parameters and result: the
// driver writes a trampoline at each function's image address, so a pointer into the image, held by
// lifted or native code, reaches the host function.
//
// The runtime functions the lifted code calls: ra_trap, ra_unsupported and ra_call.
#include <float.h>
#include <intrin.h>
#include <setjmp.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <windows.h>

#include "image.h"

// The machine state, as %State in the IR: 16 general registers, rip, 8 flag bytes, 16 vectors.
struct State {
    uint64_t gpr[16];
    uint64_t rip;
    uint8_t flags[8];
    __declspec(align(16)) uint8_t xmm[16][16];
};

enum { RAX, RCX, RDX, RBX, RSP, RBP, RSI, RDI, R8, R9 };

// The lifted functions, by address: the lift writes the table.
extern const struct { uint64_t address; void (*function)(void *); } ra_functions[];
extern const uint64_t ra_function_count;

static const char *trap_name(uint64_t code) {
    switch (code) {
    case 1: return "int3";
    case 2: return "ud2";
    case 3: return "hlt";
    case 4: return "division by zero";
    case 5: return "128-bit division";
    case 6: return "__fastfail";
    case 7: return "a switch to an address outside its table";
    case 8: return "bytes that are no instruction";
    case 9: return "a software interrupt";
    default: return "a trap";
    }
}

__declspec(noreturn) void ra_trap(uint64_t address, uint64_t code) {
    fprintf(stderr, "ra: %s at 0x%llx\n", trap_name(code), (unsigned long long)address);
    fflush(stderr);
    ExitProcess(0xE0000001);
}

// The entry hook of a port built with RA_TRACE: each function names itself as it starts.
void ra_enter(const char *name) {
    fputs("> ", stderr);
    fputs(name, stderr);
    fputs("\n", stderr);
    fflush(stderr);
}

__declspec(noreturn) void ra_unsupported(uint64_t address, uint64_t code) {
    fprintf(stderr, "ra: an instruction the lift does not cover at 0x%llx (code %llu)\n", (unsigned long long)address, (unsigned long long)code);
    fflush(stderr);
    ExitProcess(0xE0000002);
}

// The processor's answers, from the host: the port runs on a processor at least as capable as
// the one the program asks about.
void ra_cpuid(uint64_t leaf, uint64_t subleaf, void *out) {
    int regs[4];
    __cpuidex(regs, (int)leaf, (int)subleaf);
    memcpy(out, regs, sizeof regs);
}

uint64_t ra_xgetbv(uint64_t which) {
    return _xgetbv((unsigned)which);
}

uint64_t ra_rdtsc(void) {
    return __rdtsc();
}

typedef uint64_t (*native8)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);

// The C runtime functions a 32-bit program calls that walk or hand back a table of its own
// pointers. A pointer the program holds is a dword, and a function it names is its universal
// entry point, which takes eight integers and ignores the ones it does not need; the host's own
// versions would read the tables with 64-bit strides.
static uint64_t call_universal(uint32_t function) {
    native8 f = (native8)(uintptr_t)function;
    return f(0, 0, 0, 0, 0, 0, 0, 0);
}

uint64_t ra_x86_initterm(uint64_t first, uint64_t last) {
    for (uint32_t *p = (uint32_t *)(uintptr_t)first; (uintptr_t)p < (uintptr_t)last; p++) {
        if (*p) call_universal(*p);
    }
    return 0;
}

uint64_t ra_x86_initterm_e(uint64_t first, uint64_t last) {
    for (uint32_t *p = (uint32_t *)(uintptr_t)first; (uintptr_t)p < (uintptr_t)last; p++) {
        if (*p) {
            uint32_t result = (uint32_t)call_universal(*p);
            if (result) return result;
        }
    }
    return 0;
}

// The onexit tables: the program's structure of three dwords names a table; its functions live
// here, by the table's address.
struct onexit_table { uint32_t address; uint32_t count; uint32_t functions[256]; };
static struct onexit_table onexit_tables[8];

static struct onexit_table *onexit_table_of(uint64_t table, int create) {
    for (int i = 0; i < 8; i++) {
        if (onexit_tables[i].address == (uint32_t)table) return &onexit_tables[i];
    }
    if (!create) return NULL;
    for (int i = 0; i < 8; i++) {
        if (onexit_tables[i].address == 0) {
            onexit_tables[i].address = (uint32_t)table;
            onexit_tables[i].count = 0;
            return &onexit_tables[i];
        }
    }
    return NULL;
}

uint64_t ra_x86_initialize_onexit_table(uint64_t table) {
    if (!table) return (uint64_t)-1;
    memset((void *)(uintptr_t)table, 0, 12);
    struct onexit_table *t = onexit_table_of(table, 1);
    if (t) t->count = 0;
    return 0;
}

uint64_t ra_x86_register_onexit_function(uint64_t table, uint64_t function) {
    struct onexit_table *t = onexit_table_of(table, 1);
    if (!t || t->count >= 256) return (uint64_t)-1;
    t->functions[t->count++] = (uint32_t)function;
    return 0;
}

uint64_t ra_x86_execute_onexit_table(uint64_t table) {
    struct onexit_table *t = onexit_table_of(table, 0);
    if (!t) return 0;
    while (t->count > 0) {
        uint32_t function = t->functions[--t->count];
        if (function) call_universal(function);
    }
    return 0;
}

// The program's singly linked list head is 8 bytes, the host's 16, and the host's own function
// refuses a head that is not 16-byte aligned: the empty list is zeros.
uint64_t ra_x86_initialize_slist_head(uint64_t head) {
    memset((void *)(uintptr_t)head, 0, 8);
    return 0;
}

// _controlfp: the host's word has no x87 precision control, which the x86 C runtime reports as
// _PC_53 by default and Direct3D sets to _PC_24; the shim keeps that field and leaves the rest
// (the rounding mode, the exception masks, the denormal control) to the host.
static unsigned ra_precision = 0x10000;

uint64_t ra_x86_controlfp(uint64_t new_bits, uint64_t mask) {
    unsigned pc = (unsigned)mask & 0x30000u;
    if (pc) ra_precision = (ra_precision & ~pc) | ((unsigned)new_bits & pc);
    unsigned host = 0;
    _controlfp_s(&host, (unsigned)new_bits & ~0x30000u, (unsigned)mask & ~0x30000u);
    return (host & ~0x30000u) | ra_precision;
}

// setjmp and longjmp: the program's jmp_buf is a key, and the host's buffer lives in a table slot
// found by it. setjmp itself runs in the lifted function, through the macro in rhipper.h, so the
// frame it saves is alive when longjmp comes back to it.
static struct { uint64_t key; jmp_buf buf; } ra_jumps[16];

jmp_buf *ra_jmp_slot(uint64_t buffer) {
    for (int k = 0; k < 16; k++) if (ra_jumps[k].key == buffer) return &ra_jumps[k].buf;
    for (int k = 0; k < 16; k++) if (ra_jumps[k].key == 0) { ra_jumps[k].key = buffer; return &ra_jumps[k].buf; }
    fprintf(stderr, "ra: more than 16 jmp_bufs in use\n");
    fflush(stderr);
    ExitProcess(0xE0000006);
}

uint64_t ra_x86_longjmp(uint64_t buffer, uint64_t value) {
    longjmp(*ra_jmp_slot(buffer), (int)value ? (int)value : 1);
}

// The exact x87 unit: a software f80. A slot holds an 80-bit value as the x87 stores a tbyte, the
// 64-bit significand with its explicit integer bit then the sign and the 15-bit exponent, and six
// bytes of padding. Every operation unpacks its operands, works in 128 bits with a sticky bit for
// what fell below, and rounds the result by the control word: the rounding mode in bits 10 and 11
// and the precision in bits 8 and 9 (24, 53 or 64 bits), as the hardware does with the exceptions
// masked. No 128-bit division: the host's compiler would want a library for it.
typedef unsigned __int128 f80_u128;

enum { F80_ZERO, F80_FINITE, F80_INF, F80_NAN };

// An unpacked value: m * 2^(exp - 63), m normalized with bit 63 set when finite.
struct f80 { int kind; int sign; int32_t exp; uint64_t m; };

static int f80_clz64(uint64_t v) { return (int)__builtin_clzll(v); }

static int f80_clz128(f80_u128 v) {
    uint64_t high = (uint64_t)(v >> 64);
    return high ? f80_clz64(high) : 64 + f80_clz64((uint64_t)v);
}

static struct f80 f80_unpack(const void *slot) {
    uint64_t m;
    uint16_t se;
    memcpy(&m, slot, 8);
    memcpy(&se, (const uint8_t *)slot + 8, 2);
    struct f80 v = { F80_FINITE, se >> 15, 0, m };
    int e = se & 0x7fff;
    if (e == 0x7fff) { v.kind = (m << 1) == 0 ? F80_INF : F80_NAN; return v; }
    if (m == 0) { v.kind = F80_ZERO; return v; }
    // A denormal has exponent 0 and the integer bit clear; an unnormal is normalized the same way.
    int shift = f80_clz64(m);
    v.m = m << shift;
    v.exp = (e == 0 ? 1 : e) - 16383 - shift;
    return v;
}

static void f80_write(void *slot, int sign, uint32_t exponent, uint64_t m) {
    uint8_t bytes[16] = { 0 };
    uint16_t se = (uint16_t)(((unsigned)sign << 15) | exponent);
    memcpy(bytes, &m, 8);
    memcpy(bytes + 8, &se, 2);
    memcpy(slot, bytes, 16);
}

static void f80_write_indefinite(void *slot) { f80_write(slot, 1, 0x7fff, 0xc000000000000000ull); }
static void f80_write_inf(void *slot, int sign) { f80_write(slot, sign, 0x7fff, 0x8000000000000000ull); }
static void f80_write_quiet(void *slot, struct f80 v) { f80_write(slot, v.sign, 0x7fff, v.m | 0xc000000000000000ull); }

// A NaN, an infinity or a zero goes out as it is, the NaN quiet.
static void f80_write_special(void *slot, struct f80 v) {
    if (v.kind == F80_NAN) f80_write_quiet(slot, v);
    else if (v.kind == F80_INF) f80_write_inf(slot, v.sign);
    else f80_write(slot, v.sign, 0, 0);
}

// The NaN of a two-operand operation: the one with the larger significand, quiet.
static int f80_nan_result(void *slot, struct f80 a, struct f80 b) {
    if (a.kind != F80_NAN && b.kind != F80_NAN) return 0;
    struct f80 n = a.kind != F80_NAN ? b : b.kind != F80_NAN ? a : (b.m > a.m ? b : a);
    f80_write_quiet(slot, n);
    return 1;
}

static int f80_rc(uint64_t cw) { return (int)((cw >> 10) & 3); }

static int f80_precision(uint64_t cw) {
    switch ((cw >> 8) & 3) {
    case 0: return 24;
    case 2: return 53;
    default: return 64;
    }
}

// A result rounded into a binary format: the value m * 2^(exp - 127) with bit 127 set (sticky for
// what lies below), to p bits in the mode rc, with the exponent bias and the all-ones exponent
// emax of the format; the significand comes back left-aligned in 64 bits with its integer bit,
// and a denormal with the exponent 0.
struct f80_packed { int sign; int32_t biased; uint64_t m; };

static struct f80_packed f80_round(int sign, int32_t exp, f80_u128 m, int sticky, int p, int rc, int32_t emax, int32_t bias) {
    struct f80_packed r = { sign, 0, 0 };
    if (m == 0) return r;
    int32_t biased = exp + bias;
    if (biased < 1) {
        // A denormal: the bits below the format's smallest exponent go into the rounding.
        int32_t s = 1 - biased;
        if (s >= 128) { sticky |= m != 0; m = 0; }
        else { sticky |= (m & (((f80_u128)1 << s) - 1)) != 0; m >>= s; }
        biased = 1;
    }
    int shift = 128 - p;
    f80_u128 mask = ((f80_u128)1 << shift) - 1, half = (f80_u128)1 << (shift - 1);
    f80_u128 low = m & mask;
    m &= ~mask;
    int up;
    switch (rc) {
    case 0: up = low > half || (low == half && (sticky || (m & ((f80_u128)1 << shift)) != 0)); break;
    case 1: up = sign && (low != 0 || sticky); break;
    case 2: up = !sign && (low != 0 || sticky); break;
    default: up = 0; break;
    }
    if (up) {
        f80_u128 before = m;
        m += (f80_u128)1 << shift;
        if (m < before) { m = (f80_u128)1 << 127; biased++; }
    }
    if (biased == 1 && !(m >> 127)) biased = 0;
    if (biased >= emax) {
        int to_inf = rc == 0 || (rc == 1 && sign) || (rc == 2 && !sign);
        r.biased = to_inf ? emax : emax - 1;
        r.m = to_inf ? 0x8000000000000000ull : ~(uint64_t)0 << (64 - p);
        return r;
    }
    r.biased = biased;
    r.m = (uint64_t)(m >> 64);
    return r;
}

static void f80_write_rounded(void *slot, int sign, int32_t exp, f80_u128 m, int sticky, uint64_t cw) {
    struct f80_packed r = f80_round(sign, exp, m, sticky, f80_precision(cw), f80_rc(cw), 0x7fff, 16383);
    f80_write(slot, r.sign, (uint32_t)r.biased, r.m);
}

// A finite or zero value, rounded to the precision of the control word.
static void f80_write_value(void *slot, struct f80 v, uint64_t cw) {
    f80_write_rounded(slot, v.sign, v.exp, v.kind == F80_ZERO ? 0 : (f80_u128)v.m << 64, 0, cw);
}

// The magnitude of the integer nearest a finite value below 2^63, by the mode rc.
static uint64_t f80_to_integer(struct f80 v, int rc) {
    int shift = 63 - v.exp;
    uint64_t q;
    int half, sticky;
    if (shift > 64) { q = 0; half = 0; sticky = 1; }
    else if (shift == 64) { q = 0; half = (int)(v.m >> 63); sticky = (v.m << 1) != 0; }
    else {
        q = v.m >> shift;
        uint64_t rest = v.m & ((1ull << shift) - 1);
        half = (int)((rest >> (shift - 1)) & 1);
        sticky = (rest & ((1ull << (shift - 1)) - 1)) != 0;
    }
    int up;
    switch (rc) {
    case 0: up = half && (sticky || (q & 1)); break;
    case 1: up = v.sign && (half || sticky); break;
    case 2: up = !v.sign && (half || sticky); break;
    default: up = 0; break;
    }
    return q + (uint64_t)up;
}

void ra_f80_load(void *slot, const void *memory) {
    uint8_t bytes[16] = { 0 };
    memcpy(bytes, memory, 10);
    memcpy(slot, bytes, 16);
}

void ra_f80_store(void *memory, const void *slot) {
    memcpy(memory, slot, 10);
}

void ra_f80_pack(void *slot, uint64_t significand, uint64_t sign_exponent) {
    f80_write(slot, (int)(sign_exponent >> 15) & 1, (uint32_t)sign_exponent & 0x7fff, significand);
}

void ra_f80_from_f64(void *slot, double value) {
    uint64_t bits;
    memcpy(&bits, &value, 8);
    int sign = (int)(bits >> 63);
    int e = (int)((bits >> 52) & 0x7ff);
    uint64_t frac = bits & ((1ull << 52) - 1);
    if (e == 0x7ff) {
        f80_write(slot, sign, 0x7fff, frac ? 0xc000000000000000ull | (frac << 11) : 0x8000000000000000ull);
    } else if (e == 0 && frac == 0) {
        f80_write(slot, sign, 0, 0);
    } else if (e == 0) {
        int shift = f80_clz64(frac);
        f80_write(slot, sign, (uint32_t)(15372 - shift), frac << shift);
    } else {
        f80_write(slot, sign, (uint32_t)(e - 1023 + 16383), 0x8000000000000000ull | (frac << 11));
    }
}

void ra_f80_from_i64(void *slot, uint64_t value) {
    int64_t v = (int64_t)value;
    if (v == 0) { f80_write(slot, 0, 0, 0); return; }
    int sign = v < 0;
    uint64_t u = sign ? 0 - value : value;
    int shift = f80_clz64(u);
    f80_write(slot, sign, (uint32_t)(16383 + 63 - shift), u << shift);
}

double ra_f80_to_f64(const void *slot, uint64_t cw) {
    struct f80 v = f80_unpack(slot);
    uint64_t bits;
    switch (v.kind) {
    case F80_NAN: bits = (uint64_t)v.sign << 63 | 0x7ffull << 52 | 1ull << 51 | ((v.m << 1) >> 12); break;
    case F80_INF: bits = (uint64_t)v.sign << 63 | 0x7ffull << 52; break;
    case F80_ZERO: bits = (uint64_t)v.sign << 63; break;
    default: {
        struct f80_packed r = f80_round(v.sign, v.exp, (f80_u128)v.m << 64, 0, 53, f80_rc(cw), 0x7ff, 1023);
        bits = (uint64_t)r.sign << 63 | (uint64_t)r.biased << 52 | ((r.m << 1) >> 12);
        break;
    }
    }
    double d;
    memcpy(&d, &bits, 8);
    return d;
}

float ra_f80_to_f32(const void *slot, uint64_t cw) {
    struct f80 v = f80_unpack(slot);
    uint32_t bits;
    switch (v.kind) {
    case F80_NAN: bits = (uint32_t)v.sign << 31 | 0xffu << 23 | 1u << 22 | (uint32_t)((v.m << 1) >> 41); break;
    case F80_INF: bits = (uint32_t)v.sign << 31 | 0xffu << 23; break;
    case F80_ZERO: bits = (uint32_t)v.sign << 31; break;
    default: {
        struct f80_packed r = f80_round(v.sign, v.exp, (f80_u128)v.m << 64, 0, 24, f80_rc(cw), 0xff, 127);
        bits = (uint32_t)r.sign << 31 | (uint32_t)r.biased << 23 | (uint32_t)((r.m << 1) >> 41);
        break;
    }
    }
    float f;
    memcpy(&f, &bits, 4);
    return f;
}

// An integer of `bits` bits, rounded by `rounding` (the mode alone, 3 for a truncation), or the
// integer indefinite when the value is not a number or does not fit.
uint64_t ra_f80_to_int(const void *slot, uint64_t rounding, uint64_t bits) {
    struct f80 v = f80_unpack(slot);
    uint64_t limit = 1ull << (bits - 1);
    if (v.kind == F80_NAN || v.kind == F80_INF) return limit;
    if (v.kind == F80_ZERO) return 0;
    if (v.exp >= (int32_t)bits - 1) return limit;
    uint64_t q = f80_to_integer(v, (int)(rounding & 3));
    if (v.sign ? q > limit : q >= limit) return limit;
    return v.sign ? 0 - q : q;
}

static void f80_add(void *result, const void *pa, const void *pb, uint64_t cw, int negate) {
    struct f80 a = f80_unpack(pa), b = f80_unpack(pb);
    b.sign ^= negate;
    int rc = f80_rc(cw);
    if (f80_nan_result(result, a, b)) return;
    if (a.kind == F80_INF || b.kind == F80_INF) {
        if (a.kind == F80_INF && b.kind == F80_INF && a.sign != b.sign) f80_write_indefinite(result);
        else f80_write_special(result, a.kind == F80_INF ? a : b);
        return;
    }
    if (a.kind == F80_ZERO && b.kind == F80_ZERO) { f80_write(result, (a.sign & b.sign) | (rc == 1 && (a.sign | b.sign)), 0, 0); return; }
    if (a.kind == F80_ZERO) { f80_write_value(result, b, cw); return; }
    if (b.kind == F80_ZERO) { f80_write_value(result, a, cw); return; }
    // The larger magnitude first; the smaller one is aligned to it, what falls off kept as a
    // sticky bit far below the rounding position.
    if (a.exp < b.exp || (a.exp == b.exp && a.m < b.m)) { struct f80 t = a; a = b; b = t; }
    f80_u128 ma = (f80_u128)a.m << 64, mb = (f80_u128)b.m << 64;
    int32_t d = a.exp - b.exp;
    if (d >= 128) mb = 1;
    else if (d > 0) { f80_u128 lost = mb & (((f80_u128)1 << d) - 1); mb = (mb >> d) | (lost != 0); }
    f80_u128 m;
    int32_t exp = a.exp;
    if (a.sign == b.sign) {
        m = ma + mb;
        if (m < ma) { m = (m >> 1) | ((f80_u128)1 << 127) | (m & 1); exp++; }
    } else {
        m = ma - mb;
        if (m == 0) { f80_write(result, rc == 1, 0, 0); return; }
        int lz = f80_clz128(m);
        m <<= lz;
        exp -= lz;
    }
    f80_write_rounded(result, a.sign, exp, m, 0, cw);
}

void ra_f80_add(void *result, const void *a, const void *b, uint64_t cw) { f80_add(result, a, b, cw, 0); }
void ra_f80_sub(void *result, const void *a, const void *b, uint64_t cw) { f80_add(result, a, b, cw, 1); }

void ra_f80_mul(void *result, const void *pa, const void *pb, uint64_t cw) {
    struct f80 a = f80_unpack(pa), b = f80_unpack(pb);
    int sign = a.sign ^ b.sign;
    if (f80_nan_result(result, a, b)) return;
    if (a.kind == F80_INF || b.kind == F80_INF) {
        if (a.kind == F80_ZERO || b.kind == F80_ZERO) f80_write_indefinite(result);
        else f80_write_inf(result, sign);
        return;
    }
    if (a.kind == F80_ZERO || b.kind == F80_ZERO) { f80_write(result, sign, 0, 0); return; }
    f80_u128 m = (f80_u128)a.m * b.m;
    int32_t exp = a.exp + b.exp + 1;
    if (!(m >> 127)) { m <<= 1; exp--; }
    f80_write_rounded(result, sign, exp, m, 0, cw);
}

// (high:low) / d for high < d: the 64-bit quotient and the remainder, a bit at a time.
static uint64_t f80_udiv128(uint64_t high, uint64_t low, uint64_t d, uint64_t *remainder) {
    uint64_t q = 0;
    for (int i = 0; i < 64; i++) {
        int carry = (int)(high >> 63);
        high = (high << 1) | (low >> 63);
        low <<= 1;
        q <<= 1;
        if (carry || high >= d) { high -= d; q |= 1; }
    }
    *remainder = high;
    return q;
}

void ra_f80_div(void *result, const void *pa, const void *pb, uint64_t cw) {
    struct f80 a = f80_unpack(pa), b = f80_unpack(pb);
    int sign = a.sign ^ b.sign;
    if (f80_nan_result(result, a, b)) return;
    if (a.kind == F80_INF) {
        if (b.kind == F80_INF) f80_write_indefinite(result);
        else f80_write_inf(result, sign);
        return;
    }
    if (b.kind == F80_INF) { f80_write(result, sign, 0, 0); return; }
    if (b.kind == F80_ZERO) {
        if (a.kind == F80_ZERO) f80_write_indefinite(result);
        else f80_write_inf(result, sign);
        return;
    }
    if (a.kind == F80_ZERO) { f80_write(result, sign, 0, 0); return; }
    // The quotient of the significands to 65 bits: the 64 of the result and a guard bit, the
    // remainder as the sticky bit.
    int32_t exp = a.exp - b.exp;
    uint64_t high = a.m, remainder;
    int whole = a.m >= b.m;
    if (whole) high -= b.m;
    uint64_t q = f80_udiv128(high, 0, b.m, &remainder);
    f80_u128 quotient;
    if (whole) {
        quotient = ((f80_u128)1 << 64) | q;
    } else {
        int guard = remainder >= b.m - remainder;
        remainder = guard ? remainder - (b.m - remainder) : remainder * 2;
        quotient = ((f80_u128)q << 1) | (f80_u128)guard;
        exp--;
    }
    f80_write_rounded(result, sign, exp, quotient << 63, remainder != 0, cw);
}

void ra_f80_sqrt(void *result, const void *pa, uint64_t cw) {
    struct f80 v = f80_unpack(pa);
    if (v.kind == F80_NAN || v.kind == F80_ZERO) { f80_write_special(result, v); return; }
    if (v.sign) { f80_write_indefinite(result); return; }
    if (v.kind == F80_INF) { f80_write_inf(result, 0); return; }
    // The value as m * 2^k with k even, m in [2^126, 2^128), so the root of m has 64 bits; a
    // guard bit and the remainder follow.
    int32_t k = v.exp - 63;
    int odd = k & 1;
    f80_u128 n = odd ? (f80_u128)v.m << 63 : (f80_u128)v.m << 64;
    int32_t k2 = (odd ? k - 63 : k - 64) / 2;
    f80_u128 root = 0;
    for (f80_u128 bit = (f80_u128)1 << 126; bit; bit >>= 2) {
        if (n >= root + bit) { n -= root + bit; root = (root >> 1) + bit; }
        else root >>= 1;
    }
    int guard = n > root;
    f80_u128 rest = guard ? 4 * n - 4 * root - 1 : n;
    f80_u128 quotient = ((root << 1) | (f80_u128)guard) << 63;
    f80_write_rounded(result, 0, 63 + k2, quotient, rest != 0, cw);
}

void ra_f80_rndint(void *result, const void *pa, uint64_t cw) {
    struct f80 v = f80_unpack(pa);
    if (v.kind != F80_FINITE) { f80_write_special(result, v); return; }
    if (v.exp >= 63) { f80_write(result, v.sign, (uint32_t)(v.exp + 16383), v.m); return; }
    uint64_t q = f80_to_integer(v, f80_rc(cw));
    if (q == 0) { f80_write(result, v.sign, 0, 0); return; }
    int shift = f80_clz64(q);
    f80_write(result, v.sign, (uint32_t)(16383 + 63 - shift), q << shift);
}

void ra_f80_chs(void *result, const void *a) {
    uint8_t bytes[16];
    memcpy(bytes, a, 16);
    bytes[9] ^= 0x80;
    memcpy(result, bytes, 16);
}

void ra_f80_abs(void *result, const void *a) {
    uint8_t bytes[16];
    memcpy(bytes, a, 16);
    bytes[9] &= 0x7f;
    memcpy(result, bytes, 16);
}

uint64_t ra_f80_cmp(const void *pa, const void *pb) {
    struct f80 a = f80_unpack(pa), b = f80_unpack(pb);
    if (a.kind == F80_NAN || b.kind == F80_NAN) return 7;
    if (a.kind == F80_ZERO && b.kind == F80_ZERO) return 2;
    int order;
    if (a.sign != b.sign) {
        order = a.sign ? -1 : 1;
    } else {
        // The magnitudes: an infinity beyond everything, a zero below, else by exponent then significand.
        if (a.kind == F80_INF || b.kind == F80_INF) order = (a.kind == F80_INF) - (b.kind == F80_INF);
        else if (a.kind == F80_ZERO || b.kind == F80_ZERO) order = (b.kind == F80_ZERO) - (a.kind == F80_ZERO);
        else if (a.exp != b.exp) order = a.exp > b.exp ? 1 : -1;
        else order = a.m > b.m ? 1 : a.m < b.m ? -1 : 0;
        if (a.sign) order = -order;
    }
    return order < 0 ? 1 : order == 0 ? 2 : 0;
}

// The function behind the macro, for the import slot that holds its address: a setjmp through a
// pointer cannot save the caller's frame and returns as a first return does.
uint64_t (ra_x86_setjmp3)(uint64_t buffer, uint64_t count) {
    (void)buffer;
    (void)count;
    return 0;
}

// An import by ordinal has no symbol to link: the host's function, found at startup.
void *ra_resolve_ordinal(const char *dll, unsigned ordinal) {
    HMODULE module = LoadLibraryA(dll);
    void *function = module ? (void *)GetProcAddress(module, (LPCSTR)(uintptr_t)ordinal) : NULL;
    if (!function) {
        fprintf(stderr, "ra: %s has no ordinal %u\n", dll, ordinal);
        fflush(stderr);
        ExitProcess(0xE0000004);
    }
    return function;
}

// The printf and scanf family through their va_list: the program's arguments are dwords (a double
// or a 64-bit integer two of them), the host's va_list a row of 8-byte slots. The format says what
// each argument is; the adapter walks it and widens each argument into a slot.
static void widen_printf_args(const char *format, const uint32_t *args, uint64_t *slots, size_t count) {
    size_t n = 0, k = 0;
    for (const char *p = format; *p && n + 1 < count; p++) {
        if (*p != '%') continue;
        p++;
        if (*p == '%') continue;
        while (*p && strchr("-+ #0'", *p)) p++;
        if (*p == '*') { slots[n++] = args[k++]; p++; } else while (*p >= '0' && *p <= '9') p++;
        if (*p == '.') {
            p++;
            if (*p == '*') { slots[n++] = args[k++]; p++; } else while (*p >= '0' && *p <= '9') p++;
        }
        int wide = 0;
        if (p[0] == 'l' && p[1] == 'l') { wide = 1; p += 2; }
        else if (p[0] == 'I' && p[1] == '6' && p[2] == '4') { wide = 1; p += 3; }
        else if (p[0] == 'I' && p[1] == '3' && p[2] == '2') p += 3;
        else if (*p == 'j') { wide = 1; p++; }
        else if (*p == 'h') { p++; if (*p == 'h') p++; }
        else if (*p == 'l' || *p == 'L' || *p == 'w' || *p == 'I' || *p == 'z' || *p == 't') p++;
        switch (*p) {
        case 'a': case 'A': case 'e': case 'E': case 'f': case 'F': case 'g': case 'G':
            memcpy(&slots[n++], &args[k], 8);
            k += 2;
            break;
        case 'd': case 'i': case 'o': case 'u': case 'x': case 'X': case 'c': case 'C': case 's': case 'S': case 'p': case 'n': case 'Z':
            if (wide) { memcpy(&slots[n++], &args[k], 8); k += 2; } else slots[n++] = args[k++];
            break;
        case 0: return;
        default: break;
        }
    }
}

// The program's `%p` prints eight hex digits, the host's sixteen: it becomes `%08X` of the dword.
static const char *narrow_pointers(const char *format, char *copy, size_t size) {
    if (!strstr(format, "%p") || strlen(format) * 2 + 1 > size) return format;
    char *o = copy;
    for (const char *p = format; *p; p++) {
        if (p[0] == '%' && p[1] == 'p') { memcpy(o, "%08X", 4); o += 4; p++; }
        else if (p[0] == '%' && p[1] == '%') { *o++ = *p++; *o++ = *p; }
        else *o++ = *p;
    }
    *o = 0;
    return copy;
}

// scanf's arguments are all pointers, a dword each: one per conversion that is not suppressed.
static void widen_scanf_args(const char *format, const uint32_t *args, uint64_t *slots, size_t count) {
    size_t n = 0;
    for (const char *p = format; *p && n < count; p++) {
        if (*p != '%') continue;
        p++;
        if (*p == '%' || *p == '*' || *p == 0) continue;
        slots[n] = args[n];
        n++;
    }
}

uint64_t ra_x86_vsprintf(uint64_t options, uint64_t buffer, uint64_t count, uint64_t format, uint64_t locale, uint64_t args) {
    uint64_t slots[64] = {0};
    char copy[1024];
    const char *f = narrow_pointers((const char *)(uintptr_t)format, copy, sizeof copy);
    widen_printf_args(f, (const uint32_t *)(uintptr_t)args, slots, 64);
    return (uint64_t)(int64_t)__stdio_common_vsprintf(options, (char *)(uintptr_t)buffer, (size_t)count, f, (_locale_t)(uintptr_t)locale,
                                                      (va_list)slots);
}

uint64_t ra_x86_vsprintf_s(uint64_t options, uint64_t buffer, uint64_t count, uint64_t format, uint64_t locale, uint64_t args) {
    uint64_t slots[64] = {0};
    char copy[1024];
    const char *f = narrow_pointers((const char *)(uintptr_t)format, copy, sizeof copy);
    widen_printf_args(f, (const uint32_t *)(uintptr_t)args, slots, 64);
    return (uint64_t)(int64_t)__stdio_common_vsprintf_s(options, (char *)(uintptr_t)buffer, (size_t)count, f, (_locale_t)(uintptr_t)locale,
                                                        (va_list)slots);
}

uint64_t ra_x86_vsnprintf_s(uint64_t options, uint64_t buffer, uint64_t count, uint64_t max, uint64_t format, uint64_t locale, uint64_t args) {
    uint64_t slots[64] = {0};
    char copy[1024];
    const char *f = narrow_pointers((const char *)(uintptr_t)format, copy, sizeof copy);
    widen_printf_args(f, (const uint32_t *)(uintptr_t)args, slots, 64);
    return (uint64_t)(int64_t)__stdio_common_vsnprintf_s(options, (char *)(uintptr_t)buffer, (size_t)count, (size_t)max, f,
                                                         (_locale_t)(uintptr_t)locale, (va_list)slots);
}

uint64_t ra_x86_vfprintf(uint64_t options, uint64_t stream, uint64_t format, uint64_t locale, uint64_t args) {
    uint64_t slots[64] = {0};
    char copy[1024];
    const char *f = narrow_pointers((const char *)(uintptr_t)format, copy, sizeof copy);
    widen_printf_args(f, (const uint32_t *)(uintptr_t)args, slots, 64);
    return (uint64_t)(int64_t)__stdio_common_vfprintf(options, (FILE *)(uintptr_t)stream, f, (_locale_t)(uintptr_t)locale, (va_list)slots);
}

uint64_t ra_x86_vsscanf(uint64_t options, uint64_t buffer, uint64_t count, uint64_t format, uint64_t locale, uint64_t args) {
    uint64_t slots[64] = {0};
    widen_scanf_args((const char *)(uintptr_t)format, (const uint32_t *)(uintptr_t)args, slots, 64);
    return (uint64_t)(int64_t)__stdio_common_vsscanf(options, (const char *)(uintptr_t)buffer, (size_t)count, (const char *)(uintptr_t)format,
                                                     (_locale_t)(uintptr_t)locale, (va_list)slots);
}

// The strto* family writes a `char *` of eight bytes where the program expects four: the host's
// end pointer goes to a local, and its low half to the program's.
#define RA_STRTO(name, host, type, widen) \
    uint64_t name(uint64_t s, uint64_t end, uint64_t base) { \
        char *e = NULL; \
        type v = host((const char *)(uintptr_t)s, &e, (int)base); \
        if (end) *(uint32_t *)(uintptr_t)end = (uint32_t)(uintptr_t)e; \
        return (uint64_t)(widen)v; \
    }
RA_STRTO(ra_x86_strtol, strtol, long, int64_t)
RA_STRTO(ra_x86_strtoul, strtoul, unsigned long, uint64_t)
RA_STRTO(ra_x86_strtoll, strtoll, long long, int64_t)
RA_STRTO(ra_x86_strtoull, strtoull, unsigned long long, uint64_t)

double ra_x86_strtod(uint64_t s, uint64_t end) {
    char *e = NULL;
    double v = strtod((const char *)(uintptr_t)s, &e);
    if (end) *(uint32_t *)(uintptr_t)end = (uint32_t)(uintptr_t)e;
    return v;
}

// argv and the environment as dword pointers to the host's strings, which sit below 4 GB.
static int ra_argc;
static char **ra_argv;
static uint32_t *argv32;
static uint32_t argv32_address;
static uint32_t *environment32;

uint64_t ra_x86_p_argv(void) {
    if (!argv32) {
        argv32 = (uint32_t *)calloc((size_t)ra_argc + 1, sizeof(uint32_t));
        for (int i = 0; i < ra_argc; i++) argv32[i] = (uint32_t)(uintptr_t)ra_argv[i];
        argv32_address = (uint32_t)(uintptr_t)argv32;
    }
    return (uint64_t)(uintptr_t)&argv32_address;
}

uint64_t ra_x86_initial_narrow_environment(void) {
    if (!environment32) {
        char *block = GetEnvironmentStringsA();
        size_t count = 0;
        for (char *p = block; *p; p += strlen(p) + 1) count++;
        environment32 = (uint32_t *)calloc(count + 1, sizeof(uint32_t));
        size_t i = 0;
        for (char *p = block; *p; p += strlen(p) + 1) environment32[i++] = (uint32_t)(uintptr_t)p;
    }
    return (uint64_t)(uintptr_t)environment32;
}

// An indirect call in the machine model: a lifted function by its address, else native code
// through the universal adapter. The lifted caller pushed the return address; native code does
// not pop it, so this does. A 32-bit program passes every argument on the stack as a dword and
// takes the result in edx:eax; a port of one runs in a process whose addresses all fit in 32 bits
// (the image is linked /LARGEADDRESSAWARE:NO), so a dword is a pointer the native code can use.
void ra_call(void *state_, uint64_t target) {
    struct State *state = state_;
    for (uint64_t i = 0; i < ra_function_count; i++) {
        if (ra_functions[i].address == target) {
            ra_functions[i].function(state);
            return;
        }
    }
    if (target >= RA_IMAGE_BASE && target < RA_IMAGE_BASE + RA_IMAGE_SIZE) {
        fprintf(stderr, "ra: an indirect call to 0x%llx, which is no lifted function\n", (unsigned long long)target);
        fflush(stderr);
        ExitProcess(0xE0000003);
    }
    native8 f = (native8)target;
#if RA_POINTER_SIZE == 4
    uint32_t *stack = (uint32_t *)(uintptr_t)((uint32_t)state->gpr[RSP] + 4);  // past the pushed return address
    uint64_t result = f(stack[0], stack[1], stack[2], stack[3], stack[4], stack[5], stack[6], stack[7]);
    state->gpr[RAX] = (uint32_t)result;
    state->gpr[RDX] = (uint32_t)(result >> 32);
    state->gpr[RSP] = (uint32_t)state->gpr[RSP] + 4;
#else
    uint64_t *stack = (uint64_t *)(state->gpr[RSP] + 8);  // past the pushed return address
    state->gpr[RAX] = f(state->gpr[RCX], state->gpr[RDX], state->gpr[R8], state->gpr[R9], stack[4], stack[5], stack[6], stack[7]);
    state->gpr[RSP] += 8;
#endif
}

#ifndef RA_STANDALONE
static void bind_imports(void) {
    for (int i = 0; ra_imports[i].slot != 0; i++) {
        HMODULE module = LoadLibraryA(ra_imports[i].dll);
        if (!module) {
            fprintf(stderr, "ra: cannot load %s\n", ra_imports[i].dll);
            ExitProcess(0xE0000004);
        }
        FARPROC f = ra_imports[i].name ? GetProcAddress(module, ra_imports[i].name) : GetProcAddress(module, MAKEINTRESOURCEA(ra_imports[i].ordinal));
        if (!f) {
            // The host's DLL lacks it (the x86 exception handling of the C runtime, say): the
            // slot stays empty, and a call through it would fault at address zero.
            fprintf(stderr, "ra: %s has no %s; its slot stays empty\n", ra_imports[i].dll, ra_imports[i].name ? ra_imports[i].name : "(ordinal)");
            continue;
        }
#if RA_POINTER_SIZE == 4
        *(uint32_t *)(uintptr_t)ra_imports[i].slot = (uint32_t)(uintptr_t)f;
#else
        *(uint64_t *)(uintptr_t)ra_imports[i].slot = (uint64_t)(uintptr_t)f;
#endif
    }
}

#if defined(RA_ABI)
// A trampoline at each lifted function's image address: `mov rax, imm64; jmp rax`, 12 bytes.
// A function closer than 12 bytes to the next one is left alone: its original bytes run natively,
// which for a function that small (a leaf of a few instructions) gives the same result.
static int by_address(const void *a, const void *b) {
    uint64_t x = *(const uint64_t *)a, y = *(const uint64_t *)b;
    return x < y ? -1 : x > y;
}

static void install_trampolines(void) {
    uint64_t *addresses = malloc(sizeof(uint64_t) * (ra_function_count + 1));
    for (uint64_t i = 0; i < ra_function_count; i++) addresses[i] = ra_functions[i].address;
    qsort(addresses, (size_t)ra_function_count, sizeof(uint64_t), by_address);
    addresses[ra_function_count] = RA_IMAGE_BASE + RA_IMAGE_SIZE;
    for (uint64_t i = 0; i < ra_function_count; i++) {
        uint64_t address = ra_functions[i].address;
        uint64_t next = RA_IMAGE_BASE + RA_IMAGE_SIZE;
        for (uint64_t k = 0; k < ra_function_count; k++) {
            if (addresses[k] == address) {
                next = addresses[k + 1];
                break;
            }
        }
        if (next - address < 12) continue;
        uint8_t *p = (uint8_t *)(uintptr_t)address;
        p[0] = 0x48;
        p[1] = 0xB8;
        uint64_t host = (uint64_t)(uintptr_t)ra_functions[i].function;
        memcpy(p + 2, &host, 8);
        p[10] = 0xFF;
        p[11] = 0xE0;
    }
    free(addresses);
    FlushInstructionCache(GetCurrentProcess(), (void *)(uintptr_t)RA_IMAGE_BASE, RA_IMAGE_SIZE);
}
#endif
#endif  // RA_STANDALONE

// The thread environment block the lifted code reads through fs: (x86) or gs: (x64), one per
// thread of the port: the fields a program and its C runtime look at, in the layout of the
// program's pointer width, from the host's own thread, and at 0x2c (x86) or 0x58 (x64) the
// thread-local storage pointer, an array whose one slot (the program's __tls_index is 0) is the
// thread's own copy of the image's TLS template. The stack bounds and the addresses fit the
// width: a 32-bit port runs with every address below 4 GB. The main thread attaches before the
// entry, as the loader does, with DLL_PROCESS_ATTACH to the TLS callbacks; another thread
// attaches at its first access, before it reads anything of its own, with DLL_THREAD_ATTACH.
// Nothing runs at a thread's exit. The environment block of the process stays zero.
extern const uint64_t ra_tls[5];  // the template's start and end, __tls_index, the zero fill, the callbacks

#ifdef _MSC_VER
#define RA_THREAD __declspec(thread)
#else
#define RA_THREAD _Thread_local
#endif
static RA_THREAD _Alignas(16) uint8_t teb[4096];
static RA_THREAD uint64_t tls_slot;
static RA_THREAD int attached;

#ifndef RA_ABI
// The stack a TLS callback of the machine IR runs on: the main thread's, set before it attaches.
static uint64_t machine_stack_top;

// A lifted function of the machine IR called with three arguments, as the loader calls a callback.
static void call_machine(void (*function)(void *), uint64_t a, uint64_t b, uint64_t c) {
    static struct State state;
    memset(&state, 0, sizeof state);
#if RA_POINTER_SIZE == 4
    uint32_t *sp = (uint32_t *)(uintptr_t)machine_stack_top;
    *--sp = (uint32_t)c;
    *--sp = (uint32_t)b;
    *--sp = (uint32_t)a;
    *--sp = 0;  // the return address
#else
    uint64_t *sp = (uint64_t *)(uintptr_t)machine_stack_top - 4;  // the shadow space
    *--sp = 0;  // the return address
    state.gpr[RCX] = a;
    state.gpr[RDX] = b;
    state.gpr[R8] = c;
#endif
    state.gpr[RSP] = (uint64_t)(uintptr_t)sp;
    function(&state);
}
#endif

// The TLS callbacks with `reason`, in their order: host functions standalone, else the lifted
// functions at the array's image addresses.
static void run_tls_callbacks(uint32_t reason) {
    for (uint64_t slot = ra_tls[4]; slot; slot += RA_POINTER_SIZE) {
        uint64_t target = RA_POINTER_SIZE == 4 ? *(uint32_t *)(uintptr_t)slot : *(uint64_t *)(uintptr_t)slot;
        if (!target) break;
#ifdef RA_STANDALONE
        ((native8)(uintptr_t)target)(RA_IMAGE_BASE, reason, 0, 0, 0, 0, 0, 0);
#else
        for (uint64_t i = 0; i < ra_function_count; i++) {
            if (ra_functions[i].address != target) continue;
#ifdef RA_ABI
            ((native8)ra_functions[i].function)(RA_IMAGE_BASE, reason, 0, 0, 0, 0, 0, 0);
#else
            call_machine(ra_functions[i].function, RA_IMAGE_BASE, reason, 0);
#endif
            break;
        }
#endif
    }
}

static void attach_thread(uint32_t reason) {
    attached = 1;
    NT_TIB *tib = (NT_TIB *)NtCurrentTeb();
    uintptr_t self = (uintptr_t)teb;
    uintptr_t stack_base = (uintptr_t)tib->StackBase;
    uintptr_t stack_limit = (uintptr_t)tib->StackLimit;
    uint32_t pid = GetCurrentProcessId();
    uint32_t tid = GetCurrentThreadId();
#if RA_POINTER_SIZE == 4
    *(uint32_t *)(teb + 0x04) = (uint32_t)stack_base;
    *(uint32_t *)(teb + 0x08) = (uint32_t)stack_limit;
    *(uint32_t *)(teb + 0x18) = (uint32_t)self;
    *(uint32_t *)(teb + 0x20) = pid;
    *(uint32_t *)(teb + 0x24) = tid;
    *(uint32_t *)(teb + 0x2c) = (uint32_t)(uintptr_t)&tls_slot;
#else
    *(uint64_t *)(teb + 0x08) = stack_base;
    *(uint64_t *)(teb + 0x10) = stack_limit;
    *(uint64_t *)(teb + 0x30) = self;
    *(uint64_t *)(teb + 0x40) = pid;
    *(uint64_t *)(teb + 0x48) = tid;
    *(uint64_t *)(teb + 0x58) = (uint64_t)(uintptr_t)&tls_slot;
#endif
    if (ra_tls[0]) {
        size_t initialized = (size_t)(ra_tls[1] - ra_tls[0]);
        uint8_t *block = calloc(initialized + (size_t)ra_tls[3] + 16, 1);
        memcpy(block, (const void *)(uintptr_t)ra_tls[0], initialized);
        tls_slot = (uint64_t)(uintptr_t)block;
        run_tls_callbacks(reason);
    }
}

uint64_t ra_teb(void) {
    if (!attached) attach_thread(DLL_THREAD_ATTACH);
    return (uint64_t)(uintptr_t)teb;
}

// The loader's work before the entry: __tls_index written, the main thread's block, the callbacks.
static void attach_process(void) {
    if (ra_tls[0] && ra_tls[2]) *(uint32_t *)(uintptr_t)ra_tls[2] = 0;
    attach_thread(DLL_PROCESS_ATTACH);
}

// A crash of the lifted code names the place: the host address as an offset into this module,
// which llvm-symbolizer maps to the lifted function and the image address its line comment
// carries, and for an access violation the address it touched. A vectored handler, so a program
// that installs its own unhandled-exception filter does not take the report away.
static LONG WINAPI on_exception(EXCEPTION_POINTERS *info) {
    const EXCEPTION_RECORD *r = info->ExceptionRecord;
    // The exceptions a program raises and handles itself are not crashes.
    if (r->ExceptionCode != EXCEPTION_ACCESS_VIOLATION && r->ExceptionCode != EXCEPTION_ILLEGAL_INSTRUCTION
        && r->ExceptionCode != EXCEPTION_STACK_OVERFLOW && r->ExceptionCode != EXCEPTION_PRIV_INSTRUCTION
        && r->ExceptionCode != EXCEPTION_BREAKPOINT && r->ExceptionCode != EXCEPTION_INT_DIVIDE_BY_ZERO
        && r->ExceptionCode != EXCEPTION_DATATYPE_MISALIGNMENT && r->ExceptionCode != EXCEPTION_IN_PAGE_ERROR) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    uintptr_t module = (uintptr_t)GetModuleHandleA(NULL);
    uintptr_t at = (uintptr_t)r->ExceptionAddress;
    fprintf(stderr, "ra: exception 0x%lx at 0x%llx", r->ExceptionCode, (unsigned long long)at);
    if (at >= module) fprintf(stderr, " (module+0x%llx)", (unsigned long long)(at - module));
    if (r->ExceptionCode == EXCEPTION_ACCESS_VIOLATION && r->NumberParameters >= 2) {
        fprintf(stderr, ", %s 0x%llx", r->ExceptionInformation[0] == 1 ? "writing" : "reading", (unsigned long long)r->ExceptionInformation[1]);
    }
    fprintf(stderr, "\n");
    fflush(stderr);
    return EXCEPTION_CONTINUE_SEARCH;
}

int main(int argc, char **argv) {
    AddVectoredExceptionHandler(1, on_exception);
    ra_argc = argc;
    ra_argv = argv;
#ifdef RA_STANDALONE
    // The data sections are globals of the program and the imports are linked by name: there is
    // no image to map and nothing to bind.
    (void)argc;
    (void)argv;
    attach_process();
    RA_ENTRY_CALL();
    return 0;
#else
    const char *image_path = argc > 1 ? argv[1] : "image.bin";
    FILE *f = fopen(image_path, "rb");
    if (!f) {
        fprintf(stderr, "ra: cannot open %s\n", image_path);
        return 2;
    }
    void *image = VirtualAlloc((void *)(uintptr_t)RA_IMAGE_BASE, RA_IMAGE_SIZE, MEM_RESERVE | MEM_COMMIT, PAGE_EXECUTE_READWRITE);
    if (image != (void *)(uintptr_t)RA_IMAGE_BASE) {
        fprintf(stderr, "ra: cannot map the image at 0x%llx (%lu)\n", (unsigned long long)RA_IMAGE_BASE, GetLastError());
        return 2;
    }
    size_t read = fread(image, 1, RA_IMAGE_SIZE, f);
    fclose(f);
    if (read == 0) {
        fprintf(stderr, "ra: %s is empty\n", image_path);
        return 2;
    }
    bind_imports();

#ifdef RA_ABI
    install_trampolines();
    attach_process();
    RA_ENTRY_CALL();
    return 0;
#else
    static struct State state;
    const size_t stack_size = 1 << 20;
    uint8_t *stack = NULL;
#if RA_POINTER_SIZE == 4
    // A 32-bit program keeps its stack pointer in 32 bits: the stack must sit below 4 GB.
    for (uintptr_t base = 0x20000000; base < 0x80000000 && !stack; base += 0x10000000) {
        stack = VirtualAlloc((void *)base, stack_size, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    }
#else
    stack = VirtualAlloc(NULL, stack_size, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
#endif
    if (!stack) return 2;
    // The entry sees the stack pointer as after a call: a pointer past a 16-byte boundary, with
    // room above it.
    uint64_t top = ((uint64_t)(uintptr_t)(stack + stack_size - 64)) & ~15ULL;
    machine_stack_top = top;
    attach_process();
    state.gpr[RSP] = top - RA_POINTER_SIZE;
#if RA_POINTER_SIZE == 4
    *(uint32_t *)(uintptr_t)state.gpr[RSP] = 0;  // the return address of the entry
#else
    *(uint64_t *)(uintptr_t)state.gpr[RSP] = 0;  // the return address of the entry
#endif
    state.gpr[RCX] = 0;
    state.flags[5] = 0;  // DF clear
    RA_ENTRY(&state);
    return (int)state.gpr[RAX];
#endif
#endif
}
