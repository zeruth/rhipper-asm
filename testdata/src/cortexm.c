// The Cortex-M fixture: firmware for an ARMv7-M microcontroller, as a flat image whose first words
// are the vector table (the initial stack pointer, then the reset handler and the exception
// handlers, each with its low bit set: the processor has only Thumb). The reset handler copies
// the initialized data from the image to RAM, clears the rest, and calls `main`, which prints
// through the debugger's semihosting call (`bkpt 0xab`, the operation in r0 and its argument in
// r1) and ends by it. No C library, no operating system: the vector table is all that names code.

typedef unsigned int u32;

extern u32 __data_load, __data_start, __data_end, __bss_start, __bss_end, __stack_top;

// The semihosting call: the operation in r0, its argument in r1, the result in r0.
static int semihost(int operation, const void *argument) {
    register int r0 __asm__("r0") = operation;
    register const void *r1 __asm__("r1") = argument;
    __asm__ volatile("bkpt #0xab" : "+r"(r0) : "r"(r1) : "memory");
    return r0;
}

static void put(const char *text) { semihost(0x04, text); } // SYS_WRITE0

static const int values[5] = {3, 1, 4, 1, 5};
static int scale = 3;       // initialized data: copied from the image at reset
static int counted;         // cleared at reset
static volatile u32 ticks;  // what the tick handler counts

__attribute__((noinline)) static int sum(const int *v, int n) {
    int s = 0;
    for (int i = 0; i < n; i++) {
        s += v[i] * scale;
        counted++;
    }
    return s;
}

__attribute__((noinline)) static int larger(int a, int b) { return a > b ? a : b; }

static void number(int v) {
    char digits[12];
    int at = 11;
    digits[at] = 0;
    do {
        digits[--at] = (char)('0' + v % 10);
        v /= 10;
    } while (v);
    put(digits + at);
}

// The system timer: control and status, the reload value, the current value.
#define SYSTICK ((volatile u32 *)0xe000e010)

int main(void) {
    put("hello from the cortex-m\n");
    number(sum(values, 5));
    put(" ");
    number(larger(counted, 4));
    put(" ");
    // The timer counts down from 999 and raises its exception at zero, three times; the handler
    // runs between two instructions of this loop, which only waits for it.
    SYSTICK[1] = 999;
    SYSTICK[2] = 0;
    SYSTICK[0] = 7;
    while (ticks < 3) {
    }
    SYSTICK[0] = 0;
    number((int)ticks);
    put("\n");
    return 0;
}

void tick(void) { ticks++; }

void fault(void) {
    for (;;) {
    }
}

void reset(void) {
    u32 *from = &__data_load;
    for (u32 *to = &__data_start; to < &__data_end;) {
        *to++ = *from++;
    }
    for (u32 *to = &__bss_start; to < &__bss_end;) {
        *to++ = 0;
    }
    main();
    // SYS_EXIT with ADP_Stopped_ApplicationExit: the program is over.
    semihost(0x18, (void *)0x20026);
    for (;;) {
    }
}

// The vector table: the stack's top, reset, NMI, HardFault, the reserved and system entries, and
// SysTick as the fifteenth exception.
__attribute__((section(".vectors"), used)) static void *const vectors[16] = {
    &__stack_top, reset, fault, fault, fault, fault, fault, 0, 0, 0, 0, fault, fault, 0, fault, tick,
};
