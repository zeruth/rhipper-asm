// The smallest PlayStation fixture: two leaf functions, one caller, and one line printed through
// the BIOS's printf (kernel call A(3Fh), which writes to the TTY). Built with PSn00bSDK; no GPU.
#include <stdio.h>

__attribute__((noinline)) static int add(int a, int b) { return a + b; }

__attribute__((noinline)) int square(int x) { return x * x; }

int main(void) {
    printf("%s %d %d %x\n", "hello", add(1, 2), square(3), square(add(1, 2)) * 1000);
    return 0;
}
