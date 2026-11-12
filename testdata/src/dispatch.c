// Function-pointer dispatch no reading of the image resolves: the handlers are installed into a
// table under keys the program computes at run time, and every call goes through the table. The
// fixture of the trace-driven facts: a traced run names the targets of each call, and the port
// made with them reads as direct calls. No C runtime: output goes through WriteFile.
#include <windows.h>

#define NOINLINE __attribute__((noinline))

static HANDLE out;

NOINLINE static void print_int(int v) {
    char buf[16];
    char *p = buf + sizeof buf;
    unsigned u = v < 0 ? (unsigned)(-v) : (unsigned)v;
    *--p = '\n';
    *--p = '\r';
    do {
        *--p = (char)('0' + u % 10);
        u /= 10;
    } while (u);
    if (v < 0) *--p = '-';
    DWORD n = 0;
    WriteFile(out, p, (DWORD)(buf + sizeof buf - p), &n, NULL);
}

typedef int (*handler)(int, int);

NOINLINE static int h_add(int a, int b) { return a + b; }
NOINLINE static int h_sub(int a, int b) { return a - b; }
NOINLINE static int h_mul(int a, int b) { return a * b; }
NOINLINE static int h_xor(int a, int b) { return a ^ b; }

// The table is filled at run time under keys that depend on a value read at run time, so no
// constant names the handler a call reaches.
static handler table[8];
static volatile int seed = 3;

NOINLINE static void install(int key, handler h) { table[key & 7] = h; }

NOINLINE static int dispatch(int key, int a, int b) { return table[key & 7](a, b); }

int mainCRTStartup(void) {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    int k = seed;
    install(k, h_add);
    install(k + 1, h_sub);
    install(k + 2, h_mul);
    install(k + 3, h_xor);
    for (int i = 0; i < 4; i++) print_int(dispatch(k + i, 40, 2));
    print_int(dispatch(k + 2, dispatch(k, 5, 6), 7));
    ExitProcess(0);
    return 0;
}
