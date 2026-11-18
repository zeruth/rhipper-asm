// The MIPS64 little-endian half of the vector harness (the n64 ABI): the trampoline that loads a
// case's doubleword registers, hi and lo and enters the case page, and the start, the system call
// and the cache maintenance a freestanding guest needs. Every case ends in a signal (the `break`
// after the code, or the instruction's own fault); `handler` in guest.c records the machine's
// state from the signal context and resumes at `run_case_done`, which takes the host's registers
// back from `saved`.
//
// Everything here is written with its own delay slots: the assembler reorders nothing.
        .set noreorder
        .set noat
        .text
        .balign 8

// The layout of `struct case_in` in guest.c, which the vectors' generator writes: register n at
// 8n, then hi and lo.
        .set IN_HI, 256
        .set IN_LO, 264

// The case page: a `break` fill with the prologue `ld $1, 8($4); ld $4, 32($4)` just before the
// code, so the jump into the page keeps two registers free and the prologue loads them.
        .set CASE_PROLOGUE_HI, 0x3000
        .set CASE_PROLOGUE_LO, 0x1ff8

// The entry of a static freestanding guest: argc and argv off the initial stack to guest_main,
// and its return value the exit status.
        .globl __start
        .type __start, @function
__start:
        ld      $4, 0($29)
        daddiu  $5, $29, 8
        daddiu  $29, $29, -16
        jal     guest_main
        nop
        move    $4, $2
        li      $2, 5205
        syscall
1:      b       1b
        nop
        .size __start, .-__start

// long sys_call(long number, long a0, long a1, long a2, long a3, long a4, long a5): the number in
// $v0 and the six arguments in $a0..$a5. The kernel sets $a3 on an error, with the errno in $v0;
// the result is then its negation.
        .globl sys_call
        .type sys_call, @function
sys_call:
        move    $2, $4
        move    $4, $5
        move    $5, $6
        move    $6, $7
        move    $7, $8
        move    $8, $9
        move    $9, $10
        syscall
        beq     $7, $0, 1f
        nop
        dsubu   $2, $0, $2
1:      jr      $31
        nop
        .size sys_call, .-sys_call

// void flush_code(unsigned long begin, unsigned long end): the MIPS cacheflush call over
// [begin, end), both caches, so a written case executes as written.
        .globl flush_code
        .type flush_code, @function
flush_code:
        dsubu   $5, $5, $4
        li      $6, 3
        li      $2, 5197
        syscall
        jr      $31
        nop
        .size flush_code, .-flush_code

// void run_case(const struct case_in *in): the host's callee-saved registers, $gp, $sp, $fp and
// $ra go to `saved`, hi and lo and every register of the case but $1 and $4 are loaded, and the
// case page is entered at its prologue, which loads those two. The case never comes back by
// itself: the signal handler resumes at run_case_done.
        .globl run_case
        .type run_case, @function
run_case:
        lui     $8, %hi(saved)
        daddiu  $8, $8, %lo(saved)
        sd      $16, 0($8)
        sd      $17, 8($8)
        sd      $18, 16($8)
        sd      $19, 24($8)
        sd      $20, 32($8)
        sd      $21, 40($8)
        sd      $22, 48($8)
        sd      $23, 56($8)
        sd      $28, 64($8)
        sd      $29, 72($8)
        sd      $30, 80($8)
        sd      $31, 88($8)
        ld      $8, IN_HI($4)
        mthi    $8
        ld      $8, IN_LO($4)
        mtlo    $8
        lui     $1, CASE_PROLOGUE_HI
        ori     $1, $1, CASE_PROLOGUE_LO
        ld      $2, 16($4)
        ld      $3, 24($4)
        ld      $5, 40($4)
        ld      $6, 48($4)
        ld      $7, 56($4)
        ld      $8, 64($4)
        ld      $9, 72($4)
        ld      $10, 80($4)
        ld      $11, 88($4)
        ld      $12, 96($4)
        ld      $13, 104($4)
        ld      $14, 112($4)
        ld      $15, 120($4)
        ld      $16, 128($4)
        ld      $17, 136($4)
        ld      $18, 144($4)
        ld      $19, 152($4)
        ld      $20, 160($4)
        ld      $21, 168($4)
        ld      $22, 176($4)
        ld      $23, 184($4)
        ld      $24, 192($4)
        ld      $25, 200($4)
        ld      $26, 208($4)
        ld      $27, 216($4)
        ld      $28, 224($4)
        ld      $29, 232($4)
        ld      $30, 240($4)
        jr      $1
        ld      $31, 248($4)
        .size run_case, .-run_case

// The host's registers back from `saved` and the return to run_case's caller; the handler has
// already recorded the case's.
        .globl run_case_done
        .type run_case_done, @function
run_case_done:
        lui     $8, %hi(saved)
        daddiu  $8, $8, %lo(saved)
        ld      $16, 0($8)
        ld      $17, 8($8)
        ld      $18, 16($8)
        ld      $19, 24($8)
        ld      $20, 32($8)
        ld      $21, 40($8)
        ld      $22, 48($8)
        ld      $23, 56($8)
        ld      $28, 64($8)
        ld      $29, 72($8)
        ld      $30, 80($8)
        ld      $31, 88($8)
        jr      $31
        nop
        .size run_case_done, .-run_case_done

// $s0..$s7, $gp, $sp, $fp and $ra of the host, as run_case left them.
        .bss
        .balign 8
        .globl saved
        .type saved, @object
saved:
        .space 96
        .size saved, 96
