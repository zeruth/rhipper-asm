// The AArch64 half of the vector harness: the trampoline that loads a case's general registers,
// flags and vector registers and enters the case page, and the start, the system call and the
// cache maintenance a freestanding guest needs. Every case ends in a signal (the `brk` after the
// instruction, the `brk` fill a branch lands on, or the instruction's own fault); `handler` in
// guest.c records the machine's state from the signal context and resumes at `run_case_done`.
//
// The guest is built with -mgeneral-regs-only, which the trampoline's vector loads are not bound by.
        .arch armv8-a+fp+simd

// The layout of `struct case_in` in guest.c, which the vectors' generator writes.
        .set IN_X, 0
        .set IN_SP, 248
        .set IN_NZCV, 256
        .set IN_V, 272

// The case page: a `brk` fill with the prologue `ldr x9, [x0, #72]; ldr x0, [x0]` just before the
// instruction, so the jump into the page keeps two registers free and the prologue loads them.
        .set CASE_IP, 0x30002000
        .set CASE_PROLOGUE, CASE_IP - 8

        .text
        .balign 4

// The entry of a static freestanding guest: argc and argv off the initial stack to guest_main,
// whose return value is the exit status.
        .globl _start
        .type _start, %function
_start:
        ldr     x0, [sp]
        add     x1, sp, #8
        bl      guest_main
        mov     x8, #94
        svc     #0
        b       .

// long sys_call(long number, long a0, long a1, long a2, long a3, long a4, long a5)
        .globl sys_call
        .type sys_call, %function
sys_call:
        mov     x8, x0
        mov     x0, x1
        mov     x1, x2
        mov     x2, x3
        mov     x3, x4
        mov     x4, x5
        mov     x5, x6
        svc     #0
        ret

// void flush_code(unsigned long begin, unsigned long end): the data cache cleaned and the
// instruction cache invalidated over [begin, end), so a written case executes as written.
        .globl flush_code
        .type flush_code, %function
flush_code:
        mov     x2, x0
1:      dc      cvau, x2
        add     x2, x2, #64
        cmp     x2, x1
        b.lo    1b
        dsb     ish
        mov     x2, x0
2:      ic      ivau, x2
        add     x2, x2, #64
        cmp     x2, x1
        b.lo    2b
        dsb     ish
        isb
        ret

// void run_case(const struct case_in *in): the host's callee-saved registers and stack pointer go
// to `saved`, every register of the case is loaded but x9 and x0, and the case page is entered at
// its prologue, which loads those two. The case never comes back by itself: the signal handler
// puts `saved` back into the context and resumes at run_case_done.
        .globl run_case
        .type run_case, %function
run_case:
        adrp    x9, saved
        add     x9, x9, :lo12:saved
        stp     x19, x20, [x9]
        stp     x21, x22, [x9, #16]
        stp     x23, x24, [x9, #32]
        stp     x25, x26, [x9, #48]
        stp     x27, x28, [x9, #64]
        stp     x29, x30, [x9, #80]
        mov     x10, sp
        str     x10, [x9, #96]
        add     x10, x0, #IN_V
        ldp     q0, q1, [x10], #32
        ldp     q2, q3, [x10], #32
        ldp     q4, q5, [x10], #32
        ldp     q6, q7, [x10], #32
        ldp     q8, q9, [x10], #32
        ldp     q10, q11, [x10], #32
        ldp     q12, q13, [x10], #32
        ldp     q14, q15, [x10], #32
        ldp     q16, q17, [x10], #32
        ldp     q18, q19, [x10], #32
        ldp     q20, q21, [x10], #32
        ldp     q22, q23, [x10], #32
        ldp     q24, q25, [x10], #32
        ldp     q26, q27, [x10], #32
        ldp     q28, q29, [x10], #32
        ldp     q30, q31, [x10]
        ldr     x10, [x0, #IN_NZCV]
        lsl     x10, x10, #28
        msr     nzcv, x10
        ldr     x10, [x0, #IN_SP]
        mov     sp, x10
        ldp     x1, x2, [x0, #IN_X + 8]
        ldp     x3, x4, [x0, #IN_X + 24]
        ldp     x5, x6, [x0, #IN_X + 40]
        ldp     x7, x8, [x0, #IN_X + 56]
        ldp     x10, x11, [x0, #IN_X + 80]
        ldp     x12, x13, [x0, #IN_X + 96]
        ldp     x14, x15, [x0, #IN_X + 112]
        ldp     x16, x17, [x0, #IN_X + 128]
        ldp     x18, x19, [x0, #IN_X + 144]
        ldp     x20, x21, [x0, #IN_X + 160]
        ldp     x22, x23, [x0, #IN_X + 176]
        ldp     x24, x25, [x0, #IN_X + 192]
        ldp     x26, x27, [x0, #IN_X + 208]
        ldp     x28, x29, [x0, #IN_X + 224]
        ldr     x30, [x0, #IN_X + 240]
        movz    x9, #(CASE_PROLOGUE & 0xffff)
        movk    x9, #((CASE_PROLOGUE >> 16) & 0xffff), lsl #16
        br      x9

        .globl run_case_done
        .type run_case_done, %function
run_case_done:
        ret

// x19..x30 and sp of the host, as run_case left them.
        .bss
        .balign 16
        .globl saved
        .type saved, %object
saved:
        .space 104
        .size saved, 104
