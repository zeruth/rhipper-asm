// A function whose locals ask for 16-byte alignment and take more than two pages: the x86 build
// realigns its stack (`and esp, -16`) and then probes it (`mov eax, N; call __chkstk`), so the
// stack pointer is lost before the frame is made, and the lift sizes the frame from the probe.
// The callees write and read every byte of it. No C runtime but libcmt's stack probe.
#include <windows.h>

#define NOINLINE __attribute__((noinline))
#define ROWS 600

typedef struct {
    __declspec(align(16)) unsigned lanes[4];
} Quad;

static HANDLE out;

NOINLINE static void print_line(const char *label, unsigned v) {
    char buf[48];
    char *p = buf + sizeof buf;
    *--p = '\n';
    *--p = '\r';
    do {
        *--p = (char)('0' + v % 10);
        v /= 10;
    } while (v);
    *--p = ' ';
    const char *end = label;
    while (*end) end++;
    while (end > label) *--p = *--end;
    DWORD n = 0;
    WriteFile(out, p, (DWORD)(buf + sizeof buf - p), &n, NULL);
}

NOINLINE static void fill(Quad *rows, int count, unsigned seed) {
    for (int i = 0; i < count; i++) {
        for (int k = 0; k < 4; k++) {
            seed = seed * 1103515245u + 12345u;
            rows[i].lanes[k] = seed >> 8;
        }
    }
}

NOINLINE static unsigned mix(const Quad *rows, int count) {
    unsigned h = 2166136261u;
    for (int i = count - 1; i >= 0; i--) {
        for (int k = 0; k < 4; k++) h = (h ^ rows[i].lanes[k]) * 16777619u;
    }
    return h;
}

// The rows are the frame: 9600 bytes at a 16-byte boundary. A frame given less room puts them
// over whatever lies below it.
NOINLINE static unsigned sweep(unsigned seed, unsigned *aligned) {
    Quad rows[ROWS];
    fill(rows, ROWS, seed);
    *aligned = ((UINT_PTR)rows & 15) == 0;
    return mix(rows, ROWS);
}

int mainCRTStartup(void) {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    unsigned aligned = 0;
    print_line("sweep", sweep(1, &aligned));
    print_line("aligned", aligned);
    print_line("again", sweep(77, &aligned));
    ExitProcess(0);
    return 0;
}
