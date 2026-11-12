// The host of a lifted program. It maps the image at its own base, binds the import slots to the
// real functions, and calls the lifted entry point.
//
//   clang -O1 program.ll driver.c -o lifted.exe      (image.h beside program.ll)
//   lifted.exe image.bin
//
// Two modes, by image.h. Without RA_ABI (the machine IR), every lifted function takes a machine
// state: the driver gives it a stack and the runtime resolves indirect calls through the function
// table. With RA_ABI (the raised IR), every lifted function has its own parameters and result: the
// driver writes a trampoline at each function's image address, so a pointer into the image, held by
// lifted or native code, reaches the host function. With RA_DLL the port is a DLL and the driver
// is its DllMain rather than a main.
//
// The runtime functions the lifted code calls: ra_trap, ra_unsupported and ra_call.
//
// The host is Windows or a POSIX system (Linux, macOS). A Windows program's port needs Windows:
// its imports are the DLLs', and the adapters of the Microsoft C runtime, user32, the lockstep pipe
// and the crash report through a vectored handler are Windows' alone. A POSIX host builds the port
// of a 64-bit program whose imports its own libraries answer, a Linux program's, with the same
// machine model: the image mapped by mmap, the slots bound by dlsym, a crash reported by a signal.
#include <fenv.h>
#include <float.h>
#include <math.h>
#include <setjmp.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#include <intrin.h>
#include <windows.h>
#else
#include <dlfcn.h>
#include <signal.h>
#include <sys/mman.h>
#include <unistd.h>
#if defined(__x86_64__)
#include <cpuid.h>
#include <x86intrin.h>
#endif
#endif

#include "image.h"

#ifdef _WIN32
// The window adapters below call user32 whether or not the program does: the library comes with
// the driver, so a link that names only kernel32 still resolves them.
#pragma comment(lib, "user32")
#define host_exit(code) ExitProcess(code)
#else
#if RA_POINTER_SIZE == 4
#error "a 32-bit program keeps its pointers below 4 GB through a Windows linker flag: its port builds on a Windows host"
#endif
#ifdef RA_DLL
#error "the port of a DLL is a Windows DLL: it builds on a Windows host"
#endif
// The reasons a Windows loader passes the TLS callbacks; a POSIX image has none to call.
enum { DLL_PROCESS_ATTACH = 1, DLL_THREAD_ATTACH = 2 };
// The status keeps its low byte, the one a POSIX parent sees.
#define host_exit(code) _Exit((int)((code) & 0xff))
#endif

// The machine state, as %State in the IR: 16 general registers, rip, 8 flag bytes, 16 vectors.
struct State {
    uint64_t gpr[16];
    uint64_t rip;
    uint8_t flags[8];
    _Alignas(16) uint8_t xmm[16][16];
};

enum { RAX, RCX, RDX, RBX, RSP, RBP, RSI, RDI, R8, R9 };

// The lifted functions, by address: the lift writes the table.
extern const struct { uint64_t address; void (*function)(void *); } ra_functions[];
extern const uint64_t ra_function_count;

static const char *trap_name(uint64_t code) {
    switch (code) {
    case 1: return "int3";
    case 2: return "ud2";
    case 3: return "hlt";
    case 4: return "a divide error (a zero divisor or a quotient too large)";
    case 5: return "128-bit division";
    case 6: return "__fastfail";
    case 7: return "a switch to an address outside its table";
    case 8: return "bytes that are no instruction";
    case 9: return "a software interrupt";
    case 10: return "a call through a vtable slot to a function outside the vtables";
    default: return "a trap";
    }
}

_Noreturn void ra_trap(uint64_t address, uint64_t code) {
    fprintf(stderr, "ra: %s at 0x%llx\n", trap_name(code), (unsigned long long)address);
    fflush(stderr);
    host_exit(0xE0000001);
}

// The entry hook of a port built with RA_TRACE: each function names itself as it starts.
void ra_enter(const char *name) {
    fputs("> ", stderr);
    fputs(name, stderr);
    fputs("\n", stderr);
    fflush(stderr);
}

// The trace of a port emitted with tracing: `> name(args)` at entry, `< name = value:kind` at a
// return (`:p` a pointer, `:f64` and `:f32` the bits of a floating result, `:N` an integer of N
// bits), `< name` at a return of no recordable value. Each line is flushed as it is written, so a
// crash keeps the trace up to it.
static FILE *trace_file(void) {
    static FILE *file;
    if (!file) {
#ifdef _WIN32
        char path[MAX_PATH];
        DWORD length = GetEnvironmentVariableA("RHIPPER_TRACE", path, sizeof path);
        if (length > 0 && length < sizeof path && fopen_s(&file, path, "w") != 0) file = NULL;
#else
        const char *path = getenv("RHIPPER_TRACE");
        if (path && *path) file = fopen(path, "w");
#endif
        if (!file) file = stderr;
    }
    return file;
}

uint64_t ra_f64_bits(double value) {
    uint64_t bits;
    memcpy(&bits, &value, sizeof bits);
    return bits;
}

uint64_t ra_f32_bits(float value) {
    uint32_t bits;
    memcpy(&bits, &value, sizeof bits);
    return bits;
}

// Lockstep: with RHIPPER_STEP naming a pipe, the port serves the pipe and stops at each traced
// entry, writing `> name kinds value...` (`-` for no arguments, the values in hexadecimal) and
// reading commands until one lets it go on: `n` to the next entry, `r address size` for the bytes
// there as one hexadecimal line (`!` and the error when they cannot be read), `q` to run on
// without stopping. A closed pipe lets the port run on as `q` does. The pipe is a Windows named
// pipe, which `ras` connects to on a Windows host: a POSIX port runs without stopping.
#ifdef _WIN32
static HANDLE step_pipe = INVALID_HANDLE_VALUE;

static int step_open(void) {
    static int checked;
    if (!checked) {
        checked = 1;
        char name[MAX_PATH];
        DWORD length = GetEnvironmentVariableA("RHIPPER_STEP", name, sizeof name);
        if (length == 0 || length >= sizeof name) return 0;
        HANDLE pipe = CreateNamedPipeA(name, PIPE_ACCESS_DUPLEX, PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT, 1, 65536, 65536, 0, NULL);
        if (pipe == INVALID_HANDLE_VALUE) return 0;
        if (!ConnectNamedPipe(pipe, NULL) && GetLastError() != ERROR_PIPE_CONNECTED) {
            CloseHandle(pipe);
            return 0;
        }
        step_pipe = pipe;
    }
    return step_pipe != INVALID_HANDLE_VALUE;
}

static void step_close(void) {
    CloseHandle(step_pipe);
    step_pipe = INVALID_HANDLE_VALUE;
}

static int step_write(const char *text, size_t length) {
    while (length > 0) {
        DWORD written;
        if (!WriteFile(step_pipe, text, (DWORD)length, &written, NULL)) return 0;
        text += written;
        length -= written;
    }
    return 1;
}

static int step_read_line(char *line, size_t size) {
    size_t at = 0;
    for (;;) {
        char c;
        DWORD read;
        if (!ReadFile(step_pipe, &c, 1, &read, NULL) || read == 0) return 0;
        if (c == '\n') break;
        if (c != '\r' && at + 1 < size) line[at++] = c;
    }
    line[at] = 0;
    return 1;
}

static void step_stop(const char *name, const char *kinds, const uint64_t *values, size_t count) {
    char line[8192];
    int at = snprintf(line, sizeof line, "> %s %s", name, *kinds ? kinds : "-");
    for (size_t i = 0; i < count && at < (int)sizeof line - 24; i++) at += snprintf(line + at, sizeof line - at, " 0x%llx", (unsigned long long)values[i]);
    at += snprintf(line + at, sizeof line - at, "\n");
    if (!step_write(line, (size_t)at)) {
        step_close();
        return;
    }
    for (;;) {
        char command[256];
        if (!step_read_line(command, sizeof command)) {
            step_close();
            return;
        }
        if (command[0] == 'n') return;
        if (command[0] == 'q') {
            step_close();
            return;
        }
        if (command[0] == 'r') {
            unsigned long long address = 0, size = 0;
            static uint8_t bytes[4096];
            static char reply[2 * sizeof bytes + 2];
            SIZE_T got = 0;
            int ok = sscanf_s(command + 1, "%llx %llx", &address, &size) == 2 && size <= sizeof bytes;
            // A read of the port's own memory through the process API fails on an unmapped page
            // rather than faulting.
            if (ok) ok = ReadProcessMemory(GetCurrentProcess(), (const void *)(uintptr_t)address, bytes, (SIZE_T)size, &got) && got == size;
            int length;
            if (ok) {
                for (SIZE_T i = 0; i < got; i++) snprintf(reply + 2 * i, 3, "%02x", bytes[i]);
                reply[2 * got] = '\n';
                length = (int)(2 * got + 1);
            } else {
                length = snprintf(reply, sizeof reply, "!cannot read 0x%llx bytes at 0x%llx: error %lu\n", size, address, GetLastError());
            }
            if (!step_write(reply, (size_t)length)) {
                step_close();
                return;
            }
        }
    }
}
#else
static int step_open(void) {
    return 0;
}

static void step_stop(const char *name, const char *kinds, const uint64_t *values, size_t count) {
    (void)name;
    (void)kinds;
    (void)values;
    (void)count;
}
#endif

void ra_trace_enter(const char *name, const char *kinds, ...) {
    FILE *file = trace_file();
    uint64_t values[64];
    size_t count = 0;
    va_list args;
    va_start(args, kinds);
    fprintf(file, "> %s(", name);
    for (const char *k = kinds; *k; k++) {
        uint64_t value = va_arg(args, uint64_t);
        if (count < sizeof values / sizeof values[0]) values[count++] = value;
        if (k != kinds) fputs(", ", file);
        switch (*k) {
        case 'f': case 'g': fprintf(file, "f:0x%llx", (unsigned long long)value); break;
        case '?': fputs("?", file); break;
        default: fprintf(file, "0x%llx", (unsigned long long)value); break;
        }
    }
    va_end(args);
    fputs(")\n", file);
    fflush(file);
    if (step_open()) step_stop(name, kinds, values, count);
}

uint64_t ra_trace_ret(const char *name, uint64_t value, unsigned bits) {
    FILE *file = trace_file();
    fprintf(file, "< %s = 0x%llx:%u\n", name, (unsigned long long)value, bits);
    fflush(file);
    return value;
}

uint64_t ra_trace_ret_ptr(const char *name, uint64_t value) {
    FILE *file = trace_file();
    fprintf(file, "< %s = 0x%llx:p\n", name, (unsigned long long)value);
    fflush(file);
    return value;
}

double ra_trace_ret_f64(const char *name, double value) {
    FILE *file = trace_file();
    fprintf(file, "< %s = f:0x%llx:f64\n", name, (unsigned long long)ra_f64_bits(value));
    fflush(file);
    return value;
}

float ra_trace_ret_f32(const char *name, float value) {
    FILE *file = trace_file();
    fprintf(file, "< %s = f:0x%llx:f32\n", name, (unsigned long long)ra_f32_bits(value));
    fflush(file);
    return value;
}

void ra_trace_leave(const char *name) {
    FILE *file = trace_file();
    fprintf(file, "< %s\n", name);
    fflush(file);
}

_Noreturn void ra_unsupported(uint64_t address, uint64_t code) {
    fprintf(stderr, "ra: an instruction the lift does not cover at 0x%llx (code %llu)\n", (unsigned long long)address, (unsigned long long)code);
    fflush(stderr);
    host_exit(0xE0000002);
}

// The processor's answers, from the host: the port runs on a processor at least as capable as
// the one the program asks about. A host of another architecture has no answer to give, and the
// port stops rather than invent one.
#if defined(_WIN32)
void ra_cpuid(uint64_t leaf, uint64_t subleaf, void *out) {
    int regs[4];
    __cpuidex(regs, (int)leaf, (int)subleaf);
    memcpy(out, regs, sizeof regs);
}

uint64_t ra_xgetbv(uint64_t which) {
    return _xgetbv((unsigned)which);
}

uint64_t ra_rdtsc(void) {
    return __rdtsc();
}
#elif defined(__x86_64__)
void ra_cpuid(uint64_t leaf, uint64_t subleaf, void *out) {
    unsigned regs[4];
    __cpuid_count((unsigned)leaf, (unsigned)subleaf, regs[0], regs[1], regs[2], regs[3]);
    memcpy(out, regs, sizeof regs);
}

uint64_t ra_xgetbv(uint64_t which) {
    uint32_t low, high;
    __asm__ volatile("xgetbv" : "=a"(low), "=d"(high) : "c"((uint32_t)which));
    return ((uint64_t)high << 32) | low;
}

uint64_t ra_rdtsc(void) {
    return __rdtsc();
}
#else
_Noreturn static void no_x86_answer(const char *what) {
    fprintf(stderr, "ra: %s asks an x86 processor, and the host is another\n", what);
    fflush(stderr);
    host_exit(0xE0000002);
}

void ra_cpuid(uint64_t leaf, uint64_t subleaf, void *out) {
    (void)leaf;
    (void)subleaf;
    (void)out;
    no_x86_answer("cpuid");
}

uint64_t ra_xgetbv(uint64_t which) {
    (void)which;
    no_x86_answer("xgetbv");
}

uint64_t ra_rdtsc(void) {
    no_x86_answer("rdtsc");
}
#endif

typedef uint64_t (*native8)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);

// The C runtime functions a 32-bit program calls that walk or hand back a table of its own
// pointers. A pointer the program holds is a dword, and a function it names is its universal
// entry point, which takes eight integers and ignores the ones it does not need; the host's own
// versions would read the tables with 64-bit strides.
static uint64_t call_universal(uint32_t function) {
    native8 f = (native8)(uintptr_t)function;
    return f(0, 0, 0, 0, 0, 0, 0, 0);
}

uint64_t ra_x86_initterm(uint64_t first, uint64_t last) {
    for (uint32_t *p = (uint32_t *)(uintptr_t)first; (uintptr_t)p < (uintptr_t)last; p++) {
        if (*p) call_universal(*p);
    }
    return 0;
}

uint64_t ra_x86_initterm_e(uint64_t first, uint64_t last) {
    for (uint32_t *p = (uint32_t *)(uintptr_t)first; (uintptr_t)p < (uintptr_t)last; p++) {
        if (*p) {
            uint32_t result = (uint32_t)call_universal(*p);
            if (result) return result;
        }
    }
    return 0;
}

// The onexit tables: the program's structure of three dwords names a table; its functions live
// here, by the table's address.
struct onexit_table { uint32_t address; uint32_t count; uint32_t functions[256]; };
static struct onexit_table onexit_tables[8];

static struct onexit_table *onexit_table_of(uint64_t table, int create) {
    for (int i = 0; i < 8; i++) {
        if (onexit_tables[i].address == (uint32_t)table) return &onexit_tables[i];
    }
    if (!create) return NULL;
    for (int i = 0; i < 8; i++) {
        if (onexit_tables[i].address == 0) {
            onexit_tables[i].address = (uint32_t)table;
            onexit_tables[i].count = 0;
            return &onexit_tables[i];
        }
    }
    return NULL;
}

uint64_t ra_x86_initialize_onexit_table(uint64_t table) {
    if (!table) return (uint64_t)-1;
    memset((void *)(uintptr_t)table, 0, 12);
    struct onexit_table *t = onexit_table_of(table, 1);
    if (t) t->count = 0;
    return 0;
}

uint64_t ra_x86_register_onexit_function(uint64_t table, uint64_t function) {
    struct onexit_table *t = onexit_table_of(table, 1);
    if (!t || t->count >= 256) return (uint64_t)-1;
    t->functions[t->count++] = (uint32_t)function;
    return 0;
}

uint64_t ra_x86_execute_onexit_table(uint64_t table) {
    struct onexit_table *t = onexit_table_of(table, 0);
    if (!t) return 0;
    while (t->count > 0) {
        uint32_t function = t->functions[--t->count];
        if (function) call_universal(function);
    }
    return 0;
}

// The program's singly linked list head is 8 bytes, the host's 16, and the host's own function
// refuses a head that is not 16-byte aligned: the empty list is zeros.
uint64_t ra_x86_initialize_slist_head(uint64_t head) {
    memset((void *)(uintptr_t)head, 0, 8);
    return 0;
}

// `operator new` never returns null and gives a distinct block for a size of zero; the bad_alloc
// it would throw ends the program, as an unhandled one does.
uint64_t ra_x86_operator_new(uint64_t size) {
    void *block = malloc(size ? (size_t)size : 1);
    if (!block) abort();
    return (uint64_t)(uintptr_t)block;
}

uint64_t ra_x86_operator_delete(uint64_t block) {
    free((void *)(uintptr_t)block);
    return 0;
}

// The structures a window program hands user32 are laid out at the program's pointer width:
// WNDCLASSA is 40 bytes on x86 and 72 on the host, WNDCLASSEXA 48 and 80, MSG 28 and 48,
// PAINTSTRUCT 64 and 72. Each adapter widens the program's record into the host's, calls, and
// narrows what came back. The window procedure the class names is a universal entry point, which
// user32 calls with the four arguments in registers, where the stub reads its first four dwords.
// The narrowing of a handle or an address to a dword (the message's lParam, which carries the
// CREATESTRUCT of WM_CREATE or the WINDOWPOS of a move) rests on the port's address space: a
// 32-bit port is linked without large-address awareness, so every address its process allocates,
// user32's stacks and heaps included, lies below 2 GB, and a window or GDI handle is 32 bits
// significant on every Windows; only a pointer into a system DLL's own image would not fit, and no
// message carries one.
#ifdef _WIN32
struct ra_wndclass32 {
    uint32_t style, proc;
    int32_t cls_extra, wnd_extra;
    uint32_t instance, icon, cursor, background, menu, name;
};
struct ra_wndclassex32 {
    uint32_t size, style, proc;
    int32_t cls_extra, wnd_extra;
    uint32_t instance, icon, cursor, background, menu, name, icon_small;
};
struct ra_msg32 {
    uint32_t hwnd, message, wparam;
    int32_t lparam;
    uint32_t time;
    int32_t x, y;
};
struct ra_paintstruct32 {
    uint32_t hdc;
    int32_t erase;
    RECT paint;
    int32_t restore, inc_update;
    uint8_t reserved[32];
};

static void ra_widen_msg(uint64_t narrow, MSG *m) {
    const struct ra_msg32 *n = (const struct ra_msg32 *)(uintptr_t)narrow;
    memset(m, 0, sizeof *m);
    m->hwnd = (HWND)(uintptr_t)n->hwnd;
    m->message = n->message;
    m->wParam = n->wparam;
    m->lParam = n->lparam;
    m->time = n->time;
    m->pt.x = n->x;
    m->pt.y = n->y;
}

static void ra_narrow_msg(const MSG *m, uint64_t narrow) {
    struct ra_msg32 *n = (struct ra_msg32 *)(uintptr_t)narrow;
    n->hwnd = (uint32_t)(uintptr_t)m->hwnd;
    n->message = m->message;
    n->wparam = (uint32_t)m->wParam;
    n->lparam = (int32_t)m->lParam;
    n->time = m->time;
    n->x = m->pt.x;
    n->y = m->pt.y;
}

uint64_t ra_x86_register_class_a(uint64_t narrow) {
    const struct ra_wndclass32 *n = (const struct ra_wndclass32 *)(uintptr_t)narrow;
    WNDCLASSA wc = {n->style,
                    (WNDPROC)(uintptr_t)n->proc,
                    n->cls_extra,
                    n->wnd_extra,
                    (HINSTANCE)(uintptr_t)n->instance,
                    (HICON)(uintptr_t)n->icon,
                    (HCURSOR)(uintptr_t)n->cursor,
                    (HBRUSH)(uintptr_t)n->background,
                    (LPCSTR)(uintptr_t)n->menu,
                    (LPCSTR)(uintptr_t)n->name};
    return RegisterClassA(&wc);
}

uint64_t ra_x86_register_class_ex_a(uint64_t narrow) {
    const struct ra_wndclassex32 *n = (const struct ra_wndclassex32 *)(uintptr_t)narrow;
    if (n->size != sizeof *n) return 0;
    WNDCLASSEXA wc = {sizeof wc,
                      n->style,
                      (WNDPROC)(uintptr_t)n->proc,
                      n->cls_extra,
                      n->wnd_extra,
                      (HINSTANCE)(uintptr_t)n->instance,
                      (HICON)(uintptr_t)n->icon,
                      (HCURSOR)(uintptr_t)n->cursor,
                      (HBRUSH)(uintptr_t)n->background,
                      (LPCSTR)(uintptr_t)n->menu,
                      (LPCSTR)(uintptr_t)n->name,
                      (HICON)(uintptr_t)n->icon_small};
    return RegisterClassExA(&wc);
}

uint64_t ra_x86_get_message_a(uint64_t narrow, uint64_t hwnd, uint64_t first, uint64_t last) {
    MSG m;
    BOOL r = GetMessageA(&m, (HWND)(uintptr_t)hwnd, (UINT)first, (UINT)last);
    ra_narrow_msg(&m, narrow);
    return (uint64_t)(uint32_t)r;
}

uint64_t ra_x86_peek_message_a(uint64_t narrow, uint64_t hwnd, uint64_t first, uint64_t last, uint64_t remove) {
    MSG m;
    BOOL r = PeekMessageA(&m, (HWND)(uintptr_t)hwnd, (UINT)first, (UINT)last, (UINT)remove);
    if (r) ra_narrow_msg(&m, narrow);
    return (uint64_t)(uint32_t)r;
}

uint64_t ra_x86_translate_message(uint64_t narrow) {
    MSG m;
    ra_widen_msg(narrow, &m);
    return (uint64_t)(uint32_t)TranslateMessage(&m);
}

uint64_t ra_x86_dispatch_message_a(uint64_t narrow) {
    MSG m;
    ra_widen_msg(narrow, &m);
    return (uint64_t)(uint32_t)DispatchMessageA(&m);
}

uint64_t ra_x86_begin_paint(uint64_t hwnd, uint64_t narrow) {
    struct ra_paintstruct32 *n = (struct ra_paintstruct32 *)(uintptr_t)narrow;
    PAINTSTRUCT ps;
    HDC dc = BeginPaint((HWND)(uintptr_t)hwnd, &ps);
    n->hdc = (uint32_t)(uintptr_t)ps.hdc;
    n->erase = ps.fErase;
    n->paint = ps.rcPaint;
    n->restore = ps.fRestore;
    n->inc_update = ps.fIncUpdate;
    memcpy(n->reserved, ps.rgbReserved, sizeof n->reserved);
    return (uint64_t)(uint32_t)(uintptr_t)dc;
}

uint64_t ra_x86_end_paint(uint64_t hwnd, uint64_t narrow) {
    const struct ra_paintstruct32 *n = (const struct ra_paintstruct32 *)(uintptr_t)narrow;
    PAINTSTRUCT ps;
    ps.hdc = (HDC)(uintptr_t)n->hdc;
    ps.fErase = n->erase;
    ps.rcPaint = n->paint;
    ps.fRestore = n->restore;
    ps.fIncUpdate = n->inc_update;
    memcpy(ps.rgbReserved, n->reserved, sizeof ps.rgbReserved);
    return (uint64_t)(uint32_t)EndPaint((HWND)(uintptr_t)hwnd, &ps);
}
#endif

// The floating-point environment of an x86 program is the x87 control and status words as much as
// the SSE register: _controlfp, _clearfp and the C99 fenv functions read and write both. The port
// keeps the lifted words (the exact x87 unit rounds by the control word and raises its flags in the
// status word; the double unit and the host's own arithmetic use the host's) and the host's in
// step: a mode goes to both, a report merges both. The host's word has no precision control, so
// that field is the lifted word's alone.
extern uint16_t ra_x87_cw;
extern uint16_t ra_x87_sw;

enum { RA_MCW_EM = 0x8001f, RA_MCW_RC = 0x300, RA_MCW_PC = 0x30000, RA_MCW_IC = 0x40000 };

// The exception bits of the C runtime's word (_EM_INEXACT 1, _EM_UNDERFLOW 2, _EM_OVERFLOW 4,
// _EM_ZERODIVIDE 8, _EM_INVALID 0x10, _EM_DENORMAL 0x80000) from the x87's order (invalid, denormal,
// zero divide, overflow, underflow, precision in bits 0 to 5), and back.
static unsigned ra_crt_flags(unsigned x87) {
    return ((x87 >> 5) & 1) | ((x87 >> 3) & 2) | ((x87 >> 1) & 4) | ((x87 << 1) & 8) | ((x87 << 4) & 0x10) | ((x87 & 2) ? 0x80000u : 0);
}

static unsigned ra_x87_flags(unsigned crt) {
    return ((crt >> 4) & 1) | ((crt >> 18) & 2) | ((crt >> 1) & 4) | ((crt << 1) & 8) | ((crt << 3) & 0x10) | ((crt << 5) & 0x20);
}

// The C runtime's control word from the x87's: the masks, the rounding mode (bits 10 and 11 to
// bits 8 and 9) and the precision (24 bits is _PC_24 0x20000, 53 _PC_53 0x10000, 64 _PC_64 0).
// The words are the Microsoft runtime's, which a Windows host answers.
#ifdef _WIN32
static unsigned ra_crt_control(unsigned cw) {
    unsigned pc = (cw >> 8) & 3;
    return ra_crt_flags(cw & 0x3f) | ((cw >> 2) & RA_MCW_RC) | (pc == 0 ? 0x20000u : pc == 2 ? 0x10000u : 0);
}

static uint16_t ra_x87_control(unsigned crt, uint16_t cw) {
    unsigned pc = (crt & RA_MCW_PC) == 0x20000 ? 0 : (crt & RA_MCW_PC) == 0x10000 ? 2 : 3;
    return (uint16_t)((cw & 0xf0c0) | ra_x87_flags(crt & RA_MCW_EM) | ((crt & RA_MCW_RC) << 2) | (pc << 8));
}

uint64_t ra_x86_controlfp(uint64_t new_bits, uint64_t mask) {
    unsigned host = 0;
    _controlfp_s(&host, (unsigned)new_bits & ~(unsigned)(RA_MCW_PC | RA_MCW_IC), (unsigned)mask & ~(unsigned)(RA_MCW_PC | RA_MCW_IC));
    unsigned x87 = (ra_crt_control(ra_x87_cw) & ~(unsigned)mask) | ((unsigned)new_bits & (unsigned)mask);
    ra_x87_cw = ra_x87_control(x87, ra_x87_cw);
    unsigned own = RA_MCW_EM | RA_MCW_RC | RA_MCW_PC;
    return (host & ~own) | (x87 & own);
}

uint64_t ra_x86_controlfp_s(uint64_t current, uint64_t new_bits, uint64_t mask) {
    unsigned word = (unsigned)ra_x86_controlfp(new_bits, mask);
    if (current) memcpy((void *)(uintptr_t)current, &word, 4);
    return 0;
}

uint64_t ra_x86_fesetround(uint64_t mode) {
    if (mode & ~(uint64_t)RA_MCW_RC) return 1;
    ra_x86_controlfp(mode, RA_MCW_RC);
    return 0;
}

uint64_t ra_x86_statusfp(void) {
    return _statusfp() | ra_crt_flags(ra_x87_sw & 0x3f);
}

uint64_t ra_x86_clearfp(void) {
    unsigned flags = _clearfp() | ra_crt_flags(ra_x87_sw & 0x3f);
    ra_x87_sw &= (uint16_t)~0xffu;
    return flags;
}
#endif

uint64_t ra_x86_fetestexcept(uint64_t excepts) {
    return ((unsigned)fetestexcept((int)excepts) | ra_crt_flags(ra_x87_sw & 0x3f)) & (unsigned)excepts;
}

uint64_t ra_x86_feclearexcept(uint64_t excepts) {
    ra_x87_sw &= (uint16_t)~ra_x87_flags((unsigned)excepts);
    return (unsigned)feclearexcept((int)excepts);
}

// The x86 C runtime's square root is the x87's: the result rounded to the precision control, so
// a program that set 24 bits (Direct3D's default) finds a float's mantissa in its double. The
// double's root rounded to a float is the float's root: 53 bits are enough for the second
// rounding to be harmless.
double ra_x86_sqrt(double x) {
    double r = sqrt(x);
    return ((ra_x87_cw >> 8) & 3) == 0 ? (double)(float)r : r;
}

// setjmp and longjmp: the program's jmp_buf is a key, and the host's buffer lives in a table slot
// found by it. setjmp itself runs in the lifted function, through the macro in rhipper.h, so the
// frame it saves is alive when longjmp comes back to it.
static struct { uint64_t key; jmp_buf buf; } ra_jumps[16];

jmp_buf *ra_jmp_slot(uint64_t buffer) {
    for (int k = 0; k < 16; k++) if (ra_jumps[k].key == buffer) return &ra_jumps[k].buf;
    for (int k = 0; k < 16; k++) if (ra_jumps[k].key == 0) { ra_jumps[k].key = buffer; return &ra_jumps[k].buf; }
    fprintf(stderr, "ra: more than 16 jmp_bufs in use\n");
    fflush(stderr);
    host_exit(0xE0000006);
}

uint64_t ra_x86_longjmp(uint64_t buffer, uint64_t value) {
    longjmp(*ra_jmp_slot(buffer), (int)value ? (int)value : 1);
}

// The exact x87 unit: a software f80. A slot holds an 80-bit value as the x87 stores a tbyte, the
// 64-bit significand with its explicit integer bit then the sign and the 15-bit exponent, and six
// bytes of padding. Every operation unpacks its operands, works in 128 bits with a sticky bit for
// what fell below, and rounds the result by the control word: the rounding mode in bits 10 and 11
// and the precision in bits 8 and 9 (24, 53 or 64 bits), as the hardware does with the exceptions
// masked. No 128-bit division: the host's compiler would want a library for it.
typedef unsigned __int128 f80_u128;

enum { F80_ZERO, F80_FINITE, F80_INF, F80_NAN };

// The exception flags of the status word, which an operation raises as the hardware does with the
// exceptions masked: invalid, zero divide, overflow, underflow (a tiny and inexact result) and
// precision (an inexact one).
enum { F80_IE = 0x01, F80_ZE = 0x04, F80_OE = 0x08, F80_UE = 0x10, F80_PE = 0x20 };

static void f80_raise(unsigned flags) { ra_x87_sw |= (uint16_t)flags; }

// An unpacked value: m * 2^(exp - 63), m normalized with bit 63 set when finite.
struct f80 { int kind; int sign; int32_t exp; uint64_t m; };

static int f80_clz64(uint64_t v) { return (int)__builtin_clzll(v); }

static int f80_clz128(f80_u128 v) {
    uint64_t high = (uint64_t)(v >> 64);
    return high ? f80_clz64(high) : 64 + f80_clz64((uint64_t)v);
}

static struct f80 f80_unpack(const void *slot) {
    uint64_t m;
    uint16_t se;
    memcpy(&m, slot, 8);
    memcpy(&se, (const uint8_t *)slot + 8, 2);
    struct f80 v = { F80_FINITE, se >> 15, 0, m };
    int e = se & 0x7fff;
    if (e == 0x7fff) { v.kind = (m << 1) == 0 ? F80_INF : F80_NAN; return v; }
    if (m == 0) { v.kind = F80_ZERO; return v; }
    // A denormal has exponent 0 and the integer bit clear; an unnormal is normalized the same way.
    int shift = f80_clz64(m);
    v.m = m << shift;
    v.exp = (e == 0 ? 1 : e) - 16383 - shift;
    return v;
}

static void f80_write(void *slot, int sign, uint32_t exponent, uint64_t m) {
    uint8_t bytes[16] = { 0 };
    uint16_t se = (uint16_t)(((unsigned)sign << 15) | exponent);
    memcpy(bytes, &m, 8);
    memcpy(bytes + 8, &se, 2);
    memcpy(slot, bytes, 16);
}

// The real indefinite: the result of an invalid operation.
static void f80_write_indefinite(void *slot) {
    f80_raise(F80_IE);
    f80_write(slot, 1, 0x7fff, 0xc000000000000000ull);
}
static void f80_write_inf(void *slot, int sign) { f80_write(slot, sign, 0x7fff, 0x8000000000000000ull); }
static void f80_write_quiet(void *slot, struct f80 v) { f80_write(slot, v.sign, 0x7fff, v.m | 0xc000000000000000ull); }

// A NaN, an infinity or a zero goes out as it is, the NaN quiet.
static void f80_write_special(void *slot, struct f80 v) {
    if (v.kind == F80_NAN) f80_write_quiet(slot, v);
    else if (v.kind == F80_INF) f80_write_inf(slot, v.sign);
    else f80_write(slot, v.sign, 0, 0);
}

// The NaN of a two-operand operation: the one with the larger significand, quiet.
static int f80_nan_result(void *slot, struct f80 a, struct f80 b) {
    if (a.kind != F80_NAN && b.kind != F80_NAN) return 0;
    struct f80 n = a.kind != F80_NAN ? b : b.kind != F80_NAN ? a : (b.m > a.m ? b : a);
    f80_write_quiet(slot, n);
    return 1;
}

static int f80_rc(uint64_t cw) { return (int)((cw >> 10) & 3); }

static int f80_precision(uint64_t cw) {
    switch ((cw >> 8) & 3) {
    case 0: return 24;
    case 2: return 53;
    default: return 64;
    }
}

// A result rounded into a binary format: the value m * 2^(exp - 127) with bit 127 set (sticky for
// what lies below), to p bits in the mode rc, with the exponent bias and the all-ones exponent
// emax of the format; the significand comes back left-aligned in 64 bits with its integer bit,
// and a denormal with the exponent 0.
struct f80_packed { int sign; int32_t biased; uint64_t m; };

static struct f80_packed f80_round(int sign, int32_t exp, f80_u128 m, int sticky, int p, int rc, int32_t emax, int32_t bias) {
    struct f80_packed r = { sign, 0, 0 };
    if (m == 0) return r;
    int32_t biased = exp + bias;
    int tiny = biased < 1;
    if (tiny) {
        // A denormal: the bits below the format's smallest exponent go into the rounding.
        int32_t s = 1 - biased;
        if (s >= 128) { sticky |= m != 0; m = 0; }
        else { sticky |= (m & (((f80_u128)1 << s) - 1)) != 0; m >>= s; }
        biased = 1;
    }
    int shift = 128 - p;
    f80_u128 mask = ((f80_u128)1 << shift) - 1, half = (f80_u128)1 << (shift - 1);
    f80_u128 low = m & mask;
    m &= ~mask;
    int inexact = low != 0 || sticky;
    int up;
    switch (rc) {
    case 0: up = low > half || (low == half && (sticky || (m & ((f80_u128)1 << shift)) != 0)); break;
    case 1: up = sign && (low != 0 || sticky); break;
    case 2: up = !sign && (low != 0 || sticky); break;
    default: up = 0; break;
    }
    if (up) {
        f80_u128 before = m;
        m += (f80_u128)1 << shift;
        if (m < before) { m = (f80_u128)1 << 127; biased++; }
    }
    if (biased == 1 && !(m >> 127)) biased = 0;
    if (biased >= emax) {
        f80_raise(F80_OE | F80_PE);
        int to_inf = rc == 0 || (rc == 1 && sign) || (rc == 2 && !sign);
        r.biased = to_inf ? emax : emax - 1;
        r.m = to_inf ? 0x8000000000000000ull : ~(uint64_t)0 << (64 - p);
        return r;
    }
    if (inexact) f80_raise(tiny ? F80_UE | F80_PE : F80_PE);
    r.biased = biased;
    r.m = (uint64_t)(m >> 64);
    return r;
}

// Whether a finite value below 2^63 has a fraction: the bits of m below the units position.
static int f80_has_fraction(struct f80 v) {
    return v.exp < 0 || (v.m << (v.exp + 1)) != 0;
}

static void f80_write_rounded(void *slot, int sign, int32_t exp, f80_u128 m, int sticky, uint64_t cw) {
    struct f80_packed r = f80_round(sign, exp, m, sticky, f80_precision(cw), f80_rc(cw), 0x7fff, 16383);
    f80_write(slot, r.sign, (uint32_t)r.biased, r.m);
}

// A finite or zero value, rounded to the precision of the control word.
static void f80_write_value(void *slot, struct f80 v, uint64_t cw) {
    f80_write_rounded(slot, v.sign, v.exp, v.kind == F80_ZERO ? 0 : (f80_u128)v.m << 64, 0, cw);
}

// The magnitude of the integer nearest a finite value below 2^63, by the mode rc.
static uint64_t f80_to_integer(struct f80 v, int rc) {
    int shift = 63 - v.exp;
    uint64_t q;
    int half, sticky;
    if (shift > 64) { q = 0; half = 0; sticky = 1; }
    else if (shift == 64) { q = 0; half = (int)(v.m >> 63); sticky = (v.m << 1) != 0; }
    else {
        q = v.m >> shift;
        uint64_t rest = v.m & ((1ull << shift) - 1);
        half = (int)((rest >> (shift - 1)) & 1);
        sticky = (rest & ((1ull << (shift - 1)) - 1)) != 0;
    }
    int up;
    switch (rc) {
    case 0: up = half && (sticky || (q & 1)); break;
    case 1: up = v.sign && (half || sticky); break;
    case 2: up = !v.sign && (half || sticky); break;
    default: up = 0; break;
    }
    return q + (uint64_t)up;
}

void ra_f80_load(void *slot, const void *memory) {
    uint8_t bytes[16] = { 0 };
    memcpy(bytes, memory, 10);
    memcpy(slot, bytes, 16);
}

void ra_f80_store(void *memory, const void *slot) {
    memcpy(memory, slot, 10);
}

void ra_f80_pack(void *slot, uint64_t significand, uint64_t sign_exponent) {
    f80_write(slot, (int)(sign_exponent >> 15) & 1, (uint32_t)sign_exponent & 0x7fff, significand);
}

void ra_f80_from_f64(void *slot, double value) {
    uint64_t bits;
    memcpy(&bits, &value, 8);
    int sign = (int)(bits >> 63);
    int e = (int)((bits >> 52) & 0x7ff);
    uint64_t frac = bits & ((1ull << 52) - 1);
    if (e == 0x7ff) {
        f80_write(slot, sign, 0x7fff, frac ? 0xc000000000000000ull | (frac << 11) : 0x8000000000000000ull);
    } else if (e == 0 && frac == 0) {
        f80_write(slot, sign, 0, 0);
    } else if (e == 0) {
        int shift = f80_clz64(frac);
        f80_write(slot, sign, (uint32_t)(15372 - shift), frac << shift);
    } else {
        f80_write(slot, sign, (uint32_t)(e - 1023 + 16383), 0x8000000000000000ull | (frac << 11));
    }
}

void ra_f80_from_i64(void *slot, uint64_t value) {
    int64_t v = (int64_t)value;
    if (v == 0) { f80_write(slot, 0, 0, 0); return; }
    int sign = v < 0;
    uint64_t u = sign ? 0 - value : value;
    int shift = f80_clz64(u);
    f80_write(slot, sign, (uint32_t)(16383 + 63 - shift), u << shift);
}

double ra_f80_to_f64(const void *slot, uint64_t cw) {
    struct f80 v = f80_unpack(slot);
    uint64_t bits;
    switch (v.kind) {
    case F80_NAN: bits = (uint64_t)v.sign << 63 | 0x7ffull << 52 | 1ull << 51 | ((v.m << 1) >> 12); break;
    case F80_INF: bits = (uint64_t)v.sign << 63 | 0x7ffull << 52; break;
    case F80_ZERO: bits = (uint64_t)v.sign << 63; break;
    default: {
        struct f80_packed r = f80_round(v.sign, v.exp, (f80_u128)v.m << 64, 0, 53, f80_rc(cw), 0x7ff, 1023);
        bits = (uint64_t)r.sign << 63 | (uint64_t)r.biased << 52 | ((r.m << 1) >> 12);
        break;
    }
    }
    double d;
    memcpy(&d, &bits, 8);
    return d;
}

float ra_f80_to_f32(const void *slot, uint64_t cw) {
    struct f80 v = f80_unpack(slot);
    uint32_t bits;
    switch (v.kind) {
    case F80_NAN: bits = (uint32_t)v.sign << 31 | 0xffu << 23 | 1u << 22 | (uint32_t)((v.m << 1) >> 41); break;
    case F80_INF: bits = (uint32_t)v.sign << 31 | 0xffu << 23; break;
    case F80_ZERO: bits = (uint32_t)v.sign << 31; break;
    default: {
        struct f80_packed r = f80_round(v.sign, v.exp, (f80_u128)v.m << 64, 0, 24, f80_rc(cw), 0xff, 127);
        bits = (uint32_t)r.sign << 31 | (uint32_t)r.biased << 23 | (uint32_t)((r.m << 1) >> 41);
        break;
    }
    }
    float f;
    memcpy(&f, &bits, 4);
    return f;
}

// An integer of `bits` bits, rounded by `rounding` (the mode alone, 3 for a truncation), or the
// integer indefinite when the value is not a number or does not fit.
uint64_t ra_f80_to_int(const void *slot, uint64_t rounding, uint64_t bits) {
    struct f80 v = f80_unpack(slot);
    uint64_t limit = 1ull << (bits - 1);
    if (v.kind == F80_ZERO) return 0;
    if (v.kind == F80_NAN || v.kind == F80_INF || v.exp >= (int32_t)bits - 1) { f80_raise(F80_IE); return limit; }
    uint64_t q = f80_to_integer(v, (int)(rounding & 3));
    if (v.sign ? q > limit : q >= limit) { f80_raise(F80_IE); return limit; }
    if (f80_has_fraction(v)) f80_raise(F80_PE);
    return v.sign ? 0 - q : q;
}

static void f80_add(void *result, const void *pa, const void *pb, uint64_t cw, int negate) {
    struct f80 a = f80_unpack(pa), b = f80_unpack(pb);
    b.sign ^= negate;
    int rc = f80_rc(cw);
    if (f80_nan_result(result, a, b)) return;
    if (a.kind == F80_INF || b.kind == F80_INF) {
        if (a.kind == F80_INF && b.kind == F80_INF && a.sign != b.sign) f80_write_indefinite(result);
        else f80_write_special(result, a.kind == F80_INF ? a : b);
        return;
    }
    if (a.kind == F80_ZERO && b.kind == F80_ZERO) { f80_write(result, (a.sign & b.sign) | (rc == 1 && (a.sign | b.sign)), 0, 0); return; }
    if (a.kind == F80_ZERO) { f80_write_value(result, b, cw); return; }
    if (b.kind == F80_ZERO) { f80_write_value(result, a, cw); return; }
    // The larger magnitude first; the smaller one is aligned to it, what falls off kept as a
    // sticky bit far below the rounding position.
    if (a.exp < b.exp || (a.exp == b.exp && a.m < b.m)) { struct f80 t = a; a = b; b = t; }
    f80_u128 ma = (f80_u128)a.m << 64, mb = (f80_u128)b.m << 64;
    int32_t d = a.exp - b.exp;
    if (d >= 128) mb = 1;
    else if (d > 0) { f80_u128 lost = mb & (((f80_u128)1 << d) - 1); mb = (mb >> d) | (lost != 0); }
    f80_u128 m;
    int32_t exp = a.exp;
    if (a.sign == b.sign) {
        m = ma + mb;
        if (m < ma) { m = (m >> 1) | ((f80_u128)1 << 127) | (m & 1); exp++; }
    } else {
        m = ma - mb;
        if (m == 0) { f80_write(result, rc == 1, 0, 0); return; }
        int lz = f80_clz128(m);
        m <<= lz;
        exp -= lz;
    }
    f80_write_rounded(result, a.sign, exp, m, 0, cw);
}

void ra_f80_add(void *result, const void *a, const void *b, uint64_t cw) { f80_add(result, a, b, cw, 0); }
void ra_f80_sub(void *result, const void *a, const void *b, uint64_t cw) { f80_add(result, a, b, cw, 1); }

void ra_f80_mul(void *result, const void *pa, const void *pb, uint64_t cw) {
    struct f80 a = f80_unpack(pa), b = f80_unpack(pb);
    int sign = a.sign ^ b.sign;
    if (f80_nan_result(result, a, b)) return;
    if (a.kind == F80_INF || b.kind == F80_INF) {
        if (a.kind == F80_ZERO || b.kind == F80_ZERO) f80_write_indefinite(result);
        else f80_write_inf(result, sign);
        return;
    }
    if (a.kind == F80_ZERO || b.kind == F80_ZERO) { f80_write(result, sign, 0, 0); return; }
    f80_u128 m = (f80_u128)a.m * b.m;
    int32_t exp = a.exp + b.exp + 1;
    if (!(m >> 127)) { m <<= 1; exp--; }
    f80_write_rounded(result, sign, exp, m, 0, cw);
}

// (high:low) / d for high < d: the 64-bit quotient and the remainder, a bit at a time.
static uint64_t f80_udiv128(uint64_t high, uint64_t low, uint64_t d, uint64_t *remainder) {
    uint64_t q = 0;
    for (int i = 0; i < 64; i++) {
        int carry = (int)(high >> 63);
        high = (high << 1) | (low >> 63);
        low <<= 1;
        q <<= 1;
        if (carry || high >= d) { high -= d; q |= 1; }
    }
    *remainder = high;
    return q;
}

void ra_f80_div(void *result, const void *pa, const void *pb, uint64_t cw) {
    struct f80 a = f80_unpack(pa), b = f80_unpack(pb);
    int sign = a.sign ^ b.sign;
    if (f80_nan_result(result, a, b)) return;
    if (a.kind == F80_INF) {
        if (b.kind == F80_INF) f80_write_indefinite(result);
        else f80_write_inf(result, sign);
        return;
    }
    if (b.kind == F80_INF) { f80_write(result, sign, 0, 0); return; }
    if (b.kind == F80_ZERO) {
        if (a.kind == F80_ZERO) { f80_write_indefinite(result); return; }
        f80_raise(F80_ZE);
        f80_write_inf(result, sign);
        return;
    }
    if (a.kind == F80_ZERO) { f80_write(result, sign, 0, 0); return; }
    // The quotient of the significands to 65 bits: the 64 of the result and a guard bit, the
    // remainder as the sticky bit.
    int32_t exp = a.exp - b.exp;
    uint64_t high = a.m, remainder;
    int whole = a.m >= b.m;
    if (whole) high -= b.m;
    uint64_t q = f80_udiv128(high, 0, b.m, &remainder);
    f80_u128 quotient;
    if (whole) {
        quotient = ((f80_u128)1 << 64) | q;
    } else {
        int guard = remainder >= b.m - remainder;
        remainder = guard ? remainder - (b.m - remainder) : remainder * 2;
        quotient = ((f80_u128)q << 1) | (f80_u128)guard;
        exp--;
    }
    f80_write_rounded(result, sign, exp, quotient << 63, remainder != 0, cw);
}

void ra_f80_sqrt(void *result, const void *pa, uint64_t cw) {
    struct f80 v = f80_unpack(pa);
    if (v.kind == F80_NAN || v.kind == F80_ZERO) { f80_write_special(result, v); return; }
    if (v.sign) { f80_write_indefinite(result); return; }
    if (v.kind == F80_INF) { f80_write_inf(result, 0); return; }
    // The value as m * 2^k with k even, m in [2^126, 2^128), so the root of m has 64 bits; a
    // guard bit and the remainder follow.
    int32_t k = v.exp - 63;
    int odd = k & 1;
    f80_u128 n = odd ? (f80_u128)v.m << 63 : (f80_u128)v.m << 64;
    int32_t k2 = (odd ? k - 63 : k - 64) / 2;
    f80_u128 root = 0;
    for (f80_u128 bit = (f80_u128)1 << 126; bit; bit >>= 2) {
        if (n >= root + bit) { n -= root + bit; root = (root >> 1) + bit; }
        else root >>= 1;
    }
    int guard = n > root;
    f80_u128 rest = guard ? 4 * n - 4 * root - 1 : n;
    f80_u128 quotient = ((root << 1) | (f80_u128)guard) << 63;
    f80_write_rounded(result, 0, 63 + k2, quotient, rest != 0, cw);
}

void ra_f80_rndint(void *result, const void *pa, uint64_t cw) {
    struct f80 v = f80_unpack(pa);
    if (v.kind != F80_FINITE) { f80_write_special(result, v); return; }
    if (v.exp >= 63) { f80_write(result, v.sign, (uint32_t)(v.exp + 16383), v.m); return; }
    uint64_t q = f80_to_integer(v, f80_rc(cw));
    if (f80_has_fraction(v)) f80_raise(F80_PE);
    if (q == 0) { f80_write(result, v.sign, 0, 0); return; }
    int shift = f80_clz64(q);
    f80_write(result, v.sign, (uint32_t)(16383 + 63 - shift), q << shift);
}

void ra_f80_chs(void *result, const void *a) {
    uint8_t bytes[16];
    memcpy(bytes, a, 16);
    bytes[9] ^= 0x80;
    memcpy(result, bytes, 16);
}

void ra_f80_abs(void *result, const void *a) {
    uint8_t bytes[16];
    memcpy(bytes, a, 16);
    bytes[9] &= 0x7f;
    memcpy(result, bytes, 16);
}

uint64_t ra_f80_cmp(const void *pa, const void *pb) {
    struct f80 a = f80_unpack(pa), b = f80_unpack(pb);
    if (a.kind == F80_NAN || b.kind == F80_NAN) return 7;
    if (a.kind == F80_ZERO && b.kind == F80_ZERO) return 2;
    int order;
    if (a.sign != b.sign) {
        order = a.sign ? -1 : 1;
    } else {
        // The magnitudes: an infinity beyond everything, a zero below, else by exponent then significand.
        if (a.kind == F80_INF || b.kind == F80_INF) order = (a.kind == F80_INF) - (b.kind == F80_INF);
        else if (a.kind == F80_ZERO || b.kind == F80_ZERO) order = (b.kind == F80_ZERO) - (a.kind == F80_ZERO);
        else if (a.exp != b.exp) order = a.exp > b.exp ? 1 : -1;
        else order = a.m > b.m ? 1 : a.m < b.m ? -1 : 0;
        if (a.sign) order = -order;
    }
    return order < 0 ? 1 : order == 0 ? 2 : 0;
}

// The function behind the macro, for the import slot that holds its address: a setjmp through a
// pointer cannot save the caller's frame and returns as a first return does.
uint64_t (ra_x86_setjmp3)(uint64_t buffer, uint64_t count) {
    (void)buffer;
    (void)count;
    return 0;
}

// An import by ordinal has no symbol to link: the host's function, found at startup. Ordinals and
// the Microsoft runtime's printf and scanf entries below are a Windows program's alone.
#ifdef _WIN32
void *ra_resolve_ordinal(const char *dll, unsigned ordinal) {
    HMODULE module = LoadLibraryA(dll);
    void *function = module ? (void *)GetProcAddress(module, (LPCSTR)(uintptr_t)ordinal) : NULL;
    if (!function) {
        fprintf(stderr, "ra: %s has no ordinal %u\n", dll, ordinal);
        fflush(stderr);
        ExitProcess(0xE0000004);
    }
    return function;
}

// The printf and scanf family through their va_list: the program's arguments are dwords (a double
// or a 64-bit integer two of them), the host's va_list a row of 8-byte slots. The format says what
// each argument is; the adapter walks it and widens each argument into a slot.
static void widen_printf_args(const char *format, const uint32_t *args, uint64_t *slots, size_t count) {
    size_t n = 0, k = 0;
    for (const char *p = format; *p && n + 1 < count; p++) {
        if (*p != '%') continue;
        p++;
        if (*p == '%') continue;
        while (*p && strchr("-+ #0'", *p)) p++;
        if (*p == '*') { slots[n++] = args[k++]; p++; } else while (*p >= '0' && *p <= '9') p++;
        if (*p == '.') {
            p++;
            if (*p == '*') { slots[n++] = args[k++]; p++; } else while (*p >= '0' && *p <= '9') p++;
        }
        int wide = 0;
        if (p[0] == 'l' && p[1] == 'l') { wide = 1; p += 2; }
        else if (p[0] == 'I' && p[1] == '6' && p[2] == '4') { wide = 1; p += 3; }
        else if (p[0] == 'I' && p[1] == '3' && p[2] == '2') p += 3;
        else if (*p == 'j') { wide = 1; p++; }
        else if (*p == 'h') { p++; if (*p == 'h') p++; }
        else if (*p == 'l' || *p == 'L' || *p == 'w' || *p == 'I' || *p == 'z' || *p == 't') p++;
        switch (*p) {
        case 'a': case 'A': case 'e': case 'E': case 'f': case 'F': case 'g': case 'G':
            memcpy(&slots[n++], &args[k], 8);
            k += 2;
            break;
        case 'd': case 'i': case 'o': case 'u': case 'x': case 'X': case 'c': case 'C': case 's': case 'S': case 'p': case 'n': case 'Z':
            if (wide) { memcpy(&slots[n++], &args[k], 8); k += 2; } else slots[n++] = args[k++];
            break;
        case 0: return;
        default: break;
        }
    }
}

// The program's `%p` prints eight hex digits, the host's sixteen: it becomes `%08X` of the dword.
static const char *narrow_pointers(const char *format, char *copy, size_t size) {
    if (!strstr(format, "%p") || strlen(format) * 2 + 1 > size) return format;
    char *o = copy;
    for (const char *p = format; *p; p++) {
        if (p[0] == '%' && p[1] == 'p') { memcpy(o, "%08X", 4); o += 4; p++; }
        else if (p[0] == '%' && p[1] == '%') { *o++ = *p++; *o++ = *p; }
        else *o++ = *p;
    }
    *o = 0;
    return copy;
}

// scanf's arguments are all pointers, a dword each: one per conversion that is not suppressed.
static void widen_scanf_args(const char *format, const uint32_t *args, uint64_t *slots, size_t count) {
    size_t n = 0;
    for (const char *p = format; *p && n < count; p++) {
        if (*p != '%') continue;
        p++;
        if (*p == '%' || *p == '*' || *p == 0) continue;
        slots[n] = args[n];
        n++;
    }
}

uint64_t ra_x86_vsprintf(uint64_t options, uint64_t buffer, uint64_t count, uint64_t format, uint64_t locale, uint64_t args) {
    uint64_t slots[64] = {0};
    char copy[1024];
    const char *f = narrow_pointers((const char *)(uintptr_t)format, copy, sizeof copy);
    widen_printf_args(f, (const uint32_t *)(uintptr_t)args, slots, 64);
    return (uint64_t)(int64_t)__stdio_common_vsprintf(options, (char *)(uintptr_t)buffer, (size_t)count, f, (_locale_t)(uintptr_t)locale,
                                                      (va_list)slots);
}

uint64_t ra_x86_vsprintf_s(uint64_t options, uint64_t buffer, uint64_t count, uint64_t format, uint64_t locale, uint64_t args) {
    uint64_t slots[64] = {0};
    char copy[1024];
    const char *f = narrow_pointers((const char *)(uintptr_t)format, copy, sizeof copy);
    widen_printf_args(f, (const uint32_t *)(uintptr_t)args, slots, 64);
    return (uint64_t)(int64_t)__stdio_common_vsprintf_s(options, (char *)(uintptr_t)buffer, (size_t)count, f, (_locale_t)(uintptr_t)locale,
                                                        (va_list)slots);
}

uint64_t ra_x86_vsnprintf_s(uint64_t options, uint64_t buffer, uint64_t count, uint64_t max, uint64_t format, uint64_t locale, uint64_t args) {
    uint64_t slots[64] = {0};
    char copy[1024];
    const char *f = narrow_pointers((const char *)(uintptr_t)format, copy, sizeof copy);
    widen_printf_args(f, (const uint32_t *)(uintptr_t)args, slots, 64);
    return (uint64_t)(int64_t)__stdio_common_vsnprintf_s(options, (char *)(uintptr_t)buffer, (size_t)count, (size_t)max, f,
                                                         (_locale_t)(uintptr_t)locale, (va_list)slots);
}

uint64_t ra_x86_vfprintf(uint64_t options, uint64_t stream, uint64_t format, uint64_t locale, uint64_t args) {
    uint64_t slots[64] = {0};
    char copy[1024];
    const char *f = narrow_pointers((const char *)(uintptr_t)format, copy, sizeof copy);
    widen_printf_args(f, (const uint32_t *)(uintptr_t)args, slots, 64);
    return (uint64_t)(int64_t)__stdio_common_vfprintf(options, (FILE *)(uintptr_t)stream, f, (_locale_t)(uintptr_t)locale, (va_list)slots);
}

uint64_t ra_x86_vsscanf(uint64_t options, uint64_t buffer, uint64_t count, uint64_t format, uint64_t locale, uint64_t args) {
    uint64_t slots[64] = {0};
    widen_scanf_args((const char *)(uintptr_t)format, (const uint32_t *)(uintptr_t)args, slots, 64);
    return (uint64_t)(int64_t)__stdio_common_vsscanf(options, (const char *)(uintptr_t)buffer, (size_t)count, (const char *)(uintptr_t)format,
                                                     (_locale_t)(uintptr_t)locale, (va_list)slots);
}
#endif

// The strto* family writes a `char *` of eight bytes where the program expects four: the host's
// end pointer goes to a local, and its low half to the program's.
#define RA_STRTO(name, host, type, widen) \
    uint64_t name(uint64_t s, uint64_t end, uint64_t base) { \
        char *e = NULL; \
        type v = host((const char *)(uintptr_t)s, &e, (int)base); \
        if (end) *(uint32_t *)(uintptr_t)end = (uint32_t)(uintptr_t)e; \
        return (uint64_t)(widen)v; \
    }
RA_STRTO(ra_x86_strtol, strtol, long, int64_t)
RA_STRTO(ra_x86_strtoul, strtoul, unsigned long, uint64_t)
RA_STRTO(ra_x86_strtoll, strtoll, long long, int64_t)
RA_STRTO(ra_x86_strtoull, strtoull, unsigned long long, uint64_t)

double ra_x86_strtod(uint64_t s, uint64_t end) {
    char *e = NULL;
    double v = strtod((const char *)(uintptr_t)s, &e);
    if (end) *(uint32_t *)(uintptr_t)end = (uint32_t)(uintptr_t)e;
    return v;
}

// argv and the environment as dword pointers to the host's strings, which sit below 4 GB.
static int ra_argc;
static char **ra_argv;
static uint32_t *argv32;
static uint32_t argv32_address;
static uint32_t *environment32;

uint64_t ra_x86_p_argv(void) {
    if (!argv32) {
        argv32 = (uint32_t *)calloc((size_t)ra_argc + 1, sizeof(uint32_t));
        for (int i = 0; i < ra_argc; i++) argv32[i] = (uint32_t)(uintptr_t)ra_argv[i];
        argv32_address = (uint32_t)(uintptr_t)argv32;
    }
    return (uint64_t)(uintptr_t)&argv32_address;
}

uint64_t ra_x86_initial_narrow_environment(void) {
    if (!environment32) {
#ifdef _WIN32
        char *block = GetEnvironmentStringsA();
        size_t count = 0;
        for (char *p = block; *p; p += strlen(p) + 1) count++;
        environment32 = (uint32_t *)calloc(count + 1, sizeof(uint32_t));
        size_t i = 0;
        for (char *p = block; *p; p += strlen(p) + 1) environment32[i++] = (uint32_t)(uintptr_t)p;
#else
        extern char **environ;
        size_t count = 0;
        while (environ[count]) count++;
        environment32 = (uint32_t *)calloc(count + 1, sizeof(uint32_t));
        for (size_t i = 0; i < count; i++) environment32[i] = (uint32_t)(uintptr_t)environ[i];
#endif
    }
    return (uint64_t)(uintptr_t)environment32;
}

// The fail-fast reports of the x86 /GS checks (`__report_gsfailure` after a cookie mismatch,
// `__report_rangecheckfailure` on an index out of range): the host's own take another shape and
// clash with its static runtime; these end the program as the native would.
void ra_x86_report_gsfailure(void) {
    fprintf(stderr, "ra: the stack cookie check failed\n");
    fflush(stderr);
    host_exit(0xC0000409u);
}

void ra_x86_report_rangecheckfailure(void) {
    fprintf(stderr, "ra: an array index was out of range\n");
    fflush(stderr);
    host_exit(0xC0000409u);
}

// C++ exceptions: the runtime half of the protocol the lift writes into the code (rhipper.h names
// the calls). An exception is the thrown object's copy and the types it can be caught as; one is in
// flight from its throw until a handler takes it, suspended while the destructors of the way out
// run, staged between its match and the start of its catch body, and caught while the body runs. A
// rethrow puts the caught one back in flight; a catch that ends lets its exception go when nothing
// else holds it.
struct ra_exception {
    void *object;
    uint64_t size;
    // key, displacement and size of each catchable type.
    uint64_t types[3 * 16];
    uint64_t count;
    int holds;
};

static struct ra_exception *eh_in_flight, *eh_staged;
static struct ra_exception *eh_caught[64], *eh_suspended[64];
static int eh_caught_depth, eh_suspended_depth;

_Noreturn static void eh_fatal(const char *what) {
    fprintf(stderr, "ra: %s\n", what);
    fflush(stderr);
    host_exit(0xE0000006);
}

static void eh_release(struct ra_exception *e) {
    if (--e->holds == 0) {
        free(e->object);
        free(e);
    }
}

void ra_eh_throw(uint64_t object, uint64_t size) {
    if (eh_in_flight) eh_fatal("a throw while an exception is in flight");
    struct ra_exception *e = calloc(1, sizeof *e);
    e->object = malloc(size ? size : 1);
    memcpy(e->object, (const void *)(uintptr_t)object, size);
    e->size = size;
    e->holds = 1;
    eh_in_flight = e;
}

void ra_eh_throw_type(uint64_t key, uint64_t displacement, uint64_t size) {
    struct ra_exception *e = eh_in_flight;
    if (!e || e->count >= 16) eh_fatal("a catchable type with no throw to belong to");
    e->types[3 * e->count] = key;
    e->types[3 * e->count + 1] = displacement;
    e->types[3 * e->count + 2] = size;
    e->count++;
}

void ra_eh_rethrow(void) {
    if (eh_in_flight) eh_fatal("a rethrow while an exception is in flight");
    if (eh_caught_depth == 0) eh_fatal("a rethrow with no exception caught");
    struct ra_exception *e = eh_caught[eh_caught_depth - 1];
    e->holds++;
    eh_in_flight = e;
}

uint64_t ra_eh_pending(void) {
    return eh_in_flight != NULL;
}

uint64_t ra_eh_catches(uint64_t key, uint64_t slot, uint64_t flags) {
    struct ra_exception *e = eh_in_flight;
    if (!e) eh_fatal("a catch with no exception in flight");
    for (uint64_t i = 0; i < e->count; i++) {
        if (key != 0 && e->types[3 * i] != key) continue;
        if (slot) {
            char *subobject = (char *)e->object + e->types[3 * i + 1];
            if (flags & 8) {
                // By reference: the slot holds a pointer of the program's width.
                if ((flags >> 8) == 32) {
                    *(uint32_t *)(uintptr_t)slot = (uint32_t)(uintptr_t)subobject;
                } else {
                    *(uint64_t *)(uintptr_t)slot = (uint64_t)(uintptr_t)subobject;
                }
            } else {
                memcpy((void *)(uintptr_t)slot, subobject, (size_t)e->types[3 * i + 2]);
            }
        }
        eh_staged = e;
        eh_in_flight = NULL;
        return 1;
    }
    return 0;
}

void ra_eh_enter_catch(void) {
    if (eh_in_flight) eh_fatal("an exception escaped a destructor on the way to a catch");
    if (!eh_staged || eh_caught_depth == 64) eh_fatal("a catch body with no exception matched");
    eh_caught[eh_caught_depth++] = eh_staged;
    eh_staged = NULL;
}

void ra_eh_leave_catch(void) {
    if (eh_caught_depth == 0) eh_fatal("the end of a catch body with no exception caught");
    eh_release(eh_caught[--eh_caught_depth]);
}

void ra_eh_suspend(void) {
    if (eh_suspended_depth == 64) eh_fatal("too many exceptions suspended");
    eh_suspended[eh_suspended_depth++] = eh_in_flight;
    eh_in_flight = NULL;
}

void ra_eh_resume(void) {
    if (eh_in_flight) eh_fatal("an exception escaped a destructor while another was unwinding");
    if (eh_suspended_depth == 0) eh_fatal("a resume with no exception suspended");
    eh_in_flight = eh_suspended[--eh_suspended_depth];
}

// After the entry returns: an exception nothing caught ends the program, as the native runtime's
// terminate would.
static void eh_check_uncaught(void) {
    if (eh_in_flight) eh_fatal("an exception the program did not catch");
}

#if RA_POINTER_SIZE == 4
// The binding table of a 32-bit program's imports, which the lift writes: three words per import,
// the linked symbol, its DLL and name as "dll\0name\0" (zero for a function of the host's runtime)
// and the bytes a call to it pops; a zero triple ends it. A native function reached through a
// pointer is found by its linked address or by the DLL's own address of its name, which a bound
// slot or GetProcAddress hands the program; the DLL addresses are resolved at the first lookup.
extern const uint64_t ra_import_pops[];
extern uint64_t ra_pops;
static uint64_t *import_addresses;

// The trampolines the 32-bit import slots are bound to (see bind_imports): `mov rax, imm64; jmp
// rax`, 12 bytes each, in a page below 4 GB.
static uint8_t *slot_trampolines;
static size_t slot_trampoline_bytes;

// The host function a trampoline of ours jumps to, or `target` itself.
static uint64_t through_trampoline(uint64_t target) {
    uint8_t *p = (uint8_t *)(uintptr_t)target;
    if (slot_trampolines && p >= slot_trampolines && p + 12 <= slot_trampolines + slot_trampoline_bytes && p[0] == 0x48 && p[1] == 0xB8) {
        uint64_t host;
        memcpy(&host, p + 2, 8);
        return host;
    }
    return target;
}

static int64_t import_pops(uint64_t target) {
    target = through_trampoline(target);
    size_t count = 0;
    while (ra_import_pops[3 * count]) count++;
    if (!import_addresses) {
        import_addresses = calloc(count + 1, sizeof(uint64_t));
        for (size_t i = 0; i < count; i++) {
            const char *dll = (const char *)(uintptr_t)ra_import_pops[3 * i + 1];
            if (!dll) continue;
            const char *name = dll + strlen(dll) + 1;
            HMODULE module = LoadLibraryA(dll);
            if (!module) continue;
            FARPROC f = name[0] == '#' ? GetProcAddress(module, MAKEINTRESOURCEA(atoi(name + 1))) : GetProcAddress(module, name);
            import_addresses[i] = (uint64_t)(uintptr_t)f;
        }
    }
    for (size_t i = 0; i < count; i++) {
        if (ra_import_pops[3 * i] == target || (import_addresses[i] && import_addresses[i] == target)) return (int64_t)ra_import_pops[3 * i + 2];
    }
    return -1;
}
#endif

// What the x86 function last called through a pointer popped beyond its return address: an
// import's pops from the binding table, else what its universal entry point recorded in ra_pops.
// The record is consumed, so a native function outside the table reads as popping nothing rather
// than what an earlier callee popped.
uint64_t ra_pops_of(uint64_t target) {
#if RA_POINTER_SIZE == 4
    uint64_t recorded = ra_pops;
    ra_pops = 0;
    int64_t pops = import_pops(target);
    return pops >= 0 ? (uint64_t)pops : recorded;
#else
    (void)target;
    return 0;
#endif
}

// An indirect call in the machine model: a lifted function by its address, else native code
// through the universal adapter. The lifted caller pushed the return address; native code does
// not pop it, so this does, and what a 32-bit import pops beyond it comes from the binding table.
// A 32-bit program passes every argument on the stack as a dword and takes the result in edx:eax;
// a port of one runs in a process whose addresses all fit in 32 bits (the image is linked
// /LARGEADDRESSAWARE:NO), so a dword is a pointer the native code can use.
void ra_call(void *state_, uint64_t target) {
    struct State *state = state_;
    for (uint64_t i = 0; i < ra_function_count; i++) {
        if (ra_functions[i].address == target) {
            ra_functions[i].function(state);
            return;
        }
    }
    if (target >= RA_IMAGE_BASE && target < RA_IMAGE_BASE + RA_IMAGE_SIZE) {
        fprintf(stderr, "ra: an indirect call to 0x%llx, which is no lifted function\n", (unsigned long long)target);
        fflush(stderr);
        host_exit(0xE0000003);
    }
    native8 f = (native8)target;
#if RA_POINTER_SIZE == 4
    uint32_t *stack = (uint32_t *)(uintptr_t)((uint32_t)state->gpr[RSP] + 4);  // past the pushed return address
    uint64_t result = f(stack[0], stack[1], stack[2], stack[3], stack[4], stack[5], stack[6], stack[7]);
    int64_t pops = import_pops(target);
    state->gpr[RAX] = (uint32_t)result;
    state->gpr[RDX] = (uint32_t)(result >> 32);
    state->gpr[RSP] = (uint32_t)state->gpr[RSP] + 4 + (pops > 0 ? (uint32_t)pops : 0);
#elif defined(_WIN32)
    uint64_t *stack = (uint64_t *)(state->gpr[RSP] + 8);  // past the pushed return address
    state->gpr[RAX] = f(state->gpr[RCX], state->gpr[RDX], state->gpr[R8], state->gpr[R9], stack[4], stack[5], stack[6], stack[7]);
    state->gpr[RSP] += 8;
#else
    // A POSIX program calls by System V: six arguments in registers, the rest right past the
    // return address, without the shadow space.
    uint64_t *stack = (uint64_t *)(state->gpr[RSP] + 8);  // past the pushed return address
    state->gpr[RAX] = f(state->gpr[RDI], state->gpr[RSI], state->gpr[RDX], state->gpr[RCX], state->gpr[R8], state->gpr[R9], stack[0], stack[1]);
    state->gpr[RSP] += 8;
#endif
}

#ifndef RA_STANDALONE
#if RA_POINTER_SIZE == 4
// A host function's address does not fit a 32-bit program's slot: the slot is bound to a
// trampoline in a page below 4 GB, which a call through the slot's value reaches; the binding
// table sees through it to the host function.
static uint32_t slot_trampoline(FARPROC f) {
    if (!slot_trampolines) {
        size_t count = 0;
        while (ra_imports[count].slot != 0) count++;
        slot_trampoline_bytes = (count * 12 + 4095) & ~(size_t)4095;
        for (uintptr_t base = 0x10000000; base < 0x80000000 && !slot_trampolines; base += 0x01000000) {
            slot_trampolines = VirtualAlloc((void *)base, slot_trampoline_bytes, MEM_RESERVE | MEM_COMMIT, PAGE_EXECUTE_READWRITE);
        }
        if (!slot_trampolines) {
            fprintf(stderr, "ra: no room below 4 GB for the import trampolines\n");
            host_exit(0xE0000004);
        }
    }
    static size_t used;
    uint8_t *p = slot_trampolines + used * 12;
    used++;
    p[0] = 0x48;
    p[1] = 0xB8;
    uint64_t host = (uint64_t)(uintptr_t)f;
    memcpy(p + 2, &host, 8);
    p[10] = 0xFF;
    p[11] = 0xE0;
    return (uint32_t)(uintptr_t)p;
}
#endif

#ifdef _WIN32
static void bind_imports(void) {
    for (int i = 0; ra_imports[i].slot != 0; i++) {
        HMODULE module = LoadLibraryA(ra_imports[i].dll);
        if (!module) {
            fprintf(stderr, "ra: cannot load %s\n", ra_imports[i].dll);
            host_exit(0xE0000004);
        }
        FARPROC f = ra_imports[i].name ? GetProcAddress(module, ra_imports[i].name) : GetProcAddress(module, MAKEINTRESOURCEA(ra_imports[i].ordinal));
        if (!f) {
            // The host's DLL lacks it (the x86 exception handling of the C runtime, say): the
            // slot stays empty, and a call through it would fault at address zero.
            fprintf(stderr, "ra: %s has no %s; its slot stays empty\n", ra_imports[i].dll, ra_imports[i].name ? ra_imports[i].name : "(ordinal)");
            continue;
        }
#if RA_POINTER_SIZE == 4
        *(uint32_t *)(uintptr_t)ra_imports[i].slot = slot_trampoline(f);
#else
        *(uint64_t *)(uintptr_t)ra_imports[i].slot = (uint64_t)(uintptr_t)f;
#endif
    }
}
#else
// A POSIX program imports by symbol name from the libraries it needs, or from the whole process
// when the image names none for the symbol; an ordinal has no meaning there.
static void bind_imports(void) {
    for (int i = 0; ra_imports[i].slot != 0; i++) {
        const char *library = ra_imports[i].dll && *ra_imports[i].dll ? ra_imports[i].dll : NULL;
        void *module = dlopen(library, RTLD_NOW | RTLD_GLOBAL);
        if (!module) {
            fprintf(stderr, "ra: cannot load %s: %s\n", library ? library : "the process", dlerror());
            host_exit(0xE0000004);
        }
        void *f = ra_imports[i].name ? dlsym(module, ra_imports[i].name) : NULL;
        if (!f) {
            fprintf(stderr, "ra: %s has no %s; its slot stays empty\n", library ? library : "the process", ra_imports[i].name ? ra_imports[i].name : "(ordinal)");
            continue;
        }
        *(uint64_t *)(uintptr_t)ra_imports[i].slot = (uint64_t)(uintptr_t)f;
    }
}
#endif

#if defined(RA_ABI)
// A trampoline at each lifted function's image address: `mov rax, imm64; jmp rax`, 12 bytes.
// A function closer than 12 bytes to the next one is left alone: its original bytes run natively,
// which for a function that small (a leaf of a few instructions) gives the same result.
static int by_address(const void *a, const void *b) {
    uint64_t x = *(const uint64_t *)a, y = *(const uint64_t *)b;
    return x < y ? -1 : x > y;
}

static void install_trampolines(void) {
    uint64_t *addresses = malloc(sizeof(uint64_t) * (ra_function_count + 1));
    for (uint64_t i = 0; i < ra_function_count; i++) addresses[i] = ra_functions[i].address;
    qsort(addresses, (size_t)ra_function_count, sizeof(uint64_t), by_address);
    addresses[ra_function_count] = RA_IMAGE_BASE + RA_IMAGE_SIZE;
    for (uint64_t i = 0; i < ra_function_count; i++) {
        uint64_t address = ra_functions[i].address;
        uint64_t next = RA_IMAGE_BASE + RA_IMAGE_SIZE;
        for (uint64_t k = 0; k < ra_function_count; k++) {
            if (addresses[k] == address) {
                next = addresses[k + 1];
                break;
            }
        }
        if (next - address < 12) continue;
        uint8_t *p = (uint8_t *)(uintptr_t)address;
        p[0] = 0x48;
        p[1] = 0xB8;
        uint64_t host = (uint64_t)(uintptr_t)ra_functions[i].function;
        memcpy(p + 2, &host, 8);
        p[10] = 0xFF;
        p[11] = 0xE0;
    }
    free(addresses);
#ifdef _WIN32
    FlushInstructionCache(GetCurrentProcess(), (void *)(uintptr_t)RA_IMAGE_BASE, RA_IMAGE_SIZE);
#else
    __builtin___clear_cache((char *)(uintptr_t)RA_IMAGE_BASE, (char *)(uintptr_t)(RA_IMAGE_BASE + RA_IMAGE_SIZE));
#endif
}
#endif
#endif  // RA_STANDALONE

// The thread environment block the lifted code reads through fs: (x86) or gs: (x64), one per
// thread of the port: the fields a program and its C runtime look at, in the layout of the
// program's pointer width, from the host's own thread, and at 0x2c (x86) or 0x58 (x64) the
// thread-local storage pointer, an array whose one slot (the program's __tls_index is 0) is the
// thread's own copy of the image's TLS template. The stack bounds and the addresses fit the
// width: a 32-bit port runs with every address below 4 GB. The main thread attaches before the
// entry, as the loader does, with DLL_PROCESS_ATTACH to the TLS callbacks; another thread
// attaches at its first access, before it reads anything of its own, with DLL_THREAD_ATTACH.
// Nothing runs at a thread's exit. The environment block of the process stays zero.
extern const uint64_t ra_tls[5];  // the template's start and end, __tls_index, the zero fill, the callbacks

#ifdef _MSC_VER
#define RA_THREAD __declspec(thread)
#else
#define RA_THREAD _Thread_local
#endif
static RA_THREAD _Alignas(16) uint8_t teb[4096];
static RA_THREAD uint64_t tls_slot;
static RA_THREAD int attached;

#ifndef RA_ABI
// The stack a TLS callback of the machine IR runs on: the main thread's, set before it attaches.
static uint64_t machine_stack_top;

// A lifted function of the machine IR called with three arguments, as the loader calls a callback.
static void call_machine(void (*function)(void *), uint64_t a, uint64_t b, uint64_t c) {
    static struct State state;
    memset(&state, 0, sizeof state);
#if RA_POINTER_SIZE == 4
    uint32_t *sp = (uint32_t *)(uintptr_t)machine_stack_top;
    *--sp = (uint32_t)c;
    *--sp = (uint32_t)b;
    *--sp = (uint32_t)a;
    *--sp = 0;  // the return address
#else
    uint64_t *sp = (uint64_t *)(uintptr_t)machine_stack_top - 4;  // the shadow space
    *--sp = 0;  // the return address
    state.gpr[RCX] = a;
    state.gpr[RDX] = b;
    state.gpr[R8] = c;
#endif
    state.gpr[RSP] = (uint64_t)(uintptr_t)sp;
    function(&state);
}
#endif

// The TLS callbacks with `reason`, in their order: host functions standalone, else the lifted
// functions at the array's image addresses.
static void run_tls_callbacks(uint32_t reason) {
    for (uint64_t slot = ra_tls[4]; slot; slot += RA_POINTER_SIZE) {
        uint64_t target = RA_POINTER_SIZE == 4 ? *(uint32_t *)(uintptr_t)slot : *(uint64_t *)(uintptr_t)slot;
        if (!target) break;
#ifdef RA_STANDALONE
        ((native8)(uintptr_t)target)(RA_IMAGE_BASE, reason, 0, 0, 0, 0, 0, 0);
#else
        for (uint64_t i = 0; i < ra_function_count; i++) {
            if (ra_functions[i].address != target) continue;
#ifdef RA_ABI
            ((native8)ra_functions[i].function)(RA_IMAGE_BASE, reason, 0, 0, 0, 0, 0, 0);
#else
            call_machine(ra_functions[i].function, RA_IMAGE_BASE, reason, 0);
#endif
            break;
        }
#endif
    }
}

static void attach_thread(uint32_t reason) {
    attached = 1;
#ifdef _WIN32
    NT_TIB *tib = (NT_TIB *)NtCurrentTeb();
    uintptr_t self = (uintptr_t)teb;
    uintptr_t stack_base = (uintptr_t)tib->StackBase;
    uintptr_t stack_limit = (uintptr_t)tib->StackLimit;
    uint32_t pid = GetCurrentProcessId();
    uint32_t tid = GetCurrentThreadId();
#if RA_POINTER_SIZE == 4
    *(uint32_t *)(teb + 0x04) = (uint32_t)stack_base;
    *(uint32_t *)(teb + 0x08) = (uint32_t)stack_limit;
    *(uint32_t *)(teb + 0x18) = (uint32_t)self;
    *(uint32_t *)(teb + 0x20) = pid;
    *(uint32_t *)(teb + 0x24) = tid;
    *(uint32_t *)(teb + 0x2c) = (uint32_t)(uintptr_t)&tls_slot;
#else
    *(uint64_t *)(teb + 0x08) = stack_base;
    *(uint64_t *)(teb + 0x10) = stack_limit;
    *(uint64_t *)(teb + 0x30) = self;
    *(uint64_t *)(teb + 0x40) = pid;
    *(uint64_t *)(teb + 0x48) = tid;
    *(uint64_t *)(teb + 0x58) = (uint64_t)(uintptr_t)&tls_slot;
#endif
#else
    // A POSIX thread has no TIB to copy: the block names itself and its TLS pointer, and the rest,
    // the stack guard a Linux program reads at fs:0x28 among it, stays zero.
    *(uint64_t *)(teb + 0x30) = (uint64_t)(uintptr_t)teb;
    *(uint64_t *)(teb + 0x58) = (uint64_t)(uintptr_t)&tls_slot;
#endif
    if (ra_tls[0]) {
        size_t initialized = (size_t)(ra_tls[1] - ra_tls[0]);
        uint8_t *block = calloc(initialized + (size_t)ra_tls[3] + 16, 1);
        memcpy(block, (const void *)(uintptr_t)ra_tls[0], initialized);
        tls_slot = (uint64_t)(uintptr_t)block;
        run_tls_callbacks(reason);
    }
}

uint64_t ra_teb(void) {
    if (!attached) attach_thread(DLL_THREAD_ATTACH);
    return (uint64_t)(uintptr_t)teb;
}

// The loader's work before the entry: __tls_index written, the main thread's block, the callbacks.
static void attach_process(void) {
    if (ra_tls[0] && ra_tls[2]) *(uint32_t *)(uintptr_t)ra_tls[2] = 0;
    attach_thread(DLL_PROCESS_ATTACH);
}

#ifdef _WIN32
// The module the port is: the program itself, or the DLL the loader attached.
static HMODULE ra_module;

// A crash of the lifted code names the place: the host address as an offset into this module,
// which llvm-symbolizer maps to the lifted function and the image address its line comment
// carries, and for an access violation the address it touched. A vectored handler, so a program
// that installs its own unhandled-exception filter does not take the report away.
static LONG WINAPI on_exception(EXCEPTION_POINTERS *info) {
    const EXCEPTION_RECORD *r = info->ExceptionRecord;
    // The exceptions a program raises and handles itself are not crashes.
    if (r->ExceptionCode != EXCEPTION_ACCESS_VIOLATION && r->ExceptionCode != EXCEPTION_ILLEGAL_INSTRUCTION
        && r->ExceptionCode != EXCEPTION_STACK_OVERFLOW && r->ExceptionCode != EXCEPTION_PRIV_INSTRUCTION
        && r->ExceptionCode != EXCEPTION_BREAKPOINT && r->ExceptionCode != EXCEPTION_INT_DIVIDE_BY_ZERO
        && r->ExceptionCode != EXCEPTION_DATATYPE_MISALIGNMENT && r->ExceptionCode != EXCEPTION_IN_PAGE_ERROR) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    uintptr_t module = (uintptr_t)ra_module;
    uintptr_t at = (uintptr_t)r->ExceptionAddress;
    fprintf(stderr, "ra: exception 0x%lx at 0x%llx", r->ExceptionCode, (unsigned long long)at);
    if (at >= module) fprintf(stderr, " (module+0x%llx)", (unsigned long long)(at - module));
    if (r->ExceptionCode == EXCEPTION_ACCESS_VIOLATION && r->NumberParameters >= 2) {
        fprintf(stderr, ", %s 0x%llx", r->ExceptionInformation[0] == 1 ? "writing" : "reading", (unsigned long long)r->ExceptionInformation[1]);
    }
    fprintf(stderr, "\n");
    fflush(stderr);
    return EXCEPTION_CONTINUE_SEARCH;
}
#else
// A crash of the lifted code on a POSIX host names the signal and the address it touched; the
// handler then gives the signal back to its default, so the process ends as the crash would.
static void on_signal(int signal_number, siginfo_t *info, void *context) {
    (void)context;
    fprintf(stderr, "ra: signal %d at 0x%llx\n", signal_number, (unsigned long long)(uintptr_t)info->si_addr);
    fflush(stderr);
}

static void report_crashes(void) {
    struct sigaction action;
    memset(&action, 0, sizeof action);
    action.sa_sigaction = on_signal;
    action.sa_flags = SA_SIGINFO | SA_RESETHAND | SA_NODEFER;
    sigemptyset(&action.sa_mask);
    const int signals[] = {SIGSEGV, SIGBUS, SIGILL, SIGFPE};
    for (size_t i = 0; i < sizeof signals / sizeof signals[0]; i++) sigaction(signals[i], &action, NULL);
}
#endif

#if defined(RA_ABI) && !defined(RA_DLL)
// The entry, and the program's exit code: `main`'s result when the driver's startup stands in for
// the runtime's (`RA_ENTRY_RETURNS`), else nothing the driver reads.
static int ra_entry(void) {
#ifdef RA_ENTRY_RETURNS
    int code = (int)RA_ENTRY_CALL();
#else
    int code = 0;
    RA_ENTRY_CALL();
#endif
    eh_check_uncaught();
    return code;
}
#endif

#ifdef RA_DLL
#if !defined(RA_ABI) || !defined(RA_STANDALONE)
#error "a DLL ports standalone at the ABI level: its data are the port's own and its exports are host functions"
#endif
// The port of a DLL is a DLL: the host's loader maps it, relocates it and resolves its imports,
// and calls this with every reason as it would the program's own DllMain. The process attach is
// the driver's startup, then the lifted DllMain with the loader's arguments; its answer at the
// attach is the port's, so a DllMain that refuses fails the load as before. The handler that
// reports a crash leaves with the DLL.
static PVOID exception_handler;

BOOL WINAPI DllMain(HINSTANCE instance, DWORD reason, LPVOID reserved) {
    if (reason == DLL_PROCESS_ATTACH) {
        ra_module = (HMODULE)instance;
        exception_handler = AddVectoredExceptionHandler(1, on_exception);
        attach_process();
    }
#ifdef RA_ENTRY_RETURNS
    int ok = (int)RA_ENTRY_CALL(instance, reason, reserved) != 0;
#else
    int ok = 1;
    RA_ENTRY_CALL(instance, reason, reserved);
#endif
    if (reason == DLL_PROCESS_ATTACH) eh_check_uncaught();
    if (reason == DLL_PROCESS_DETACH && exception_handler) {
        RemoveVectoredExceptionHandler(exception_handler);
        exception_handler = NULL;
    }
    return ok ? TRUE : FALSE;
}
#else  // RA_DLL
#ifndef RA_STANDALONE
// Memory at `at`, or anywhere when `at` is zero; NULL when it cannot be had there. On a POSIX
// host the address is a hint rather than MAP_FIXED, so a mapping the process already holds is
// never replaced, and one the kernel placed elsewhere is given back.
static void *host_map(uintptr_t at, size_t size, int executable) {
#ifdef _WIN32
    return VirtualAlloc((void *)at, size, MEM_RESERVE | MEM_COMMIT, executable ? PAGE_EXECUTE_READWRITE : PAGE_READWRITE);
#else
    int protect = PROT_READ | PROT_WRITE | (executable ? PROT_EXEC : 0);
    void *p = mmap((void *)at, size, protect, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p == MAP_FAILED) return NULL;
    if (at && p != (void *)at) {
        munmap(p, size);
        return NULL;
    }
    return p;
#endif
}
#endif

int main(int argc, char **argv) {
#ifdef _WIN32
    ra_module = GetModuleHandleA(NULL);
    AddVectoredExceptionHandler(1, on_exception);
#else
    report_crashes();
#endif
    ra_argc = argc;
    ra_argv = argv;
#ifdef RA_STANDALONE
    // The data sections are globals of the program and the imports are linked by name: there is
    // no image to map and nothing to bind.
    (void)argc;
    (void)argv;
    attach_process();
    return ra_entry();
#else
    const char *image_path = argc > 1 ? argv[1] : "image.bin";
    FILE *f = fopen(image_path, "rb");
    if (!f) {
        fprintf(stderr, "ra: cannot open %s\n", image_path);
        return 2;
    }
    void *image = host_map((uintptr_t)RA_IMAGE_BASE, RA_IMAGE_SIZE, 1);
    if (image != (void *)(uintptr_t)RA_IMAGE_BASE) {
        fprintf(stderr, "ra: cannot map the image at 0x%llx\n", (unsigned long long)RA_IMAGE_BASE);
        return 2;
    }
    size_t read = fread(image, 1, RA_IMAGE_SIZE, f);
    fclose(f);
    if (read == 0) {
        fprintf(stderr, "ra: %s is empty\n", image_path);
        return 2;
    }
    bind_imports();

#ifdef RA_ABI
    install_trampolines();
    attach_process();
    return ra_entry();
#else
    static struct State state;
    const size_t stack_size = 1 << 20;
    uint8_t *stack = NULL;
#if RA_POINTER_SIZE == 4
    // A 32-bit program keeps its stack pointer in 32 bits: the stack must sit below 4 GB.
    for (uintptr_t base = 0x20000000; base < 0x80000000 && !stack; base += 0x10000000) stack = host_map(base, stack_size, 0);
#else
    stack = host_map(0, stack_size, 0);
#endif
    if (!stack) return 2;
    // The entry sees the stack pointer as after a call: a pointer past a 16-byte boundary, with
    // room above it.
    uint64_t top = ((uint64_t)(uintptr_t)(stack + stack_size - 64)) & ~15ULL;
    machine_stack_top = top;
    attach_process();
    state.gpr[RSP] = top - RA_POINTER_SIZE;
#if RA_POINTER_SIZE == 4
    *(uint32_t *)(uintptr_t)state.gpr[RSP] = 0;  // the return address of the entry
#else
    *(uint64_t *)(uintptr_t)state.gpr[RSP] = 0;  // the return address of the entry
#endif
    state.gpr[RCX] = 0;
    state.flags[5] = 0;  // DF clear
    RA_ENTRY(&state);
    eh_check_uncaught();
    return (int)state.gpr[RAX];
#endif
#endif
}
#endif  // RA_DLL
