// Control flow, data and the FPU on the SH-4 for the decoder: loops, a dense switch, recursion, a
// table of function pointers, division through the runtime's helpers, single-precision arithmetic,
// and the forms the compiler does not choose on its own (the register banks, `fipr`, `ftrv` and the
// double mode by `fpscr.pr`) in inline assembly. Freestanding: output goes through an external
// `write`, which the decoder never follows.

#define NOINLINE __attribute__((noinline))

extern int write(int fd, const void *buf, unsigned len);
extern void exit(int code) __attribute__((noreturn));

static int table[10] = {5, 3, 8, 1, 9, 2, 7, 4, 6, 0};
static volatile int sink;

NOINLINE static void print_str(const char *s) {
    const char *p = s;
    while (*p) p++;
    write(1, s, (unsigned)(p - s));
}

NOINLINE static void print_int(int v) {
    char buf[16];
    char *p = buf + sizeof buf;
    unsigned u = v < 0 ? (unsigned)(-v) : (unsigned)v;
    *--p = '\n';
    do {
        *--p = (char)('0' + u % 10);
        u /= 10;
    } while (u);
    if (v < 0) *--p = '-';
    write(1, p, (unsigned)(buf + sizeof buf - p));
}

NOINLINE __attribute__((noreturn)) static void fail(int code) {
    print_str("fail\n");
    exit(code);
}

NOINLINE static int collatz(int n) {
    int steps = 0;
    while (n != 1) {
        n = (n & 1) ? 3 * n + 1 : n / 2;
        steps++;
    }
    return steps;
}

NOINLINE static int fib(int n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }

NOINLINE static int classify(int x) {
    switch (x) {
    case 0: print_str("A"); return 10;
    case 1: print_str("B"); sink += 2; return 11;
    case 2: print_str("C"); sink *= 3; return 12;
    case 3: print_str("D"); return collatz(27);
    case 4: print_str("E"); sink ^= 7; return 14;
    case 5: print_str("F"); return fib(10);
    case 6: print_str("G"); sink -= 1; return 16;
    case 7: print_str("H"); return table[x];
    case 8: print_str("I"); sink |= 9; return 18;
    default: print_str("?"); return -1;
    }
}

NOINLINE static int op_add(int a, int b) { return a + b; }
NOINLINE static int op_sub(int a, int b) { return a - b; }
NOINLINE static int op_mul(int a, int b) { return a * b; }
NOINLINE static int op_div(int a, int b) {
    if (b == 0) fail(3);
    return a / b;
}

static int (*const ops[4])(int, int) = {op_add, op_sub, op_mul, op_div};

NOINLINE static void sort_table(void) {
    for (int i = 1; i < 10; i++) {
        int v = table[i], j = i - 1;
        while (j >= 0 && table[j] > v) {
            table[j + 1] = table[j];
            j--;
        }
        table[j + 1] = v;
    }
}

NOINLINE static unsigned mix(unsigned a, unsigned b) {
    unsigned h = a * 2654435761u;
    h ^= b >> 3;
    h = (h << 7) | (h >> 25);
    return h ^ (h >> 16);
}

NOINLINE static long long wide(long long a, int s) { return (a << s) + (a >> 3) - (unsigned long long)a / 7; }

NOINLINE static short narrow(const unsigned char *p, const short *q) { return (short)(p[0] + p[1] * 3 + q[2] - (signed char)p[3]); }

NOINLINE static float poly(float x) {
    float acc = 0.0f;
    for (int i = 0; i < 4; i++) acc = acc * x + (float)table[i];
    return x > 1.5f ? acc / x : -acc;
}

NOINLINE static int to_int(float a, float b) { return (int)(a * b - b) + (a == b) + (a < b); }

// The inner product of two vectors and a vector by the back bank's matrix.
NOINLINE static float dot(const float *a, const float *b) {
    float r;
    __asm__ volatile("fmov.s @%1+,fr0\n\tfmov.s @%1+,fr1\n\tfmov.s @%1+,fr2\n\tfmov.s @%1+,fr3\n\t"
                     "fmov.s @%2+,fr4\n\tfmov.s @%2+,fr5\n\tfmov.s @%2+,fr6\n\tfmov.s @%2+,fr7\n\t"
                     "fipr fv0,fv4\n\tfmov fr7,%0"
                     : "=f"(r), "+r"(a), "+r"(b)
                     :
                     : "fr0", "fr1", "fr2", "fr3", "fr4", "fr5", "fr6", "fr7");
    return r;
}

NOINLINE static void transform(float *v) {
    __asm__ volatile("frchg\n\tfldi1 fr0\n\tfldi0 fr1\n\tfrchg\n\t"
                     "fmov.s @%0+,fr8\n\tfmov.s @%0+,fr9\n\tfmov.s @%0+,fr10\n\tfmov.s @%0+,fr11\n\t"
                     "ftrv xmtrx,fv8\n\tfmov.s fr11,@-%0\n\tfmov.s fr10,@-%0\n\tfmov.s fr9,@-%0\n\tfmov.s fr8,@-%0"
                     : "+r"(v)
                     :
                     : "fr8", "fr9", "fr10", "fr11", "memory");
}

// The double mode: `fpscr.pr` set, the pairs `dr0` and `dr2` added, converted and the mode put back.
NOINLINE static int twice(int a) {
    int r;
    __asm__ volatile("sts fpscr,r1\n\tmov r1,r2\n\tmov.l 1f,r3\n\tor r3,r2\n\tlds r2,fpscr\n\t"
                     "lds %1,fpul\n\tfloat fpul,dr0\n\tfmov dr0,dr2\n\tfadd dr0,dr2\n\tfmul dr2,dr2\n\tfsqrt dr2\n\t"
                     "fcmp/gt dr0,dr2\n\tfcnvds dr2,fpul\n\tfcnvsd fpul,dr4\n\tftrc dr2,fpul\n\tsts fpul,%0\n\t"
                     "fschg\n\tfmov dr0,xd2\n\tfmov xd2,@-r15\n\tfmov @r15+,dr4\n\tfschg\n\t"
                     "lds r1,fpscr\n\tbra 2f\n\tnop\n\t.align 2\n1:\t.long 0x80000\n2:"
                     : "=r"(r)
                     : "r"(a)
                     : "r1", "r2", "r3", "fr0", "fr1", "fr2", "fr3", "fr4", "fr5", "t", "memory");
    return r;
}

// The system and control forms, the banked registers and the cache operations.
NOINLINE static unsigned control(unsigned *p) {
    unsigned sr, r;
    __asm__ volatile("stc sr,%0\n\tstc r0_bank,%1\n\tldc %1,r1_bank\n\tstc.l gbr,@-r15\n\tldc.l @r15+,gbr\n\t"
                     "mov.l @(4,gbr),r0\n\tmov.w r0,@(2,gbr)\n\tmovca.l r0,@%2\n\tocbwb @%2\n\tpref @%2\n\t"
                     "clrmac\n\tmac.w @%2+,@%2+\n\tsts macl,r0\n\tsett\n\tmovt %1\n\tclrt\n\tclrs\n\tsets\n\t"
                     "tas.b @%2\n\tswap.w %1,%1\n\txtrct %0,%1\n\tdt %0\n\trotcl %1\n\tshll16 %1\n\tsleep"
                     : "=&r"(sr), "=&r"(r), "+r"(p)
                     :
                     : "r0", "t", "macl", "mach", "memory");
    return sr ^ r;
}

int main(void) {
    print_int(collatz(27));
    print_int(fib(15));
    int total = 0;
    for (int i = 0; i <= 9; i++) total += classify(i);
    print_str("\n");
    print_int(total);
    for (int i = 0; i < 4; i++) print_int(ops[i](42, 6));
    sort_table();
    print_int((int)(mix(123456789u, 987654321u) % 100000u));
    print_int((int)wide(0x123456789ll, 5));
    static const unsigned char bytes[4] = {1, 2, 3, 0xff};
    static const short shorts[3] = {0, 0, -7};
    print_int(narrow(bytes, shorts));
    print_int(to_int(poly(2.0f), 0.5f));
    float a[4] = {1, 2, 3, 4}, b[4] = {4, 3, 2, 1};
    print_int((int)dot(a, b));
    transform(a);
    print_int(twice(sink));
    unsigned word = 7;
    print_int((int)(control(&word) & 0xff));
    if (sink == 0x7fffffff) fail(2);
    print_int(sink);
    return 0;
}
