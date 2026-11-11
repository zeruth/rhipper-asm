// The x87 unit at its extended precision, for the exact unit of the lift: a program built without
// SSE sums series in the eight registers under the two precision controls (Windows starts a thread
// at 53 bits; the program then asks for 64), takes a square root, and moves a tbyte in and out of
// the unit. Each result prints as a 17-digit integer, the value scaled by 1e16 and truncated; the
// digits past the fifteenth are the ones a unit of doubles gets wrong. 32-bit only: x64 code has
// no x87 arithmetic. No C runtime and no libm: the results print through WriteFile.
#include <windows.h>

#define NOINLINE __attribute__((noinline))

static HANDLE out;

// The MSVC toolchain marks a program that uses floating point; without a C runtime, this is it.
int _fltused = 1;

NOINLINE static void print_int(long long v) {
    char buf[24];
    char *p = buf + sizeof buf;
    unsigned long long u = v < 0 ? (unsigned long long)(-v) : (unsigned long long)v;
    *--p = '\n';
    *--p = '\r';
    do {
        *--p = (char)('0' + u % 10);
        u /= 10;
    } while (u);
    if (v < 0) *--p = '-';
    DWORD n = 0;
    WriteFile(out, p, (DWORD)(buf + sizeof buf - p), &n, NULL);
}

// The control word of the unit: the precision control is bits 8 and 9.
static unsigned short control_word(void) {
    unsigned short cw;
    __asm__ volatile("fnstcw %0" : "=m"(cw));
    return cw;
}

static void set_control_word(unsigned short cw) { __asm__ volatile("fldcw %0" : : "m"(cw)); }

// x in [1, 10) as its first 17 digits, scaled in the unit itself: a call would round x to a
// double on the way.
#define digits17(x) ((long long)((x) * 1e16))

// The harmonic series: a thousand divisions and additions, each rounded at the unit's precision.
NOINLINE static double harmonic(int n) {
    double sum = 0.0;
    for (int k = 1; k <= n; k++) sum += 1.0 / k;
    return sum;
}

// The Leibniz series for pi / 4, with the terms alternating in sign.
NOINLINE static double leibniz(int n) {
    double sum = 0.0, sign = 1.0;
    for (int k = 1; k <= n; k++, sign = -sign) sum += sign / (2.0 * k - 1.0);
    return sum;
}

NOINLINE static double root(double x) {
    __asm__("fsqrt" : "+t"(x));
    return x;
}

// A third, computed in the unit and stored as a tbyte: the full 64-bit significand.
NOINLINE static void third(unsigned char tbyte[10]) {
    int three = 3;
    __asm__ volatile("fld1\n\tfidivl %1\n\tfstpt %0" : "=m"(*(unsigned char(*)[10])tbyte) : "m"(three));
}

// A tbyte back into the unit, out as a double: rounded to 53 bits on the way.
NOINLINE static double from_tbyte(const unsigned char tbyte[10]) {
    double d;
    __asm__ volatile("fldt %1\n\tfstpl %0" : "=m"(d) : "m"(*(const unsigned char(*)[10])tbyte));
    return d;
}

static unsigned word(const unsigned char *p, int n) {
    unsigned v = 0;
    for (int i = n - 1; i >= 0; i--) v = (v << 8) | p[i];
    return v;
}

int mainCRTStartup(void) {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    unsigned short initial = control_word();
    print_int(initial);
    print_int(digits17(harmonic(1000)));
    set_control_word((unsigned short)(initial | 0x0300));
    print_int(control_word());
    print_int(digits17(harmonic(1000)));
    print_int(digits17(4.0 * leibniz(1000)));
    print_int(digits17(root(2.0)));
    print_int(digits17(root(3.0) * root(3.0)));
    unsigned char tbyte[10];
    third(tbyte);
    print_int(word(tbyte, 4));
    print_int(word(tbyte + 4, 4));
    print_int(word(tbyte + 8, 2));
    print_int(digits17(from_tbyte(tbyte) * 3.0));
    print_int((long long)(from_tbyte(tbyte) * 3e16));
    set_control_word(initial);
    print_int(digits17(harmonic(1000)));
    ExitProcess(0);
    return 0;
}
