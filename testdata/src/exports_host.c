// The host of the exports fixture: loads the DLL its command line names (the last word), calls
// its exports by name and by ordinal, reads its data export, checks that the forwarder resolves
// to kernel32's function, and prints each. The pipeline runs it over the port of the DLL and over
// the DLL itself and compares the two. No C runtime.
#include <windows.h>

typedef int (*binary_fn)(int, int);
typedef int (*unary_fn)(int);

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
    const char *cmd = GetCommandLineA();
    const char *name = cmd;
    for (const char *p = cmd; *p; p++) {
        if (*p == ' ' && p[1] && p[1] != ' ') name = p + 1;
    }
    HMODULE dll = LoadLibraryA(name);
    if (!dll) {
        put("cannot load ");
        put(name);
        put("\r\n");
        ExitProcess(1);
    }
    binary_fn add = (binary_fn)GetProcAddress(dll, "ra_add");
    unary_fn twice = (unary_fn)GetProcAddress(dll, "ra_twice");
    unary_fn hidden = (unary_fn)GetProcAddress(dll, MAKEINTRESOURCEA(20));
    const int *counter = (const int *)GetProcAddress(dll, "ra_counter");
    FARPROC tick = GetProcAddress(dll, "ra_tick");
    FARPROC get_tick_count = GetProcAddress(GetModuleHandleA("kernel32.dll"), "GetTickCount");
    line("ra_add(2, 3) = ", add ? add(2, 3) : -1);
    line("ra_twice(21) = ", twice ? twice(21) : -1);
    line("#20(10) = ", hidden ? hidden(10) : -1);
    line("ra_counter = ", counter ? *counter : -1);
    put(tick && tick == get_tick_count ? "ra_tick -> KERNEL32.GetTickCount\r\n" : "ra_tick: not forwarded\r\n");
    FreeLibrary(dll);
    ExitProcess(0);
    return 0;
}
