// The 32-bit ARM half of the vector harness: the trampoline that loads a case's registers and
// flags and enters the case page in ARM or Thumb state, and the start, the system call and the
// cache maintenance a freestanding guest needs. Every case ends in a signal (the `bkpt` after the
// instruction, the `bkpt` fill a branch lands on, or the instruction's own fault); `handler` in
// guest.c records the machine's state from the signal context and resumes at `run_case_done`,
// back in ARM state.
//
// The guest is built with -marm: everything here is ARM code, and only the case page runs Thumb.
        .syntax unified
        .arch armv7-a
        .arm

// The layout of `struct case_in` in guest.c, which the vectors' generator writes.
        .set IN_R, 0
        .set IN_NZCV, 64
        .set IN_THUMB, 68

// The case page: a `bkpt` fill with the prologue `ldr r12, [r0, #48]; ldr r0, [r0]` (in the
// state of the case) just before the instruction, so the jump into the page keeps two registers
// free and the prologue loads them.
        .set CASE_IP, 0x30002000
        .set CASE_PROLOGUE, CASE_IP - 8

        .text
        .balign 4

// The entry of a static freestanding guest: argc and argv off the initial stack to guest_main,
// whose return value is the exit status.
        .globl _start
        .type _start, %function
_start:
        ldr     r0, [sp]
        add     r1, sp, #4
        bl      guest_main
        mov     r7, #248
        svc     #0
        b       .

// long sys_call(long number, long a0, long a1, long a2, long a3, long a4, long a5): the number
// in r7, the first three arguments in r1..r3 and the rest on the caller's stack.
        .globl sys_call
        .type sys_call, %function
sys_call:
        push    {r4, r5, r7}
        mov     r7, r0
        mov     r0, r1
        mov     r1, r2
        mov     r2, r3
        ldr     r3, [sp, #12]
        ldr     r4, [sp, #16]
        ldr     r5, [sp, #20]
        svc     #0
        pop     {r4, r5, r7}
        bx      lr

// void flush_code(unsigned long begin, unsigned long end): the ARM-private cacheflush call over
// [begin, end), so a written case executes as written.
        .globl flush_code
        .type flush_code, %function
flush_code:
        push    {r7}
        mov     r2, #0
        movw    r7, #0x0002
        movt    r7, #0x000f
        svc     #0
        pop     {r7}
        bx      lr

// void run_case(const struct case_in *in): the host's callee-saved registers, stack pointer and
// return address go to `saved`, every register of the case is loaded but r0 and r12, and the
// case page is entered at its prologue (Thumb when the case says so), which loads those two. The
// case never comes back by itself: the signal handler puts `saved` back into the context and
// resumes at run_case_done.
        .globl run_case
        .type run_case, %function
run_case:
        movw    r12, :lower16:saved
        movt    r12, :upper16:saved
        stmia   r12, {r4-r11}
        str     sp, [r12, #32]
        str     lr, [r12, #36]
        ldr     r1, [r0, #IN_NZCV]
        lsl     r1, r1, #28
        msr     APSR_nzcvq, r1
        ldr     r12, [r0, #IN_THUMB]
        movw    r1, #(CASE_PROLOGUE & 0xffff)
        movt    r1, #((CASE_PROLOGUE >> 16) & 0xffff)
        orr     r12, r12, r1
        ldr     sp, [r0, #IN_R + 52]
        ldr     lr, [r0, #IN_R + 56]
        ldr     r1, [r0, #IN_R + 4]
        ldr     r2, [r0, #IN_R + 8]
        ldr     r3, [r0, #IN_R + 12]
        ldr     r4, [r0, #IN_R + 16]
        ldr     r5, [r0, #IN_R + 20]
        ldr     r6, [r0, #IN_R + 24]
        ldr     r7, [r0, #IN_R + 28]
        ldr     r8, [r0, #IN_R + 32]
        ldr     r9, [r0, #IN_R + 36]
        ldr     r10, [r0, #IN_R + 40]
        ldr     r11, [r0, #IN_R + 44]
        bx      r12

        .globl run_case_done
        .type run_case_done, %function
run_case_done:
        bx      lr

// r4..r11, sp and lr of the host, as run_case left them.
        .bss
        .balign 8
        .globl saved
        .type saved, %object
saved:
        .space 40
        .size saved, 40
