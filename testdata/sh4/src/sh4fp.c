// The SH-4 floating-point fixture: the program of the PowerPC's floating-point fixture
// (`testdata/ppc/src/ppcfp.c`), arithmetic that prints whole numbers, built for the SH-4 with
// every floating-point number a single, as most of the Dreamcast's own code keeps them: the
// unit stays in its single-precision mode and a `double` of the source is a single here, so
// the numbers it prints are this machine's own. Only the system call is the machine's: `trapa`,
// the call's number in r3, its arguments from r4 and the result in r0.

// The program is entered here: the unit's status register is cleared, which is the mode this
// code is built for (single precision, a register a single), and the program begins.
__asm__(".text\n"
        ".global _fp_start\n"
        "_fp_start:\n"
        "    mov #0, r0\n"
        "    lds r0, fpscr\n"
        "    mov.l 1f, r0\n"
        "    jmp @r0\n"
        "    nop\n"
        "    .align 2\n"
        "1:  .long __start\n");

#define HAVE_SYSCALL3
static long syscall3(long number, long a, long b, long c) {
    register long r3 __asm__("r3") = number;
    register long r4 __asm__("r4") = a;
    register long r5 __asm__("r5") = b;
    register long r6 __asm__("r6") = c;
    register long r0 __asm__("r0");
    __asm__ volatile("trapa #0x13" : "=r"(r0) : "r"(r3), "r"(r4), "r"(r5), "r"(r6) : "memory");
    return r0;
}

#include "../../ppc/src/ppcfp.c"
