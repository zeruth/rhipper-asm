// The Thumb fixture of the handhelds' processors: the integer program of the SH-4 fixture
// (`testdata/sh4/src/sh4.c`), built for ARMv4T in Thumb, the instruction set of an ARM7TDMI
// before Thumb-2: sixteen-bit instructions alone but for the two halves of a `bl`, no IT blocks,
// no divide, no wide multiply. Only the system call is the machine's own: `svc 0`, the call's
// number in r7, its arguments from r0 and the result in r0. The compiler leaves the divisions
// to a library the program does not have, so they are here.

typedef unsigned int u32;

#define HAVE_SYSCALL3
static long syscall3(long number, long a, long b, long c) {
    register long r7 __asm__("r7") = number;
    register long r0 __asm__("r0") = a;
    register long r1 __asm__("r1") = b;
    register long r2 __asm__("r2") = c;
    __asm__ volatile("svc #0" : "+r"(r0) : "r"(r7), "r"(r1), "r"(r2) : "memory");
    return r0;
}

// The quotient and the remainder by shifting and subtracting.
__attribute__((noinline)) static u32 divide_unsigned(u32 n, u32 d, u32 *remainder) {
    u32 q = 0, r = 0;
    for (int bit = 31; bit >= 0; bit--) {
        r = (r << 1) | ((n >> bit) & 1);
        if (r >= d) {
            r -= d;
            q |= 1u << bit;
        }
    }
    *remainder = r;
    return q;
}

u32 __aeabi_uidiv(u32 n, u32 d) {
    u32 r;
    return divide_unsigned(n, d, &r);
}

int __aeabi_idiv(int n, int d) {
    u32 r;
    u32 q = divide_unsigned(n < 0 ? 0u - (u32)n : (u32)n, d < 0 ? 0u - (u32)d : (u32)d, &r);
    return (n < 0) != (d < 0) ? (int)(0u - q) : (int)q;
}

// The product of two sixty-four-bit numbers from the halves of their words, sixteen bits at a
// time: the processor's multiply keeps the low word of a product alone.
unsigned long long __aeabi_lmul(unsigned long long a, unsigned long long b) {
    u32 al = (u32)a, ah = (u32)(a >> 32), bl = (u32)b, bh = (u32)(b >> 32);
    u32 a0 = al & 0xffff, a1 = al >> 16, b0 = bl & 0xffff, b1 = bl >> 16;
    u32 low = a0 * b0, middle = a1 * b0, other = a0 * b1, high = a1 * b1;
    u32 carry = (low >> 16) + (middle & 0xffff) + (other & 0xffff);
    low = (low & 0xffff) | (carry << 16);
    high += (middle >> 16) + (other >> 16) + (carry >> 16);
    high += al * bh + ah * bl;
    return ((unsigned long long)high << 32) | low;
}

#include "../sh4/src/sh4.c"
