// The x86 fixtures that speak to their system themselves: the integer program of the SH-4
// fixture (`testdata/sh4/src/sh4.c`), a static Linux program without a C library, for either
// width. Only the system call is the machine's own. The 64-bit program's (`x64sys`) is
// `syscall`, the call's number in rax (1 to write, 60 to end), its arguments in rdi, rsi and
// rdx, the result in rax; the instruction leaves rcx and r11 changed. The 32-bit program's
// (`x86sys`) is `int 0x80`, the number in eax (4 to write, 1 to end), the arguments in ebx, ecx
// and edx, the result in eax.

#define HAVE_SYSCALL3
#ifdef __x86_64__
static long syscall3(long number, long a, long b, long c) {
    register long rax __asm__("rax") = number == 4 ? 1 : 60;
    register long rdi __asm__("rdi") = a;
    register long rsi __asm__("rsi") = b;
    register long rdx __asm__("rdx") = c;
    __asm__ volatile("syscall" : "+r"(rax) : "r"(rdi), "r"(rsi), "r"(rdx) : "memory", "rcx", "r11");
    return rax;
}
#else
static long syscall3(long number, long a, long b, long c) {
    register long eax __asm__("eax") = number;
    register long ebx __asm__("ebx") = a;
    register long ecx __asm__("ecx") = b;
    register long edx __asm__("edx") = c;
    __asm__ volatile("int $0x80" : "+r"(eax) : "r"(ebx), "r"(ecx), "r"(edx) : "memory");
    return eax;
}
#endif

#include "../sh4/src/sh4.c"
