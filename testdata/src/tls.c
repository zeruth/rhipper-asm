// Thread-local storage without a C runtime. The program defines what tlssup.obj would: the index
// the loader writes, the bounds of the template (`.tls` sorts before the variables' `.tls$`,
// `.tls$ZZZ` after them) and the directory the linker names in the headers, with one callback that
// counts how the loader calls it. Two threads, one after the other, step a thread-local counter
// from its initial value; each prints what it reached, and the main thread's own count stays.
#include <windows.h>

#define NOINLINE __attribute__((noinline))

static HANDLE out;

__declspec(thread) int counter = 100;
static int seen[2];
static int attached_process, attached_thread;

static void NTAPI on_tls(PVOID handle, DWORD reason, PVOID reserved) {
    (void)handle;
    (void)reserved;
    if (reason == DLL_PROCESS_ATTACH) attached_process++;
    if (reason == DLL_THREAD_ATTACH) attached_thread++;
}

unsigned long _tls_index = 0;
__attribute__((section(".tls"))) char _tls_start = 0;
__attribute__((section(".tls$ZZZ"))) char _tls_end = 0;
static const PIMAGE_TLS_CALLBACK tls_callbacks[] = {on_tls, 0};
__attribute__((section(".rdata$T"))) const IMAGE_TLS_DIRECTORY _tls_used = {
    (ULONG_PTR)&_tls_start, (ULONG_PTR)&_tls_end, (ULONG_PTR)&_tls_index, (ULONG_PTR)tls_callbacks, 0, 0};
#if defined(__i386__)
// The x86 code reaches the thread-local array through fs:[__tls_array], an absolute symbol.
__asm__(".globl __tls_array\n__tls_array = 0x2c");
#endif

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

static DWORD WINAPI worker(LPVOID arg) {
    int k = (int)(UINT_PTR)arg;
    for (int i = 1; i <= 3 * k; i++) counter += i;
    seen[k - 1] = counter;
    return 0;
}

NOINLINE static void run(int k) {
    HANDLE t = CreateThread(NULL, 0, worker, (LPVOID)(UINT_PTR)k, 0, NULL);
    WaitForSingleObject(t, INFINITE);
    CloseHandle(t);
}

int mainCRTStartup(void) {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    counter += 5;
    run(1);
    run(2);
    print_str("main ");
    print_int(counter);
    print_str("thread 1 ");
    print_int(seen[0]);
    print_str("thread 2 ");
    print_int(seen[1]);
    print_str("attach process ");
    print_int(attached_process);
    print_str("attach thread ");
    print_int(attached_thread);
    ExitProcess(0);
    return 0;
}
