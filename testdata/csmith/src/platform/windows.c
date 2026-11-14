// What a csmith program gets instead of a C library on Windows: the entry that calls `main` and
// exits with its result, the three memory functions the compiler calls for a struct copy or a
// cleared local, and a printf of the conversions csmith's runtime uses, written through kernel32.
// The compiler's own helpers (the stack probe, the 64-bit division of x86) come from libcmt, as
// in any program of the MSVC toolchain. Nothing here allocates or reads.
#include <stdarg.h>

#include "stdio.h"
#include "string.h"

__declspec(dllimport) void *__stdcall GetStdHandle(unsigned long which);
__declspec(dllimport) int __stdcall WriteFile(void *file, const void *buf, unsigned long n, unsigned long *written, void *overlapped);
__declspec(dllimport) __declspec(noreturn) void __stdcall ExitProcess(unsigned code);

#define STD_OUTPUT_HANDLE ((unsigned long)-11)

int main(void);

void mainCRTStartup(void) { ExitProcess((unsigned)main()); }

void *memcpy(void *to, const void *from, size_t n) {
    char *d = to;
    const char *s = from;
    while (n--) *d++ = *s++;
    return to;
}

void *memmove(void *to, const void *from, size_t n) {
    char *d = to;
    const char *s = from;
    if (d <= s) {
        while (n--) *d++ = *s++;
    } else {
        while (n--) d[n] = s[n];
    }
    return to;
}

void *memset(void *to, int byte, size_t n) {
    char *d = to;
    while (n--) *d++ = (char)byte;
    return to;
}

// `v` in base `base` with the digits of `digits`, written backwards from `end`; the first digit.
static char *format_unsigned(unsigned v, unsigned base, const char *digits, char *end) {
    char *p = end;
    do {
        *--p = digits[v % base];
        v /= base;
    } while (v);
    return p;
}

static void put(char *out, int *n, char c) {
    if (*n < 255) out[(*n)++] = c;
}

static void put_str(char *out, int *n, const char *s) {
    while (*s) put(out, n, *s++);
}

// %d %u %x %X %s %c and %%, with an `l` before the integer ones (a long is an int here), no width
// or flags, at most 255 characters per call, to the standard output. Returns the count written.
int printf(const char *fmt, ...) {
    char out[256];
    char digits[12];
    int n = 0;
    unsigned long written = 0;
    va_list ap;
    digits[sizeof digits - 1] = 0;
    va_start(ap, fmt);
    for (const char *p = fmt; *p; p++) {
        if (*p != '%') {
            put(out, &n, *p);
            continue;
        }
        if (p[1] == 'l') p++;
        switch (*++p) {
        case 'd': {
            int v = va_arg(ap, int);
            unsigned u = v < 0 ? 0u - (unsigned)v : (unsigned)v;
            if (v < 0) put(out, &n, '-');
            put_str(out, &n, format_unsigned(u, 10, "0123456789", digits + sizeof digits - 1));
            break;
        }
        case 'u':
            put_str(out, &n, format_unsigned(va_arg(ap, unsigned), 10, "0123456789", digits + sizeof digits - 1));
            break;
        case 'x':
            put_str(out, &n, format_unsigned(va_arg(ap, unsigned), 16, "0123456789abcdef", digits + sizeof digits - 1));
            break;
        case 'X':
            put_str(out, &n, format_unsigned(va_arg(ap, unsigned), 16, "0123456789ABCDEF", digits + sizeof digits - 1));
            break;
        case 's':
            put_str(out, &n, va_arg(ap, const char *));
            break;
        case 'c':
            put(out, &n, (char)va_arg(ap, int));
            break;
        case '%':
            put(out, &n, '%');
            break;
        case 0:
            p--;
            break;
        default:
            put(out, &n, '%');
            put(out, &n, *p);
            break;
        }
    }
    va_end(ap);
    WriteFile(GetStdHandle(STD_OUTPUT_HANDLE), out, (unsigned long)n, &written, 0);
    return n;
}
