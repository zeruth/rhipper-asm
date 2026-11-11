/* calls: function pointers and calling conventions on x86. __cdecl (the caller pops), __stdcall
 * (the callee pops, `ret N`), __fastcall (ecx, edx, then the stack), varargs (va_list over the
 * caller's arguments), callbacks and tables of pointers, a pointer returned and called, 64-bit and
 * double arguments by each convention, structs passed and returned by value through them. The GL
 * variant (/GL /LTCG) lets the compiler choose its own conventions between functions it sees whole:
 * registers no declared convention uses, arguments it drops. */
#include <stdarg.h>
#include "rhtest.h"

/* conventions, each with a few argument shapes */
NOINLINE int __cdecl cd3(int a, int b, int c) { return a * 100 + b * 10 + c; }
NOINLINE int __stdcall sd3(int a, int b, int c) { return a * 100 - b * 10 + c; }
NOINLINE int __fastcall fc1(int a) { return a * 3 + 1; }
NOINLINE int __fastcall fc2(int a, int b) { return a * b - (a ^ b); }
NOINLINE int __fastcall fc4(int a, int b, int c, int d) { return (a - b) * (c + d); }
NOINLINE int __fastcall fc_byte(u8 a, u16 b, int c) { return a + b * 2 + c * 3; }
NOINLINE i64 __stdcall sd_i64(i64 a, int b, i64 c) { return a * b + c; }
NOINLINE i64 __fastcall fc_i64(int a, i64 b, int c) { return b - (i64)a * c; }
NOINLINE double __stdcall sd_dbl(double a, int b, double c) { return a * b + c; }
NOINLINE double __fastcall fc_dbl(int a, double b, int c) { return b * a - c; }
NOINLINE int __stdcall sd8(int a, int b, int c, int d, int e, int f, int g, int h) {
    return ((((((a * 3 + b) * 3 + c) * 3 + d) * 3 + e) * 3 + f) * 3 + g) * 3 + h;
}

/* structs by value through the conventions */
typedef struct {
    int x, y;
} Pt;
typedef struct {
    int a, b, c, d;
} Quad;

NOINLINE Pt __stdcall pt_add(Pt p, Pt q) {
    Pt r;
    r.x = p.x + q.x;
    r.y = p.y * q.y;
    return r;
}
NOINLINE Quad __fastcall quad_mix(int k, Quad q) {
    Quad r;
    r.a = q.d + k;
    r.b = q.c * k;
    r.c = q.b - k;
    r.d = q.a ^ k;
    return r;
}
NOINLINE u32 quad_hash(Quad q) { return (u32)(q.a * 31 + q.b) * 31u + (u32)(q.c * 31 + q.d); }

/* varargs */
NOINLINE int sum_va(int n, ...) {
    va_list ap;
    int s = 0, k;
    va_start(ap, n);
    for (k = 0; k < n; k++) s += va_arg(ap, int) * (k + 1);
    va_end(ap);
    return s;
}
NOINLINE i64 mixed_va(const char *fmt, ...) {
    va_list ap;
    i64 s = 0;
    va_start(ap, fmt);
    for (; *fmt; fmt++) {
        if (*fmt == 'i')
            s = s * 3 + va_arg(ap, int);
        else if (*fmt == 'l')
            s = s * 5 + va_arg(ap, i64);
        else if (*fmt == 'd')
            s = s * 7 + (i64)(va_arg(ap, double) * 16.0);
    }
    va_end(ap);
    return s;
}
/* a va_list handed on */
NOINLINE int vsum(int n, va_list ap) {
    int s = 0;
    while (n--) s ^= va_arg(ap, int) << (n & 7);
    return s;
}
NOINLINE int sum_via(int n, ...) {
    va_list ap;
    int r;
    va_start(ap, n);
    r = vsum(n, ap);
    va_end(ap);
    return r;
}

/* function pointers: a table, a callback, a pointer returned */
typedef int (*binop)(int, int);
NOINLINE int op_add(int a, int b) { return a + b; }
NOINLINE int op_sub(int a, int b) { return a - b; }
NOINLINE int op_mul(int a, int b) { return a * b; }
NOINLINE int op_xor(int a, int b) { return a ^ b; }
static const binop ops[] = {op_add, op_sub, op_mul, op_xor};

NOINLINE int fold(const int *v, int n, binop f, int init) {
    int k;
    for (k = 0; k < n; k++) init = f(init, v[k]);
    return init;
}
NOINLINE binop pick(u32 k) { return ops[k & 3]; }
NOINLINE int run_ops(u32 s) {
    int v[5], k, r = 0;
    for (k = 0; k < 5; k++) v[k] = (int)s * (k + 2) - 9;
    for (k = 0; k < 4; k++) r = r * 7 + fold(v, 5, pick(s + k), k);
    return r;
}

/* callbacks of each convention, and a pointer in a struct */
typedef int(__stdcall *sd_cb)(int, int, int);
typedef int(__fastcall *fc_cb)(int, int);
typedef struct {
    const char *name;
    fc_cb f;
    int bias;
} Handler;

NOINLINE int call_sd(sd_cb f, int x) { return f(x, x + 1, x + 2) + f(1, 2, 3); }
NOINLINE int call_fc(fc_cb f, int x) { return f(x, 3) - f(5, x); }
NOINLINE int __fastcall fc_or(int a, int b) { return (a | b) + 1; }
static Handler handlers[] = {{"mul", fc2, 1}, {"or", fc_or, 2}};
NOINLINE int run_handlers(int x) {
    int r = 0, k;
    for (k = 0; k < 2; k++) r += handlers[k].f(x + handlers[k].bias, handlers[k].name[0]);
    return r;
}

/* recursion through a pointer, and a pointer to a varargs function */
typedef struct Self {
    u32 (*f)(u32, const struct Self *);
} Self;
NOINLINE u32 tri(u32 n, const Self *self) { return n ? n + self->f(n - 1, self) : 0; }
NOINLINE int call_va(int (*f)(int, ...), int a) { return f(3, a, a * 2, a * 3) + f(1, a); }

/* a sort with a comparator (its own: the CRT's qsort is a program of its own) */
NOINLINE void isort(int *v, int n, int (*cmp)(const int *, const int *)) {
    int i, j;
    for (i = 1; i < n; i++) {
        int t = v[i];
        for (j = i; j > 0 && cmp(&v[j - 1], &t) > 0; j--) v[j] = v[j - 1];
        v[j] = t;
    }
}
NOINLINE int cmp_desc(const int *a, const int *b) { return *b - *a; }
NOINLINE int cmp_mod(const int *a, const int *b) { return (*a % 7) - (*b % 7); }
NOINLINE u32 sort_check(u32 s, int (*cmp)(const int *, const int *)) {
    int v[9], k;
    u32 h = 0;
    for (k = 0; k < 9; k++) v[k] = (int)((s * 2654435761u) >> (k * 3)) % 50;
    isort(v, 9, cmp);
    for (k = 0; k < 9; k++) h = h * 33 + (u32)v[k];
    return h;
}

/* a callee that never writes its ecx input: /GL keeps the caller's pointer in ecx across the
 * call and reads it after, which the decompiler, told every call kills ecx, lost as
 * `extraout_ECX` (a constructor calling a method on `this`, a game client's FUN_00d7be90) */
typedef struct { u32 v[4]; u32 n; } Box;
NOINLINE void __fastcall box_touch(Box *b, u32 k) { b->v[k & 3] += k; b->n++; }
NOINLINE Box *__fastcall box_chain(Box *b, u32 k) {
    b->n = k;
    if (k & 1) box_touch(b, k);
    box_touch(b, k * 3);
    return b;
}
/* a callee-saved register as an input: /GL may hand a static callee the caller's zero in edi,
 * which it pushes, unset, as the arguments of its own calls (the CRT's _tsopen_nolock and its
 * __lseeki64_nolock(fh, 0, 2), a VS2010 client). The push is no save: read so, the value was
 * lost */
NOINLINE u32 __cdecl seek3(u32 fh, u32 lo, u32 hi, u32 origin) { return fh * 1000 + lo + hi * 10 + origin * 100; }
static NOINLINE u32 opener(u32 *fh, u32 mode, u32 zero) {
    u32 r = 0;
    if (mode & 1) r += seek3(*fh, zero, zero, 2);
    if (mode & 2) r += seek3(*fh, zero, zero, zero);
    *fh += r;
    return r;
}
NOINLINE u32 open_helper(u32 s) {
    u32 zero = 0, fh = s, r;
    r = opener(&fh, s & 3, zero);
    r += opener(&fh, (s + 1) & 3, zero);
    return r ^ fh;
}
/* a COM interface from C: `this` the last word pushed, the vtable often loaded into ecx (`mov
 * ecx, [eax]; push eax; call [ecx + 8]`), which is then no __thiscall's object. Read so, Release
 * took its vtable for `this` and dropped the stack one (IMalloc's, in a VS2010 client) */
typedef struct Thing Thing;
typedef struct {
    u32 (__stdcall *query)(Thing *t, u32 iid, void **out);
    u32 (__stdcall *add_ref)(Thing *t);
    u32 (__stdcall *release)(Thing *t);
    u32 (__stdcall *get)(Thing *t, u32 k, u32 m);
} ThingVtbl;
struct Thing { const ThingVtbl *vt; u32 refs, base; };
static NOINLINE u32 __stdcall thing_query(Thing *t, u32 iid, void **out) { *out = t; t->refs++; return iid == 7 ? 0 : 1; }
static NOINLINE u32 __stdcall thing_add_ref(Thing *t) { return ++t->refs; }
static NOINLINE u32 __stdcall thing_release(Thing *t) { return --t->refs * 100 + t->base; }
static NOINLINE u32 __stdcall thing_get(Thing *t, u32 k, u32 m) { return t->base * k + m + t->refs; }
static const ThingVtbl thing_vtbl = {thing_query, thing_add_ref, thing_release, thing_get};
static Thing g_thing = {&thing_vtbl, 1, 0};
static NOINLINE Thing *thing_open(u32 s) { g_thing.base = s; g_thing.refs = 1; return &g_thing; }
NOINLINE u32 com_calls(u32 s) {
    Thing *t = thing_open(s), *u = 0;
    u32 r = t->vt->add_ref(t);
    r += t->vt->get(t, s, 3) * 10;
    r += t->vt->query(t, s, (void **)&u) * 1000;
    r += u->vt->get(u, 2, s) * 100000;
    r += t->vt->release(t);
    return r + u->vt->release(u) * 7;
}
/* and a Release last before a /GS function's restores, `push eax; call edx; ...; pop esi; pop ebx;
 * call __security_check_cookie`: the pops were counted as owed to the cookie check, which took
 * Release's one word (IMalloc's in a VS2010 client) */
NOINLINE u32 com_release_last(u32 s) {
    char name[40];
    Thing *t = thing_open(s);
    u32 k, r = 0;
    for (k = 0; k < sizeof name; k++) name[k] = (char)(s + k);
    for (k = 0; k < sizeof name; k += 3) r += (u8)name[k] * k;
    t->vt->add_ref(t);
    r += t->vt->get(t, (u32)name[s & 31], r & 7);
    return r + t->vt->release(t);
}
NOINLINE u32 boxes(u32 s) {
    Box b = {{1, 2, 3, 4}, 0};
    Box *p = box_chain(&b, s);
    return p->v[0] + p->v[1] * 10 + p->v[2] * 100 + p->v[3] * 1000 + p->n * 10000;
}

/* A recursive search that never writes edx, its callers keeping a pointer there across it (as
 * /GL lets them): the first child whose subtree has a set node, else this node's own slot. Its
 * one result is in eax; the recursion passing only its own on made it an edx:eax one, and the
 * caller's child pointer came back as the lost edx, 0 (a VS2010 client's 0x414500, 2026-10-25) */
typedef struct Tree {
    struct Tree *kids[4];
    int *slots;
} Tree;
NOINLINE int *tree_find(Tree *t, int k) {
    int i;
    for (i = 0; i < 4; i++) {
        if (t->kids[i]) {
            int *r = tree_find(t->kids[i], k);
            if (r[0] != 0) return r;
        }
    }
    return t->slots + k * 2;
}
NOINLINE u32 tree_search(u32 s) {
    static int slots[5][8];
    static Tree leaf[3], root;
    int i, j, *r;
    u32 h = 0;
    for (i = 0; i < 5; i++)
        for (j = 0; j < 8; j++) slots[i][j] = 0;
    for (i = 0; i < 3; i++) {
        leaf[i].kids[0] = leaf[i].kids[1] = leaf[i].kids[2] = leaf[i].kids[3] = 0;
        leaf[i].slots = slots[i];
    }
    root.kids[0] = &leaf[0];
    root.kids[1] = 0;
    root.kids[2] = &leaf[1];
    root.kids[3] = &leaf[2];
    root.slots = slots[3];
    slots[1][(s & 3) * 2] = (int)s * 5;     /* leaf 1's slot: found through the second set child */
    r = tree_find(&root, (int)(s & 3));
    h = h * 31 + (u32)(r - &slots[0][0]) * 7 + (u32)r[0];
    r = tree_find(&root, (int)((s + 1) & 3));
    h = h * 31 + (u32)(r - &slots[0][0]) * 7 + (u32)r[0];
    return h;
}

void rhtest_run(void) {
    u32 s = seed();   /* 7 */
    int i = (int)s;
    Pt p = {i, i + 1}, q = {3, -i};
    Quad qd = {i, i * 2, i * 3, i * 4};
    Self self = {tri};
    check("cd3", (u32)cd3(i, i + 1, i + 2));
    check("sd3", (u32)sd3(i, i + 1, i + 2));
    check("fc1", (u32)fc1(i));
    check("fc2", (u32)fc2(i, -i * 5));
    check("fc4", (u32)fc4(i, 2, i * 3, -1));
    check("fc_byte", (u32)fc_byte((u8)(s * 40), (u16)(s * 9000), -i));
    check64("sd_i64", (u64)sd_i64((i64)s << 33, i, -(i64)s * 1000000007));
    check64("fc_i64", (u64)fc_i64(i, (i64)s << 40, -i * 3));
    check("sd_dbl", (u32)(int)(sd_dbl(i * 1.5, i, 0.25) * 4.0));
    check("fc_dbl", (u32)(int)(fc_dbl(i, i / 4.0, 3) * 8.0));
    check("sd8", (u32)sd8(i, 1, 2, i, 4, 5, i, 7));
    p = pt_add(p, q);
    check("pt_add", (u32)(p.x * 1000 + p.y));
    check("quad_mix", quad_hash(quad_mix(i, qd)));
    check("sum_va", (u32)sum_va(4, i, 2, i * 3, 4));
    check64("mixed_va", (u64)mixed_va("ildi", i, (i64)s << 35, i * 0.75, -i));
    check("sum_via", (u32)sum_via(5, i, i + 1, i + 2, i + 3, i + 4));
    check("run_ops", (u32)run_ops(s));
    check("call_sd", (u32)call_sd(sd3, i));
    check("call_fc", (u32)call_fc(fc2, i));
    check("handlers", (u32)run_handlers(i));
    check("tri", tri(s * 9, &self));
    check("call_va", (u32)call_va(sum_va, i));
    check("sort_desc", sort_check(s, cmp_desc));
    check("sort_mod", sort_check(s + 1, cmp_mod));
    check("boxes", boxes(s));
    check("open_helper", open_helper(s));
    check("com_calls", com_calls(s));
    check("com_release_last", com_release_last(s));
    check("tree_search", tree_search(s));
}
