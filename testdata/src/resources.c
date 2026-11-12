// Resources and typed data, without a C runtime: the string table and an RCDATA block of
// resources.rc read through user32 and kernel32 (the port links the resources ras extracts from
// the image, so the same calls find them in the port), and a table of integers, a table of
// strings and a table of doubles read by an index the compiler cannot fold, so the data section
// is read as arrays rather than bytes.
#include <windows.h>

#define NOINLINE __attribute__((noinline))

static HANDLE out;
int _fltused = 1;

static const int squares[8] = {0, 1, 4, 9, 16, 25, 36, 49};
static const char *const names[4] = {"zero", "one", "two", "three"};
static const double halves[4] = {0.5, 1.5, 2.5, 3.5};
static short counters[4];

NOINLINE static void print_str(const char *s) {
    DWORD n = 0;
    const char *p = s;
    while (*p) p++;
    WriteFile(out, s, (DWORD)(p - s), &n, NULL);
}

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

NOINLINE static int square_of(int i) {
    return squares[i & 7];
}

NOINLINE static const char *name_of(int i) {
    return names[i & 3];
}

NOINLINE static int tenths_of(int i) {
    return (int)(halves[i & 3] * 10.0);
}

NOINLINE static void count(int i) {
    counters[i & 3] += (short)(i + 1);
}

NOINLINE static void print_string_resource(HMODULE module, UINT id) {
    char buf[64];
    int n = LoadStringA(module, id, buf, sizeof buf);
    print_int(n);
    if (n > 0) {
        print_str(buf);
        print_str("\r\n");
    }
}

NOINLINE static void print_rcdata(HMODULE module, int id) {
    HRSRC found = FindResourceA(module, MAKEINTRESOURCEA(id), RT_RCDATA);
    if (!found) {
        print_str("no such block\r\n");
        return;
    }
    DWORD size = SizeofResource(module, found);
    const unsigned char *bytes = (const unsigned char *)LockResource(LoadResource(module, found));
    int sum = 0;
    // Scalar: the fixture is about resources, not about clang's vector epilogue.
#pragma clang loop vectorize(disable) unroll(disable)
    for (DWORD i = 0; i < size; i++) sum += bytes[i];
    print_int((int)size);
    print_int(sum);
}

int mainCRTStartup(void) {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    HMODULE module = GetModuleHandleA(NULL);
    for (UINT id = 1; id <= 4; id++) print_string_resource(module, id);
    print_rcdata(module, 100);
    print_rcdata(module, 101);
    for (int i = 0; i < 8; i++) print_int(square_of(i));
    for (int i = 0; i < 4; i++) {
        print_str(name_of(i));
        print_str("\r\n");
    }
    for (int i = 0; i < 4; i++) print_int(tenths_of(i));
    for (int i = 0; i < 10; i++) count(i);
    for (int i = 0; i < 4; i++) print_int(counters[i]);
    ExitProcess(0);
    return 0;
}
