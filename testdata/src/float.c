// Floating point for the lift: scalar SSE arithmetic in both widths, conversions both ways,
// comparisons with NaN and infinity, a loop that accumulates, and a struct of floats. No C runtime
// and no libm: the results print as scaled integers through WriteFile.
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

// x scaled by 1000, rounded toward zero.
NOINLINE static long long milli(double x) { return (long long)(x * 1000.0); }

struct vec3 {
    float x, y, z;
};

NOINLINE static float dot(struct vec3 a, struct vec3 b) { return a.x * b.x + a.y * b.y + a.z * b.z; }

NOINLINE static double mean(const double *values, int count) {
    double sum = 0.0;
    for (int i = 0; i < count; i++) sum += values[i];
    return count ? sum / (double)count : 0.0;
}

NOINLINE static int classify(double x) {
    if (x != x) return 0;             // NaN
    if (x > 1e308 * 10.0) return 1;   // +inf
    if (x < -1e308 * 10.0) return 2;  // -inf
    if (x == 0.0) return 3;
    return x < 0.0 ? 4 : 5;
}

NOINLINE static double newton_sqrt(double x) {
    double r = x > 1.0 ? x / 2.0 : 1.0;
    for (int i = 0; i < 30; i++) r = 0.5 * (r + x / r);
    return r;
}

NOINLINE static float mix(float a, float b, float t) { return a + (b - a) * t; }

static volatile double zero = 0.0;
static volatile int seven = 7;

int mainCRTStartup(void) {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    double values[5] = {1.5, -2.25, 10.0, 0.125, 3.0};
    print_int(milli(mean(values, 5)));
    print_int(milli(newton_sqrt(2.0)));
    print_int(milli(newton_sqrt(144.0)));
    struct vec3 a = {1.0f, 2.0f, 3.0f}, b = {4.0f, -5.0f, 6.0f};
    print_int(milli((double)dot(a, b)));
    print_int(milli((double)mix(10.0f, 20.0f, 0.25f)));
    print_int(classify(zero / zero));
    print_int(classify(1.0 / zero));
    print_int(classify(-1.0 / zero));
    print_int(classify(zero));
    print_int(classify(-0.5));
    print_int(classify(2.5));
    print_int((long long)(seven * 1.5));
    print_int(milli((double)(float)(seven / 3.0)));
    print_int((int)(-7.9));
    print_int((int)(7.9f));
    unsigned u = 3000000000u;
    print_int(milli((double)u / 1000000.0));
    double big = 1e15;
    print_int((long long)(big + 0.5) % 1000);
    print_int(milli(values[0] > values[1] ? values[0] - values[1] : values[1] - values[0]));
    ExitProcess(0);
    return 0;
}
