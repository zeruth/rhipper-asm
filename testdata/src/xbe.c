// A homebrew program of the original Xbox: no C runtime, no XAPI, the kernel reached through the
// thunk table the XBE header names, which the kernel fills with its exports' addresses before the
// entry runs. The program prints through DbgPrint (the kernel's printf), takes and returns pool
// memory, and goes back to the dashboard. clang builds it as a PE at the Xbox's base and the
// writer in crates/ra-asm-pe/tests/xbe.rs rewrites that as the XBE; with XBE_HOST it builds as a
// program of this machine whose DbgPrint is printf, which prints the expected output.

#define NOINLINE __attribute__((noinline))

#ifdef XBE_HOST
#include <stdio.h>
#include <stdlib.h>
#define DbgPrint printf
static void *ExAllocatePool(unsigned long n) { return malloc(n); }
static void ExFreePool(void *p) { free(p); }
static void HalReturnToFirmware(unsigned long routine) {
    (void)routine;
    exit(0);
}
#else
// The kernel thunk table: each slot holds an ordinal under the top bit until the kernel writes the
// export's address over it. Its own section, so the XBE writer finds it.
__attribute__((section(".kthunk"))) void *xbe_kernel_thunks[] = {
    (void *)(0x80000000u | 8),   // DbgPrint
    (void *)(0x80000000u | 14),  // ExAllocatePool
    (void *)(0x80000000u | 17),  // ExFreePool
    (void *)(0x80000000u | 49),  // HalReturnToFirmware
    0,
};
#define DbgPrint ((unsigned long(__cdecl *)(const char *, ...))xbe_kernel_thunks[0])
#define ExAllocatePool ((void *(__stdcall *)(unsigned long))xbe_kernel_thunks[1])
#define ExFreePool ((void(__stdcall *)(void *))xbe_kernel_thunks[2])
#define HalReturnToFirmware ((void(__stdcall *)(unsigned long))xbe_kernel_thunks[3])
#endif

NOINLINE static int fib(int n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }

NOINLINE static unsigned checksum(const unsigned char *p, unsigned n) {
    unsigned sum = 0;
    for (unsigned i = 0; i < n; i++) sum = sum * 31 + p[i];
    return sum;
}

void XbeEntry(void) {
    DbgPrint("hello from the xbox\n");
    for (int i = 1; i <= 10; i++) DbgPrint("fib(%d) = %d\n", i, fib(i));
    unsigned char *pool = ExAllocatePool(64);
    for (int i = 0; i < 64; i++) pool[i] = (unsigned char)(i * 7);
    DbgPrint("pool %u bytes, checksum %08x\n", 64u, checksum(pool, 64));
    ExFreePool(pool);
    HalReturnToFirmware(1);
}

#ifdef XBE_HOST
int main(void) {
    XbeEntry();
    return 0;
}
#endif
