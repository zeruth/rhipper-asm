// Classes with virtual functions, for the RTTI and vtable analysis: a base with three virtual
// functions, a derived class, a second base, and a class derived from both, so that one class has
// two vtables and adjustor thunks. The calls go through base pointers that the compiler cannot
// devirtualize. No C++ runtime: the objects are constant-initialized (constinit proves it), nothing
// is allocated, no destructor is virtual, and output goes through WriteFile.
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

// The type descriptors of the RTTI point at type_info's vtable, which the C++ runtime defines.
// There is none here, so the program defines the symbol itself. On x86 clang prefixes the label
// with an underscore, and the build passes /alternatename to the linker for it.
__attribute__((used)) static const void *type_info_vftable[1] __asm__("??_7type_info@@6B@") = {nullptr};

struct Shape {
    int id;
    constexpr Shape(int id) : id(id) {}
    virtual int area() const { return 0; }
    virtual const char *name() const { return "shape"; }
    virtual int scale(int k) const { return k; }
};

struct Square : Shape {
    int side;
    constexpr Square(int id, int side) : Shape(id), side(side) {}
    int area() const override { return side * side; }
    const char *name() const override { return "square"; }
};

struct Named {
    virtual const char *label() const { return "named"; }
    virtual int weight() const { return 1; }
};

struct Rect : Shape, Named {
    int w, h;
    constexpr Rect(int id, int w, int h) : Shape(id), w(w), h(h) {}
    int area() const override { return w * h; }
    const char *name() const override { return "rect"; }
    int scale(int k) const override { return k * w; }
    const char *label() const override { return "labelled rect"; }
    int weight() const override { return w + h; }
};

constinit static Square square(1, 7);
constinit static Rect rect(2, 3, 5);
constinit static Named plain;
static Shape *const shapes[] = {&square, &rect};
static const Named *const named[] = {&rect, &plain};

NOINLINE static int describe(const Shape *s) {
    print_str(s->name());
    print_str(" ");
    print_int(s->area());
    return s->scale(2) + s->id;
}

NOINLINE static int describe_named(const Named *n) {
    print_str(n->label());
    print_str("\r\n");
    return n->weight();
}

extern "C" int mainCRTStartup(void) {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    int total = 0;
    for (int i = 0; i < 2; i++) total += describe(shapes[i]);
    for (int i = 0; i < 2; i++) total += describe_named(named[i]);
    print_int(total);
    ExitProcess(0);
    return 0;
}
