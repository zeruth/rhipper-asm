/* misc: what the other programs leave out. Thread-local storage, both kinds: __declspec(thread)
 * variables (the image's TLS directory, reached through fs:[0x2c] and _tls_index) and the TlsAlloc
 * API. Compiler intrinsics that become single instructions (rol/ror, bswap, bsf/bsr, bt, popcnt,
 * lock xadd and cmpxchg, mul's high half). Bitfields, signed and unsigned, across a word. alloca
 * (`sub esp` by a run-time size, through __alloca_probe_16). setjmp and longjmp (the CRT's
 * _setjmp3, a longjmp out of a nested call). */
#include "rhtest.h"

#include <intrin.h>
#include <malloc.h>
#include <setjmp.h>
#include <stdint.h>
#include <string.h>
#include <sys/stat.h>
#include <wchar.h>

__declspec(dllimport) unsigned long __stdcall TlsAlloc(void);
__declspec(dllimport) int __stdcall TlsFree(unsigned long index);
__declspec(dllimport) void *__stdcall TlsGetValue(unsigned long index);
__declspec(dllimport) int __stdcall TlsSetValue(unsigned long index, void *value);
typedef struct { unsigned int MaxCharSize; unsigned char DefaultChar[2]; unsigned char LeadByte[12]; } CpInfo;
__declspec(dllimport) int __stdcall GetCPInfo(unsigned int codepage, CpInfo *info);
/* (a window class: user32, its x86 WNDCLASSEXA) */
#pragma comment(lib, "user32.lib")
typedef long(__stdcall *WndProc32)(void *, unsigned int, unsigned int, long);
typedef struct {
    unsigned int cbSize, style;
    WndProc32 proc;
    int cbClsExtra, cbWndExtra;
    void *hInstance, *hIcon, *hCursor, *hbrBackground;
    const char *menu, *name;
    void *hIconSm;
} WndClassExA32;
__declspec(dllimport) unsigned short __stdcall RegisterClassExA(const WndClassExA32 *wc);
__declspec(dllimport) void *__stdcall CreateWindowExA(unsigned long ex, const char *cls, const char *title,
                                                      unsigned long style, int x, int y, int w, int h, void *parent,
                                                      void *menu, void *inst, void *param);
__declspec(dllimport) long __stdcall SendMessageA(void *hwnd, unsigned int msg, unsigned int wp, long lp);
__declspec(dllimport) long __stdcall DefWindowProcA(void *hwnd, unsigned int msg, unsigned int wp, long lp);
__declspec(dllimport) int __stdcall DestroyWindow(void *hwnd);
__declspec(dllimport) void *__stdcall GetModuleHandleA(const char *name);

/* ---- thread-local storage ---- */
static __declspec(thread) u32 t_count = 5;
static __declspec(thread) u32 t_table[4] = {1, 2, 3, 4};

NOINLINE void tls_bump(u32 k) {
    t_count += k;
    t_table[k & 3] ^= t_count;
}
NOINLINE u32 tls_static(u32 s) {
    u32 k;
    for (k = 0; k < s; k++) tls_bump(k);
    return t_count * 1000 + t_table[0] + t_table[1] * 3 + t_table[2] * 5 + t_table[3] * 7;
}
NOINLINE u32 tls_api(u32 s) {
    unsigned long slot = TlsAlloc();
    u32 v;
    if (slot == 0xFFFFFFFFu) return 0xDEAD;
    TlsSetValue(slot, (void *)(uintptr_t)(s * 3 + 1));
    v = (u32)(uintptr_t)TlsGetValue(slot);
    TlsFree(slot);
    return v;
}

/* ---- intrinsics ---- */
NOINLINE u32 rotates(u32 a, int n) { return _rotl(a, n) ^ _rotr(a, n + 3) ^ (u32)_rotl8((u8)a, (u8)n) << 8; }
NOINLINE u32 swaps(u32 a) { return _byteswap_ulong(a) ^ _byteswap_ushort((u16)a); }
NOINLINE u64 swap64(u64 a) { return _byteswap_uint64(a); }
NOINLINE u32 scans(u32 a) {
    unsigned long lo = 99, hi = 99;
    unsigned char f = _BitScanForward(&lo, a);
    unsigned char r = _BitScanReverse(&hi, a);
    return lo | hi << 8 | (u32)f << 16 | (u32)r << 17;
}
NOINLINE u32 bits(u32 a, u32 b) {
    long v = (long)a;
    u32 t = (u32)_bittest(&v, (long)(b & 31));
    u32 c = (u32)_bittestandcomplement(&v, (long)((b + 1) & 31));
    return t | c << 1 | (u32)v << 2;
}
NOINLINE u32 popcount(u32 a) { return __popcnt(a) | __popcnt16((u16)a) << 8; }
NOINLINE u64 mulhi(u32 a, u32 b) { return __emulu(a, b); }
NOINLINE u32 atomics(u32 s) {
    volatile long n = (long)s;
    long r = _InterlockedIncrement(&n), old, miss;
    r += _InterlockedExchangeAdd(&n, 10);
    old = _InterlockedCompareExchange(&n, 100, (long)s + 11);
    miss = _InterlockedCompareExchange(&n, 5, 1);
    r = r * 31 + old * 7 + miss + _InterlockedExchange(&n, 3);
    return (u32)(r ^ n);
}

/* ---- bitfields ---- */
struct Packed {
    u32 a : 3;
    u32 b : 7;
    i32 c : 5;        /* (signed: read back sign-extended) */
    u32 d : 1;
    u32 e : 16;       /* (ends the first word) */
    u16 f : 4;
    u16 g : 12;
};
NOINLINE void pack(struct Packed *p, u32 s) {
    p->a = s;
    p->b = s * 9;
    p->c = -(i32)s;
    p->d = s & 1;
    p->e = s * 1000;
    p->f = (u16)(s + 3);
    p->g = (u16)(s * 77);
}
NOINLINE u32 unpack(const struct Packed *p) {
    return p->a + p->b * 10 + (u32)(p->c * 100) + p->d * 7 + p->e * 3 + p->f * 11 + p->g * 13;
}
NOINLINE u32 bitfields(u32 s) {
    struct Packed p;
    u32 raw[2];
    memset(&p, 0, sizeof p);
    pack(&p, s);
    p.c += 3;
    p.b ^= 0x55;
    memcpy(raw, &p, sizeof raw);
    return unpack(&p) ^ raw[0] ^ raw[1] << 3;
}

/* ---- alloca ---- */
NOINLINE u32 fill_sum(u8 *buf, u32 n) {
    u32 h = 0, k;
    for (k = 0; k < n; k++) {
        buf[k] = (u8)(k * 7 + n);
        h = h * 33 + buf[k];
    }
    return h;
}
NOINLINE u32 stack_alloc(u32 s) {
    u32 n = s * 37 + 5;                /* (a size known only at run time) */
    u8 *buf = (u8 *)_alloca(n);
    u32 h = fill_sum(buf, n), k;
    u32 *words = (u32 *)_alloca(s * sizeof(u32));
    for (k = 0; k < s; k++) words[k] = buf[k * 3] + k;
    for (k = 0; k < s; k++) h ^= words[k] << (k & 7);
    return h;
}

/* ---- setjmp / longjmp ---- */
static jmp_buf g_jump;
static u32 g_depth;
NOINLINE void dive(u32 n) {
    g_depth++;
    if (n == 0) longjmp(g_jump, (int)g_depth);
    dive(n - 1);
    g_depth += 100;                    /* (never reached) */
}
NOINLINE u32 jumps(u32 s) {
    volatile u32 tries = 0;
    int r;
    g_depth = 0;
    r = setjmp(g_jump);
    tries++;
    if (tries < 3) dive(s + tries);
    return (u32)r * 1000 + tries * 10 + g_depth;
}

/* an 8-byte value a callee fills, returned whole in edx:eax: QueryPerformanceCounter's
 * LARGE_INTEGER. Taken for an object's fields, it was a byte array and the function's return a
 * `byte [8]`, no C (a VS2010 client) */
NOINLINE void fill64(u64 *p, u32 s) { *p = ((u64)(s * 3) << 32) | (s ^ 0x55u); }
NOINLINE u64 filled(u32 s) {
    u64 v;
    fill64(&v, s);
    if (s & 1) return v;
    return 0;
}

/* a switch in a function with a frame pointer (_alloca keeps ebp at /O2): its cases, reached only
 * through the jump table, read the parameters through ebp. Taken for ebp read unset, it made the
 * function take ebp as an input, and its callers stubs (memcpy's tails, a VS2010 client) */
NOINLINE u32 framed_switch(u32 k, u32 a, u32 b) {
    volatile u8 *p = (volatile u8 *)_alloca(16 + (k & 7));
    p[0] = (u8)k;
    switch (k % 7) {
    case 0: return a + p[0];
    case 1: return a * b;
    case 2: return a - b;
    case 3: return b << 3;
    case 4: return a ^ b ^ p[0];
    case 5: return a | b;
    default: return b - a;
    }
}
/* a Windows structure on the stack that a kernel32 call fills, as __setmbcp_nolock's CPINFO: the
 * decompiler, knowing GetCPInfo's type, names such a local `_Stack_1c`, which the translator took
 * for one of its unnamed pieces (a VS2010 client) */
NOINLINE u32 code_page(u32 s) {
    CpInfo ci;
    u32 r, k;
    if (!GetCPInfo(1252, &ci)) return 0;
    r = ci.MaxCharSize * 1000 + ci.DefaultChar[0] + s;
    for (k = 0; k < 12 && ci.LeadByte[k]; k++) r = r * 7 + ci.LeadByte[k];
    return r;
}
/* a window class and its procedure: x86's WNDCLASSEXA is 48 bytes of 4-byte pointers, and the
 * procedure Windows calls is a reference address. Handed to x64's RegisterClassExA as it was, the
 * class was never made and a game client's WinMain had no window. A negative result comes back as
 * one */
static u32 g_msgs;
static long __stdcall test_proc(void *hwnd, unsigned int msg, unsigned int wp, long lp) {
    if (msg == 0x8001) {
        g_msgs += wp;
        return -5;
    }
    if (msg == 1) g_msgs += 1000;   /* WM_CREATE */
    return DefWindowProcA(hwnd, msg, wp, lp);
}
NOINLINE u32 window_class(u32 s) {
    WndClassExA32 wc = {sizeof wc, 0, test_proc, 0, 0, 0, 0, 0, 0, 0, "rhtest_class", 0};
    void *hwnd;
    long r;
    wc.hInstance = GetModuleHandleA(0);
    if (!RegisterClassExA(&wc)) return 1;
    hwnd = CreateWindowExA(0, "rhtest_class", "rhtest", 0, 0, 0, 10, 10, 0, 0, wc.hInstance, 0);
    if (!hwnd) return 2;
    r = SendMessageA(hwnd, 0x8001, s, 0);
    DestroyWindow(hwnd);
    return g_msgs * 10 + (u32)(r + 5);
}
/* a byte-order mark read into a 4-byte local, written from its address plus an offset and
 * compared whole: where a function happens to sit at the mark's value (UTF-8's EF BB BF, 0xbfbbef
 * in a game client's image), Ghidra types the local `code *`, and the write through `&mark + k` is
 * no arithmetic on the pointer (the CRT's _tsopen_nolock was a stub for it) */
NOINLINE u32 bom_kind(const u8 *text, u32 n) {
    u32 mark = 0, k;
    for (k = 0; k < 3 && k < n; k++) *((u8 *)&mark + k) = text[k];
    if (mark == 0xbfbbef) return 8;
    if ((mark & 0xffff) == 0xfeff) return 16;
    return 0;
}
NOINLINE u32 framed_calls(u32 s) {
    u32 r = 0, k;
    for (k = 0; k < 7; k++) r = r * 33 + framed_switch(s + k, s * 5 + k, k * 11);
    return r;
}

/* A file by handle: SetFilePointerEx's LARGE_INTEGER by value, pushed as two words. Ghidra's type
 * library lays it out 8-aligned, one slot late, and the call read dwMoveMethod past what was pushed
 * (a VS2010 client's file reader, 0x4fa4b0, a stub; ApplyFacts splits it, the runtime joins it).
 * And the CRT's _wstat64 of it (FindFirstFileExW's enumerators, undeclared to the port) */
typedef union { struct { unsigned long Low; long High; } s; long long q; } LargeInt;
__declspec(dllimport) void *__stdcall CreateFileW(const wchar_t *, unsigned long, unsigned long, void *, unsigned long,
                                                  unsigned long, void *);
__declspec(dllimport) int __stdcall SetFilePointerEx(void *, LargeInt, LargeInt *, unsigned long);
__declspec(dllimport) int __stdcall SetEndOfFile(void *);
__declspec(dllimport) int __stdcall CloseHandle(void *);
__declspec(dllimport) int __stdcall DeleteFileW(const wchar_t *);
__declspec(dllimport) unsigned long __stdcall GetTempPathW(unsigned long, wchar_t *);
NOINLINE u32 file_seek(u32 s) {
    wchar_t path[300];
    LargeInt d, at;
    struct _stat64 st;
    void *f;
    u32 h = 0;
    unsigned long n = GetTempPathW(260, path), k = 0;
    const wchar_t *name = L"rhtest_seek.bin";
    do path[n + k] = name[k]; while (name[k++]);
    f = CreateFileW(path, 0xC0000000u /* read, write */, 0, 0, 2 /* CREATE_ALWAYS */, 0x80, 0);
    if (f == (void *)-1) return 1;
    d.q = (long long)s * 1000 + 3;
    h = h * 31 + (u32)SetFilePointerEx(f, d, &at, 0 /* FILE_BEGIN */);
    h = h * 31 + (u32)at.q;
    SetEndOfFile(f);
    d.q = -(long long)s;
    h = h * 31 + (u32)SetFilePointerEx(f, d, &at, 1 /* FILE_CURRENT */);
    h = h * 31 + (u32)at.q;
    d.q = 0;
    SetFilePointerEx(f, d, &at, 2 /* FILE_END */);
    h = h * 31 + (u32)at.q;
    CloseHandle(f);
    if (_wstat64(path, &st) == 0) h = h * 31 + (u32)st.st_size;
    DeleteFileW(path);
    return h;
}

/* A COM object Windows hands an x86 caller, called through its vtable: the shell's IMalloc
 * (a client's My Documents lookup, 0x420900; a 4-byte vtable the runtime gives it) */
typedef struct MallocVtbl {
    long(__stdcall *QueryInterface)(void *, const void *, void **);
    unsigned long(__stdcall *AddRef)(void *);
    unsigned long(__stdcall *Release)(void *);
    void *(__stdcall *Alloc)(void *, unsigned long);
    void *(__stdcall *Realloc)(void *, void *, unsigned long);
    void(__stdcall *Free)(void *, void *);
} MallocVtbl;
typedef struct { MallocVtbl *v; } Malloc;
__declspec(dllimport) long __stdcall SHGetMalloc(Malloc **out);
#pragma comment(lib, "shell32.lib")
NOINLINE u32 shell_malloc(u32 s) {
    Malloc *m = 0;
    u32 *p, h = 0, k;
    if (SHGetMalloc(&m) < 0 || !m) return 1;
    p = (u32 *)m->v->Alloc(m, s * 4);
    if (!p) return 2;
    for (k = 0; k < s; k++) p[k] = k * k + s;
    p = (u32 *)m->v->Realloc(m, p, s * 8);
    for (k = 0; k < s; k++) h = h * 31 + p[k];
    m->v->Free(m, p);
    m->v->Release(m);
    return h;
}

/* A heap's free-list unlink, as a VS2010 client's (0x4f56b0): the bucket index computed into an
   address-taken local (std::min/max pick a pointer to it), which MSVC keeps in the dead parameter's
   slot, and the function's eax the parameter on one path, a list head or a mask on the others.
   Ghidra merged them all into the parameter: the C stored the head in the slot, then read the
   index back through the pointer and cleared the wrong bit of the free-bucket mask */
typedef struct Link { struct Link *next, *prev; u32 pad[3], size; } Link;
typedef struct { u32 pad; Link heads[24]; u32 mask; } Buckets;
static const int *pick_max(const int *a, const int *b) { return *a < *b ? b : a; }
static const int *pick_min(const int *a, const int *b) { return *b < *a ? b : a; }
NOINLINE u32 bucket_unlink(Buckets *b, Link *n) {
    if (n != n->next) {
        int i, zero = 0, top = 23;
        unsigned long hi;
        n->next->prev = n->prev;
        n->prev->next = n->next;
        n->next = n->prev = n;
        i = _BitScanReverse(&hi, n->size) ? (int)hi - 7 : -8;
        {
            Link *head = &b->heads[*pick_max(&zero, pick_min(&i, &top))];
            if (head->next == head) b->mask &= ~(1u << (*pick_max(&zero, pick_min(&i, &top)) & 31));
            return (u32)(uintptr_t)head;
        }
    }
    return (u32)(uintptr_t)n;
}
NOINLINE u32 free_buckets(u32 s) {
    static Buckets b;
    static Link nodes[6];
    u32 k, h = 0;
    for (k = 0; k < 24; k++) b.heads[k].next = b.heads[k].prev = &b.heads[k];
    b.mask = 0xffffffu;
    for (k = 0; k < 6; k++) {
        /* each node alone in its bucket: unlinking it empties that bucket */
        Link *head = &b.heads[k * 3 + (s & 1)];
        nodes[k].size = 0x80u << (k * 3 + (s & 1));
        nodes[k].next = nodes[k].prev = head;
        head->next = head->prev = &nodes[k];
    }
    for (k = 0; k < 6; k++) h = h * 31 + (bucket_unlink(&b, &nodes[k]) != 0) + b.mask;
    return h ^ b.mask;
}

/* Lua's table rehash: `nums[ceillog2(k)]++` with ceillog2(k) = log2(k - 1) + 1, the +1 folded by
   MSVC into an array based one element on, and log2(0) = -1 (unsigned 0xffffffff) indexing its
   first element: a 32-bit address wraps there, a 64-bit index went 16 GB on */
static const u8 log2_lut[16] = {0, 0, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 3};
NOINLINE u32 log2_floor(u32 x) {
    u32 l = (u32)-1;
    while (x >= 16) { x >>= 4; l += 4; }
    return l + (x ? log2_lut[x] + 1 : 0);
}
NOINLINE u32 folded_index(u32 s) {
    u32 nums[33] = {0}, k, h = 0;
    for (k = 1; k <= s * 5; k++) nums[log2_floor(k - 1) + 1]++;
    for (k = 0; k < 33; k++) h = h * 31 + nums[k];
    return h;
}

/* Lua's llex: a jump-table switch, a case that calls a noreturn error function with its arguments
   pushed (the `add esp` after the call dead code), and a cookie-guarded buffer. protos32 seeded
   every address nothing else reached at the switch's esp, the dead `add esp` among them, and the
   epilogue's cookie read became 15 stack parameters */
static jmp_buf lex_jb;
__declspec(noreturn) NOINLINE void lex_error(const char *what, u32 a, u32 b, u32 c) {
    (void)what;
    longjmp(lex_jb, (int)(a + b + c + 1));
}
NOINLINE u32 lex_token(const char *p, u32 *pos) {
    char buf[64];
    u32 n = 0;
    for (;;) {
        switch (p[*pos]) {
            case ' ': case '\t': (*pos)++; continue;
            case '-': (*pos)++; return '-';
            case '=': (*pos)++; return p[*pos] == '=' ? ((*pos)++, 0x100) : '=';
            case '<': (*pos)++; return '<';
            case '>': (*pos)++; return '>';
            case '~': (*pos)++; if (p[*pos] != '=') lex_error("bad ~", *pos, n, 3); (*pos)++; return 0x101;
            case '"': case '\'':
                for ((*pos)++; p[*pos] && p[*pos] != '"' && p[*pos] != '\''; (*pos)++)
                    if (n < sizeof buf - 1) buf[n++] = p[*pos];
                buf[n] = 0;
                if (!p[*pos]) lex_error("unfinished string", *pos, n, (u32)buf[0]);
                (*pos)++;
                return 0x200 + n + (u32)buf[0];
            case 0: return 0;
            default: (*pos)++; return (u32)(unsigned char)p[*pos - 1] | 0x400;
        }
    }
}
NOINLINE u32 lex_switch(u32 s) {
    static const char src[] = "a = 'xy' ~= b == c -- d \"ok";
    u32 pos = 0, t;
    volatile u32 h = s;   /* (changed after setjmp, read after longjmp) */
    int e = setjmp(lex_jb);
    if (e) return h * 31 + (u32)e;
    while ((t = lex_token(src, &pos)) != 0) h = h * 31 + t;
    return h;
}

/* memcpy(dst, obj->vt->base(obj) + off, n) with the getter a method: MSVC /O2 pushes n before the
   virtual call (`push ebx; call [vt + 4]; add eax, edi; push eax; push dst; call memcpy; add esp,
   0xc`), and protos32 took the early push for the getter's argument and its `this` */
struct Blob;
typedef struct { u32(__fastcall *size)(struct Blob *); u8 *(__fastcall *base)(struct Blob *); } BlobVt;
typedef struct Blob { const BlobVt *vt; u8 data[64]; } Blob;
static u32 __fastcall blob_size(Blob *b) { return sizeof b->data; }
static u8 *__fastcall blob_base(Blob *b) { return b->data; }
static const BlobVt blob_vt = {blob_size, blob_base};
NOINLINE u32 blob_read(Blob *b, u8 *dst, u32 off, u32 n) {
    memcpy(dst, b->vt->base(b) + off, n);
    return n;
}
NOINLINE u32 early_push(u32 s) {
    Blob b;
    u8 out[16];
    u32 k, h = 0;
    b.vt = &blob_vt;
    for (k = 0; k < 64; k++) b.data[k] = (u8)(k * s + 1);
    for (k = 0; k < 4; k++) {
        blob_read(&b, out, k * 8 + s, 8 + k);
        h = h * 31 + out[0] + out[7] + out[7 + k];
    }
    return h;
}

/* a string literal spelled as a Windows pointer typedef, "PATH" (P and capitals): the literal as
 * written, not the translator's type name for such typedefs (the C runtime's _spawnvpe searched
 * "GEN_LPANY") */
NOINLINE u32 char_sum(const char *p, u32 k) { u32 h = k; while (*p) h = h * 31 + (u8)*p++; return h; }

void rhtest_run(void) {
    u32 s = seed();   /* 7 */
    check("typedef literal", char_sum("PATH", s) ^ char_sum("LPCSTR", s + 1));
    check("tls_static", tls_static(s));
    check("tls_api", tls_api(s));
    check("rotates", rotates(0x12345678u * s, (int)s));
    check("swaps", swaps(0xA1B2C3D4u ^ s));
    check64("swap64", swap64(0x0102030405060708ull * s));
    check("scans", scans(0x00F0F000u >> (s & 3)));
    check("bits", bits(0x5A5A5A5Au ^ s, s));
    check("popcount", popcount(0xF0F0F0F1u * s));
    check64("mulhi", mulhi(0xFFFFFFF0u + s, 0x80000001u));
    check("atomics", atomics(s));
    check("bitfields", bitfields(s));
    check("stack_alloc", stack_alloc(s));
    check("jumps", jumps(s));
    check64("filled", filled(s));
    check("framed_switch", framed_calls(s));
    check("code_page", code_page(s));
    check("window_class", window_class(s));
    check("file_seek", file_seek(s));
    check("shell_malloc", shell_malloc(s));
    check("free_buckets", free_buckets(s));
    check("folded_index", folded_index(s));
    check("lex_switch", lex_switch(s));
    check("early_push", early_push(s));
    {
        static const u8 utf8[] = {0xef, 0xbb, 0xbf, 'x'}, utf16[] = {0xff, 0xfe, 'x', 0};
        check("bom_kind", bom_kind(utf8, 4) * 100 + bom_kind(utf16, s & 3) * 10 + bom_kind(utf8 + 3, 1));
    }
}
