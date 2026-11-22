// The ARM64 fixture that speaks to its system itself: the integer program of the SH-4 fixture
// (`testdata/sh4/src/sh4.c`), a static program for ARM64 without a C library. Only the system
// call is the machine's own. As a Linux program (`a64sys`) it is `svc 0`, the call's number in
// x8 by the numbering the newer machines share (64 to write, 93 to end), its arguments from x0
// and the result in x0. As a program of the Switch's system (`-DHORIZON`, the fixture
// `a64.nro`) the instruction's own number names the call: `svc 0x27` writes a line to the
// debugger's log, the text x0 points at and its length in x1, and `svc 7` ends the process.

#ifdef HORIZON
// The start of a program of the Switch's system as an NRO file has it: a branch over the file's
// header, which lies in the code's first bytes and which the build writes, to the code that
// calls the program. The system gives the process its stack.
__asm__(".section .nro,\"ax\",%progbits\n"
        ".global nro_start\n"
        "nro_start:\n"
        "    b nro_entry\n"
        "    .word 0\n"
        "    .space 8\n"
        "    .space 0x70\n"
        "nro_entry:\n"
        "    bl _start\n"
        "    b .\n"
        ".text\n");
#endif

#define HAVE_SYSCALL3
static long syscall3(long number, long a, long b, long c) {
#ifdef HORIZON
    if (number == 4) {
        register long x0 __asm__("x0") = b;
        register long x1 __asm__("x1") = c;
        __asm__ volatile("svc #0x27" : "+r"(x0) : "r"(x1) : "memory", "x2", "x3", "x4", "x5", "x6", "x7");
        return c;
    }
    register long x0 __asm__("x0") = a;
    __asm__ volatile("svc #7" : "+r"(x0) : : "memory");
    return x0;
#else
    register long x8 __asm__("x8") = number == 4 ? 64 : 93;
    register long x0 __asm__("x0") = a;
    register long x1 __asm__("x1") = b;
    register long x2 __asm__("x2") = c;
    __asm__ volatile("svc #0" : "+r"(x0) : "r"(x8), "r"(x1), "r"(x2) : "memory");
    return x0;
#endif
}

#include "../sh4/src/sh4.c"
