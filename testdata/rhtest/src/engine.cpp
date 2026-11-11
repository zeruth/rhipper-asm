// engine.cpp: the shapes a game engine's C++ takes that a translation can lose: a stream's
// `s << a << b << c` with every operand pushed before the first call and a virtual getter's call
// among them; a method with a 4-byte local (its prologue's `push ecx`) calling its own virtual
// slot with `this` still in ecx; `getter()->f()->g(0, &a, &b, n)`, a getter of no words before
// calls taking the words pushed ahead of it; a float read through a byte-swapping stream reader
// (`call read; fld [local]; ret`); a reference count released on a worker thread while the main
// thread spins on a member lock; a vector's floats forwarded to another virtual slot from a frame with
// no locals; a singleton's method that never reads `this`; an outer call's arguments pushed before the
// branches that make its object; a 16-byte aligned local's realigned frame.
#include <windows.h>
#include <intrin.h>
#include <string.h>
#include <math.h>
#include "rhtest.h"

// ---- a stream of text, its operators chained --------------------------------------------------

class Stream {
public:
    Stream() : len(0), hash(2166136261u) { buf[0] = 0; }
    NOINLINE Stream &operator<<(const char *s) {
        while (*s && len < sizeof buf - 1) put(*s++);
        buf[len] = 0;
        return *this;
    }
    NOINLINE Stream &operator<<(int v) {
        char t[12];
        int n = 0;
        u32 u = v < 0 ? 0u - (u32)v : (u32)v;
        if (v < 0) put('-');
        do { t[n++] = (char)('0' + u % 10); u /= 10; } while (u);
        while (n) put(t[--n]);
        buf[len] = 0;
        return *this;
    }
    NOINLINE Stream &operator<<(u8 c) {
        put((char)c);
        buf[len] = 0;
        return *this;
    }
    u32 len, hash;
    char buf[96];
private:
    void put(char c) {
        if (len < sizeof buf - 1) buf[len++] = c;
        hash = (hash ^ (u8)c) * 16777619u;
    }
};

class Named {
public:
    virtual ~Named() {}
    virtual const char *name() const = 0;
    virtual int count() const = 0;
    virtual Named *next() const = 0;
};

class Node : public Named {
public:
    Node(const char *n, int c, Node *nx) : nm(n), cnt(c), nxt(nx) {}
    NOINLINE const char *name() const override { return nm; }
    NOINLINE int count() const override { return cnt; }
    NOINLINE Named *next() const override { return nxt; }
private:
    const char *nm;
    int cnt;
    Node *nxt;
};

NOINLINE static void report(Stream &s, const Named *n, int extra)
{
    // (every operand pushed before the first operator call, the getters' calls among them)
    s << "[" << n->name() << ":" << n->count() + extra << "|" << n->next()->name() << "]" << (u8)'.';
}

NOINLINE static void streams()
{
    Node c("gamma", 30 + (int)seed(), 0);
    Node b("beta", -20, &c);
    Node a("alpha", 10, &b);
    c = Node("gamma", 30 + (int)seed(), &a);
    Stream s;
    report(s, &a, 1);
    report(s, &b, (int)seed());
    report(s, &c, -100);
    check("stream len", s.len);
    check("stream hash", s.hash);
    check("stream tail", (u8)s.buf[s.len - 2]);
}

// ---- a method calling its own slot, `this` in ecx, a local reserved by `push ecx` --------------

class Handle {
public:
    Handle(u8 p) : prio(p), level(0xff), flags(0), loads(0) {}
    virtual ~Handle() {}
    virtual u8 priority() { return prio; }
    // (the shape of a resource handle's request: its own priority against the one asked for,
    // through a pointer to whichever is smaller)
    NOINLINE u32 request(u8 asked, int mode) {
        u8 mine = priority();
        u8 *pick = mine <= asked ? &mine : &asked;
        if (level == 0xff) {
            level = *pick;
            loads++;
            flags = (u8)((flags & ~1) | (mode == 1));
            return level;
        }
        if (mode == 0 && (flags & 1) == 0) {
            u8 *lower = level <= *pick ? &level : pick;
            level = *lower;
            return level | 0x100u;
        }
        flags |= (u8)mode;
        return level | 0x200u | (u32)flags << 12;
    }
    u8 prio, level, flags;
    u32 loads;
};

class Texture : public Handle {
public:
    Texture(u8 p, u8 bias) : Handle(p), bias_(bias) {}
    NOINLINE u8 priority() override { return (u8)(prio + bias_); }
private:
    u8 bias_;
};

NOINLINE static void own_slot()
{
    Texture t((u8)(3 + seed()), 2);
    Handle h(9);
    u32 acc = 0;
    acc = acc * 131 + t.request(20, 1);
    acc = acc * 131 + t.request(4, 0);
    acc = acc * 131 + t.request(1, 2);
    acc = acc * 131 + h.request((u8)(5 + seed()), 0);
    acc = acc * 131 + h.request(2, 0);
    check("own slot acc", acc);
    check("own slot loads", t.loads + h.loads * 16);
}

// ---- getter()->f()->g(0, &a, &b, n): a getter of no words amid pushed words ------------------

class Sink {
public:
    NOINLINE u32 put(u32 zero, const u32 *a, const u32 *b, u32 n) {
        total = total * 7 + zero + *a * 3 + *b * 5 + n;
        return total;
    }
    u32 total;
};

class Middle {
public:
    NOINLINE Sink *sink() { return &s; }
    Sink s;
};

class Source {
public:
    virtual ~Source() {}
    virtual Middle *middle() { return &m; }
    virtual u32 scale(u32 v) { return v * 2; }
    Middle m;
};

NOINLINE static u32 feed(Source *src, u32 x, u32 n)
{
    u32 a = x + 1, b = x * 2;
    return src->middle()->sink()->put(0, &a, &b, src->scale(n));
}

NOINLINE static void getter_chains()
{
    Source src;
    u32 r = 0, i;
    src.m.s.total = seed();
    for (i = 0; i < 5; i++) r ^= feed(&src, i + seed(), i * 3);
    check("getter chain", r);
    check("getter total", src.m.s.total);
}

// ---- a float read through a byte-swapping reader: `call read; fld [local]; ret` -----------------

class Reader {
public:
    Reader(const u8 *d, u32 n) : data(d), size(n), pos(0), swap(1) {}
    // (bytes in the file's order, swapped to the machine's: the caller reads them as a float)
    NOINLINE u32 read(void *out, u32 width, u32 count) {
        u8 *o = (u8 *)out;
        u32 i, j;
        for (i = 0; i < count; i++) {
            for (j = 0; j < width; j++)
                o[i * width + j] = pos < size ? data[pos++] : 0;
            if (swap) {
                for (j = 0; j < width / 2; j++) {
                    u8 t = o[i * width + j];
                    o[i * width + j] = o[i * width + width - 1 - j];
                    o[i * width + width - 1 - j] = t;
                }
            }
        }
        return i;
    }
    NOINLINE float read_float() {
        float f;
        read(&f, 4, 1);
        return f;
    }
    const u8 *data;
    u32 size, pos, swap;
};

NOINLINE static void float_reader()
{
    // 1.5f, -2.25f, 1000.0f big-endian
    static const u8 bytes[] = {0x3f, 0xc0, 0, 0, 0xc0, 0x10, 0, 0, 0x44, 0x7a, 0, 0};
    Reader r(bytes, sizeof bytes);
    float a = r.read_float(), b = r.read_float(), c = r.read_float();
    float sum = a * 4 + b * 8 + c / 8 + (float)seed();
    check("reader sum x16", (u32)(int)(sum * 16));
    check("reader pos", r.pos);
}

// ---- a reference count released on a worker thread, a member lock spun on -------------------

class Shared {
public:
    Shared() : refs(1), lock(0), value(0) {}
    NOINLINE void add_ref() { _InterlockedIncrement(&refs); }
    NOINLINE long release() { return _InterlockedDecrement(&refs); }
    NOINLINE void bump(u32 v) {
        while (_InterlockedExchange(&lock, 1)) YieldProcessor();
        u32 t = value;
        if ((v & 31) == 0) SwitchToThread();
        value = t + v;
        _InterlockedExchange(&lock, 0);
    }
    volatile long refs, lock;
    u32 value;
};

static DWORD WINAPI holder(LPVOID p)
{
    Shared *s = (Shared *)p;
    for (u32 i = 1; i <= 500; i++) s->bump(i);
    s->release();
    return 0;
}

NOINLINE static void shared_object()
{
    Shared s;
    HANDLE t[3];
    for (int i = 0; i < 3; i++) {
        s.add_ref();
        t[i] = CreateThread(NULL, 0, holder, &s, 0, NULL);
    }
    for (u32 i = 0; i < 300 + seed(); i++) s.bump(2);
    WaitForMultipleObjects(3, t, TRUE, INFINITE);
    for (int i = 0; i < 3; i++) CloseHandle(t[i]);
    check("shared value", s.value);
    check("shared refs", (u32)s.release());
}

// ---- a vector's floats forwarded to another virtual slot, a frame with no locals ------------------
// (`sub esp, 0xc` then each float stored to [esp + k], room for the call's arguments, not the frame's:
// taken for the frame, a client's GameObject::vf4 called its slot with none of its three floats)

class Mover {
public:
    Mover() : x(0), y(0), z(0), moves(0) {}
    NOINLINE virtual void moveTo(float nx, float ny, float nz) { x = nx; y = ny; z = nz; moves++; }
    NOINLINE virtual void moveToV(const float *v) { moveTo(v[0], v[1], v[2]); }
    float x, y, z;
    u32 moves;
};

// ---- a singleton's method that never reads `this`, its caller's dead `mov ecx, [g]` ----------------

class Registry {
public:
    NOINLINE u32 stamp(u32 *p) { if (p) p[1] += 3; return p ? p[1] : 0; }
};
static Registry *volatile g_registry;

// ---- an outer call's arguments pushed before the branches that make its object --------------------
// (`push n; push 0x11; c ? call a->get : call b->get; jmp join; join: mov ecx, eax; call use`: each
// branch's getter takes none of the words, the joined call both)

class Obj {
public:
    u32 base;
    NOINLINE u32 use(u32 tag, u32 n) { return base * tag + n; }
};
class Maker {
public:
    Obj o;
    NOINLINE virtual Obj *get() { return &o; }
};

NOINLINE static u32 join_args(Maker *a, Maker *b, int c, u32 n)
{
    return (c ? a->get() : b->get())->use(0x11, n);
}

// ---- a 16-byte aligned local: the realigned frame's arguments read through ebx ------------------

NOINLINE static u32 aligned_sum(u32 a, u32 b)
{
    __declspec(align(16)) volatile u32 v[4];
    v[0] = a; v[1] = b; v[2] = a ^ b; v[3] = a + b;
    return v[0] * 3 + v[1] * 5 + v[2] + v[3] * 7;
}

// ---- a 64-bit member returned by a virtual getter, no direct caller: `mov eax, [ecx + 8]; mov edx,
// [ecx + 0xc]; ret` (typed by its callers alone, a JSON parser's IntToken lost its high word) ---------

class Token {
public:
    NOINLINE virtual ~Token() {}
    NOINLINE virtual long long value() { return 0; }
};
class IntToken : public Token {
public:
    explicit IntToken(long long n) : v(n) {}
    NOINLINE virtual long long value() { return v; }
    long long v;
};
static Token *volatile g_token;

// ---- a float result in xmm0 under /GL /arch:SSE, its body SSE alone ---------------------------------

NOINLINE static float inv_sqrt(float x)
{
    return 1.0f / sqrtf(x);
}

NOINLINE static void engine_shapes()
{
    Mover m;
    float v[3] = { 1.5f, (float)seed(), -2.25f };
    m.moveToV(v);
    check("mover", (u32)(int)(m.x * 4 + m.y * 16 + m.z * 64) + m.moves);
    static Registry r;
    u32 slot[2] = { seed(), seed() * 2 };
    g_registry = &r;
    check("registry", g_registry->stamp(slot) + g_registry->stamp(slot));
    Maker a, b;
    a.o.base = seed();
    b.o.base = seed() + 100;
    check("join args", join_args(&a, &b, (int)(seed() & 1), 9) + join_args(&a, &b, 0, 4) * 3);
    check("aligned", aligned_sum(seed(), seed() * 11));
    IntToken t(((long long)seed() << 33) | 0x1234);
    g_token = &t;
    long long tv = g_token->value();
    check("token lo", (u32)tv);
    check("token hi", (u32)(tv >> 32));
    check("inv sqrt", (u32)(int)(inv_sqrt((float)(seed() * seed() + 3)) * 1e6f));
}

extern "C" void rhtest_run(void)
{
    streams();
    own_slot();
    getter_chains();
    float_reader();
    shared_object();
    engine_shapes();
}
