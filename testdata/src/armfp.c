// The floating-point fixture of 32-bit ARM: the program of the PowerPC's floating-point fixture
// (`testdata/ppc/src/ppcfp.c`), single and double arithmetic that prints whole numbers, built
// for ARMv6 with the VFP unit and its registers for arguments, as a handheld's ARM11 runs it.
// Only the system call is the machine's own: `svc 0`, the call's number in r7, its arguments
// from r0 and the result in r0. The divisions of integers the compiler leaves to a library are
// here: the processor has none.

#define HAVE_SYSCALL3
static long syscall3(long number, long a, long b, long c) {
    register long r7 __asm__("r7") = number;
    register long r0 __asm__("r0") = a;
    register long r1 __asm__("r1") = b;
    register long r2 __asm__("r2") = c;
    __asm__ volatile("svc #0" : "+r"(r0) : "r"(r7), "r"(r1), "r"(r2) : "memory");
    return r0;
}

// The quotient by shifting and subtracting.
unsigned __aeabi_uidiv(unsigned n, unsigned d) {
    unsigned q = 0, r = 0;
    for (int bit = 31; bit >= 0; bit--) {
        r = (r << 1) | ((n >> bit) & 1);
        if (r >= d) {
            r -= d;
            q |= 1u << bit;
        }
    }
    return q;
}

int __aeabi_idiv(int n, int d) {
    unsigned q = __aeabi_uidiv(n < 0 ? 0u - (unsigned)n : (unsigned)n, d < 0 ? 0u - (unsigned)d : (unsigned)d);
    return (n < 0) != (d < 0) ? (int)(0u - q) : (int)q;
}

#include "../ppc/src/ppcfp.c"
