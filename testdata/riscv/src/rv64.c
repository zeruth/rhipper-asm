// The RISC-V fixture: a freestanding program for RV64IM (the base integer instructions and the
// multiply and divide ones, none compressed), built without a C library. `build.ps1` keeps the
// disassembler's listing of it beside the program, which every word must decode to, and what it
// printed and how it ended under an emulator of the machine, which the lifted program must
// repeat. It prints through the Linux `write` call and ends by `exit`. A division by zero and
// the one division that overflows are here on purpose: the machine gives each a result.

typedef unsigned long u64;
typedef unsigned int u32;

static volatile u64 sink;

__attribute__((noinline)) u64 mix(u64 a, u64 b) {
    u64 r = a * 0x9e3779b97f4a7c15ul + (b ^ (a >> 31));
    r ^= r << 13;
    r = (r >> 7) | (r << 57);
    return r - (b & 0xff00ul) + (a | 1);
}

__attribute__((noinline)) long divide(long a, long b, u32 c, int d) {
    long q = a / b + a % b;
    u64 u = (u64)a / (u64)b + (u64)a % (u64)b;
    u32 w = c / 7 + c % 13 + (u32)(d * 3) + ((u32)d >> 2) + (u32)(d >> 3) + (c << 5);
    return q + (long)u + w + (a < b) + ((u64)a < (u64)b) + (d < 5) + (c < 9u);
}

__attribute__((noinline)) int count(const signed char *bytes, const short *halves, const int *words, const u64 *wide, int n) {
    int total = 0;
    for (int i = 0; i < n; i++) {
        total += bytes[i] + (unsigned char)bytes[i] + halves[i] + (unsigned short)halves[i] + words[i] + (int)(u32)words[i] + (int)wide[i];
    }
    return total;
}

__attribute__((noinline)) void store(signed char *bytes, short *halves, int *words, u64 *wide, int n, u64 seed) {
    for (int i = 0; i < n; i++) {
        seed = mix(seed, (u64)i);
        bytes[i] = (signed char)seed;
        halves[i] = (short)(seed >> 8);
        words[i] = (int)(seed >> 16);
        wide[i] = seed;
    }
}

__attribute__((noinline)) long pick(long k, long a, long b) {
    switch (k) {
        case 0: return a + b;
        case 1: return a - b;
        case 2: return a & b;
        case 3: return a | b;
        case 4: return a ^ b;
        case 5: return a << (b & 63);
        case 6: return a >> (b & 63);
        case 7: return (long)((u64)a >> (b & 63));
        default: return a >= b ? a : b;
    }
}

__attribute__((noinline)) int words32(int a, int b) {
    int r = a + b * 5 - (a << (b & 31)) + (a >> (b & 31)) + (int)((u32)a >> (b & 31));
    return r / (b | 1) + r % (b | 1) + (int)((u32)r / ((u32)b | 1)) + (int)((u32)r % ((u32)b | 1));
}

// The Linux system call: the number in a7, the arguments from a0, the result in a0.
static long syscall3(long number, long a, long b, long c) {
    register long a7 __asm__("a7") = number;
    register long a0 __asm__("a0") = a;
    register long a1 __asm__("a1") = b;
    register long a2 __asm__("a2") = c;
    __asm__ volatile("ecall" : "+r"(a0) : "r"(a7), "r"(a1), "r"(a2) : "memory");
    return a0;
}

// Writes a number and a line end to the standard output.
__attribute__((noinline)) static void print(long v) {
    char text[24];
    int at = 23;
    u64 u = v < 0 ? 0 - (u64)v : (u64)v;
    text[at] = '\n';
    do {
        text[--at] = (char)('0' + u % 10);
        u /= 10;
    } while (u);
    if (v < 0) {
        text[--at] = '-';
    }
    syscall3(64, 1, (long)(text + at), 24 - at);
}

void _start(void) {
    signed char bytes[8];
    short halves[8];
    int words[8];
    u64 wide[8];
    store(bytes, halves, words, wide, 8, 0x1234);
    long total = count(bytes, halves, words, wide, 8);
    print(total);
    for (long k = 0; k < 9; k++) {
        total += pick(k, total, k + 3);
        print(total);
    }
    print(divide(total, 17, (u32)total, (int)total));
    print(divide(-total, -3, 5, -9) + divide(7, 0, 0, 0) + divide((long)(1ul << 63), -1, 1, 1));
    print(words32((int)total, 9));
    print((long)mix((u64)total, 99));
    sink = (u64)total;
    syscall3(93, total & 0x7f, 0, 0);
    for (;;) {
    }
}
