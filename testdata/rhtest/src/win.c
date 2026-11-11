/* win.c: the Win32 a game client leans on, where x86 and x64 part: window messages whose lParam
 * points to a structure of handles (WINDOWPOS, NCCALCSIZE_PARAMS, CREATESTRUCT) handed to the
 * reference's window procedure and on to DefWindowProc, the thread message queue's MSG, Winsock's
 * WSADATA, a spin lock on a global by `xchg` shared by threads, TLS read through an import MSVC
 * keeps in a register across branches, critical sections and events between threads. */
#define WIN32_LEAN_AND_MEAN
#include <winsock2.h>
#include <windows.h>
#include <intrin.h>
#include <string.h>
#include "rhtest.h"

#pragma comment(lib, "user32.lib")
#pragma comment(lib, "ws2_32.lib")

/* ---- a window procedure that changes what it is handed ------------------------------------- */

static u32 g_created;       /* lpCreateParams, as WM_CREATE's CREATESTRUCT held it */
static u32 g_create_cx;     /* its cx */
static u32 g_changing;      /* WM_WINDOWPOSCHANGING seen, with the size asked for */
static u32 g_asked;         /* the cx and cy asked for, packed */
static u32 g_calcs;         /* WM_NCCALCSIZE with its parameters */
static u32 g_changed;       /* WM_WINDOWPOSCHANGED's x, y packed */

NOINLINE static LRESULT CALLBACK wndproc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp)
{
    switch (msg) {
    case WM_CREATE: {
        const CREATESTRUCTA *c = (const CREATESTRUCTA *)lp;
        g_created = (u32)(UINT_PTR)c->lpCreateParams;
        g_create_cx = (u32)c->cx;
        break;
    }
    case WM_WINDOWPOSCHANGING: {
        WINDOWPOS *p = (WINDOWPOS *)lp;
        if (!(p->flags & SWP_NOSIZE)) {
            g_changing++;
            g_asked = ((u32)p->cx << 16) | (u32)p->cy;
            /* (narrowed to a multiple of 64, and never above 256: what the procedure asks for wins) */
            p->cx = p->cx > 256 ? 256 : p->cx & ~63;
        }
        break;
    }
    case WM_NCCALCSIZE:
        if (wp) {
            NCCALCSIZE_PARAMS *n = (NCCALCSIZE_PARAMS *)lp;
            g_calcs++;
            /* (a border of 8 on every side, and DefWindowProc not asked) */
            n->rgrc[0].left += 8;
            n->rgrc[0].top += 8;
            n->rgrc[0].right -= 8;
            n->rgrc[0].bottom -= 8;
            return 0;
        }
        break;
    case WM_WINDOWPOSCHANGED: {
        const WINDOWPOS *p = (const WINDOWPOS *)lp;
        g_changed = ((u32)(p->x & 0xffff) << 16) | (u32)(p->y & 0xffff);
        break;
    }
    case WM_USER + 7:
        return (LRESULT)(wp * 3 + lp);
    }
    return DefWindowProcA(hwnd, msg, wp, lp);
}

NOINLINE static void window_messages(void)
{
    WNDCLASSA wc;
    HWND hwnd;
    RECT r, c;
    memset(&wc, 0, sizeof wc);
    wc.lpfnWndProc = wndproc;
    wc.hInstance = GetModuleHandleA(NULL);
    wc.lpszClassName = "rhtest_win";
    check("class registered", RegisterClassA(&wc) != 0);
    hwnd = CreateWindowExA(0, "rhtest_win", "rhtest", WS_POPUP, 40 + seed(), 30, 320 + seed(), 200,
                           NULL, NULL, wc.hInstance, (LPVOID)(UINT_PTR)(0x1234 + seed()));
    check("window made", hwnd != NULL);
    check("create params", g_created);
    check("create cx", g_create_cx);
    SetWindowPos(hwnd, NULL, 100 + seed(), 50, 300, 150 + seed(), SWP_NOZORDER | SWP_NOACTIVATE);
    check("pos changing seen", g_changing != 0);
    check("size asked", g_asked);
    check("calcsize seen", g_calcs != 0);
    check("pos changed", g_changed);
    GetWindowRect(hwnd, &r);
    check("window x", (u32)r.left);
    check("window y", (u32)r.top);
    check("window cx", (u32)(r.right - r.left));
    check("window cy", (u32)(r.bottom - r.top));
    GetClientRect(hwnd, &c);
    check("client cx", (u32)(c.right - c.left));
    check("client cy", (u32)(c.bottom - c.top));
    check("sent", (u32)SendMessageA(hwnd, WM_USER + 7, 5 + seed(), 100));
    DestroyWindow(hwnd);
    UnregisterClassA("rhtest_win", wc.hInstance);
}

/* ---- the thread's message queue: MSG back from PeekMessage ---------------------------------- */

NOINLINE static void thread_queue(void)
{
    MSG m;
    u32 sum = 0, n = 0;
    PeekMessageA(&m, NULL, 0, 0, PM_NOREMOVE);           /* (makes the queue) */
    PostThreadMessageA(GetCurrentThreadId(), WM_APP + 1, 0x11 + seed(), 0x2200);
    PostThreadMessageA(GetCurrentThreadId(), WM_APP + 2, 0x33, (LPARAM)-5);
    while (PeekMessageA(&m, NULL, WM_APP, WM_APP + 10, PM_REMOVE)) {
        n++;
        sum = sum * 31 + (u32)m.message;
        sum = sum * 31 + (u32)m.wParam;
        sum = sum * 31 + (u32)m.lParam;
        check("msg hwnd", m.hwnd == NULL);
    }
    check("queued", n);
    check("queue sum", sum);
}

/* ---- Winsock's WSADATA, laid out apart on x86 and x64 --------------------------------------- */

NOINLINE static void winsock(void)
{
    WSADATA d;
    int r;
    memset(&d, 0xcc, sizeof d);
    r = WSAStartup(MAKEWORD(2, 2), &d);
    check("wsastartup", (u32)r);
    check("wsa version", d.wVersion);
    check("wsa high version", d.wHighVersion);
    check("wsa description", (u32)strlen(d.szDescription) > 0);
    check("wsa description starts", (u32)(u8)d.szDescription[0]);
    WSACleanup();
}

/* ---- a spin lock on a global, `xchg`, shared by threads ------------------------------------- */

static volatile long g_spin;
static u32 g_counter;
static u32 g_rounds;

NOINLINE static void spin_lock(void)
{
    while (_InterlockedExchange(&g_spin, 1) != 0) {
        /* (a waiter's own write of 1 must never land after the holder's release) */
        YieldProcessor();
    }
}

NOINLINE static void spin_unlock(void)
{
    _InterlockedExchange(&g_spin, 0);
}

NOINLINE static DWORD WINAPI spinner(LPVOID arg)
{
    u32 k, n = (u32)(UINT_PTR)arg;
    for (k = 0; k < g_rounds; k++) {
        spin_lock();
        /* (read, yield, write: unprotected, two threads in here lose an increment) */
        {
            u32 v = g_counter;
            if ((k & 63) == 0) SwitchToThread();
            g_counter = v + n;
        }
        spin_unlock();
    }
    return 0;
}

NOINLINE static void spin_threads(void)
{
    HANDLE t[4];
    u32 i;
    g_counter = 0;
    g_rounds = 2000 + seed();
    for (i = 0; i < 4; i++) t[i] = CreateThread(NULL, 0, spinner, (LPVOID)(UINT_PTR)(i + 1), 0, NULL);
    WaitForMultipleObjects(4, t, TRUE, INFINITE);
    for (i = 0; i < 4; i++) CloseHandle(t[i]);
    check("spin counter", g_counter);
    check("spin free", (u32)g_spin);
}

/* ---- TLS through an import MSVC keeps in a register across branches ------------------------- */

static DWORD g_tls;

typedef struct { u32 depth, hits, last; } TlsState;

NOINLINE static u32 tls_step(u32 v)
{
    TlsState *s = (TlsState *)TlsGetValue(g_tls);
    if (v & 1) {
        s->depth++;
        s = (TlsState *)TlsGetValue(g_tls);
        s->last = v;
    } else {
        s->hits += v;
    }
    /* (the third read after the join: the import still in its register) */
    s = (TlsState *)TlsGetValue(g_tls);
    return s->depth * 1000 + s->hits + s->last;
}

NOINLINE static void tls(void)
{
    TlsState st = {0, 0, 0};
    u32 v, acc = 0;
    g_tls = TlsAlloc();
    TlsSetValue(g_tls, &st);
    for (v = 0; v < 9 + seed(); v++) acc = acc * 7 + tls_step(v);
    check("tls acc", acc);
    check("tls depth", st.depth);
    TlsFree(g_tls);
}

/* ---- a job queue: a critical section, an event, a worker thread ----------------------------- */

static CRITICAL_SECTION g_cs;
static HANDLE g_work, g_done;
static u32 g_jobs[16];
static u32 g_head, g_tail, g_result;

NOINLINE static DWORD WINAPI worker(LPVOID arg)
{
    (void)arg;
    for (;;) {
        u32 job;
        WaitForSingleObject(g_work, INFINITE);
        EnterCriticalSection(&g_cs);
        job = g_head == g_tail ? 0 : g_jobs[g_head++ & 15];
        LeaveCriticalSection(&g_cs);
        if (job == 0xffffffff) break;
        g_result = g_result * 33 + job;
        SetEvent(g_done);
    }
    SetEvent(g_done);
    return 0;
}

NOINLINE static void job_queue(void)
{
    HANDLE t;
    u32 i;
    InitializeCriticalSection(&g_cs);
    g_work = CreateEventA(NULL, FALSE, FALSE, NULL);
    g_done = CreateEventA(NULL, FALSE, FALSE, NULL);
    t = CreateThread(NULL, 0, worker, NULL, 0, NULL);
    for (i = 1; i <= 6; i++) {
        EnterCriticalSection(&g_cs);
        g_jobs[g_tail++ & 15] = i * 11 + seed();
        LeaveCriticalSection(&g_cs);
        SetEvent(g_work);
        WaitForSingleObject(g_done, INFINITE);
    }
    EnterCriticalSection(&g_cs);
    g_jobs[g_tail++ & 15] = 0xffffffff;
    LeaveCriticalSection(&g_cs);
    SetEvent(g_work);
    WaitForSingleObject(t, INFINITE);
    CloseHandle(t);
    CloseHandle(g_work);
    CloseHandle(g_done);
    DeleteCriticalSection(&g_cs);
    check("jobs result", g_result);
}

void rhtest_run(void)
{
    window_messages();
    thread_queue();
    winsock();
    spin_threads();
    tls();
    job_queue();
}
