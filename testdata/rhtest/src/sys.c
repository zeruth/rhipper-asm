/* sys.c: the kernel32 a game client calls with structures that are laid out apart on x86 and x64
 * (pointer- and ULONG_PTR-sized fields) or that the system calls back into: OVERLAPPED file I/O,
 * WIN32_FIND_DATA, BY_HANDLE_FILE_INFORMATION, MEMORY_BASIC_INFORMATION, SYSTEM_INFO's address
 * range, private heaps, the interlocked singly linked list (16-byte aligned entries on x64),
 * InitOnceExecuteOnce, the timer queue, QueueUserWorkItem, a thread's TIMERPROC, FLS callbacks
 * run at a thread's exit, and functions found by GetProcAddress called through their pointers.
 * Only what is the same on every machine is printed (no sizes of memory, no counts of processors,
 * no versions): the expectation is made here and compared on another. */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <malloc.h>
#include <string.h>
#include "rhtest.h"

#pragma comment(lib, "user32.lib")

/* ---- a file through OVERLAPPED offsets ------------------------------------------------------ */

static const char g_file[] = "rhtest_sys.tmp";

NOINLINE static u32 fnv(const u8 *p, u32 n)
{
    u32 h = 2166136261u, i;
    for (i = 0; i < n; i++) h = (h ^ p[i]) * 16777619u;
    return h;
}

NOINLINE static void overlapped_file(void)
{
    HANDLE f;
    OVERLAPPED o;
    u8 buf[64], back[64];
    DWORD n = 0;
    u32 i;
    BY_HANDLE_FILE_INFORMATION info;
    for (i = 0; i < sizeof buf; i++) buf[i] = (u8)(i * 7 + seed());
    f = CreateFileA(g_file, GENERIC_READ | GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
    check("file opened", f != INVALID_HANDLE_VALUE);
    /* (two writes at offsets the OVERLAPPED says, the second before the first) */
    memset(&o, 0, sizeof o);
    o.Offset = 32;
    check("write at 32", WriteFile(f, buf + 32, 32, &n, &o) && n == 32);
    memset(&o, 0, sizeof o);
    o.Offset = 0;
    check("write at 0", WriteFile(f, buf, 32, &n, &o) && n == 32);
    check("overlapped result", GetOverlappedResult(f, &o, &n, FALSE) && n == 32);
    memset(back, 0, sizeof back);
    memset(&o, 0, sizeof o);
    o.Offset = 16;
    check("read at 16", ReadFile(f, back, 40, &n, &o) && n == 40);
    check("read hash", fnv(back, 40));
    check("read matches", memcmp(back, buf + 16, 40) == 0);
    check("file info", GetFileInformationByHandle(f, &info));
    check("info size", info.nFileSizeLow);
    check("info links", info.nNumberOfLinks);
    check("info attributes", info.dwFileAttributes & FILE_ATTRIBUTE_ARCHIVE ? 1u : 0u);
    CloseHandle(f);
}

NOINLINE static void find_file(void)
{
    WIN32_FIND_DATAA d;
    HANDLE h;
    memset(&d, 0xcc, sizeof d);
    h = FindFirstFileA(g_file, &d);
    check("found", h != INVALID_HANDLE_VALUE);
    check("found size", d.nFileSizeLow);
    check("found name", fnv((const u8 *)d.cFileName, (u32)strlen(d.cFileName)));
    check("found more", FindNextFileA(h, &d));
    FindClose(h);
    check("deleted", DeleteFileA(g_file));
    check("gone", GetFileAttributesA(g_file) == INVALID_FILE_ATTRIBUTES);
}

/* ---- virtual memory: MEMORY_BASIC_INFORMATION, protections --------------------------------- */

NOINLINE static void virtual_memory(void)
{
    MEMORY_BASIC_INFORMATION m;
    DWORD old = 0;
    u8 *p = (u8 *)VirtualAlloc(NULL, 0x3000, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    check("allocated", p != NULL);
    p[0x1000 + seed()] = 0x5a;
    check("query", (u32)VirtualQuery(p + 0x1800, &m, sizeof m) == sizeof m);
    check("query base", (u32)((u8 *)m.BaseAddress - p));
    check("query alloc base", m.AllocationBase == p);
    check("query size", (u32)m.RegionSize);
    check("query state", m.State);
    check("query protect", m.Protect);
    check("query type", m.Type);
    check("protect", VirtualProtect(p + 0x1000, 0x1000, PAGE_READONLY, &old));
    check("protect old", old);
    VirtualQuery(p, &m, sizeof m);
    check("split size", (u32)m.RegionSize);
    VirtualQuery(p + 0x1000, &m, sizeof m);
    check("split protect", m.Protect);
    check("byte kept", p[0x1007]);
    check("freed", VirtualFree(p, 0, MEM_RELEASE));
}

NOINLINE static void system_info(void)
{
    SYSTEM_INFO s;
    GetSystemInfo(&s);
    /* (what an x86 process sees: 4 KB pages, 64 KB granularity, its 2 GB, Intel) */
    check("page size", s.dwPageSize);
    check("granularity", s.dwAllocationGranularity);
    check("min address", (u32)(UINT_PTR)s.lpMinimumApplicationAddress);
    check("max address", (u32)(UINT_PTR)s.lpMaximumApplicationAddress);
    check("architecture", s.wProcessorArchitecture);
    check("processors", s.dwNumberOfProcessors > 0);
}

/* ---- a private heap ----------------------------------------------------------------------- */

NOINLINE static void private_heap(void)
{
    HANDLE h = HeapCreate(0, 0x10000, 0);
    u32 *a, *b, i, sum = 0;
    check("heap made", h != NULL);
    a = (u32 *)HeapAlloc(h, HEAP_ZERO_MEMORY, 40);
    check("heap zeroed", a[9] == 0);
    for (i = 0; i < 10; i++) a[i] = i * i + seed();
    check("heap size", (u32)HeapSize(h, 0, a));
    a = (u32 *)HeapReAlloc(h, 0, a, 400);
    check("heap realloc size", (u32)HeapSize(h, 0, a));
    for (i = 0; i < 10; i++) sum += a[i];
    check("heap kept", sum);
    b = (u32 *)HeapAlloc(h, 0, 8);
    check("heap valid", HeapValidate(h, 0, NULL));
    check("heap free a", HeapFree(h, 0, a));
    check("heap free b", HeapFree(h, 0, b));
    check("heap destroyed", HeapDestroy(h));
}

/* ---- the interlocked singly linked list: 8-byte entries on x86, 16 on x64 ------------------- */

typedef struct {
    SLIST_ENTRY link;          /* (first: the entry is the item) */
    u32 value;
} Item;

NOINLINE static void slist(void)
{
    SLIST_HEADER *head = (SLIST_HEADER *)_aligned_malloc(sizeof(SLIST_HEADER), MEMORY_ALLOCATION_ALIGNMENT);
    Item *items = (Item *)_aligned_malloc(sizeof(Item) * 6, MEMORY_ALLOCATION_ALIGNMENT);
    SLIST_ENTRY *e;
    u32 i, order = 0;
    InitializeSListHead(head);
    for (i = 0; i < 6; i++) {
        items[i].value = i + 1 + seed();
        InterlockedPushEntrySList(head, &items[i].link);
    }
    check("slist depth", QueryDepthSList(head));
    e = InterlockedPopEntrySList(head);
    order = ((Item *)e)->value;
    while ((e = InterlockedPopEntrySList(head)) != NULL) order = order * 10 + ((Item *)e)->value;
    check("slist order", order);
    check("slist empty", QueryDepthSList(head));
    _aligned_free(items);
    _aligned_free(head);
}

/* ---- one-time initialization, a callback with a context ----------------------------------- */

static INIT_ONCE g_once = INIT_ONCE_STATIC_INIT;
static u32 g_once_runs;

NOINLINE static BOOL CALLBACK once_fn(PINIT_ONCE once, PVOID param, PVOID *context)
{
    (void)once;
    g_once_runs++;
    /* (its low INIT_ONCE_CTX_RESERVED_BITS clear: Windows keeps its state there) */
    *context = (PVOID)(UINT_PTR)((*(u32 *)param * 2 + 1) << INIT_ONCE_CTX_RESERVED_BITS);
    return TRUE;
}

NOINLINE static void init_once(void)
{
    u32 param = 20 + seed();
    PVOID ctx = NULL;
    u32 i;
    for (i = 0; i < 3; i++) InitOnceExecuteOnce(&g_once, once_fn, &param, &ctx);
    check("once runs", g_once_runs);
    check("once context", (u32)(UINT_PTR)ctx);
}

/* ---- the system's threads calling in: a timer queue, a work item -------------------------- */

static HANDLE g_fired;
static volatile long g_timer_hits, g_work_sum;

NOINLINE static void CALLBACK on_timer(PVOID param, BOOLEAN timed_out)
{
    InterlockedExchangeAdd(&g_timer_hits, (long)(UINT_PTR)param + (timed_out ? 100 : 0));
    SetEvent(g_fired);
}

NOINLINE static DWORD WINAPI on_work(LPVOID param)
{
    InterlockedExchangeAdd(&g_work_sum, (long)(UINT_PTR)param);
    SetEvent(g_fired);
    return 0;
}

NOINLINE static void system_threads(void)
{
    HANDLE timer = NULL;
    g_fired = CreateEventA(NULL, FALSE, FALSE, NULL);
    check("timer made", CreateTimerQueueTimer(&timer, NULL, on_timer, (PVOID)(UINT_PTR)(5 + seed()), 10, 0,
                                              WT_EXECUTEONLYONCE));
    WaitForSingleObject(g_fired, INFINITE);
    DeleteTimerQueueTimer(NULL, timer, INVALID_HANDLE_VALUE);
    check("timer hits", (u32)g_timer_hits);
    check("work queued", QueueUserWorkItem(on_work, (PVOID)(UINT_PTR)(41 + seed()), WT_EXECUTEDEFAULT));
    WaitForSingleObject(g_fired, INFINITE);
    check("work sum", (u32)g_work_sum);
    CloseHandle(g_fired);
}

/* ---- a TIMERPROC from the thread's own message loop ---------------------------------------- */

static u32 g_ticks;
static UINT_PTR g_timer_id;

NOINLINE static void CALLBACK on_tick(HWND hwnd, UINT msg, UINT_PTR id, DWORD time)
{
    (void)time;
    g_ticks++;
    if (hwnd == NULL && msg == WM_TIMER && id == g_timer_id && g_ticks >= 3) KillTimer(NULL, id);
}

NOINLINE static void timer_proc(void)
{
    MSG m;
    g_timer_id = SetTimer(NULL, 0, 10, on_tick);
    check("timer set", g_timer_id != 0);
    while (g_ticks < 3 && GetMessageA(&m, NULL, 0, 0) > 0) DispatchMessageA(&m);
    check("ticks", g_ticks);
}

/* ---- FLS: a callback at the thread's exit ------------------------------------------------- */

static volatile long g_fls_freed;

NOINLINE static void WINAPI fls_free(PVOID data)
{
    if (data) InterlockedExchangeAdd(&g_fls_freed, (long)*(u32 *)data);
}

static DWORD g_fls;

NOINLINE static DWORD WINAPI fls_thread(LPVOID param)
{
    FlsSetValue(g_fls, param);
    return 0;
}

NOINLINE static void fls(void)
{
    u32 v1 = 7 + seed(), v2 = 30;
    HANDLE t;
    g_fls = FlsAlloc(fls_free);
    check("fls index", g_fls != FLS_OUT_OF_INDEXES);
    t = CreateThread(NULL, 0, fls_thread, &v1, 0, NULL);
    WaitForSingleObject(t, INFINITE);
    CloseHandle(t);
    t = CreateThread(NULL, 0, fls_thread, &v2, 0, NULL);
    WaitForSingleObject(t, INFINITE);
    CloseHandle(t);
    check("fls freed", (u32)g_fls_freed);
    FlsFree(g_fls);
}

/* ---- functions found at run time, called through their pointers --------------------------- */

typedef int (WINAPI *lstrlen_fn)(LPCSTR);
typedef DWORD (WINAPI *pid_fn)(void);
typedef BOOL (WINAPI *env_fn)(LPCSTR, LPCSTR);
typedef DWORD (WINAPI *getenv_fn)(LPCSTR, LPSTR, DWORD);

NOINLINE static void proc_addresses(void)
{
    HMODULE k = GetModuleHandleA("kernel32.dll");
    lstrlen_fn len = (lstrlen_fn)GetProcAddress(k, "lstrlenA");
    pid_fn pid = (pid_fn)GetProcAddress(k, "GetCurrentProcessId");
    env_fn set = (env_fn)GetProcAddress(k, "SetEnvironmentVariableA");
    getenv_fn get = (getenv_fn)GetProcAddress(k, "GetEnvironmentVariableA");
    char buf[32];
    check("found lstrlenA", len != NULL);
    check("lstrlenA", (u32)len("rhtest sys") + seed());
    check("pid same", pid() == GetCurrentProcessId());
    check("env set", set("RHTEST_SYS", "seven"));
    check("env get", get("RHTEST_SYS", buf, sizeof buf));
    check("env value", fnv((const u8 *)buf, (u32)strlen(buf)));
    check("missing proc", GetProcAddress(k, "NoSuchFunctionAtAll") == NULL);
}

void rhtest_run(void)
{
    overlapped_file();
    find_file();
    virtual_memory();
    system_info();
    private_heap();
    slist();
    init_once();
    system_threads();
    timer_proc();
    fls();
    proc_addresses();
}
