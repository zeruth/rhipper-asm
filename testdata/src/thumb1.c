// The Thumb fixture of the handhelds' processors: the integer program of the SH-4 fixture
// (`testdata/sh4/src/sh4.c`), built for ARMv4T in Thumb, the instruction set of an ARM7TDMI
// before Thumb-2: sixteen-bit instructions alone but for the two halves of a `bl`, no IT blocks,
// no divide, no wide multiply. Only the system call is the machine's own: `svc 0`, the call's
// number in r7, its arguments from r0 and the result in r0. The compiler leaves the divisions
// to a library the program does not have, so they are here.

typedef unsigned int u32;

#define HAVE_SYSCALL3
#ifndef GBA
static long syscall3(long number, long a, long b, long c) {
    register long r7 __asm__("r7") = number;
    register long r0 __asm__("r0") = a;
    register long r1 __asm__("r1") = b;
    register long r2 __asm__("r2") = c;
    __asm__ volatile("svc #0" : "+r"(r0) : "r"(r7), "r"(r1), "r"(r2) : "memory");
    return r0;
}
#else
// The program as a Game Boy Advance cartridge (`-DGBA`, the fixture `gba.gba`): the ROM's header
// first, then the code the console enters, in ARM code, which sets the stack and goes to the
// Thumb program. The header's logo is the console maker's and is left empty, so the image is
// for an emulator and this project's run, not for the console's own boot code. The program has
// no data to copy to RAM and none to clear.
__asm__(".section .header,\"ax\",%progbits\n"
        ".arm\n"
        ".global rom_start\n"
        "rom_start:\n"
        "    b reset\n"
        "    .space 156\n"
        "    .ascii \"RHIPPER ASM \"\n"
        "    .ascii \"ARAE\"\n"
        "    .ascii \"00\"\n"
        "    .byte 0x96, 0, 0\n"
        "    .space 7\n"
        "    .byte 0\n"
        "    .byte 0\n" // the header's complement, which the build writes
        "    .space 2\n"
        "reset:\n"
        "    ldr sp, =0x03007f00\n"
        "    ldr r0, =_start\n"
        "    bx r0\n"
        "    .ltorg\n"
        ".text\n"
        ".thumb\n");

// The system under the program is the console's BIOS and an emulator's log. A write is a line
// for the log (`swi 0xff`, the text r0 points at, to its zero); the end is the BIOS's Stop
// (`swi 3`), which puts the console to sleep.
static long syscall3(long number, long a, long b, long c) {
    if (number == 4) {
        char text[24];
        const char *from = (const char *)b;
        int n = 0;
        for (; n < c && n < 23; n++) {
            text[n] = from[n];
        }
        text[n] = 0;
        register long r0 __asm__("r0") = (long)text;
        __asm__ volatile("svc #0xff" : "+r"(r0) : : "r1", "r2", "r3", "memory");
        return c;
    }
    register long r0 __asm__("r0") = a;
    __asm__ volatile("svc #3" : "+r"(r0) : : "r1", "r2", "r3", "memory");
    return r0;
}
#endif

// A program of both instruction sets (`-DMIXED`, the fixture `armthumb`): the program is Thumb,
// the arithmetic it calls for is ARM code, and it is entered through ARM code, so a call goes
// each way across the change of state.
#ifdef MIXED
#define ARM_CODE __attribute__((target("arm")))
#else
#define ARM_CODE
#endif

// The quotient and the remainder by shifting and subtracting.
ARM_CODE __attribute__((noinline)) static u32 divide_unsigned(u32 n, u32 d, u32 *remainder) {
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

ARM_CODE u32 __aeabi_uidiv(u32 n, u32 d) {
    u32 r;
    return divide_unsigned(n, d, &r);
}

ARM_CODE int __aeabi_idiv(int n, int d) {
    u32 r;
    u32 q = divide_unsigned(n < 0 ? 0u - (u32)n : (u32)n, d < 0 ? 0u - (u32)d : (u32)d, &r);
    return (n < 0) != (d < 0) ? (int)(0u - q) : (int)q;
}

// The product of two sixty-four-bit numbers from the halves of their words, sixteen bits at a
// time: the processor's multiply keeps the low word of a product alone.
ARM_CODE unsigned long long __aeabi_lmul(unsigned long long a, unsigned long long b) {
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

#ifdef MIXED
// The entry of the program of both sets: ARM code that calls the Thumb program.
ARM_CODE void start_arm(void) {
    _start();
    for (;;) {
    }
}
#endif
