// A printf of %d %u %x %s %c and %%: enough for a fixture to print its checks, nothing a lift has
// to model beyond integer arithmetic and one write. A call's output is cut at 255 characters.
#include <stdarg.h>

#include "freestanding.h"

unsigned long fs_strlen(const char *s) {
    const char *p = s;
    while (*p) p++;
    return (unsigned long)(p - s);
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

int fs_printf(const char *fmt, ...) {
    char out[256];
    char digits[12];
    int n = 0;
    va_list ap;
    va_start(ap, fmt);
    for (const char *p = fmt; *p; p++) {
        if (*p != '%') {
            put(out, &n, *p);
            continue;
        }
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
    fs_write(1, out, (unsigned long)n);
    return n;
}
