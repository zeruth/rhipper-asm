/* core: integer arithmetic, comparisons, extension, branches, loops, recursion (level 1);
 * switches (jump tables and trees), arrays, structs by value and by pointer, globals, strings, a
 * linked list (level 2). */
#include "rhtest.h"

/* ==== level 1: integers, branches, loops, recursion ========================================== */
NOINLINE i32 add3(i32 a, i32 b, i32 c) { return a + b - c; }
NOINLINE i32 mul_sub(i32 a, i32 b) { return a * b - (a << 3); }
NOINLINE i32 sdiv(i32 a, i32 b) { return a / b; }
NOINLINE i32 smod(i32 a, i32 b) { return a % b; }
NOINLINE u32 udiv(u32 a, u32 b) { return a / b; }
NOINLINE u32 umod(u32 a, u32 b) { return a % b; }
NOINLINE i32 div_const(i32 a) { return a / 7 + a % 10 + a / -3; }      /* magic-number division */
NOINLINE u32 udiv_const(u32 a) { return a / 7u + a % 1000u; }
NOINLINE i32 sar(i32 a, int n) { return a >> n; }
NOINLINE u32 shr(u32 a, int n) { return a >> n; }
NOINLINE u32 rotl(u32 a, int n) { return (a << (n & 31)) | (a >> ((32 - n) & 31)); }
NOINLINE u32 bits(u32 a, u32 b) { return (a & b) ^ (a | ~b) ^ (a << 5); }

NOINLINE i32 sext8(u32 v) { return (i8)v; }
NOINLINE i32 sext16(u32 v) { return (i16)v; }
NOINLINE u32 zext8(u32 v) { return (u8)v; }
NOINLINE u32 zext16(u32 v) { return (u16)v; }

NOINLINE i32 imin(i32 a, i32 b) { return a < b ? a : b; }
NOINLINE u32 umax(u32 a, u32 b) { return a > b ? a : b; }
NOINLINE i32 iabs(i32 a) { return a < 0 ? -a : a; }
NOINLINE i32 sign(i32 a) { return (a > 0) - (a < 0); }
/* the same bits compared signed and unsigned */
NOINLINE u32 cmp_mix(i32 a, u32 b) { return ((u32)(a < (i32)b) << 2) | ((u32)((u32)a < b) << 1) | (a == (i32)b); }

NOINLINE i32 classify(i32 v) {
    if (v < -100) return 1;
    if (v < 0) return 2;
    if (v == 0) return 3;
    if (v <= 10 && (v & 1)) return 4;
    if (v <= 10) return 5;
    return v > 1000 ? 6 : 7;
}

NOINLINE u32 sum_to(u32 n) {
    u32 s = 0, k;
    for (k = 1; k <= n; k++) s += k * k;
    return s;
}

NOINLINE u32 collatz(u32 n) {
    u32 steps = 0;
    while (n != 1) {
        n = (n & 1) ? 3 * n + 1 : n / 2;
        steps++;
    }
    return steps;
}

NOINLINE u32 gcd(u32 a, u32 b) {
    while (b) { u32 t = a % b; a = b; b = t; }
    return a;
}

NOINLINE u32 isqrt(u32 n) {
    u32 r = 0;
    while ((r + 1) * (r + 1) <= n) r++;
    return r;
}

NOINLINE u32 popcount(u32 v) {
    u32 c = 0;
    do { c += v & 1; } while (v >>= 1);
    return c;
}

NOINLINE u32 fib(u32 n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }
NOINLINE u32 ack(u32 m, u32 n) {
    if (m == 0) return n + 1;
    if (n == 0) return ack(m - 1, 1);
    return ack(m - 1, ack(m, n - 1));
}
/* mutual recursion */
NOINLINE int is_odd(u32 n);
NOINLINE int is_even(u32 n) { return n == 0 ? 1 : is_odd(n - 1); }
NOINLINE int is_odd(u32 n) { return n == 0 ? 0 : is_even(n - 1); }

/* a loop with break and continue, nested */
NOINLINE u32 nested(u32 n) {
    u32 s = 0, i, j;
    for (i = 0; i < n; i++) {
        if (i % 3 == 0) continue;
        for (j = 0; j < i; j++) {
            if (j * i > 40) break;
            s += i ^ j;
        }
    }
    return s;
}

static void level1(void) {
    u32 s = seed();   /* 7 */
    check("add3", (u32)add3((i32)s, 100, 30));
    check("mul_sub", (u32)mul_sub((i32)s * 1000, -37));
    check("sdiv", (u32)sdiv(-(i32)s * 1001, 13));
    check("smod", (u32)smod(-(i32)s * 1001, 13));
    check("udiv", udiv(0xfffffff0u - s, s * 3));
    check("umod", umod(0xfffffff0u - s, s * 3));
    check("div_const", (u32)div_const(-(i32)s * 12345));
    check("udiv_const", udiv_const(s * 0x1234567u));
    check("sar", (u32)sar(-(i32)s * 4096, (int)s - 3));
    check("shr", shr(0x80000000u | s, (int)s));
    check("rotl", rotl(0x12345678u + s, (int)s * 3));
    check("bits", bits(0xdeadbeefu ^ s, 0x0f0f0f0fu));
    check("sext8", (u32)sext8(0x80u + s));
    check("sext16", (u32)sext16(0x8000u + s));
    check("zext8", zext8(0xffffff80u + s));
    check("zext16", zext16(0xffff8000u + s));
    check("imin", (u32)imin(-(i32)s, (i32)s));
    check("umax", umax((u32)-(i32)s, s));
    check("iabs", (u32)iabs(-(i32)s * 3));
    check("sign", (u32)sign(-(i32)s) + 10 * (u32)sign(0) + 100 * (u32)sign((i32)s));
    check("cmp_mix", cmp_mix(-(i32)s, s));
    check("classify", classify(-200) | classify(-(i32)s) << 4 | classify((i32)s - 7) << 8 |
                      classify((i32)s) << 12 | classify((i32)s + 1) << 16 | classify((i32)s * 1000) << 20);
    check("sum_to", sum_to(s * 10));
    check("collatz", collatz(s * 4 + 27));
    check("gcd", gcd(s * 1071, s * 462));
    check("isqrt", isqrt(s * 100000));
    check("popcount", popcount(0xf0f0f0f0u ^ s));
    check("fib", fib(s * 3));
    check("ack", ack(2, s - 4));
    check("parity", (u32)is_even(s * 3) + 2 * (u32)is_odd(s * 5));
    check("nested", nested(s * 3));
}

/* ==== level 2: switches, arrays, structs, globals, strings ==================================== */

/* dense: a jump table */
NOINLINE i32 dense(u32 v) {
    switch (v) {
        case 0: return 11;
        case 1: return 23;
        case 2: return -5;
        case 3: return 47;
        case 4: return 90;
        case 5: return 4;
        case 6: return 1000;
        case 7: return -77;
        case 8: return 31;
        case 9: return 64;
        default: return 0;
    }
}

/* dense with shared and fall-through cases */
NOINLINE u32 fallthrough(u32 v) {
    u32 r = 1;
    switch (v % 8) {
        case 0: r += 1;
        case 1: r *= 3; break;
        case 2:
        case 3: r = 50; break;
        case 4: r += 100;
        case 5: r += 1000;
        case 6: r <<= 2; break;
        default: r = 0xabc;
    }
    return r;
}

/* sparse: a compare tree */
NOINLINE u32 sparse(u32 v) {
    switch (v) {
        case 1: return 1;
        case 10: return 2;
        case 100: return 3;
        case 1000: return 4;
        case 10000: return 5;
        case 65535: return 6;
        case 1000000: return 7;
        default: return 99;
    }
}

/* a switch over chars, the table indexed off 'a' */
NOINLINE u32 vowels(const char *s) {
    u32 n = 0;
    for (; *s; s++) {
        switch (*s) {
            case 'a': case 'e': case 'i': case 'o': case 'u': n += 1; break;
            case 'y': n += 0x100; break;
            case ' ': n += 0x10000; break;
            default: break;
        }
    }
    return n;
}

/* a switch on a value whose register then holds a pointer, so that the decompiler types the switch
   a pointer and its cases `(int *)0x3` (a client's message switch, `switch(piVar1)`) */
NOINLINE u32 kind_switch(u32 id, int *table) {
    int *p = (int *)((id & 0xffff) - 6);
    switch ((u32)p) {
        case 0: return 3;
        case 1: return 5;
        case 3: return 9;
        case 6: return 17;
        case 7: return 19;
        default: break;
    }
    p = table + ((u32)p & 3);
    return (u32)*p + 100;
}

/* a signed remainder by a power of two as a shift count, the C runtime's __ZeroTail: MSVC's
   `and edx, 0x8000001f; jns; dec; or edx, -0x20; inc` for `bit % 32`, which the decompiler folds
   to `bit & 0x1f` (a shift masks its count) */
NOINLINE u32 zero_tail(const u32 *words, int bit) {
    int i = bit / 32;
    if (words[i] & ~(0xffffffffu << (31 - bit % 32))) return 0;
    for (i++; i < 3; i++)
        if (words[i]) return 0;
    return 1;
}

NOINLINE u32 sieve(u32 limit) {
    static u8 composite[2000];
    u32 i, j, count = 0;
    for (i = 0; i < limit; i++) composite[i] = 0;
    for (i = 2; i < limit; i++) {
        if (composite[i]) continue;
        count++;
        for (j = i * i; j < limit; j += i) composite[j] = 1;
    }
    return count;
}

NOINLINE u32 lcg(u32 *state) {
    *state = *state * 1103515245u + 12345u;
    return (*state >> 16) & 0x7fff;
}

NOINLINE void insertion_sort(i32 *a, int n) {
    int i, j;
    for (i = 1; i < n; i++) {
        i32 v = a[i];
        for (j = i - 1; j >= 0 && a[j] > v; j--) a[j + 1] = a[j];
        a[j + 1] = v;
    }
}

NOINLINE u32 sort_check(void) {
    i32 a[24];
    u32 st = seed(), h = 0;
    int k;
    for (k = 0; k < 24; k++) a[k] = (i32)lcg(&st) - 16000;
    insertion_sort(a, 24);
    for (k = 0; k < 24; k++) {
        if (k && a[k - 1] > a[k]) return 0xbad;
        h = h * 31 + (u32)a[k];
    }
    return h;
}

NOINLINE void matmul(const i32 a[4][4], const i32 b[4][4], i32 out[4][4]) {
    int i, j, k;
    for (i = 0; i < 4; i++)
        for (j = 0; j < 4; j++) {
            i32 s = 0;
            for (k = 0; k < 4; k++) s += a[i][k] * b[k][j];
            out[i][j] = s;
        }
}

NOINLINE u32 matmul_check(void) {
    i32 a[4][4], b[4][4], c[4][4];
    u32 h = 0;
    int i, j;
    for (i = 0; i < 4; i++)
        for (j = 0; j < 4; j++) {
            a[i][j] = (i32)(seed() * (u32)(i + 1)) - j;
            b[i][j] = (i - j) * 3 + 1;
        }
    matmul(a, b, c);
    for (i = 0; i < 4; i++)
        for (j = 0; j < 4; j++) h = h * 7 + (u32)c[i][j];
    return h;
}

typedef struct { i32 x, y; } Point;
typedef struct { Point lo, hi; i32 tag; } Box;   /* 20 bytes: returned through a hidden pointer */

NOINLINE Point padd(Point a, Point b) { Point r; r.x = a.x + b.x; r.y = a.y + b.y; return r; }  /* edx:eax */
NOINLINE Point pscale(const Point *p, i32 k) { Point r; r.x = p->x * k; r.y = p->y * k; return r; }
NOINLINE Box make_box(Point a, Point b, i32 tag) {
    Box r;
    r.lo.x = imin(a.x, b.x); r.lo.y = imin(a.y, b.y);
    r.hi.x = a.x ^ b.x ^ r.lo.x; r.hi.y = a.y ^ b.y ^ r.lo.y;
    r.tag = tag;
    return r;
}
NOINLINE i32 box_area(Box b) { return (b.hi.x - b.lo.x) * (b.hi.y - b.lo.y) + b.tag; }
NOINLINE void box_grow(Box *b, i32 d) { b->lo.x -= d; b->lo.y -= d; b->hi.x += d; b->hi.y += d; }

NOINLINE u32 struct_check(void) {
    Point a, b, c;
    Box bx;
    a.x = (i32)seed(); a.y = -3;
    b.x = 20; b.y = (i32)seed() * 4;
    c = padd(a, pscale(&b, 2));
    bx = make_box(a, c, 5);
    box_grow(&bx, 1);
    return (u32)(c.x * 1000 + c.y) ^ ((u32)box_area(bx) << 12);
}

/* a global table of records with string pointers, read-only data */
typedef struct { const char *name; u32 weight; u16 flags; u8 kind; } Item;
static const Item g_items[] = {
    {"anvil", 300, 0x11, 1}, {"bucket", 12, 0x02, 2}, {"candle", 1, 0x80, 3},
    {"dagger", 25, 0x41, 1}, {"ember", 0, 0xff, 4}, {"flask", 7, 0x10, 2},
};

NOINLINE u32 str_len(const char *s) { const char *p = s; while (*p) p++; return (u32)(p - s); }
NOINLINE int str_cmp(const char *a, const char *b) {
    while (*a && *a == *b) a++, b++;
    return (u8)*a - (u8)*b;
}
NOINLINE void str_rev(char *s) {
    char *e = s + str_len(s);
    while (s < --e) { char t = *s; *s++ = *e; *e = t; }
}
NOINLINE u32 str_hash(const char *s) { u32 h = 5381; while (*s) h = h * 33 ^ (u8)*s++; return h; }
NOINLINE i32 parse_int(const char *s) {
    i32 v = 0, neg = 0;
    while (*s == ' ') s++;
    if (*s == '-') neg = 1, s++;
    while (*s >= '0' && *s <= '9') v = v * 10 + (*s++ - '0');
    return neg ? -v : v;
}

NOINLINE u32 items_check(u32 kind) {
    u32 i, h = 0;
    for (i = 0; i < sizeof g_items / sizeof g_items[0]; i++) {
        const Item *it = &g_items[i];
        if (it->kind == kind || (it->flags & 0x80)) h = h * 131 + it->weight + str_len(it->name) + it->flags;
    }
    return h;
}

NOINLINE u32 string_check(void) {
    char buf[32];
    const char *src = "Rhipper translates";
    u32 i = 0;
    for (; src[i]; i++) buf[i] = src[i];
    buf[i] = 0;
    str_rev(buf);
    return str_hash(buf) ^ (u32)str_cmp(buf, src) ^ ((u32)parse_int("  -48213") * seed());
}

/* globals written and read across functions: a zeroed one, an initialized one, a static local */
u32 g_counter;
i32 g_bias = -40;
NOINLINE void bump(u32 by) { g_counter += by + (u32)g_bias; g_bias++; }
NOINLINE u32 ticket(void) { static u32 next = 100; return next++ * 3; }

/* a linked list in a static pool */
typedef struct Node { struct Node *next; i32 value; } Node;
static Node g_pool[16];
NOINLINE Node *push(Node *head, Node *n, i32 v) { n->value = v; n->next = head; return n; }
NOINLINE Node *reverse(Node *head) {
    Node *prev = 0;
    while (head) { Node *nx = head->next; head->next = prev; prev = head; head = nx; }
    return prev;
}
NOINLINE u32 list_check(void) {
    Node *head = 0;
    u32 h = 0, k;
    for (k = 0; k < 16; k++) head = push(head, &g_pool[k], (i32)(k * seed()) - 50);
    head = reverse(head);
    for (; head; head = head->next) h = h * 17 + (u32)head->value;
    return h;
}

/* a clamp through pointers, std::min/max's shape: `p = v < 0x17 ? (v < 1 ? &lo : &v) : &hi`, MSVC
 * keeping v in a parameter's slot it no longer needs. The decompiler guarded the load through p
 * over the locals only and dropped the store into the slot (a game client's graphics
 * allocator's bucket index) */
typedef struct { u32 links[2]; u32 size; } Chunk;
NOINLINE u32 bucket_of(Chunk *c, u32 *mask) {
    u32 n = c->size, lo = 0, hi = 0x17;
    int bit = 31;
    const u32 *p;
    u32 v;
    if (n < 0x80) return 0xffffffffu;
    while (!(n >> bit)) bit--;
    v = (u32)(bit - 7);
    p = (int)v < 0x17 ? ((int)v < 1 ? &lo : &v) : &hi;
    *mask |= 1u << *p;
    return *p;
}
/* INT_MIN shifted right arithmetically, the mask of the buckets at or above k: Ghidra prints the
 * constant as `-0x80000000`, which C reads as unsigned, and the shift filled zeros */
NOINLINE u32 high_mask(u32 k, u32 bits) { return (u32)((i32)0x80000000 >> (30 - (k & 31))) & bits; }
NOINLINE u32 buckets(u32 s) {
    Chunk c[4] = {{{0}, 0x90}, {{0}, 0x1000 * s}, {{0}, 0x40}, {{0}, 0x7fffffff}};
    u32 mask = 0, r = 0, k;
    for (k = 0; k < 4; k++) r = r * 37 + bucket_of(&c[k], &mask);
    return r ^ mask;
}

/* ---- layouts the decompiler has to read exactly: a packed record (unaligned fields, `#pragma
 * pack(1)`), a structure over 8 bytes by value both ways (returned through the hidden pointer the
 * caller passes, passed as words copied onto the stack), a union read as another of its members,
 * and narrow signed values widened (movsx) and compared ---- */
#pragma pack(push, 1)
typedef struct { u8 tag; u32 id; u16 len; u32 crc; i8 bias; } Packed;
#pragma pack(pop)
NOINLINE u32 packed_check(u32 s) {
    Packed p[3];
    u32 k, h = 0;
    for (k = 0; k < 3; k++) {
        p[k].tag = (u8)(s + k);
        p[k].id = s * 0x01010101u + k;
        p[k].len = (u16)(s * 1000 + k);
        p[k].crc = ~p[k].id ^ (u32)p[k].len << 8;
        p[k].bias = (i8)(-(i32)s * (i32)(k + 1));
    }
    for (k = 0; k < 3; k++) h = h * 31 + p[k].tag + p[k].id + p[k].len + p[k].crc + (u32)(i32)p[k].bias;
    return h ^ (u32)sizeof(Packed) << 24;
}
typedef struct { u32 v[5]; double d; i16 w; } Big;
NOINLINE Big make_big(u32 s) {
    Big b;
    u32 k;
    for (k = 0; k < 5; k++) b.v[k] = s * (k + 3) ^ k;
    b.d = (double)s * 1.25;
    b.w = (i16)(-(i32)s * 300);
    return b;
}
NOINLINE u32 sum_big(Big b, u32 scale) {
    u32 k, r = 0;
    for (k = 0; k < 5; k++) r = r * 7 + b.v[k] * scale;
    return r + (u32)(i32)(b.d * 8.0) + (u32)(i32)b.w;
}
NOINLINE u32 big_check(u32 s) {
    Big a = make_big(s), c = make_big(s + 1);
    a.v[2] += c.v[4];
    return sum_big(a, 3) ^ sum_big(c, s) << 1;
}
typedef union { float f; u32 u; u8 b[4]; i16 h[2]; } Pun;
NOINLINE u32 pun_check(u32 s) {
    Pun p;
    p.f = (float)s * -2.5f;
    return p.u ^ (u32)p.b[3] << 3 ^ (u32)(i32)p.h[1];
}
NOINLINE u32 narrow_check(u32 s) {
    i8 a = (i8)(s * 37), b = (i8)(-(i32)s * 21);
    i16 c = (i16)(s * 9001), d = (i16)(-(i32)s * 4801);
    u32 r = 0;
    r += (a < b) | (c < d) << 1 | (a < 0) << 2 | (d < 0) << 3;
    r = r * 1000 + (u32)(i32)(a * b) + (u32)(i32)(c / (b ? b : 1)) + (u32)(i32)(i16)(c + d);
    return r ^ (u32)(u8)a << 16 ^ (u32)(u16)d;
}

static void level2(void) {
    u32 s = seed(), k, h;
    for (h = 0, k = 0; k < 12; k++) h = h * 13 + (u32)dense(k + s - 7);
    check("dense", h);
    for (h = 0, k = 0; k < 9; k++) h = h * 13 + fallthrough(k + s);
    check("fallthrough", h);
    check("sparse", sparse(1) | sparse(s * 1000 / 7) << 4 | sparse(65535) << 8 | sparse(1000000) << 12 | sparse(s) << 16);
    check("vowels", vowels("the quick brown fox jumps over every lazy dog"));
    {
        static u32 tail[3] = { 0x80000000u, 0, 0 };
        check("zero_tail", zero_tail(tail, (int)s) | zero_tail(tail, 0) << 1 | zero_tail(tail, 40 + (int)s) << 2);
    }
    {
        static int kinds[4] = { 40, 41, 42, 43 };
        for (h = 0, k = 0; k < 16; k++) h = h * 7 + kind_switch((k + s) | (k << 16), kinds);
        check("kind_switch", h);
    }
    check("sieve", sieve(s * 250));
    check("sort", sort_check());
    check("matmul", matmul_check());
    check("struct", struct_check());
    check("items", items_check(1) ^ items_check(2) << 1);
    check("string", string_check());
    for (k = 0; k < s; k++) bump(k);
    check("globals", g_counter ^ ((u32)g_bias << 16));
    h = ticket();
    h += ticket() * 10;
    h += ticket() * 100;
    check("ticket", h);
    check("list", list_check());
    check("buckets", buckets(s));
    check("high_mask", high_mask(s, 0xdeadbeef) ^ high_mask(s + 20, 0xffffffff) << 1);
    check("packed", packed_check(s));
    check("big", big_check(s));
    check("pun", pun_check(s));
    check("narrow", narrow_check(s));
}

void rhtest_run(void) {
    level1();
    level2();
}
