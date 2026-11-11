/* crt: the C runtime through the /MD ucrt imports. Formatted output (printf's family, each with
 * the va_list adapter Rhipper's runtime gives an x86 caller of the x64 ucrt), sscanf, qsort and
 * bsearch with callbacks into translated code, the allocator (malloc, calloc, realloc, free), the
 * string and memory functions, conversions, ctype, and stdout itself (printf, fflush). A string
 * result is checked by its hash, so a check is one line whatever the text. */
#define _CRT_SECURE_NO_WARNINGS   /* (the classic names, as old binaries call them) */
#include <ctype.h>
#include <malloc.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "rhtest.h"

#if defined(_MSC_VER) && _MSC_VER < 1900
/* (no snprintf before VS2015, and its _snprintf leaves a truncated string unterminated and returns
 * -1: C99's, as the newer CRT has it, built on it) */
NOINLINE int rh_snprintf(char *buf, size_t n, const char *f, ...) {
    va_list ap;
    int r;
    va_start(ap, f);
    r = _vscprintf(f, ap);
    va_end(ap);
    if (n) {
        va_start(ap, f);
        _vsnprintf(buf, n - 1, f, ap);
        va_end(ap);
        buf[n - 1] = 0;
    }
    return r;
}
#define snprintf rh_snprintf
#endif

NOINLINE u32 hstr(const char *s) {
    u32 h = 2166136261u;
    while (*s) h = (h ^ (u8)*s++) * 16777619u;
    return h;
}

/* formatted output */
NOINLINE u32 fmt_ints(int i, unsigned u, i64 l) {
    char buf[128];
    int n = sprintf(buf, "%d|%5d|%-4d|%x|%08X|%u|%lld|%llx|%+d|%%", i, -i, i, u, u, u, l, (u64)l, i);
    return hstr(buf) ^ (u32)n;
}
NOINLINE u32 fmt_floats(double d, float f) {
    char buf[128];
    int n = sprintf(buf, "%f|%.3f|%e|%g|%10.2f|%.0f|%g", d, d / 3, d * 1e10, d, (double)f, d, (double)f / 7);
    return hstr(buf) ^ (u32)n;
}
NOINLINE u32 fmt_strings(const char *s, char c) {
    char buf[96];
    int n = snprintf(buf, sizeof buf, "[%s][%10s][%-6.3s][%c][%p]", s, s, s, c, (void *)0);
    return hstr(buf) ^ (u32)n;
}
NOINLINE u32 fmt_truncated(int k) {
    char buf[8];
    int n = snprintf(buf, sizeof buf, "%d-%d-%d", k, k * 11, k * 111);
    return hstr(buf) ^ (u32)n;
}
/* a va_list handed to the CRT */
NOINLINE int vfmt(char *out, size_t n, const char *f, ...) {
    va_list ap;
    int r;
    va_start(ap, f);
    r = vsnprintf(out, n, f, ap);
    va_end(ap);
    return r;
}
NOINLINE u32 fmt_va(int k) {
    char buf[64];
    int n = vfmt(buf, sizeof buf, "%s=%d,%x,%.2f", "k", k, k * 3, k / 4.0);
    return hstr(buf) ^ (u32)n;
}

/* scanning */
NOINLINE u32 scan(const char *s) {
    int a = 0, b = 0;
    unsigned x = 0;
    double d = 0;
    char w[16] = {0};
    int n = sscanf(s, "%d %d %x %lf %15s", &a, &b, &x, &d, w);
    return (u32)(a * 31 + b) ^ x ^ (u32)(int)(d * 100) ^ hstr(w) ^ (u32)n << 28;
}

/* conversions */
NOINLINE u32 convert(const char *s) {
    char *end;
    long v = strtol(s, &end, 0);
    unsigned long u = strtoul(end, &end, 16);
    double d = strtod(end, &end);
    return (u32)v * 7 + (u32)u + (u32)(int)(d * 1000) + (u32)atoi("  -42z") + (u32)(end - s);
}

/* the CRT's `long`, the decompiler's type for strtoul's and strtol's results: 4 bytes on x86, and
 * the translation's spelling of it (translate32 port_longs, gen_long) must keep them where the
 * host's long is 8 (an LP64 port, aarch64): a sum that wraps at 2^32, halved after, and the
 * sign extension of a negative one. An 8-byte long halves 0x100000002 to 0x80000001 */
NOINLINE u64 longs(const char *s) {
    char *end;
    unsigned long u = strtoul(s, &end, 16);
    long v = strtol(end, &end, 10);
    unsigned long w = u + (unsigned long)(end - s);
    return ((u64)(w / 2) << 32) + (u64)(i64)v;
}

/* qsort and bsearch, the comparator translated code the CRT calls */
typedef struct {
    int key;
    const char *name;
} Item;
static int cmp_item(const void *a, const void *b) {
    const Item *x = (const Item *)a, *y = (const Item *)b;
    return x->key != y->key ? (x->key < y->key ? -1 : 1) : strcmp(x->name, y->name);
}
static int cmp_int(const void *a, const void *b) { return *(const int *)a - *(const int *)b; }
NOINLINE u32 sort_items(u32 s) {
    static const char *names[] = {"pear", "fig", "apple", "kiwi", "plum", "date", "lime", "yuzu"};
    Item it[8];
    u32 h = 0;
    int k;
    for (k = 0; k < 8; k++) {
        it[k].key = (int)((s * (k + 3)) % 5);
        it[k].name = names[k];
    }
    qsort(it, 8, sizeof it[0], cmp_item);
    for (k = 0; k < 8; k++) h = h * 31 + hstr(it[k].name) + (u32)it[k].key;
    return h;
}
NOINLINE u32 search(u32 s) {
    int v[16], k, found = 0;
    for (k = 0; k < 16; k++) v[k] = (int)((s * 2654435761u >> (k & 15)) % 100);
    qsort(v, 16, sizeof v[0], cmp_int);
    for (k = 0; k < 100; k += 7) found += bsearch(&k, v, 16, sizeof v[0], cmp_int) != 0;
    return (u32)found * 1000 + (u32)v[0] + (u32)v[15] * 3;
}

/* the allocator */
NOINLINE u32 heap(u32 s) {
    int *a = (int *)malloc(16 * sizeof(int));
    int *b = (int *)calloc(8, sizeof(int));
    u32 h = 0;
    int k;
    if (!a || !b) return 0xdead;
    for (k = 0; k < 16; k++) a[k] = (int)(s * k);
    a = (int *)realloc(a, 64 * sizeof(int));
    if (!a) return 0xbeef;
    for (k = 16; k < 64; k++) a[k] = k ^ (int)s;
    for (k = 0; k < 64; k++) h = h * 33 + (u32)a[k];
    for (k = 0; k < 8; k++) h += (u32)b[k];
    free(a);
    free(b);
    return h;
}

/* strings and memory */
NOINLINE u32 strings(u32 s) {
    char buf[64], other[32];
    const char *p;
    u32 h;
    strcpy(buf, "alpha");
    strcat(buf, "-beta");
    strncat(buf, "-gamma-delta", 6);
    memset(other, 'x', sizeof other);
    other[sizeof other - 1] = 0;
    memcpy(other, buf, 5);
    memmove(buf + 2, buf, 8);
    p = strstr(buf, "beta");
    h = hstr(buf) ^ hstr(other) * 3;
    h += (u32)strlen(buf) * 5 + (u32)(strchr(buf, 'g') - buf) + (u32)(strrchr(buf, 'a') - buf) * 7;
    h += (u32)(p ? p - buf : 99) + (u32)(strcmp("abc", "abd") < 0) + (u32)(strncmp(buf, other, 2) == 0) * 11;
    h += (u32)(memcmp("abc", "abc", 3) == 0) * 13 + (u32)(memchr(buf, '-', sizeof buf) != 0) * 17;
    return h ^ s;
}

/* ctype */
NOINLINE u32 ctypes(const char *s) {
    u32 h = 0;
    for (; *s; s++) {
        int c = (u8)*s;
        h = h * 7 + (u32)(isalpha(c) != 0) + (u32)(isdigit(c) != 0) * 2 + (u32)(isspace(c) != 0) * 4 +
            (u32)(isupper(c) != 0) * 8 + (u32)(ispunct(c) != 0) * 16 + (u32)toupper(c) + (u32)tolower(c) * 3;
    }
    return h;
}

/* integer helpers */
NOINLINE u32 ints(int k) {
    div_t q = div(-k * 100, 7);
    ldiv_t lq = ldiv((long)k * 100000, -13);
    return (u32)abs(-k * 3) + (u32)labs(-(long)k * 5) + (u32)q.quot * 17 + (u32)q.rem + (u32)lq.quot + (u32)lq.rem * 3;
}

/* _malloca: its buffer checked for NULL past the 8-byte marker, an address on the stack compared
 * with a small constant (the CRT's __crtLCMapStringA_stat, stubbed in a game client); and sized at
 * run time, which the decompiler loses: the buffer is a fixed stack address, and the port must
 * give it room below the frame (as a 0x18-byte local, __crtGetStringTypeA_stat's overran) */
NOINLINE u32 malloca_sum(int n) {
    int *p = (int *)_malloca(n * sizeof(int));
    u32 h = 0;
    int k;
    if (p == NULL) return 0;
    for (k = 0; k < n; k++) p[k] = k * k + n;
    for (k = 0; k < n; k++) h = h * 31 + (u32)p[k];
    _freea(p);
    return h;
}

/* stdout: the CRT's own stream, flushed so its lines come where they are written */
NOINLINE int print(int k) {
    int n = printf("printf: %d %s %.2f\n", k, "line", k * 0.5);
    fflush(stdout);
    return n;
}

void rhtest_run(void) {
    u32 s = seed();   /* 7 */
    int i = (int)s;
    char text[64];
    check("fmt_ints", fmt_ints(-i * 1234, s * 0x10001u, -(i64)s * 1000000007));
    check("fmt_floats", fmt_floats(i * 3.14159, (float)i / 3.0f));
    check("fmt_strings", fmt_strings("rhipper", (char)('a' + i)));
    check("fmt_trunc", fmt_truncated(i));
    check("fmt_va", fmt_va(i));
    sprintf(text, "%d %d %x %f word%d", i, -i * 9, s * 255, i * 1.25, i);
    check("scan", scan(text));
    sprintf(text, "0x%x %x %d.%d", s * 100, s * 7, i, i * 3);
    check("convert", convert(text));
    sprintf(text, "%x %d", 0xfffffff0u + s, -i);   /* "fffffff7 -7": 11 characters */
    check64("longs", longs(text));
    check("sort_items", sort_items(s));
    check("search", search(s));
    check("heap", heap(s));
    check("strings", strings(s));
    check("ctypes", ctypes("Hello, World 42!"));
    check("ints", ints(i));
    check("malloca", malloca_sum(i * 3));
    check("print", (u32)print(i));
}
