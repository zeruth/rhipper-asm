// The SH multiply-and-accumulate fixture: sums of products by `mac.l` and `mac.w`, which a
// compiler does not write and the earlier consoles' hand-written code does. Each sum's two
// halves, `mach` and `macl`, are printed, through the Linux `write` call as the integer
// fixture prints (`sh4.c`), and the program ends with the low bits of their total.
//
// The fixture as built sums by `mac.l` only: the emulator it is held to (qemu 8.2) faults on a
// `mac.w` of an address that is not a multiple of four, which every second one is. `-DWORDS`
// builds the sums by `mac.w` in as well, for a machine or an emulator that runs them.

typedef unsigned int u32;

static long syscall3(long number, long a, long b, long c) {
    register long r3 __asm__("r3") = number;
    register long r4 __asm__("r4") = a;
    register long r5 __asm__("r5") = b;
    register long r6 __asm__("r6") = c;
    register long r0 __asm__("r0");
    __asm__ volatile("trapa #0x13" : "=r"(r0) : "r"(r3), "r"(r4), "r"(r5), "r"(r6) : "memory");
    return r0;
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

// long_sum(a, b, count, out): the sum of the products of two rows of words, its high half
// into out[0] and its low into out[1]. word_sum is the same over rows of halves.
__asm__(".text\n"
        ".align 1\n"
        "_long_sum:\n"
        "    clrmac\n"
        "1:  mac.l @r4+, @r5+\n"
        "    dt r6\n"
        "    bf 1b\n"
        "    sts mach, r0\n"
        "    mov.l r0, @r7\n"
        "    sts macl, r0\n"
        "    rts\n"
        "    mov.l r0, @(4, r7)\n"
        "_word_sum:\n"
        "    clrmac\n"
        "1:  mac.w @r4+, @r5+\n"
        "    dt r6\n"
        "    bf 1b\n"
        "    sts mach, r0\n"
        "    mov.l r0, @r7\n"
        "    sts macl, r0\n"
        "    rts\n"
        "    mov.l r0, @(4, r7)\n");

void long_sum(const int *a, const int *b, int count, int *out);
void word_sum(const short *a, const short *b, int count, int *out);

static const int longs_a[6] = {0x12345678, -0x0fedcba9, 0x7fffffff, -0x7fffffff - 1, 3, -1};
static const int longs_b[6] = {0x0badf00d, 0x13579bdf, 0x7fffffff, -0x7fffffff - 1, -5, -1};
#ifdef WORDS
static const short words_a[6] = {0x1234, -0x0fed, 0x7fff, -0x8000, 3, -1};
static const short words_b[6] = {0x0bad, 0x1357, 0x7fff, -0x8000, -5, -1};
#endif

void _start(void) {
    int out[2];
    int total = 0;
    for (int count = 1; count <= 6; count++) {
        long_sum(longs_a, longs_b, count, out);
        print(out[0]);
        print(out[1]);
        total += out[0] + out[1];
#ifdef WORDS
        word_sum(words_a, words_b, count, out);
        print(out[0]);
        print(out[1]);
        total += out[0] + out[1];
#endif
    }
    syscall3(1, total & 0x7f, 0, 0);
    for (;;) {
    }
}
