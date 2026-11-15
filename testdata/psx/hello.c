// The PlayStation loader's fixture: a PSn00bSDK program (PS-EXE, R3000 little-endian) that prints
// through the SDK's printf, which ends in the BIOS's putchar. Built in WSL by the CMakeLists.txt
// beside it with the SDK's toolchain file:
//
//   cmake -S testdata/psx -B out/psx -G Ninja -DCMAKE_TOOLCHAIN_FILE=<PSn00bSDK>/lib/libpsn00b/cmake/sdk.cmake
//   cmake --build out/psx && cp out/psx/hello.exe testdata/psx/hello.exe
//
// hello.txt is what the source prints; no emulator ran it.
#include <stdio.h>

static int counter = 5;

__attribute__((noinline)) static int add(int a, int b) { return a + b; }

__attribute__((noinline)) int square(int x) { return x * x; }

int main(void) {
    printf("hello from the PlayStation\n");
    counter += add(1, 2);
    printf("%d %d %x\n", counter, square(counter), square(add(1, 2)) * 1000);
    return 0;
}
