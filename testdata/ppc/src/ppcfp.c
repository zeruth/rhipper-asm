// The PowerPC floating-point fixture: a freestanding program of single and double arithmetic,
// built for the 750 without a C library, for the decoder and the lift of the processor's
// floating-point unit. It prints whole numbers only, each a result scaled and converted, through
// the Linux `write` call, and ends by `exit`.

typedef unsigned int u32;

static long syscall3(long number, long a, long b, long c) {
    register long r0 __asm__("r0") = number;
    register long r3 __asm__("r3") = a;
    register long r4 __asm__("r4") = b;
    register long r5 __asm__("r5") = c;
    __asm__ volatile("sc" : "+r"(r3) : "r"(r0), "r"(r4), "r"(r5) : "memory", "cr0", "ctr", "r6", "r7", "r8", "r9", "r10", "r11", "r12");
    return r3;
}

__attribute__((noinline)) static void print(int v) {
    char text[16];
    int at = 15;
    u32 u = v < 0 ? 0u - (u32)v : (u32)v;
    text[at] = '\n';
    do {
        text[--at] = (char)('0' + u % 10);
        u /= 10;
    } while (u);
    if (v < 0) {
        text[--at] = '-';
    }
    syscall3(4, 1, (long)(text + at), 16 - at);
}

__attribute__((noinline)) double arithmetic(double a, double b) {
    return (a + b) * (a - b) / (b + 0.5);
}

__attribute__((noinline)) float singles(float a, float b) {
    return (a + b) * (a - b) / (b + 0.5f);
}

// A sum of products the compiler makes multiply-adds of; the numbers are whole, so a fused
// multiply-add and a rounded one agree.
__attribute__((noinline)) double polynomial(double x) {
    return ((3.0 * x + 5.0) * x - 7.0) * x + 11.0;
}

__attribute__((noinline)) float polynomial_single(float x) {
    return ((2.0f * x - 3.0f) * x + 4.0f) * x - 6.0f;
}

__attribute__((noinline)) int compare(double a, double b) {
    int r = 0;
    if (a < b) r |= 1;
    if (a > b) r |= 2;
    if (a == b) r |= 4;
    if (a <= b) r |= 8;
    if (a >= b) r |= 16;
    if (a != b) r |= 32;
    return r;
}

// Newton's square root: divisions and a loop on a compare.
__attribute__((noinline)) double root(double v) {
    double x = v > 1.0 ? v * 0.5 : 1.0;
    for (int n = 0; n < 30; n++) {
        double next = 0.5 * (x + v / x);
        if (next == x) break;
        x = next;
    }
    return x;
}

__attribute__((noinline)) double magnitude(double v) {
    return v < 0 ? -v : v;
}

__attribute__((noinline)) double negate(double v) {
    return -v;
}

__attribute__((noinline)) float narrow(double v) {
    return (float)v;
}

__attribute__((noinline)) double from_int(int v) {
    return (double)v;
}

__attribute__((noinline)) double from_unsigned(u32 v) {
    return (double)v;
}

__attribute__((noinline)) int to_int(double v) {
    return (int)v;
}

// Loads and stores of singles by an index, and of doubles.
__attribute__((noinline)) float sum(const float *values, int count) {
    float total = 0;
    for (int n = 0; n < count; n++) {
        total += values[n];
    }
    return total;
}

__attribute__((noinline)) void scale(double *values, const float *by, int count) {
    for (int n = 0; n < count; n++) {
        values[n] = values[n] * by[count - 1 - n];
    }
}

static volatile double zero;

void _start(void) {
    int total = 0;
#define PRINT(v) do { int value = (v); total += value; print(value); } while (0)
    PRINT(to_int(arithmetic(12.5, 3.25) * 1000.0));
    PRINT(to_int(arithmetic(-7.75, 100.125) * 1000.0));
    PRINT(to_int(singles(12.5f, 3.25f) * 1000.0f));
    PRINT(to_int(singles(0.1f, 0.3f) * 1000000.0f));
    PRINT(to_int(polynomial(4.0)));
    PRINT(to_int(polynomial(-13.0)));
    PRINT(to_int(polynomial_single(9.0f)));
    PRINT(compare(1.0, 2.0));
    PRINT(compare(2.0, 1.0));
    PRINT(compare(-3.5, -3.5));
    PRINT(compare(zero / zero, 1.0));
    PRINT(to_int(root(2.0) * 1000000000.0));
    PRINT(to_int(root(1234567.0) * 1000.0));
    PRINT(to_int(root(0.0625) * 1000000.0));
    PRINT(to_int(magnitude(-123456.789) * 100.0));
    PRINT(to_int(magnitude(98.5) + negate(0.25) * 8.0));
    PRINT(to_int(narrow(1.0 / 3.0) * 1000000000.0));
    PRINT(to_int(from_int(-2000000000) / 7.0));
    PRINT(to_int(from_unsigned(4000000000u) / 3.0));
    // Past the ends of an int, and what is no number: the conversion keeps to the ends.
    PRINT(to_int(1e12) / 1000);
    PRINT(to_int(-1e12) / 1000);
    PRINT(to_int(zero / zero) / 1000);
    PRINT(to_int(-0.999));
    PRINT(to_int(2147483646.5) / 1000);
    float singles_of[8];
    double doubles_of[8];
    for (int n = 0; n < 8; n++) {
        singles_of[n] = (float)(n * n) + 0.25f;
        doubles_of[n] = from_int(n) * 1.5 - 4.0;
    }
    PRINT(to_int(sum(singles_of, 8) * 4.0f));
    scale(doubles_of, singles_of, 8);
    for (int n = 0; n < 8; n += 3) {
        PRINT(to_int(doubles_of[n] * 16.0));
    }
    syscall3(1, total & 0x7f, 0, 0);
    for (;;) {
    }
}
