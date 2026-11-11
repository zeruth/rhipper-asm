// A struct of mixed fields behind a pointer, for the type recovery: the functions read and write
// a record through a pointer parameter at the offsets of its fields, and the C of the port must
// show a struct with named fields where the machine code has a base register and displacements.
// No C runtime: the results print through WriteFile.
#include <windows.h>

#define NOINLINE __attribute__((noinline))

static HANDLE out;

// The MSVC toolchain marks a program that uses floating point; without a C runtime, this is it.
int _fltused = 1;

struct record {
    int id;             // 0
    unsigned char kind; // 4
    short delta;        // 6
    long long total;    // 8
    double scale;       // 16
    const char *name;   // 24
    unsigned int flags; // 32
};

NOINLINE static void print_int(long long v) {
    char buf[24];
    char *p = buf + sizeof buf;
    unsigned long long u = v < 0 ? (unsigned long long)(-v) : (unsigned long long)v;
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

// Reads and writes fields of every width: the int, the byte, the short, the long long and the
// unsigned int of the record.
NOINLINE static long long weigh(struct record *r) {
    r->total += r->id * (long long)r->delta;
    r->flags |= 1u << r->kind;
    return r->total;
}

// The pointer field, followed: the length of the record's name.
NOINLINE static int name_length(const struct record *r) {
    int n = 0;
    while (r->name[n]) n++;
    return n;
}

// The double field, scaled and truncated.
NOINLINE static long long scaled(const struct record *r) { return (long long)(r->scale * (double)r->total); }

// A record filled through its pointer, field by field.
NOINLINE static void fill(struct record *r, int id, unsigned char kind, short delta, const char *name) {
    r->id = id;
    r->kind = kind;
    r->delta = delta;
    r->total = 100;
    r->scale = 1.5;
    r->name = name;
    r->flags = 0;
}

int mainCRTStartup(void) {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    struct record r;
    fill(&r, 7, 3, -2, "seven");
    print_int(weigh(&r));
    print_int(r.flags);
    print_int(name_length(&r));
    print_int(scaled(&r));
    r.kind = 5;
    r.delta = 10;
    print_int(weigh(&r));
    print_int(r.flags);
    print_int(scaled(&r));
    ExitProcess(0);
    return 0;
}
