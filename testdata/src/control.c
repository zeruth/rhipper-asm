// Control flow and data for the analysis and the lift: loops, if-else, a dense switch, recursion,
// a table of function pointers, globals, locals, and a function that never returns called from the
// middle of a function. No C runtime: output goes through WriteFile.
#include <windows.h>

#define NOINLINE __attribute__((noinline))

static HANDLE out;

static int table[10] = {5, 3, 8, 1, 9, 2, 7, 4, 6, 0};
static volatile int sink;
static const char *names[] = {"zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"};

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

NOINLINE __attribute__((noreturn)) static void fail(int code) {
    print_str("fail\r\n");
    ExitProcess((UINT)code);
}

NOINLINE static int collatz(int n) {
    int steps = 0;
    while (n != 1) {
        n = (n & 1) ? 3 * n + 1 : n / 2;
        steps++;
    }
    return steps;
}

NOINLINE static int fib(int n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }

NOINLINE static int classify(int x) {
    // Each case does different work and calls out, so the compiler keeps a jump table.
    switch (x) {
    case 0: print_str("A"); return 10;
    case 1: print_str("B"); sink += 2; return 11;
    case 2: print_str("C"); sink *= 3; return 12;
    case 3: print_str("D"); return collatz(27);
    case 4: print_str("E"); sink ^= 7; return 14;
    case 5: print_str("F"); return fib(10);
    case 6: print_str("G"); sink -= 1; return 16;
    case 7: print_str("H"); return table[x];
    case 8: print_str("I"); sink |= 9; return 18;
    default: print_str("?"); return -1;
    }
}

NOINLINE static int op_add(int a, int b) { return a + b; }
NOINLINE static int op_sub(int a, int b) { return a - b; }
NOINLINE static int op_mul(int a, int b) { return a * b; }
NOINLINE static int op_div(int a, int b) {
    if (b == 0) fail(3);
    return a / b;
}

static int (*const ops[4])(int, int) = {op_add, op_sub, op_mul, op_div};

NOINLINE static int sum_table(void) {
    int s = 0;
    for (int i = 0; i < 10; i++) s += table[i];
    return s;
}

NOINLINE static void sort_table(void) {
    for (int i = 1; i < 10; i++) {
        int v = table[i], j = i - 1;
        while (j >= 0 && table[j] > v) {
            table[j + 1] = table[j];
            j--;
        }
        table[j + 1] = v;
    }
}

NOINLINE static unsigned mix(unsigned a, unsigned b) {
    unsigned h = a * 2654435761u;
    h ^= b >> 3;
    h = (h << 7) | (h >> 25);
    return h ^ (h >> 16);
}

// An import called through a pointer kept in data: one word holds the import slot's value, the
// other what GetProcAddress returned. Volatile, so the compiler keeps the load and the call
// through it rather than folding the call into a direct one.
typedef BOOL(WINAPI *write_fn)(HANDLE, LPCVOID, DWORD, LPDWORD, LPOVERLAPPED);
static write_fn volatile write_slot;
static write_fn volatile write_proc;

NOINLINE static void print_slot(const char *s) {
    DWORD n = 0;
    const char *p = s;
    while (*p) p++;
    write_slot(out, s, (DWORD)(p - s), &n, NULL);
}

NOINLINE static void print_proc(const char *s) {
    DWORD n = 0;
    const char *p = s;
    while (*p) p++;
    write_proc(out, s, (DWORD)(p - s), &n, NULL);
}

int mainCRTStartup(void) {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    write_slot = WriteFile;
    // A port of the 32-bit program that maps the image at its base gets nothing from
    // GetProcAddress (the host's module handle does not fit a dword); then the slot's value,
    // still called through a word the lift cannot resolve, since two stores write it.
    write_fn proc = (write_fn)GetProcAddress(GetModuleHandleA("kernel32.dll"), "WriteFile");
    write_proc = proc ? proc : WriteFile;
    print_slot("slot\r\n");
    print_proc("proc\r\n");
    print_int(collatz(27));
    print_int(fib(15));
    int total = 0;
    for (int i = 0; i <= 9; i++) total += classify(i);
    print_str("\r\n");
    print_int(total);
    for (int i = 0; i < 4; i++) print_int(ops[i](42, 6));
    print_int(sum_table());
    sort_table();
    for (int i = 0; i < 10; i++) print_str(names[table[i]]), print_str(i == 9 ? "\r\n" : " ");
    print_int((int)(mix(123456789u, 987654321u) % 100000u));
    if (sink == 0x7fffffff) fail(2);
    print_int(sink);
    ExitProcess(0);
    return 0;
}
