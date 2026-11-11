/* cpp: C++ as MSVC builds it for x86. __thiscall methods (this in ecx), constructors and
 * destructors, virtual dispatch through vtables, a pure virtual base, virtual destructors called
 * through a base pointer, multiple inheritance (this adjusted for the second base, and the
 * adjustor thunks in its vtable), virtual inheritance (a vbtable), static objects built by the
 * CRT's dynamic initializers (_initterm) and destroyed at exit (atexit), function templates and
 * class templates, operator overloading, references, operator new and delete. No exceptions and
 * no STL: those are programs of their own. */
#include "rhtest.h"

#include <new>

/* a counter of constructions and destructions, by kind, read at the end */
static u32 g_events = 0;
NOINLINE void event(u32 k) { g_events = g_events * 31 + k; }

/* ---- a plain class: members, a constructor, const methods, a static member ---- */
class Counter {
public:
    explicit Counter(int start) : n_(start), steps_(0) { event(1); }
    ~Counter() { event(2); }
    NOINLINE void add(int k) { n_ += k; steps_++; total_ += k; }
    NOINLINE int value() const { return n_ * 10 + steps_; }
    static NOINLINE int total() { return total_; }

private:
    int n_;
    int steps_;
    static int total_;
};
int Counter::total_ = 0;

NOINLINE u32 counters(u32 s) {
    Counter a((int)s), b(-(int)s);
    for (u32 k = 0; k < s; k++) {
        a.add((int)k);
        b.add((int)(k * k));
    }
    return (u32)(a.value() * 1000 + b.value()) ^ (u32)Counter::total();
}

/* ---- virtual dispatch: an abstract base, two derived shapes, a virtual destructor ---- */
class Shape {
public:
    Shape(int id) : id_(id) { event(3); }
    virtual ~Shape() { event(4); }
    virtual int area() const = 0;
    virtual int perimeter() const = 0;
    virtual int kind() const { return 0; }
    int id() const { return id_; }

private:
    int id_;
};

class Rect : public Shape {
public:
    Rect(int id, int w, int h) : Shape(id), w_(w), h_(h) { event(5); }
    ~Rect() override { event(6); }
    int area() const override { return w_ * h_; }
    int perimeter() const override { return 2 * (w_ + h_); }
    int kind() const override { return 1; }

protected:
    int w_, h_;
};

class Square : public Rect {
public:
    Square(int id, int s) : Rect(id, s, s) { event(7); }
    ~Square() override { event(8); }
    int kind() const override { return 2; }
};

class Tri : public Shape {
public:
    Tri(int id, int a, int b, int c) : Shape(id), a_(a), b_(b), c_(c) { event(9); }
    ~Tri() override { event(10); }
    int area() const override { return (a_ * b_) / 2; }      /* (a right triangle's legs) */
    int perimeter() const override { return a_ + b_ + c_; }

private:
    int a_, b_, c_;
};

NOINLINE u32 measure(const Shape &s) { return (u32)(s.area() * 7 + s.perimeter() * 3 + s.kind() * 100 + s.id()); }

NOINLINE u32 shapes(u32 s) {
    Shape *list[4];
    list[0] = new Rect(1, (int)s, (int)s + 2);
    list[1] = new Square(2, (int)s * 2);
    list[2] = new Tri(3, 3 * (int)s, 4 * (int)s, 5 * (int)s);
    list[3] = new Square(4, 1);
    u32 h = 0;
    for (int k = 0; k < 4; k++) h = h * 131 + measure(*list[k]);
    for (int k = 0; k < 4; k++) delete list[k];                /* (through the base: virtual) */
    return h;
}

/* ---- multiple inheritance: the second base's this is adjusted, its vtable thunks back ---- */
/* (members set in constructors, not in the class: VS2010 has no C++11 member initializers) */
struct Named {
    Named() : tag('n') {}
    virtual ~Named() {}
    virtual u32 name_hash() const = 0;
    char tag;
};
struct Valued {
    Valued() : scale(3) {}
    virtual ~Valued() {}
    virtual int get() const = 0;
    virtual void set(int v) = 0;
    int scale;
};
class Item : public Named, public Valued {
public:
    Item(const char *n, int v) : name_(n), v_(v) { event(11); }
    ~Item() override { event(12); }
    u32 name_hash() const override {
        u32 h = 5381;
        for (const char *p = name_; *p; p++) h = h * 33 + (u8)*p;
        return h;
    }
    int get() const override { return v_ * scale; }
    void set(int v) override { v_ = v; }

private:
    const char *name_;
    int v_;
};

NOINLINE int bump(Valued &v, int k) {
    v.set(v.get() + k);
    return v.get();
}
NOINLINE u32 multi(u32 s) {
    Item *it = new Item("widget", (int)s);
    Valued *v = it;                       /* (this + sizeof(Named)) */
    Named *n = it;
    int r = bump(*v, 5);
    u32 h = n->name_hash() ^ (u32)r ^ (u32)(it->get() << 4);
    delete v;                             /* (the thunk adjusts back to the Item) */
    return h;
}

/* ---- virtual inheritance: one shared base reached through a vbtable ---- */
struct Base0 {
    Base0() : b0(0) {}
    int b0;
    virtual int who() const { return 10 + b0; }
    virtual ~Base0() {}
};
struct Left : virtual Base0 {
    Left() : l(1) {}
    int l;
    int who() const override { return 20 + l + b0; }
};
struct Right : virtual Base0 {
    Right() : r(2) {}
    int r;
};
struct Diamond : Left, Right {
    Diamond() : d(3) {}
    int d;
    int who() const override { return 30 + l + r + d + b0; }
};
NOINLINE int via_base(const Base0 *b) { return b->who(); }
NOINLINE u32 diamond(u32 s) {
    Diamond dm;
    dm.b0 = (int)s;
    Left *lp = &dm;
    Right *rp = &dm;
    rp->r += (int)s;
    return (u32)(via_base(&dm) * 1000 + lp->who() * 10 + rp->b0);
}

/* ---- templates ---- */
template <typename T>
NOINLINE T clamp_t(T v, T lo, T hi) { return v < lo ? lo : (v > hi ? hi : v); }

template <typename T, int N>
class Ring {
public:
    Ring() : head_(0), count_(0) {}
    void push(T v) {
        data_[(head_ + count_) % N] = v;
        if (count_ < N) count_++;
        else head_ = (head_ + 1) % N;
    }
    T sum() const {
        T s = T();
        for (int k = 0; k < count_; k++) s = s + data_[(head_ + k) % N];
        return s;
    }
    int size() const { return count_; }

private:
    T data_[N];
    int head_, count_;
};

/* ---- operator overloading, a value type passed and returned by value ---- */
struct V2 {
    int x, y;
    V2 operator+(const V2 &o) const {
        V2 r = {x + o.x, y + o.y};
        return r;
    }
    V2 operator*(int k) const {
        V2 r = {x * k, y * k};
        return r;
    }
    bool operator==(const V2 &o) const { return x == o.x && y == o.y; }
    V2 &operator+=(const V2 &o) {
        x += o.x;
        y += o.y;
        return *this;
    }
};
NOINLINE V2 walk(V2 p, int steps) {
    V2 d = {1, -2};
    for (int k = 0; k < steps; k++) p += d * (k + 1);
    return p;
}

NOINLINE u32 templates(u32 s) {
    Ring<int, 5> ri;
    Ring<V2, 3> rv;
    for (u32 k = 0; k < s + 4; k++) {
        V2 p = {(int)k, (int)(s - k)};
        ri.push((int)(k * k));
        rv.push(p);
    }
    V2 t = rv.sum();
    V2 start = {(int)s, 0};
    V2 w = walk(start, (int)s);
    V2 same = {w.x, w.y};
    return (u32)(ri.sum() * 7 + ri.size()) ^ (u32)(t.x * 100 + t.y) ^ (u32)(w.x * 1000 + w.y) ^
           (u32)clamp_t<int>(-(int)s * 9, -20, 20) * 3 ^ (u32)(clamp_t<double>(s * 1.5, 0.0, 9.0) * 10) ^
           (u32)(w == same) << 30;
}

/* ---- statics: built by the CRT's dynamic initializers before main, torn down at exit ---- */
struct Registry {
    int entries[8];
    int n;
    Registry() : n(0) {
        for (int k = 0; k < 8; k++) entries[k] = k * k + 1;
        event(13);
    }
    ~Registry() { event(14); }
    NOINLINE int sum() const {
        int s = 0;
        for (int k = 0; k < 8; k++) s += entries[k];
        return s + n;
    }
};
static Registry g_registry;
static Counter g_counter(100);

/* ---- references and placement new ---- */
NOINLINE void swap_ref(int &a, int &b) {
    int t = a;
    a = b;
    b = t;
}
NOINLINE u32 refs(u32 s) {
    int a = (int)s, b = (int)s * 3;
    swap_ref(a, b);
    RH_ALIGNAS(Counter) unsigned char buf[sizeof(Counter)];
    Counter *c = new (buf) Counter(a);
    c->add(b);
    u32 r = (u32)c->value();
    c->~Counter();
    return r ^ (u32)(a * 100 + b);
}

/* ---- an object its constructor fills, read here only past its first field and never stored here:
 * the CRT's _LocaleUpdate, whose flag and owner ___crtGetStringTypeA reads after the constructor.
 * Made locals of their own, the constructor's 16 bytes overran a 4-byte one (a VS2010 client)
 * ---- */
struct Update {
    int *info;
    int mode;
    int *owner;
    bool own;
    explicit Update(int *p);
};
NOINLINE Update::Update(int *p) : info(p), mode(*p * 2), owner(p + 1), own(*p > 3) { event(15); }
NOINLINE int update_use(const Update *u, int k) { return *u->info * 3 + u->mode + k; }
static int g_cell[2];
NOINLINE u32 updates(u32 s) {
    g_cell[0] = (int)s;
    g_cell[1] = 0x1f;
    Update u(g_cell);
    int r = update_use(&u, (int)s);
    if (u.own) *u.owner &= ~2;
    return (u32)r * 100 + (u32)g_cell[1];
}

/* ---- virtual methods that return another virtual's result, called only through their vtables: no
 * caller's read types their return. The game's allocator Alloc (`mov eax, esi; pop esi; ret`) and
 * the slot it forwards to (`call eax; pop ebp; ret 0x10`) were void, every allocation 0 (a game
 * client) */
struct Space {
    virtual ~Space() {}
    virtual u8 *raw(u32 n) = 0;
};
struct Arena : Space {
    u8 buf[64];
    u32 used;
    Arena() : used(0) {}
    NOINLINE u8 *raw(u32 n) override {
        u8 *p = buf + used;
        used += n;
        return p;
    }
};
struct Allocator {
    Space *space;
    explicit Allocator(Space *s) : space(s) {}
    virtual ~Allocator() {}
    NOINLINE virtual u8 *get(u32 n) {
        u8 *p = space->raw(n < 4 ? 4 : n);
        if (!p) event(16);
        return p;
    }
};
static Arena g_arena;
static Allocator g_alloc(&g_arena);
NOINLINE Allocator *the_allocator(u32 s) { return s ? &g_alloc : nullptr; }
NOINLINE u32 allocs(u32 s) {
    Allocator *a = the_allocator(s);
    u8 *p = a->get(s), *q = a->get(2);
    if (!p || !q) return 0;
    p[0] = (u8)s;
    return (u32)(q - p) * 100 + p[0] + g_arena.used;
}

/* ---- a constructor's result read past a jump: `new T` is `p = alloc(); if (p) { call ctor; jmp
 * join } xor eax, eax; join: mov [g], eax`. Read only up to the jump, the constructor returned
 * nothing and the global its object went to stayed 0 (a game client's graphics allocators) ---- */
struct Gadget {
    u32 parts[6];
    u32 count;
    NOINLINE explicit Gadget(u32 n) : count(n) {
        for (u32 k = 0; k < 6; k++) parts[k] = n * k;
    }
};
static Gadget *g_gadget;
NOINLINE u32 gadgets(u32 s) {
    g_gadget = new Gadget(s);
    u32 r = g_gadget->count * 10 + g_gadget->parts[5];
    delete g_gadget;
    return r;
}

/* ---- a constructor calling two methods on `this`, ecx unchanged across the first: the
 * decompiler shows the second's `this` and the constructor's result as `in_ecx`, ecx as found,
 * which the port took for a local of its own (a game client's hkThreadMemory constructor) ---- */
struct Pool {
    u32 slots[17][2];
    u32 base, step;
    NOINLINE void clear() {
        for (u32 k = 0; k < 17; k++) slots[k][0] = slots[k][1] = 0;
    }
    NOINLINE void setup(u32 b, u32 st) {
        base = b;
        step = st;
    }
    NOINLINE Pool() {
        clear();
        setup(0, 8);
    }
};
NOINLINE u32 pools(u32 s) {
    Pool *p = new Pool();
    p->slots[s % 17][1] = s;
    u32 r = p->base * 100 + p->step * 10 + p->slots[s % 17][1];
    delete p;
    return r;
}

/* ---- a __cdecl call's argument slot, its cleanup deferred, stored into again as the next virtual
 * call's: `push esi; call note; mov eax, esp; mov [eax], 3; push x; call [vt + 4]`. Counted as the
 * first call's alone, the 3 went unpassed (a game client's Havok thread memory) ---- */
struct Sink {
    u32 got;
    virtual ~Sink() {}
    NOINLINE virtual u32 take(u32 x, u32 flags) {
        got = x * 16 + flags;
        return got;
    }
};
static Sink *g_sink;
NOINLINE void __cdecl note(Sink *s) { g_sink = s; }
NOINLINE u32 sinks(Sink *s, u32 x) {
    note(s);
    return s->take(x, 3);
}

/* ---- a virtual method that forwards to another object's through an indirect tail jump, `mov ecx,
 * [ecx + 4]; mov eax, [ecx]; jmp [eax + 4]`, the caller's arguments handed on as they are. As a
 * call of nothing, a game client's graphics allocator got garbage for its sizes ---- */
struct Backend {
    virtual ~Backend() {}
    NOINLINE virtual u32 grab(u32 size, u32 align) { return size * 1000 + align; }
};
struct Front {
    Backend *back;
    virtual ~Front() {}
    NOINLINE virtual u32 grab(u32 size, u32 align) { return back->grab(size, align); }
};
NOINLINE Front *the_front(Front *f) { return f; }
NOINLINE u32 fronts(u32 s) {
    Backend b;
    Front f;
    f.back = &b;
    return the_front(&f)->grab(s * 3, 16);
}

/* ---- function-local statics built on first use: MSVC 14's thread-safe initialization (the
 * _Init_thread_header / _Init_thread_footer guard and the thread's epoch in its TLS block), VS2010's
 * plain guard bit; a second call takes the built object, and a destructor registered with atexit
 * runs at exit ---- */
struct Lazy {
    u32 n, base;
    explicit Lazy(u32 b) : n(0), base(b * 17) { g_events += 1000; }
    ~Lazy() { g_events += 1; }
    NOINLINE u32 next() { return base + n++; }
};
NOINLINE Lazy &the_lazy(u32 s) {
    static Lazy lazy(s);
    return lazy;
}
NOINLINE u32 lazies(u32 s) {
    static u32 calls = s * 3;
    u32 r = the_lazy(s).next();
    r = r * 10 + the_lazy(s + 100).next();
    calls += r;
    return r ^ calls << 4;
}

/* ---- a virtual call reached only by a branch, `this` loaded before it: the other way in
 * returns first, and before the call in address order is a `ret`. A VS2010 client's 0x4de9a0:
 * `mov ecx, esi; test al, al; jne L` ... `L: mov edx, [esi]; mov eax, [edx + 8]; call eax`, its
 * AddRef given no `this` by a scan back across that ret (2026-10-25) ---- */
struct Counted {
    int refs, inits;
    Counted() : refs(1), inits(0) {}
    virtual ~Counted() {}
    virtual bool init(int k) { inits += k; return (k & 1) != 0; }
    virtual void grab() { refs++; }
    virtual void drop() { refs--; }
};
NOINLINE int counted_take(Counted *c, int k) {
    if (!c->init(k)) {
        c->drop();
        return -c->refs;
    }
    c->grab();
    return c->refs * 10 + c->inits;
}
NOINLINE u32 counteds(u32 s) {
    Counted a, b;
    return (u32)(counted_take(&a, (int)s) * 100 + counted_take(&b, (int)s + 1));
}

void rhtest_run(void) {
    u32 s = seed();   /* 7 */
    check("statics", (u32)g_registry.sum() ^ (u32)g_counter.value());
    check("counters", counters(s));
    check("shapes", shapes(s));
    check("multi", multi(s));
    check("diamond", diamond(s));
    check("templates", templates(s));
    check("refs", refs(s));
    check("updates", updates(s));
    check("allocs", allocs(s));
    check("gadgets", gadgets(s));
    check("pools", pools(s));
    check("fronts", fronts(s));
    check("lazies", lazies(s) * 3 + lazies(s + 1));
    {
        Sink sk;
        check("sinks", sinks(&sk, s) * 100 + (g_sink == &sk));
    }
    check("counteds", counteds(s));
    check("events", g_events);
}
