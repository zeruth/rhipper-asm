// The MIPS32 little-endian half of the vector harness (the o32 ABI): the trampoline that loads a
// case's registers, hi and lo and enters the case page, and the start, the system call and the
// cache maintenance a freestanding guest needs. Every case ends in a signal (the `break` after
// the code, the `break` fill a branch lands on, or the instruction's own fault); `handler` in
// guest.c records the machine's state from the signal context and resumes at `run_case_done`,
// which takes the host's registers back from `saved`.
//
// Everything here is written with its own delay slots: the assembler reorders nothing.
        .set noreorder
        .set noat
        .text
        .balign 4

// The layout of `struct case_in` in guest.c, which the vectors' generator writes: register n at
// 4n, then hi and lo.
        .set IN_HI, 128
        .set IN_LO, 132

// The case page: a `break` fill with the prologue `lw $1, 4($4); lw $4, 16($4)` just before the
// code, so the jump into the page keeps two registers free and the prologue loads them.
        .set CASE_IP, 0x30002000
        .set CASE_PROLOGUE, CASE_IP - 8

// The entry of a static freestanding guest, `__start` as the MIPS linkers name it: argc and argv
// off the initial stack to guest_main, with the 16 bytes of argument space o32 wants reserved,
// and its return value the exit status.
        .globl __start
        .type __start, @function
__start:
        lw      $4, 0($29)
        addiu   $5, $29, 4
        addiu   $29, $29, -16
        jal     guest_main
        nop
        move    $4, $2
        li      $2, 4246
        syscall
1:      b       1b
        nop
        .size __start, .-__start

// long sys_call(long number, long a0, long a1, long a2, long a3, long a4, long a5): the number in
// $v0, the first four arguments in $a0..$a3 and the last two at 16 and 20 off the stack. The
// kernel sets $a3 on an error, with the errno in $v0; the result is then its negation.
        .globl sys_call
        .type sys_call, @function
sys_call:
        addiu   $29, $29, -32
        move    $2, $4
        move    $4, $5
        move    $5, $6
        move    $6, $7
        lw      $7, 48($29)
        lw      $8, 52($29)
        sw      $8, 16($29)
        lw      $8, 56($29)
        sw      $8, 20($29)
        syscall
        beq     $7, $0, 1f
        nop
        subu    $2, $0, $2
1:      jr      $31
        addiu   $29, $29, 32
        .size sys_call, .-sys_call

// void flush_code(unsigned long begin, unsigned long end): the MIPS cacheflush call over
// [begin, end), both caches, so a written case executes as written.
        .globl flush_code
        .type flush_code, @function
flush_code:
        subu    $5, $5, $4
        li      $6, 3
        li      $2, 4147
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
        addiu   $8, $8, %lo(saved)
        sw      $16, 0($8)
        sw      $17, 4($8)
        sw      $18, 8($8)
        sw      $19, 12($8)
        sw      $20, 16($8)
        sw      $21, 20($8)
        sw      $22, 24($8)
        sw      $23, 28($8)
        sw      $28, 32($8)
        sw      $29, 36($8)
        sw      $30, 40($8)
        sw      $31, 44($8)
        lw      $8, IN_HI($4)
        mthi    $8
        lw      $8, IN_LO($4)
        mtlo    $8
        lui     $1, %hi(CASE_PROLOGUE)
        addiu   $1, $1, %lo(CASE_PROLOGUE)
        lw      $2, 8($4)
        lw      $3, 12($4)
        lw      $5, 20($4)
        lw      $6, 24($4)
        lw      $7, 28($4)
        lw      $8, 32($4)
        lw      $9, 36($4)
        lw      $10, 40($4)
        lw      $11, 44($4)
        lw      $12, 48($4)
        lw      $13, 52($4)
        lw      $14, 56($4)
        lw      $15, 60($4)
        lw      $16, 64($4)
        lw      $17, 68($4)
        lw      $18, 72($4)
        lw      $19, 76($4)
        lw      $20, 80($4)
        lw      $21, 84($4)
        lw      $22, 88($4)
        lw      $23, 92($4)
        lw      $24, 96($4)
        lw      $25, 100($4)
        lw      $26, 104($4)
        lw      $27, 108($4)
        lw      $28, 112($4)
        lw      $29, 116($4)
        lw      $30, 120($4)
        jr      $1
        lw      $31, 124($4)
        .size run_case, .-run_case

// The host's registers back from `saved` and the return to run_case's caller; the handler has
// already recorded the case's.
        .globl run_case_done
        .type run_case_done, @function
run_case_done:
        lui     $8, %hi(saved)
        addiu   $8, $8, %lo(saved)
        lw      $16, 0($8)
        lw      $17, 4($8)
        lw      $18, 8($8)
        lw      $19, 12($8)
        lw      $20, 16($8)
        lw      $21, 20($8)
        lw      $22, 24($8)
        lw      $23, 28($8)
        lw      $28, 32($8)
        lw      $29, 36($8)
        lw      $30, 40($8)
        lw      $31, 44($8)
        jr      $31
        nop
        .size run_case_done, .-run_case_done

// $s0..$s7, $gp, $sp, $fp and $ra of the host, as run_case left them.
        .bss
        .balign 8
        .globl saved
        .type saved, @object
saved:
        .space 48
        .size saved, 48
