// The 64-bit PowerPC fixture: the integer program of the SH-4 fixture
// (`testdata/sh4/src/sh4.c`), built for big-endian 64-bit PowerPC without a C library, for the
// lift of the processors of the later consoles. Only the system call is the machine's own:
// `sc`, the call's number in r0, its arguments from r3 and the result in r3.

#define HAVE_SYSCALL3
static long syscall3(long number, long a, long b, long c) {
    register long r0 __asm__("r0") = number;
    register long r3 __asm__("r3") = a;
    register long r4 __asm__("r4") = b;
    register long r5 __asm__("r5") = c;
    __asm__ volatile("sc" : "+r"(r3) : "r"(r0), "r"(r4), "r"(r5) : "memory", "cr0", "ctr", "r6", "r7", "r8", "r9", "r10", "r11", "r12");
    return r3;
}

#include "../../sh4/src/sh4.c"
