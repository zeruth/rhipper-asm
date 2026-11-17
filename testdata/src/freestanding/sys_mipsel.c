// The MIPS32 Linux machine (the o32 ABI), little- or big-endian: the entry and the two syscalls. Built
// without abicalls and without PIC, so nothing sets up $gp and a call is a plain `jal`.
#include "freestanding.h"

// The o32 syscall numbers: Linux's `asm/unistd_o32.h`.
#define SYS_WRITE 4004
#define SYS_EXIT_GROUP 4246

// The kernel returns the result in $v0 and an error flag in $a3; a set flag makes $v0 the errno.
static long syscall3(long number, long a, long b, long c) {
    register long v0 __asm__("$2") = number;
    register long a0 __asm__("$4") = a;
    register long a1 __asm__("$5") = b;
    register long a2 __asm__("$6") = c;
    register long a3 __asm__("$7");
    __asm__ volatile("syscall"
                     : "+r"(v0), "=r"(a3)
                     : "r"(a0), "r"(a1), "r"(a2)
                     : "$1", "$3", "$8", "$9", "$10", "$11", "$12", "$13", "$14", "$15", "$24", "$25", "hi", "lo", "memory");
    return a3 ? -v0 : v0;
}

long fs_write(int fd, const void *buf, unsigned long n) { return syscall3(SYS_WRITE, fd, (long)buf, (long)n); }

void fs_exit(int status) {
    for (;;) syscall3(SYS_EXIT_GROUP, status, 0, 0);
}

// The entry, `__start` as the MIPS linkers name it: the kernel leaves the stack 8-byte aligned
// with the arguments on it; o32 wants the caller to reserve 16 bytes of argument space before a
// call. Each `jal` carries its own `nop` in the delay slot rather than leaving the assembler to
// place one.
__asm__(".text\n"
        ".globl __start\n"
        ".type __start, @function\n"
        ".set noreorder\n"
        "__start:\n"
        "    addiu $sp, $sp, -16\n"
        "    jal main\n"
        "    nop\n"
        "    move $a0, $v0\n"
        "    jal fs_exit\n"
        "    nop\n"
        ".set reorder\n"
        ".size __start, .-__start\n");
