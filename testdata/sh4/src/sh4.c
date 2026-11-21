// The SH-4 fixture: a freestanding little-endian program of integer code, built without a C
// library, for the lift of the processor. `build.sh` keeps the disassembler's listing of it
// beside the program, and what it printed and how it ended under an emulator of the machine,
// which the lifted program must repeat. It prints through the Linux `write` call and ends by
// `exit`: the call's number in r3, its arguments from r4, the result in r0, by `trapa`.

typedef unsigned int u32;
typedef unsigned long long u64;

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

__attribute__((noinline)) u32 mix(u32 a, u32 b) {
    u32 r = a * 0x9e3779b9u + (b ^ (a >> 15));
    r ^= r << 13;
    r = (r >> 7) | (r << 25);
    return r - (b & 0xff00u) + (a | 1);
}

__attribute__((noinline)) int shifts(int a, int b) {
    return (a << (b & 31)) + (a >> (b & 31)) + (int)((u32)a >> (b & 31)) + (a << 3) + (a >> 9) + (int)((u32)a >> 17);
}

__attribute__((noinline)) int compare(int a, int b, u32 c, u32 d) {
    return (a < b) + 2 * (a >= b) + 4 * (c < d) + 8 * (c >= d) + 16 * (a == b) + 32 * (a > 0) + 64 * (a >= 0);
}

__attribute__((noinline)) int count(const signed char *bytes, const short *halves, const int *words, int n) {
    int total = 0;
    for (int i = 0; i < n; i++) {
        total += bytes[i] + (unsigned char)bytes[i] + halves[i] + (unsigned short)halves[i] + words[i];
    }
    return total;
}

__attribute__((noinline)) void store(signed char *bytes, short *halves, int *words, int n, u32 seed) {
    for (int i = 0; i < n; i++) {
        seed = mix(seed, (u32)i);
        bytes[i] = (signed char)seed;
        halves[i] = (short)(seed >> 8);
        words[i] = (int)(seed >> 3);
    }
}

__attribute__((noinline)) int pick(int k, int a, int b) {
    switch (k) {
        case 0: return a + b;
        case 1: return a - b;
        case 2: return a & b;
        case 3: return a | b;
        case 4: return a ^ b;
        case 5: return -a;
        case 6: return ~a;
        case 7: return a * b;
        default: return a >= b ? a : b;
    }
}

__attribute__((noinline)) u64 wide(u32 a, u32 b, int c, int d) {
    return (u64)a * b + (u64)((long long)c * d) + ((u64)a << 32) + b;
}

void _start(void) {
    signed char bytes[8];
    short halves[8];
    int words[8];
    store(bytes, halves, words, 8, 0x1234);
    int total = count(bytes, halves, words, 8);
    print(total);
    for (int k = 0; k < 9; k++) {
        total += pick(k, total, k + 3);
        print(total);
    }
    print(shifts(total, 5) + shifts(-total, 29));
    print(compare(total, 7, (u32)total, 7u) + compare(-3, -3, 1u, 2u));
    u64 w = wide((u32)total, 0x89abcdefu, total, -77);
    print((int)w);
    print((int)(w >> 32));
    print((int)mix((u32)total, 99));
    syscall3(1, total & 0x7f, 0, 0);
    for (;;) {
    }
}
