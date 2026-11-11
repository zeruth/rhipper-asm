/* seh.c: structured exceptions the hardware raises, not only RaiseException's: an access violation
 * reading and writing (its ExceptionInformation, the address and the kind, ULONG_PTR-sized), an
 * integer division by zero, a privileged instruction, a breakpoint; the filter reading
 * GetExceptionInformation's record and continuing past a fault after fixing what faulted;
 * RaiseException's parameters as ULONG_PTRs; a vectored handler of the program's own counting what
 * passes it; nested __try with __finally run on every way out (fall-through, an exception, a
 * return, a leave); and AbnormalTermination; the chain after SEH frames returned (their nodes
 * unlinked), a SetThreadName raise after _beginthreadex. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <process.h>
#include "rhtest.h"

static volatile int *g_null;
static volatile int g_zero;
static int g_target;

NOINLINE static u32 av_filter(EXCEPTION_POINTERS *ep, u32 *kind, u32 *addr)
{
    EXCEPTION_RECORD *r = ep->ExceptionRecord;
    if (r->ExceptionCode != EXCEPTION_ACCESS_VIOLATION) return EXCEPTION_CONTINUE_SEARCH;
    *kind = (u32)r->ExceptionInformation[0];
    *addr = (u32)r->ExceptionInformation[1];
    return EXCEPTION_EXECUTE_HANDLER;
}

NOINLINE static void access_violations(void)
{
    u32 kind = 99, addr = 99, hit = 0;
    __try {
        hit = (u32)*(volatile int *)((char *)g_null + 0x10 + seed());
    } __except (av_filter(GetExceptionInformation(), &kind, &addr)) {
        hit = 1000;
    }
    check("av read caught", hit);
    check("av read kind", kind);
    check("av read address", addr);
    __try {
        *(volatile int *)((char *)g_null + 0x20) = 5;
    } __except (av_filter(GetExceptionInformation(), &kind, &addr)) {
        hit = 2000;
    }
    check("av write caught", hit);
    check("av write kind", kind);
    check("av write address", addr);
}

NOINLINE static void arithmetic(void)
{
    u32 code = 0;
    volatile int r = 0;
    __try {
        r = 100 / (g_zero + (int)seed() - 7);
    } __except (code = GetExceptionCode(), EXCEPTION_EXECUTE_HANDLER) {
        r = -1;
    }
    check("div zero code", code);
    check("div zero result", (u32)r);
    code = 0;
    __try {
        r = (int)0x80000000 / (g_zero - 1 - (int)seed() + 7);
    } __except (code = GetExceptionCode(), EXCEPTION_EXECUTE_HANDLER) {
        r = -2;
    }
    check("int overflow code", code);
    check("int overflow result", (u32)r);
}

NOINLINE static void instructions(void)
{
    u32 code = 0;
    __try {
        __debugbreak();
    } __except (code = GetExceptionCode(), EXCEPTION_EXECUTE_HANDLER) {
    }
    check("breakpoint code", code);
    code = 0;
    __try {
        __halt();
    } __except (code = GetExceptionCode(), EXCEPTION_EXECUTE_HANDLER) {
    }
    check("privileged code", code);
}

/* ---- a filter that fixes what faulted and continues ---------------------------------------- */

static u32 g_fixes;

NOINLINE static LONG fix_and_continue(EXCEPTION_POINTERS *ep, char *page)
{
    DWORD old;
    EXCEPTION_RECORD *r = ep->ExceptionRecord;
    if (r->ExceptionCode != EXCEPTION_ACCESS_VIOLATION || (char *)r->ExceptionInformation[1] < page
        || (char *)r->ExceptionInformation[1] >= page + 0x1000)
        return EXCEPTION_CONTINUE_SEARCH;
    /* (the page made readable: the faulting load, retried, succeeds) */
    g_fixes++;
    VirtualProtect(page, 0x1000, PAGE_READWRITE, &old);
    return EXCEPTION_CONTINUE_EXECUTION;
}

NOINLINE static int read_at(volatile int *p)
{
    return *p;
}

NOINLINE static void continue_execution(void)
{
    char *page = (char *)VirtualAlloc(NULL, 0x1000, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    DWORD old;
    int v = 0;
    *(int *)(page + 0x40) = 0x77 + (int)seed();
    VirtualProtect(page, 0x1000, PAGE_NOACCESS, &old);
    __try {
        v = read_at((volatile int *)(page + 0x40));
    } __except (fix_and_continue(GetExceptionInformation(), page)) {
        v = -1;
    }
    check("continued", (u32)v);
    check("continued fixes", g_fixes);
    VirtualFree(page, 0, MEM_RELEASE);
}

/* ---- RaiseException's parameters ----------------------------------------------------------- */

NOINLINE static LONG params_filter(EXCEPTION_POINTERS *ep, u32 *out)
{
    EXCEPTION_RECORD *r = ep->ExceptionRecord;
    u32 i;
    if (r->ExceptionCode != 0xE0001234u) return EXCEPTION_CONTINUE_SEARCH;
    out[0] = r->NumberParameters;
    out[1] = r->ExceptionFlags;
    for (i = 0; i < r->NumberParameters && i < 4; i++) out[2 + i] = (u32)r->ExceptionInformation[i];
    return EXCEPTION_EXECUTE_HANDLER;
}

NOINLINE static void raised_params(void)
{
    ULONG_PTR args[4];
    u32 got[6] = {0, 0, 0, 0, 0, 0};
    args[0] = 0x11 + seed();
    args[1] = 0xffffffff;
    args[2] = (ULONG_PTR)&g_target;
    args[3] = 0;
    __try {
        RaiseException(0xE0001234u, EXCEPTION_NONCONTINUABLE, 4, args);
    } __except (params_filter(GetExceptionInformation(), got)) {
    }
    check("raised count", got[0]);
    check("raised flags", got[1]);
    check("raised arg0", got[2]);
    check("raised arg1", got[3]);
    check("raised arg2 is pointer", got[4] == (u32)(UINT_PTR)&g_target);
    check("raised arg3", got[5]);
}

/* ---- a vectored handler of the program's own ----------------------------------------------- */

static volatile long g_vectored;

NOINLINE static LONG CALLBACK vectored(EXCEPTION_POINTERS *ep)
{
    if (ep->ExceptionRecord->ExceptionCode == 0xE0005678u)
        InterlockedExchangeAdd(&g_vectored, (long)ep->ExceptionRecord->NumberParameters + 1);
    return EXCEPTION_CONTINUE_SEARCH;
}

NOINLINE static void vectored_handler(void)
{
    PVOID h = AddVectoredExceptionHandler(1, vectored);
    u32 caught = 0;
    ULONG_PTR one = 1;
    check("vectored added", h != NULL);
    __try {
        RaiseException(0xE0005678u, 0, 1, &one);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        caught = 1;
    }
    __try {
        RaiseException(0xE0005678u, 0, 0, NULL);
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        caught += 10;
    }
    check("vectored count", (u32)g_vectored);
    check("vectored caught", caught);
    check("vectored removed", RemoveVectoredExceptionHandler(h));
}

/* ---- __finally on every way out ------------------------------------------------------------- */

static u32 g_trail;

NOINLINE static int finally_return(int x)
{
    __try {
        if (x > 3) return x * 2;
        g_trail = g_trail * 10 + 1;
    } __finally {
        g_trail = g_trail * 10 + (AbnormalTermination() ? 9 : 2);
    }
    return x;
}

NOINLINE static void finally_leave(int x)
{
    __try {
        if (x) __leave;
        g_trail = g_trail * 10 + 3;
    } __finally {
        g_trail = g_trail * 10 + (AbnormalTermination() ? 8 : 4);
    }
}

NOINLINE static void finally_nested(void)
{
    __try {
        __try {
            __try {
                g_trail = g_trail * 10 + 5;
                *(volatile int *)g_null = 1;
            } __finally {
                g_trail = g_trail * 10 + (AbnormalTermination() ? 6 : 0);
            }
        } __finally {
            g_trail = g_trail * 10 + 7;
        }
    } __except (EXCEPTION_EXECUTE_HANDLER) {
        g_trail = g_trail * 10 + 1;
    }
}

NOINLINE static void finally_paths(void)
{
    g_trail = 0;
    check("finally return big", (u32)finally_return(5 + (int)seed() - 7));
    check("finally return small", (u32)finally_return(1));
    finally_leave(1);
    finally_leave(0);
    finally_nested();
    check("finally trail", g_trail);
}

/* ---- the chain after an SEH frame has returned --------------------------------------------- */

/* A frame that registers in fs:[0] and returns must unlink itself: VS2010's __SEH_prolog4 and
   __SEH_epilog4 (its own code and the /MT CRT's, _beginthreadex's __initptd among them). A node
   left behind sat at the head of the chain in a dead frame, and the next raise walked into
   whatever reused its stack (a client's SetThreadName, 0x406D1388, died unhandled). */

static u32 g_unlinked;

NOINLINE static u32 guarded_leaf(u32 x)
{
    __try {
        g_unlinked = g_unlinked * 10 + (x & 7);
    } __finally {
        g_unlinked = g_unlinked * 10 + (AbnormalTermination() ? 9 : 1);
    }
    return x + 1;
}

/* the dead frames' stack reused: a guarded leaf's node, if left linked, is overwritten here */
NOINLINE static u32 scribble(u32 x)
{
    volatile u32 junk[64];
    u32 i, sum = 0;
    for (i = 0; i < 64; i++) junk[i] = x + i * 4;
    for (i = 0; i < 64; i++) sum += junk[i];
    return sum;
}

static unsigned __stdcall quiet_thread(void *arg)
{
    return (unsigned)(UINT_PTR)arg + 1;
}

typedef struct { DWORD type; LPCSTR name; DWORD thread; DWORD flags; } ThreadNameInfo;

NOINLINE static u32 name_thread(DWORD id)
{
    ThreadNameInfo info;
    u32 caught = 0;
    info.type = 0x1000;
    info.name = "rhtest worker";
    info.thread = id;
    info.flags = 0;
    __try {
        RaiseException(0x406D1388u, 0, sizeof info / sizeof(ULONG_PTR), (ULONG_PTR *)&info);
    } __except (GetExceptionCode() == 0x406D1388u ? EXCEPTION_EXECUTE_HANDLER : EXCEPTION_CONTINUE_SEARCH) {
        caught = 1;
    }
    return caught;
}

NOINLINE static void chain_after_return(void)
{
    unsigned id = 0;
    DWORD code = 0;
    HANDLE h;
    g_unlinked = 0;
    check("guarded leaf", guarded_leaf(seed()));
    check("guarded leaf again", guarded_leaf(seed() + 2));
    check("scribble", scribble(seed()));
    check("raise after guarded leaves", name_thread(1));
    h = (HANDLE)_beginthreadex(NULL, 0, quiet_thread, (void *)(UINT_PTR)seed(), CREATE_SUSPENDED, &id);
    check("thread made", h != NULL);
    check("scribble after the thread", scribble(seed() + 1));
    check("raise after _beginthreadex", name_thread(id));
    ResumeThread(h);
    WaitForSingleObject(h, INFINITE);
    GetExitCodeThread(h, &code);
    CloseHandle(h);
    check("thread exit", code);
    check("guarded trail", g_unlinked);
}

void rhtest_run(void)
{
    access_violations();
    arithmetic();
    instructions();
    continue_execution();
    raised_params();
    vectored_handler();
    finally_paths();
    chain_after_return();
}
