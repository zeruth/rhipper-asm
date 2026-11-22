// The MSP430 fixture: the integer program of the SH-4 fixture (`testdata/sh4/src/sh4.c`) built
// for the 16-bit microcontroller, with no library and no system under it.
//
// The machine's `int` is sixteen bits, and the program is written for one of thirty-two: it is
// compiled with `int` read as `long`, so its arithmetic is the arithmetic every other machine's
// fixture does, and what it prints is what they print.
//
// A microcontroller's program has no system to call. It prints through a serial port's
// transmit register, a byte a store (0x060e, where the F5 series has its second port's), and
// it ends by leaving its code in the first output port's register (0x0202) and stopping the
// processor: the bits CPUOFF, OSCOFF, SCG0 and SCG1 of the status register set and interrupts
// off, which is the mode nothing wakes it from.
//
// The compiler leaves the multiplies, the divide and the shifts of a long by a count to the
// run-time library, which is not here: they are below, written so that none of them needs
// another (a shift by one, an add, a compare).

#define HAVE_SYSCALL3
static long syscall3(long number, long a, long b, long c) {
    if (number == 4) {
        const char *text = (const char *)(unsigned)b;
        for (long n = 0; n < c; n++) {
            *(volatile unsigned char *)0x060e = (unsigned char)text[n];
        }
        return c;
    }
    *(volatile unsigned char *)0x0202 = (unsigned char)a;
    __asm__ volatile("bis #0xf0, r2");
    for (;;) {
    }
}

#define HELPER __attribute__((noinline, optnone, used))

// The shifts of a long by a count, one place a turn.
HELPER unsigned long __mspabi_slll(unsigned long value, unsigned count) {
    while (count != 0) {
        value = value + value;
        count = count - 1;
    }
    return value;
}

HELPER unsigned long __mspabi_srll(unsigned long value, unsigned count) {
    while (count != 0) {
        value = value >> 1;
        count = count - 1;
    }
    return value;
}

HELPER long __mspabi_sral(long value, unsigned count) {
    while (count != 0) {
        value = value >> 1;
        count = count - 1;
    }
    return value;
}

// The product of two longs, by adding the one where the other has a bit.
HELPER unsigned long __mspabi_mpyl(unsigned long a, unsigned long b) {
    unsigned long product = 0;
    while (b != 0) {
        if (b & 1) {
            product = product + a;
        }
        a = a + a;
        b = b >> 1;
    }
    return product;
}

// The quotient of two longs without their signs, a bit a turn from the top.
HELPER unsigned long __mspabi_divul(unsigned long a, unsigned long b) {
    unsigned long quotient = 0, rest = 0;
    unsigned turn = 32;
    while (turn != 0) {
        rest = (rest + rest) | (a >> 31);
        a = a + a;
        quotient = quotient + quotient;
        if (rest >= b) {
            rest = rest - b;
            quotient = quotient | 1;
        }
        turn = turn - 1;
    }
    return quotient;
}

// The product of two long longs, which the compiler asks for in registers of its own: the one
// in r8 to r11, the other in r12 to r15, the product in r12 to r15. The stub hands them to the
// function below as any function takes them.
HELPER unsigned long long multiply_wide(unsigned long long a, unsigned long long b) {
    unsigned long long product = 0;
    unsigned turn = 64;
    while (turn != 0) {
        if (b & 1) {
            product = product + a;
        }
        a = a + a;
        b = b >> 1;
        turn = turn - 1;
    }
    return product;
}

__asm__(".text\n"
        ".global __mspabi_mpyll\n"
        "__mspabi_mpyll:\n"
        "    push r15\n"
        "    push r14\n"
        "    push r13\n"
        "    push r12\n"
        "    mov r8, r12\n"
        "    mov r9, r13\n"
        "    mov r10, r14\n"
        "    mov r11, r15\n"
        "    call #multiply_wide\n"
        "    add #8, r1\n"
        "    ret\n");

#define int long
#include "../../sh4/src/sh4.c"
