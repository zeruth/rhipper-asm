// The program of the second-image fixture: it imports the exports DLL through its import table,
// calls two of its functions and reads its data export. Ported with the DLL as its second image
// (`ras emit --with`), the calls reach the DLL's lifted functions and the port loads no DLL for
// them. No C runtime.
#include <windows.h>

__declspec(dllimport) int ra_add(int a, int b);
__declspec(dllimport) int ra_twice(int a);
__declspec(dllimport) extern int ra_counter;

static HANDLE out;

static void put(const char *text) {
    DWORD written = 0;
    WriteFile(out, text, lstrlenA(text), &written, NULL);
}

static void put_number(int n) {
    char buf[12];
    char *p = buf + sizeof buf - 1;
    unsigned u = n < 0 ? 0u - (unsigned)n : (unsigned)n;
    *p = 0;
    do {
        *--p = (char)('0' + u % 10);
        u /= 10;
    } while (u);
    if (n < 0) *--p = '-';
    put(p);
}

static void line(const char *label, int value) {
    put(label);
    put_number(value);
    put("\r\n");
}

int mainCRTStartup(void) {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    line("ra_add(2, 3) = ", ra_add(2, 3));
    line("ra_twice(21) = ", ra_twice(21));
    line("ra_counter = ", ra_counter);
    line("ra_add(ra_counter, ra_twice(-4)) = ", ra_add(ra_counter, ra_twice(-4)));
    ExitProcess(0);
    return 0;
}
