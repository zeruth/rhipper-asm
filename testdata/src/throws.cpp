// C++ exceptions over the x64 unwind data and the frame handler's FuncInfo: a throw caught by
// value and one caught by reference to a base class, both through a function without a handler
// of its own, a rethrow out of an inner catch into an outer catch-all, and a local destroyed as
// the stack unwinds. No C++ runtime beyond vcruntime's throw: the objects are plain, nothing is
// allocated, and output goes through WriteFile.
#include <windows.h>

#define NOINLINE __attribute__((noinline))

static HANDLE out;

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

// The type descriptors of the thrown types point at type_info's vtable, which the C++ runtime
// defines; there is none here, so the program defines the symbol itself, as classes does.
__attribute__((used)) static const void *type_info_vftable[1] __asm__("??_7type_info@@6B@") = {nullptr};

struct Error {
    int code;
};

struct Range : Error {
    int limit;
};

// A local with a destructor: the unwinder runs it when a throw passes the frame.
struct Guard {
    const char *name;
    NOINLINE ~Guard() {
        print_str("~");
        print_str(name);
        print_str("\r\n");
    }
};

NOINLINE static int check(int n) {
    Guard g{"check"};
    if (n > 3) throw Range{{n}, 3};
    if (n < 0) throw n;
    return n * 2;
}

// No handler of its own: a throw passes through, destroying its guard.
NOINLINE static int relay(int n) {
    Guard g{"relay"};
    return check(n) + 1;
}

NOINLINE static int attempt(int n) {
    try {
        return relay(n);
    } catch (const Error &e) {
        print_str("error ");
        print_int(e.code);
        return -1;
    } catch (int v) {
        print_str("int ");
        print_int(v);
        return -2;
    }
}

NOINLINE static void nested() {
    try {
        try {
            throw Error{7};
        } catch (const Error &e) {
            print_str("inner ");
            print_int(e.code);
            throw;
        }
    } catch (...) {
        print_str("outer\r\n");
    }
}

extern "C" int mainCRTStartup() {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    static const int inputs[] = {2, 5, -1};
    for (int n : inputs) print_int(attempt(n));
    nested();
    print_str("done\r\n");
    return 0;
}
