// The smallest fixture: three imports, two leaf functions, one caller. No C runtime.
#include <windows.h>

__attribute__((noinline)) static int add(int a, int b) { return a + b; }

__attribute__((noinline)) int square(int x) { return x * x; }

int mainCRTStartup(void) {
    HANDLE out = GetStdHandle(STD_OUTPUT_HANDLE);
    DWORD written = 0;
    char buf[4] = {'0' + (char)(add(1, 2) % 10), '0' + (char)(square(3) % 10), '\r', '\n'};
    WriteFile(out, buf, 4, &written, NULL);
    ExitProcess(0);
    return 0;
}
