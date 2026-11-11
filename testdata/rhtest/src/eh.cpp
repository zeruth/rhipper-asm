/* eh: exceptions as MSVC builds them for x86. C++ (/EHsc): a frame linked on fs:[0] whose handler
 * is a thunk to __CxxFrameHandler3 with the function's FuncInfo, a state variable the code steps
 * as objects come to life, catch funclets that return where the parent goes on, unwind funclets
 * that destroy what the throw leaves behind, a match by the thrown type's RTTI. And C's structured
 * exceptions: __try/__except with a filter, __finally, through _except_handler4's scope table. */
#include "rhtest.h"

#include <exception>

extern "C" __declspec(dllimport) void __stdcall RaiseException(unsigned long code, unsigned long flags,
                                                               unsigned long n, const unsigned long *args);

static u32 g_events = 0;
NOINLINE void event(u32 k) { g_events = g_events * 31 + k; }

/* an object whose destructor the unwinding runs */
struct Guard {
    u32 id;
    explicit Guard(u32 i) : id(i) { event(100 + i); }
    ~Guard() { event(200 + id); }
};

struct Base {
    int code;
    explicit Base(int c) : code(c) {}
    virtual ~Base() {}
    virtual int kind() const { return 1; }
};
struct Derived : Base {
    explicit Derived(int c) : Base(c) {}
    int kind() const override { return 2; }
};
struct Problem : std::exception {
    int n;
    explicit Problem(int k) : n(k) {}
    const char *what() const noexcept override { return "problem"; }
};

/* ---- throw and catch by value, by type ---- */
NOINLINE void thrower(u32 k) {
    Guard g(k);
    if (k == 1) throw (int)(k * 10);
    if (k == 2) throw Derived((int)k * 7);
    if (k == 3) throw Problem((int)k);
    if (k == 4) throw 2.5;
    event(k);
}
NOINLINE u32 by_type(u32 k) {
    try {
        thrower(k);
        return 1;
    } catch (int v) {
        return 1000 + (u32)v;
    } catch (const Base &b) {                /* (a Derived, caught by its base) */
        return 2000 + (u32)(b.code * 10 + b.kind());
    } catch (const std::exception &e) {
        return 3000 + (u32)e.what()[0];
    } catch (...) {
        return 4000;
    }
}

/* ---- across frames, rethrow, nested ---- */
NOINLINE u32 deep(u32 n) {
    Guard g(10 + n);
    if (n == 0) throw Problem(42);
    return deep(n - 1) + 1;
}
NOINLINE u32 rethrows(u32 s) {
    u32 r = 0;
    try {
        try {
            deep(s & 3);
        } catch (Problem &p) {
            r = (u32)p.n;
            p.n += 1;
            throw;                          /* (the same object, on out) */
        }
    } catch (const Problem &p) {
        r = r * 100 + (u32)p.n;
    }
    return r;
}
NOINLINE u32 nested(u32 s) {
    u32 r = 0;
    try {
        Guard a(20);
        try {
            Guard b(21);
            if (s) throw s;
        } catch (u32 v) {
            r += v;
            Guard c(22);
            throw (int)v * 2;               /* (from inside a catch) */
        }
    } catch (int v) {
        r = r * 1000 + (u32)v;
    }
    return r;
}

/* ---- a constructor that throws: what it built is destroyed ---- */
struct Pair {
    Guard first, second;
    Pair(u32 a, u32 b) : first(a), second(b) {
        if (a + b > 60) throw Base((int)(a + b));
    }
};
NOINLINE u32 ctor(u32 s) {
    try {
        Pair p(30, 30 + s);
        return 1;
    } catch (const Base &b) {
        return (u32)b.code;
    }
}

/* ---- C's structured exceptions ---- */
static u32 g_filtered;
extern "C" NOINLINE int filter(unsigned long code) {
    g_filtered = code;
    return code == 0xE0001234u ? 1 : 0;      /* (EXCEPTION_EXECUTE_HANDLER) */
}
extern "C" NOINLINE u32 seh(u32 s) {
    u32 r = 0;
    __try {
        __try {
            r = s;
            unsigned long args[1] = {s};
            RaiseException(0xE0001234u, 0, 1, args);
            r = 999;
        } __finally {
            r += 100;
        }
    } __except (filter(0xE0001234u)) {
        r = r * 10 + 7;
    }
    return r ^ g_filtered;
}

/* a parameter reassigned in a __try and read by its __finally: MSVC keeps it in the parameter's
 * slot, which the __finally funclet reads through ebp. The frame linked by __SEH_prolog4, whose
 * call fixup shows no fs:[0] store, was not aliased, and the store went as dead (the CRT's
 * __wfsopen, its stream unlocked as garbage in a VS2010 client) */
static u32 g_fin[2];
NOINLINE u32 finally_param(u32 *p, u32 k) {
    __try {
        if (k & 1) p = &g_fin[1];
        *p += k;
    }
    __finally {
        g_fin[0] += *p * 3;
    }
    return g_fin[0] * 100 + g_fin[1];
}

void rhtest_run(void) {
    u32 s = seed();   /* 7 */
    /* (each case's constructions and destructions, in order, after it: the events' hash) */
    check("by_int", by_type(s - 6));
    check("events", g_events);
    check("by_base", by_type(s - 5));
    check("events", g_events);
    check("by_std", by_type(s - 4));
    check("events", g_events);
    check("by_any", by_type(s - 3));
    check("events", g_events);
    check("by_none", by_type(s - 2));
    check("events", g_events);
    check("rethrows", rethrows(s));
    check("events", g_events);
    check("nested", nested(s));
    check("events", g_events);
    check("ctor", ctor(s));
    check("events", g_events);
    check("seh", seh(s));
    check("finally_param", finally_param(&g_fin[0], s));
}
