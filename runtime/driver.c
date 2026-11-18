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
// A Windows program (RA_WIN32) ports there too: its kernel32, user32 and C runtime imports resolve
// against the Win32 layer below, which maps files, memory, threads, time, the console and the
// message loop onto the host. `ras doctor --host <exe>` names the imports the layer lacks.
#ifndef _WIN32
#define _GNU_SOURCE
#endif
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
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <signal.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>
#if defined(__GLIBC__) || defined(__linux__)
#include <malloc.h>
#include <sys/syscall.h>
#endif
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
#if RA_POINTER_SIZE == 4 && !(defined(__linux__) && defined(__x86_64__) && defined(RA_STANDALONE))
#error "a 32-bit program keeps its pointers below 4 GB: off Windows its port builds standalone on x86-64 Linux, whose MAP_32BIT and non-PIE link keep them there"
#endif
#ifdef RA_DLL
#error "the port of a DLL is a Windows DLL: it builds on a Windows host"
#endif
// The reasons a Windows loader passes the TLS callbacks; a POSIX image has none to call.
enum { DLL_PROCESS_ATTACH = 1, DLL_THREAD_ATTACH = 2 };
// The status keeps its low byte, the one a POSIX parent sees.
#define host_exit(code) _Exit((int)((code) & 0xff))
#endif

// The machine state, as %State in the IR: 16 general registers, rip, 8 flag bytes, 32 vector
// slots (xmm0..15, then the upper halves of ymm0..15).
struct State {
    uint64_t gpr[16];
    uint64_t rip;
    uint8_t flags[8];
    _Alignas(16) uint8_t xmm[32][16];
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

// The entry hook of a port built with RA_TRACE or RA_TRACE_LAST: each function names itself as it
// starts, on stderr under RA_TRACE, and in the ring of the last functions entered, which a crash
// or a trap lists after its report; RA_TRACE_LAST keeps the ring alone, for a port whose whole
// trace is longer than a failure's output holds. The ring is one for every thread: a race muddles
// a name in the report, not the port. A port built with RA_TRACE_LAST names every function all
// the same when the RA_TRACE environment variable is set, so the one build that reported a
// failure gives its whole trace on a second run.
#define ENTERED 24
static const char *entered[ENTERED];
static unsigned entered_count;

#ifdef RA_TRACE_LAST
static int whole_trace(void) {
    static int whole = -1;
    if (whole < 0) {
#ifdef _WIN32
        whole = GetEnvironmentVariableA("RA_TRACE", NULL, 0) != 0;
#else
        whole = getenv("RA_TRACE") != NULL;
#endif
    }
    return whole;
}
#endif

void ra_enter(const char *name) {
    entered[entered_count++ % ENTERED] = name;
#ifdef RA_TRACE_LAST
    if (!whole_trace()) return;
#endif
    fputs("> ", stderr);
    fputs(name, stderr);
    fputs("\n", stderr);
    fflush(stderr);
}

// The last functions entered, oldest first; nothing in a port built without the hook.
static void report_entered(void) {
    if (entered_count == 0) return;
    fputs("ra: the last functions entered:", stderr);
    unsigned first = entered_count > ENTERED ? entered_count - ENTERED : 0;
    for (unsigned i = first; i < entered_count; i++) {
        fputc(' ', stderr);
        fputs(entered[i % ENTERED], stderr);
    }
    fputs("\n", stderr);
    fflush(stderr);
}

_Noreturn void ra_trap(uint64_t address, uint64_t code) {
    fprintf(stderr, "ra: %s at 0x%llx\n", trap_name(code), (unsigned long long)address);
    report_entered();
    fflush(stderr);
    host_exit(0xE0000001);
}

// A check of a port emitted checked that did not hold: the assumption by its number, with the
// function it was made in, what was assumed and the address it was made at, from the port's table.
_Noreturn void ra_assume_failed(uint64_t id, const char *function, const char *kind, uint64_t site) {
    fprintf(stderr, "ra: assumption %llu does not hold: %s in %s at 0x%llx\n", (unsigned long long)id, kind, function,
            (unsigned long long)site);
    fflush(stderr);
    host_exit(0xE0000005);
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

// The R5900's FPU, as the interpreter's crates/ra-asm-lift/src/run/ftz.rs computes it: a register's
// value is exact in a double, and the double's result is truncated into a register.
#define RA_FTZ_MAX 0x7fffffffu
#define RA_FTZ_SIGN 0x80000000u
#define RA_FTZ_MIN_NORMAL 0x00800000u

static double ra_ftz_value(uint32_t bits) {
    uint64_t sign = (uint64_t)(bits & RA_FTZ_SIGN) << 32, exponent = (bits >> 23) & 0xff, out;
    double value;
    out = exponent == 0 ? sign : sign | ((exponent + 1023 - 127) << 52) | ((uint64_t)(bits & 0x7fffff) << 29);
    memcpy(&value, &out, sizeof value);
    return value;
}

static uint32_t ra_ftz_bits(double x) {
    uint64_t b;
    memcpy(&b, &x, sizeof b);
    uint32_t sign = (uint32_t)(b >> 32) & RA_FTZ_SIGN;
    int64_t exponent = (int64_t)((b >> 52) & 0x7ff) - 1023 + 127;
    if (x == 0.0 || exponent < 1) return sign;
    if (exponent > 255) return sign | RA_FTZ_MAX;
    return sign | ((uint32_t)exponent << 23) | ((uint32_t)(b >> 29) & 0x7fffff);
}

// The sum rounded to nearest and its exact error (the two-sum): a sum on the register's grid with
// the exact value nearer zero steps toward zero.
static uint32_t ra_ftz_sum(double a, double b) {
    double s = a + b;
    double bb = s - a, error = (a - (s - bb)) + (b - bb);
    uint32_t r = ra_ftz_bits(s);
    if (error != 0.0 && ra_ftz_value(r) == s && (error < 0.0) != (s < 0.0))
        return (r & RA_FTZ_MAX) == RA_FTZ_MIN_NORMAL ? r & RA_FTZ_SIGN : r - 1;
    return r;
}

// Whether g * sqrt(b) <= a for the normal magnitudes g, a and b, in integers: g * g * b against
// a * a, both of up to 96 bits as two words.
static int ra_ftz_root_at_most(uint32_t g, uint32_t a, uint32_t b) {
    uint64_t mg = (g & 0x7fffff) | RA_FTZ_MIN_NORMAL, ma = (a & 0x7fffff) | RA_FTZ_MIN_NORMAL, mb = (b & 0x7fffff) | RA_FTZ_MIN_NORMAL;
    int eg = (int)((g >> 23) & 0xff), ea = (int)((a >> 23) & 0xff), eb = (int)((b >> 23) & 0xff);
    int k = 2 * ea - 2 * eg - eb + 127 + 23;
    if (k < 0) return 0;
    if (k > 48) return 1;
    uint64_t p = mg * mg, p0 = p & 0xffffffffu, p1 = p >> 32;
    uint64_t low = p0 * mb, high = p1 * mb + (low >> 32);
    uint64_t left_hi = high >> 32, left_lo = (high << 32) | (low & 0xffffffffu);
    uint64_t q = ma * ma, right_hi = k == 0 ? 0 : q >> (64 - k), right_lo = q << k;
    return left_hi != right_hi ? left_hi < right_hi : left_lo <= right_lo;
}

uint32_t ra_f32_ftz_add(uint32_t a, uint32_t b) {
    return ra_ftz_sum(ra_ftz_value(a), ra_ftz_value(b));
}

uint32_t ra_f32_ftz_sub(uint32_t a, uint32_t b) {
    return ra_ftz_sum(ra_ftz_value(a), -ra_ftz_value(b));
}

uint32_t ra_f32_ftz_mul(uint32_t a, uint32_t b) {
    return ra_ftz_bits(ra_ftz_value(a) * ra_ftz_value(b));
}

uint32_t ra_f32_ftz_div(uint32_t a, uint32_t b) {
    if ((b & 0x7f800000) == 0) return ((a ^ b) & RA_FTZ_SIGN) | RA_FTZ_MAX;
    return ra_ftz_bits(ra_ftz_value(a) / ra_ftz_value(b));
}

uint32_t ra_f32_ftz_sqrt(uint32_t a) {
    if ((a & 0x7f800000) == 0) return a & RA_FTZ_SIGN;
    return ra_ftz_bits(sqrt(ra_ftz_value(a & RA_FTZ_MAX)));
}

uint32_t ra_f32_ftz_rsqrt(uint32_t a, uint32_t b) {
    uint32_t sign = a & RA_FTZ_SIGN, r;
    if ((b & 0x7f800000) == 0) return sign | RA_FTZ_MAX;
    if ((a & 0x7f800000) == 0) return sign;
    a &= RA_FTZ_MAX;
    b &= RA_FTZ_MAX;
    r = ra_ftz_bits(ra_ftz_value(a) / sqrt(ra_ftz_value(b)));
    if (r < RA_FTZ_MIN_NORMAL) r = RA_FTZ_MIN_NORMAL;
    while (r > RA_FTZ_MIN_NORMAL && !ra_ftz_root_at_most(r, a, b)) r--;
    if (r == RA_FTZ_MIN_NORMAL && !ra_ftz_root_at_most(r, a, b)) return sign;
    while (r < RA_FTZ_MAX && ra_ftz_root_at_most(r + 1, a, b)) r++;
    return sign | r;
}

uint32_t ra_f32_ftz_cvt_s_w(uint32_t a) {
    return ra_ftz_bits((double)(int32_t)a);
}

uint32_t ra_f32_ftz_cvt_w_s(uint32_t a) {
    if ((a & 0x7f800000) > 0x4e800000) return (a & RA_FTZ_SIGN) ? 0x80000000u : 0x7fffffffu;
    return (uint32_t)(int32_t)trunc(ra_ftz_value(a));
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
    report_entered();
    fflush(stderr);
    host_exit(0xE0000002);
}

// A structure an import of a 32-bit program takes that its adapter cannot convert: a host's value
// wider than the program's field, or a structure whose layouts do not say how.
_Noreturn void ra_adapt_failed(const char *what, uint64_t value) {
    fprintf(stderr, "ra: %s: 0x%llx\n", what, (unsigned long long)value);
    report_entered();
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

// `pause`, the spin-wait hint of an x86 program: the host's own where it has one, so a loop of
// them the program times still takes time. It answers nothing.
uint64_t ra_pause(void) {
#if defined(__x86_64__) || defined(_M_X64) || defined(__i386__) || defined(_M_IX86)
    __builtin_ia32_pause();
#elif defined(__aarch64__) || defined(_M_ARM64)
    __asm__ volatile("yield");
#endif
    return 0;
}

// A Linux system call of a MIPS o32 program, by its number (`4000 + n`) on the four argument
// registers: the kernel's result, negative with the error number on a failure. A program without
// a C library prints and ends through these; the host's C library answers them on any host, as
// it answers the `write` and `exit` an ARM64 Linux program imports. The arguments are o32's
// 32-bit words, its pointers the port's own below 4 GB.
uint64_t ra_syscall(uint64_t number, uint64_t a0, uint64_t a1, uint64_t a2, uint64_t a3) {
    (void)a3;
    switch (number) {
    case 4001:  // exit
    case 4246:  // exit_group
        fflush(stdout);
        fflush(stderr);
        host_exit((int)(uint32_t)a0);
    case 4004: {  // write
        uint32_t fd = (uint32_t)a0, len = (uint32_t)a2;
        if (fd != 1 && fd != 2) return (uint64_t)-9;  // -EBADF
        FILE *out = fd == 2 ? stderr : stdout;
        fwrite((const void *)(uintptr_t)(uint32_t)a1, 1, len, out);
        fflush(out);
        return len;
    }
    default:
        fprintf(stderr, "ra: the system call %llu is not answered\n", (unsigned long long)number);
        fflush(stderr);
        host_exit(0xE0000002);
    }
}

typedef uint64_t (*native8)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);

// The C runtime functions a 32-bit program calls that walk or hand back a table of its own
// pointers. A pointer the program holds is a dword, and a function it names is its universal
// entry point, which takes eight integers and ignores the ones it does not need; the host's own
// versions would read the tables with 64-bit strides.
static uint64_t call_universal(uint32_t function) {
    native8 f = (native8)(uintptr_t)function;
    return f(0, 0, 0, 0, 0, 0, 0, 0);
}

// The C++ runtime's array iterators (`eh vector constructor iterator` and its destructor twin),
// linked into the image from the runtime library: each element's structor is a `thiscall`
// function of the program, the element in `ecx`, which the universal entry point takes as its
// seventh argument. The destructors run from the last element down. The constructor's unwinding
// of the elements built so far, when one throws, is left to the program's own frames.
uint64_t ra_x86_eh_vector_constructor_iterator(uint64_t array, uint64_t size, uint64_t count, uint64_t constructor, uint64_t undo) {
    (void)undo;
    native8 f = (native8)(uintptr_t)(uint32_t)constructor;
    for (uint32_t k = 0; k < (uint32_t)count; k++) f(0, 0, 0, 0, 0, 0, (uint32_t)(array + (uint64_t)k * (uint32_t)size), 0);
    return 0;
}

uint64_t ra_x86_eh_vector_destructor_iterator(uint64_t array, uint64_t size, uint64_t count, uint64_t destructor) {
    native8 f = (native8)(uintptr_t)(uint32_t)destructor;
    for (uint32_t k = (uint32_t)count; k-- > 0;) f(0, 0, 0, 0, 0, 0, (uint32_t)(array + (uint64_t)k * (uint32_t)size), 0);
    return 0;
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

// The interlocked list itself, in the program's layout: the head is {the first entry, a 16-bit
// depth, a 16-bit sequence} in one 8-byte word, and an entry's first dword links the next; the
// host's operations would read a 16-byte head and 8-byte links. Each operation swaps the whole
// head at once, and a push counts in the sequence, so a pop that raced a pop and a push fails its
// swap and reads the list again.
static volatile uint64_t *slist32_head(uint64_t head) {
    return (volatile uint64_t *)(uintptr_t)head;
}

static uint64_t slist32_depth(uint64_t word) {
    return (uint16_t)(word >> 32);
}

uint64_t ra_x86_interlocked_push_entry_slist(uint64_t head, uint64_t entry) {
    volatile uint64_t *h = slist32_head(head);
    uint64_t old = __atomic_load_n(h, __ATOMIC_SEQ_CST), next;
    do {
        *(volatile uint32_t *)(uintptr_t)entry = (uint32_t)old;
        next = (uint32_t)entry | (uint64_t)(uint16_t)(slist32_depth(old) + 1) << 32 | (uint64_t)(uint16_t)((old >> 48) + 1) << 48;
    } while (!__atomic_compare_exchange_n(h, &old, next, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST));
    return (uint32_t)old;
}

uint64_t ra_x86_interlocked_pop_entry_slist(uint64_t head) {
    volatile uint64_t *h = slist32_head(head);
    uint64_t old = __atomic_load_n(h, __ATOMIC_SEQ_CST), next;
    do {
        uint32_t first = (uint32_t)old;
        if (!first) return 0;
        next = *(volatile uint32_t *)(uintptr_t)first | (uint64_t)(uint16_t)(slist32_depth(old) - 1) << 32 | (old & 0xffff000000000000ULL);
    } while (!__atomic_compare_exchange_n(h, &old, next, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST));
    return (uint32_t)old;
}

uint64_t ra_x86_interlocked_flush_slist(uint64_t head) {
    volatile uint64_t *h = slist32_head(head);
    uint64_t old = __atomic_load_n(h, __ATOMIC_SEQ_CST);
    while (!__atomic_compare_exchange_n(h, &old, old & 0xffff000000000000ULL, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST)) {}
    return (uint32_t)old;
}

uint64_t ra_x86_query_depth_slist(uint64_t head) {
    return slist32_depth(__atomic_load_n(slist32_head(head), __ATOMIC_SEQ_CST));
}

// EncodePointer and DecodePointer for a program that keeps the encoded pointer in a dword: the
// 64-bit kernel32's encoding takes all 64 bits, so its decode of the dword is no pointer. The
// runtime's flips the bits of a constant: the null pointer encodes to something else, as kernel32's
// does, and the encoding keeps to 32 bits.
#define RA_POINTER_KEY 0x5ca1ab1eu

uint64_t ra_x86_encode_pointer(uint64_t pointer) {
    return (uint32_t)pointer ^ RA_POINTER_KEY;
}

uint64_t ra_x86_decode_pointer(uint64_t pointer) {
    return (uint32_t)pointer ^ RA_POINTER_KEY;
}

#ifdef _WIN32
// CommandLineToArgvW for a program whose pointers are dwords: shell32's table holds 8-byte
// pointers, whose high halves the program would read as the next argument. The runtime's is one
// LocalAlloc block, the table of dword pointers and the strings after it, below 4 GB as every
// allocation of the port is, so the program's LocalFree frees it all.
uint64_t ra_x86_command_line_to_argv_w(uint64_t line, uint64_t count) {
    typedef LPWSTR *(WINAPI * split_fn)(LPCWSTR, int *);
    static split_fn split;
    if (!split) split = (split_fn)(void *)GetProcAddress(LoadLibraryA("shell32.dll"), "CommandLineToArgvW");
    int n = 0;
    LPWSTR *wide = split ? split((LPCWSTR)(uintptr_t)line, &n) : NULL;
    if (!wide) return 0;
    size_t bytes = ((size_t)n + 1) * sizeof(uint32_t);
    for (int i = 0; i < n; i++) bytes += (wcslen(wide[i]) + 1) * sizeof(WCHAR);
    uint32_t *table = (uint32_t *)LocalAlloc(LMEM_FIXED, bytes);
    if (table) {
        WCHAR *text = (WCHAR *)(table + n + 1);
        for (int i = 0; i < n; i++) {
            size_t length = wcslen(wide[i]) + 1;
            memcpy(text, wide[i], length * sizeof(WCHAR));
            table[i] = (uint32_t)(uintptr_t)text;
            text += length;
        }
        table[n] = 0;
        if (count) *(int32_t *)(uintptr_t)count = n;
    }
    LocalFree(wide);
    return (uint64_t)(uintptr_t)table;
}
#endif

// The interlocked operations of the 32-bit kernel32, on the program's 32-bit value at `target`:
// the 64-bit kernel32 exports none of them. Each is a full barrier, as kernel32's is, and returns
// what kernel32's does: the new value for the increment and the decrement, the old one otherwise.
uint64_t ra_x86_interlocked_increment(uint64_t target) {
    return (uint32_t)__atomic_add_fetch((volatile int32_t *)(uintptr_t)target, 1, __ATOMIC_SEQ_CST);
}

uint64_t ra_x86_interlocked_decrement(uint64_t target) {
    return (uint32_t)__atomic_sub_fetch((volatile int32_t *)(uintptr_t)target, 1, __ATOMIC_SEQ_CST);
}

uint64_t ra_x86_interlocked_exchange(uint64_t target, uint64_t value) {
    return (uint32_t)__atomic_exchange_n((volatile int32_t *)(uintptr_t)target, (int32_t)value, __ATOMIC_SEQ_CST);
}

uint64_t ra_x86_interlocked_exchange_add(uint64_t target, uint64_t value) {
    return (uint32_t)__atomic_fetch_add((volatile int32_t *)(uintptr_t)target, (int32_t)value, __ATOMIC_SEQ_CST);
}

uint64_t ra_x86_interlocked_compare_exchange(uint64_t target, uint64_t exchange, uint64_t comparand) {
    int32_t expected = (int32_t)comparand;
    __atomic_compare_exchange_n((volatile int32_t *)(uintptr_t)target, &expected, (int32_t)exchange, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST);
    return (uint32_t)expected;
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

// The run-time type information of a 32-bit program, which the host's `__RTDynamicCast`,
// `__RTCastToVoid` and `__RTtypeid` would read in the 64-bit layout (image-relative offsets in
// eight-byte slots): the program's structures hold absolute dwords. The dword before an object's
// vtable names its complete object locator {signature, offset, cdOffset, type descriptor, class
// hierarchy descriptor}; the hierarchy's base class array lists the complete class and every base,
// depth first, each {type descriptor, contained bases, PMD {mdisp, pdisp, vdisp}, attributes}, so
// the bases a class contains follow it in the array. The two images of a port each carry a
// descriptor for a type they share, so types compare by their decorated names.
static uint32_t rtti32_u32(uint64_t at) {
    return *(uint32_t *)(uintptr_t)at;
}

static int32_t rtti32_i32(uint64_t at) {
    return *(int32_t *)(uintptr_t)at;
}

static int rtti32_same_type(uint32_t a, uint32_t b) {
    return a == b || strcmp((const char *)(uintptr_t)(a + 8), (const char *)(uintptr_t)(b + 8)) == 0;
}

static uint32_t rtti32_locator(uint32_t object) {
    return rtti32_u32(rtti32_u32(object) - 4);
}

// The offset of a base in the complete object, through the vbtable for a virtual one.
static int32_t rtti32_base_offset(uint32_t complete, uint32_t pmd) {
    int32_t offset = 0, pdisp = rtti32_i32(pmd + 4);
    if (pdisp >= 0) offset = pdisp + rtti32_i32(rtti32_u32(complete + (uint32_t)pdisp) + rtti32_i32(pmd + 8));
    return offset + rtti32_i32(pmd);
}

// The complete object `object` is a subobject of.
static uint32_t rtti32_complete(uint32_t object) {
    uint32_t locator = rtti32_locator(object);
    uint32_t complete = object - rtti32_u32(locator + 4);
    uint32_t cd_offset = rtti32_u32(locator + 8);
    if (cd_offset) complete -= rtti32_u32(object - cd_offset);
    return complete;
}

// `dynamic_cast`: the target instance that contains the source instance (a downcast), unless two
// at different offsets do; else a cross cast to a target the complete object holds once and
// publicly, from a public source. A failed cast of a reference would throw `bad_cast`, which no
// handler of the port can take: it ends the program.
uint64_t ra_x86_rt_dynamic_cast(uint64_t object, uint64_t vf_delta, uint64_t source, uint64_t target, uint64_t is_reference) {
    if (!object) return 0;
    uint32_t complete = rtti32_complete((uint32_t)object);
    int32_t source_offset = (int32_t)((uint32_t)object - (uint32_t)vf_delta - complete);
    uint32_t hierarchy = rtti32_u32(rtti32_locator((uint32_t)object) + 16);
    uint32_t bases = rtti32_u32(hierarchy + 8), array = rtti32_u32(hierarchy + 12);
    uint32_t found = 0, cross = 0, source_base = 0;
    int32_t found_offset = 0;
    int ambiguous = 0;
    for (uint32_t i = 0; i < bases && !ambiguous; i++) {
        uint32_t base = rtti32_u32(array + 4 * i);
        int is_source = rtti32_same_type(rtti32_u32(base), (uint32_t)source) && rtti32_base_offset(complete, base + 8) == source_offset;
        if (is_source && !source_base) source_base = base;
        if (!rtti32_same_type(rtti32_u32(base), (uint32_t)target)) continue;
        if (!cross) cross = base;
        uint32_t contained = rtti32_u32(base + 4);
        for (uint32_t j = 1; j <= contained && i + j < bases; j++) {
            uint32_t inner = rtti32_u32(array + 4 * (i + j));
            if (!rtti32_same_type(rtti32_u32(inner), (uint32_t)source) || rtti32_base_offset(complete, inner + 8) != source_offset) continue;
            // BCD_NOTVISIBLE: the source is a private base of this target.
            if (rtti32_u32(inner + 20) & 1) continue;
            int32_t offset = rtti32_base_offset(complete, base + 8);
            if (found && offset != found_offset) ambiguous = 1;
            found = base;
            found_offset = offset;
        }
    }
    uint32_t result = 0;
    if (ambiguous) {
        result = 0;
    } else if (found) {
        result = complete + (uint32_t)found_offset;
    } else if (cross && !(rtti32_u32(cross + 20) & 3) && source_base && !(rtti32_u32(source_base + 20) & 4)) {
        // BCD_NOTVISIBLE | BCD_AMBIGUOUS on the target, BCD_PRIVORPROTINCOMPOBJ on the source.
        result = complete + (uint32_t)rtti32_base_offset(complete, cross + 8);
    }
    if (!result && is_reference) {
        fprintf(stderr, "ra: a dynamic_cast of a reference failed\n");
        fflush(stderr);
        host_exit(0xE0000006);
    }
    return result;
}

uint64_t ra_x86_rt_cast_to_void(uint64_t object) {
    return object ? rtti32_complete((uint32_t)object) : 0;
}

// `typeid` of an object: its complete object's type descriptor, which is the program's own.
uint64_t ra_x86_rt_typeid(uint64_t object) {
    if (!object) {
        fprintf(stderr, "ra: typeid of a null pointer\n");
        fflush(stderr);
        host_exit(0xE0000006);
    }
    return rtti32_u32(rtti32_locator((uint32_t)object) + 12);
}

// `type_info::name()`: the host's `__std_type_info_name` reads the cached undecorated name in an
// eight-byte slot before the decorated name, where the program's descriptor keeps a dword, and
// would take the dword and the name's first bytes for a cached pointer. The host undecorates a copy
// of the descriptor in its own layout, and the name it allocates lies under 4 GB (the port is not
// large-address-aware), so the program's dword caches it and a descriptor is undecorated once.
#ifdef _WIN32
struct ra_std_type_info_data64 {
    const char *undecorated;
    char decorated[1];
};
// The list of the names the host allocated: an interlocked list's header, sixteen bytes on a
// sixteen-byte boundary, which the host's push faults on anywhere else.
struct ra_type_info_node64 {
    SLIST_HEADER header;
};
const char *__cdecl __std_type_info_name(struct ra_std_type_info_data64 *data, struct ra_type_info_node64 *root);
#endif

// `type_info::operator==` and `hash_code()` of a 32-bit program: the decorated name follows the
// dword of the cached name, and the comparison and the hash skip its first character, as the C++
// runtime's do. The hash is the 32-bit runtime's (FNV-1a over the name).
uint64_t ra_x86_std_type_info_compare(uint64_t left, uint64_t right) {
    if (left == right) return 0;
    return (uint32_t)strcmp((const char *)(uintptr_t)(left + 5), (const char *)(uintptr_t)(right + 5));
}

uint64_t ra_x86_std_type_info_hash(uint64_t data) {
    uint32_t hash = 2166136261u;
    for (const unsigned char *p = (const unsigned char *)(uintptr_t)(data + 5); *p; p++) hash = (hash ^ *p) * 16777619u;
    return hash;
}

uint64_t ra_x86_std_type_info_name(uint64_t data, uint64_t root) {
    (void)root;
    uint32_t *cached = (uint32_t *)(uintptr_t)data;
    if (*cached) return *cached;
#ifdef _WIN32
    const char *decorated = (const char *)(uintptr_t)(data + 4);
    size_t size = strlen(decorated) + 1;
    struct ra_std_type_info_data64 *copy = malloc(sizeof(const char *) + size);
    if (!copy) abort();
    copy->undecorated = NULL;
    memcpy(copy->decorated, decorated, size);
    static struct ra_type_info_node64 node;
    *cached = (uint32_t)(uintptr_t)__std_type_info_name(copy, &node);
    return *cached;
#else
    fprintf(stderr, "ra: type_info::name() needs the host's C++ runtime to undecorate a name\n");
    fflush(stderr);
    host_exit(0xE0000006);
    return 0;
#endif
}

// The structures a window program hands user32 are laid out at the program's pointer width:
// WNDCLASSA is 40 bytes on x86 and 72 on the host, WNDCLASSEXA 48 and 80, MSG 28 and 48,
// PAINTSTRUCT 64 and 72. Each adapter widens the program's record into the host's, calls, and
// narrows what came back. The window procedure the class names is a universal entry point, which
// user32 calls through an adapter of its own (`wndproc32_call`): a message whose lParam points to a
// structure of handles goes to the program as a copy in its own layout and comes back from it.
// The narrowing of a handle or an address to a dword rests on the port's address space: a 32-bit
// port is linked without large-address awareness, so every address its process allocates,
// user32's stacks and heaps included, lies below 2 GB, and a window or GDI handle is 32 bits
// significant on every Windows; only a pointer into a system DLL's own image would not fit, and no
// message carries one.
#ifdef _WIN32
#ifdef _MSC_VER
#define RA_THREAD32 __declspec(thread)
#else
#define RA_THREAD32 _Thread_local
#endif

// A value the host hands the program as a dword, which stops the program when it does not fit.
static uint32_t dword32_of(uint64_t value, const char *what) {
    if (value >> 32 && (value >> 31) != 0x1ffffffffULL) ra_adapt_failed(what, value);
    return (uint32_t)value;
}

// The host's value for the program's dword: the host's own when the program kept it, so a handle
// keeps its upper half, and else the dword sign-extended, as WoW64 extends a handle.
static uint64_t wide32_of(uint32_t guest, uint64_t host) {
    return (uint32_t)host == guest ? host : (uint64_t)(int64_t)(int32_t)guest;
}

// A handle the program passes the host: INVALID_HANDLE_VALUE and the pseudo-handles are negative.
static HANDLE handle32(uint64_t value) {
    return (HANDLE)(intptr_t)(int32_t)(uint32_t)value;
}

// The timer queue writes the new timer's handle into a HANDLE of the program's, a dword; its
// callback is the program's universal entry, which takes the parameter and the flag in registers.
uint64_t ra_x86_create_timer_queue_timer(uint64_t timer, uint64_t queue, uint64_t callback, uint64_t parameter, uint64_t due,
                                         uint64_t period, uint64_t flags) {
    HANDLE made = NULL;
    BOOL ok = CreateTimerQueueTimer(&made, queue ? handle32(queue) : NULL, (WAITORTIMERCALLBACK)(uintptr_t)(uint32_t)callback,
                                    (PVOID)(uintptr_t)(uint32_t)parameter, (DWORD)due, (DWORD)period, (ULONG)flags);
    if (ok) *(uint32_t *)(uintptr_t)timer = dword32_of((uint64_t)(uintptr_t)made, "the host's timer handle does not fit in 4 bytes");
    return (uint32_t)ok;
}

uint64_t ra_x86_delete_timer_queue_timer(uint64_t queue, uint64_t timer, uint64_t event) {
    return (uint32_t)DeleteTimerQueueTimer(queue ? handle32(queue) : NULL, handle32(timer), event ? handle32(event) : NULL);
}

// InitOnceExecuteOnce over the program's INIT_ONCE, a dword the host's would write 8 bytes into:
// done is bit 1, the context above INIT_ONCE_CTX_RESERVED_BITS as Windows keeps it. The callback
// is the program's universal entry, called with the host's convention; it writes a dword context.
typedef BOOL(WINAPI *ra_init_once_fn32)(PVOID once, PVOID parameter, PVOID context);

uint64_t ra_x86_init_once_execute_once(uint64_t once, uint64_t fn, uint64_t parameter, uint64_t context) {
    uint32_t *state = (uint32_t *)(uintptr_t)(uint32_t)once;
    if (!(*state & 2)) {
        uint32_t made = 0;
        uint32_t *out = context ? (uint32_t *)(uintptr_t)(uint32_t)context : &made;
        ra_init_once_fn32 callback = (ra_init_once_fn32)(uintptr_t)(uint32_t)fn;
        if (!callback((PVOID)(uintptr_t)(uint32_t)once, (PVOID)(uintptr_t)(uint32_t)parameter, out)) return 0;
        *state = (*out & ~3u) | 2;
    }
    if (context) *(uint32_t *)(uintptr_t)(uint32_t)context = *state & ~3u;
    return 1;
}

// A CRITICAL_SECTION is 24 bytes to the program and 40 to the host, and its lock state must be one
// place every thread sees, which a copy per call is not: the runtime keeps a host section per
// program section and tags the program's 24 bytes with its slot.
#define RA_CS32_SLOTS 256
#define RA_CS32_TAG 0x52430000u
static CRITICAL_SECTION ra_cs32[RA_CS32_SLOTS];
static volatile LONG ra_cs32_used;

static CRITICAL_SECTION *cs32(uint64_t program) {
    uint32_t tag = *(const uint32_t *)(uintptr_t)(uint32_t)program;
    if ((tag & 0xffff0000u) != RA_CS32_TAG || (tag & 0xffffu) >= RA_CS32_SLOTS) ra_adapt_failed("a critical section the runtime did not initialize", tag);
    return &ra_cs32[tag & 0xffffu];
}

uint64_t ra_x86_initialize_critical_section(uint64_t program) {
    LONG slot = InterlockedIncrement(&ra_cs32_used) - 1;
    if (slot >= RA_CS32_SLOTS) ra_adapt_failed("more critical sections than the runtime keeps", (uint64_t)slot);
    InitializeCriticalSection(&ra_cs32[slot]);
    memset((void *)(uintptr_t)(uint32_t)program, 0, 24);
    *(uint32_t *)(uintptr_t)(uint32_t)program = RA_CS32_TAG | (uint32_t)slot;
    return 0;
}

uint64_t ra_x86_initialize_critical_section_and_spin_count(uint64_t program, uint64_t spin) {
    (void)spin;
    ra_x86_initialize_critical_section(program);
    return 1;
}

uint64_t ra_x86_initialize_critical_section_ex(uint64_t program, uint64_t spin, uint64_t flags) {
    (void)spin;
    (void)flags;
    ra_x86_initialize_critical_section(program);
    return 1;
}

uint64_t ra_x86_enter_critical_section(uint64_t program) {
    EnterCriticalSection(cs32(program));
    return 0;
}

uint64_t ra_x86_leave_critical_section(uint64_t program) {
    LeaveCriticalSection(cs32(program));
    return 0;
}

uint64_t ra_x86_try_enter_critical_section(uint64_t program) {
    return (uint32_t)TryEnterCriticalSection(cs32(program));
}

uint64_t ra_x86_delete_critical_section(uint64_t program) {
    DeleteCriticalSection(cs32(program));
    return 0;
}

// WaitForMultipleObjects over the program's array of dword handles, widened as WoW64 widens them.
uint64_t ra_x86_wait_for_multiple_objects(uint64_t count, uint64_t handles, uint64_t all, uint64_t milliseconds) {
    HANDLE wide[MAXIMUM_WAIT_OBJECTS];
    const uint32_t *program = (const uint32_t *)(uintptr_t)(uint32_t)handles;
    DWORD n = (DWORD)count > MAXIMUM_WAIT_OBJECTS ? MAXIMUM_WAIT_OBJECTS : (DWORD)count;
    for (DWORD i = 0; i < n; i++) wide[i] = handle32(program[i]);
    return (uint32_t)WaitForMultipleObjects(n, wide, (BOOL)all, (DWORD)milliseconds);
}

struct ra_createstruct32 {
    uint32_t params, instance, menu, parent;
    int32_t cy, cx, y, x, style;
    uint32_t name, cls, ex_style;
};
struct ra_windowpos32 {
    uint32_t hwnd, after;
    int32_t x, y, cx, cy;
    uint32_t flags;
};
struct ra_nccalcsize32 {
    RECT rects[3];
    uint32_t pos;
};

// A message whose lParam the program reads in its own layout, for as long as its window procedure
// runs: the host's structure and the program's copy, which the copy's union starts. The program's
// DefWindowProcA finds its lParam here and hands user32 the host's structure, brought up to date.
struct message32 {
    union {
        struct ra_createstruct32 create;
        struct ra_windowpos32 pos;
        struct {
            struct ra_nccalcsize32 params;
            struct ra_windowpos32 pos;
        } calc;
    } guest;
    void *host;
    UINT message;
    struct message32 *outer;
};
static RA_THREAD32 struct message32 *messages32;

// Whether the program reads `message`'s lParam as a structure of its layout: CREATESTRUCTA (48
// bytes on x86, 80 on the host) of WM_NCCREATE and WM_CREATE, WINDOWPOS (28 and 40) of a move, and
// NCCALCSIZE_PARAMS (52 and 56) with the WINDOWPOS it points to, of WM_NCCALCSIZE with wParam TRUE
// (with FALSE, a RECT, which is one layout at both widths).
static int message32_converts(UINT message, WPARAM wparam, LPARAM lparam) {
    if (!lparam) return 0;
    switch (message) {
    case WM_NCCREATE:
    case WM_CREATE:
    case WM_WINDOWPOSCHANGING:
    case WM_WINDOWPOSCHANGED: return 1;
    case WM_NCCALCSIZE: return wparam != 0;
    default: return 0;
    }
}

static void windowpos32_narrow(struct ra_windowpos32 *g, const WINDOWPOS *h) {
    g->hwnd = dword32_of((uint64_t)(uintptr_t)h->hwnd, "the host's WINDOWPOS.hwnd does not fit in 4 bytes");
    g->after = dword32_of((uint64_t)(uintptr_t)h->hwndInsertAfter, "the host's WINDOWPOS.hwndInsertAfter does not fit in 4 bytes");
    g->x = h->x;
    g->y = h->y;
    g->cx = h->cx;
    g->cy = h->cy;
    g->flags = h->flags;
}

static void windowpos32_widen(WINDOWPOS *h, const struct ra_windowpos32 *g) {
    h->hwnd = (HWND)(uintptr_t)wide32_of(g->hwnd, (uint64_t)(uintptr_t)h->hwnd);
    h->hwndInsertAfter = (HWND)(uintptr_t)wide32_of(g->after, (uint64_t)(uintptr_t)h->hwndInsertAfter);
    h->x = g->x;
    h->y = g->y;
    h->cx = g->cx;
    h->cy = g->cy;
    h->flags = g->flags;
}

// The host's structure into the program's copy.
static void message32_narrow(struct message32 *m) {
    switch (m->message) {
    case WM_NCCREATE:
    case WM_CREATE: {
        const CREATESTRUCTA *h = m->host;
        struct ra_createstruct32 *g = &m->guest.create;
        g->params = dword32_of((uint64_t)(uintptr_t)h->lpCreateParams, "the host's CREATESTRUCTA.lpCreateParams does not fit in 4 bytes");
        g->instance = dword32_of((uint64_t)(uintptr_t)h->hInstance, "the host's CREATESTRUCTA.hInstance does not fit in 4 bytes");
        g->menu = dword32_of((uint64_t)(uintptr_t)h->hMenu, "the host's CREATESTRUCTA.hMenu does not fit in 4 bytes");
        g->parent = dword32_of((uint64_t)(uintptr_t)h->hwndParent, "the host's CREATESTRUCTA.hwndParent does not fit in 4 bytes");
        g->cy = h->cy;
        g->cx = h->cx;
        g->y = h->y;
        g->x = h->x;
        g->style = h->style;
        g->name = dword32_of((uint64_t)(uintptr_t)h->lpszName, "the host's CREATESTRUCTA.lpszName does not fit in 4 bytes");
        g->cls = dword32_of((uint64_t)(uintptr_t)h->lpszClass, "the host's CREATESTRUCTA.lpszClass does not fit in 4 bytes");
        g->ex_style = h->dwExStyle;
        break;
    }
    case WM_WINDOWPOSCHANGING:
    case WM_WINDOWPOSCHANGED: windowpos32_narrow(&m->guest.pos, m->host); break;
    case WM_NCCALCSIZE: {
        const NCCALCSIZE_PARAMS *h = m->host;
        memcpy(m->guest.calc.params.rects, h->rgrc, sizeof h->rgrc);
        m->guest.calc.params.pos = 0;
        if (h->lppos) {
            windowpos32_narrow(&m->guest.calc.pos, h->lppos);
            m->guest.calc.params.pos = dword32_of((uint64_t)(uintptr_t)&m->guest.calc.pos, "the copy of a WINDOWPOS does not fit in 4 bytes");
        }
        break;
    }
    default: break;
    }
}

// The program's copy, as its procedure left it, back into the host's structure.
static void message32_widen(struct message32 *m) {
    switch (m->message) {
    case WM_NCCREATE:
    case WM_CREATE: {
        CREATESTRUCTA *h = m->host;
        const struct ra_createstruct32 *g = &m->guest.create;
        h->lpCreateParams = (LPVOID)(uintptr_t)wide32_of(g->params, (uint64_t)(uintptr_t)h->lpCreateParams);
        h->hInstance = (HINSTANCE)(uintptr_t)wide32_of(g->instance, (uint64_t)(uintptr_t)h->hInstance);
        h->hMenu = (HMENU)(uintptr_t)wide32_of(g->menu, (uint64_t)(uintptr_t)h->hMenu);
        h->hwndParent = (HWND)(uintptr_t)wide32_of(g->parent, (uint64_t)(uintptr_t)h->hwndParent);
        h->cy = g->cy;
        h->cx = g->cx;
        h->y = g->y;
        h->x = g->x;
        h->style = g->style;
        h->lpszName = (LPCSTR)(uintptr_t)wide32_of(g->name, (uint64_t)(uintptr_t)h->lpszName);
        h->lpszClass = (LPCSTR)(uintptr_t)wide32_of(g->cls, (uint64_t)(uintptr_t)h->lpszClass);
        h->dwExStyle = g->ex_style;
        break;
    }
    case WM_WINDOWPOSCHANGING:
    case WM_WINDOWPOSCHANGED: windowpos32_widen(m->host, &m->guest.pos); break;
    case WM_NCCALCSIZE: {
        NCCALCSIZE_PARAMS *h = m->host;
        memcpy(h->rgrc, m->guest.calc.params.rects, sizeof h->rgrc);
        if (h->lppos && m->guest.calc.params.pos) windowpos32_widen(h->lppos, &m->guest.calc.pos);
        break;
    }
    default: break;
    }
}

// The program's window procedures, one adapter each that user32 calls: the arguments narrow to the
// program's dwords, and its LRESULT, a dword, sign-extends as WoW64 extends it.
enum { WNDPROC32_SLOTS = 8 };
static uint32_t wndprocs32[WNDPROC32_SLOTS];

static LRESULT wndproc32_call(unsigned slot, HWND hwnd, UINT message, WPARAM wparam, LPARAM lparam) {
    native8 procedure = (native8)(uintptr_t)wndprocs32[slot];
    uint64_t window = (uint64_t)(uintptr_t)hwnd;
    if (!message32_converts(message, wparam, lparam)) return (LRESULT)(int32_t)procedure(window, message, wparam, (uint64_t)lparam, 0, 0, 0, 0);
    struct message32 m;
    memset(&m, 0, sizeof m);
    m.host = (void *)lparam;
    m.message = message;
    m.outer = messages32;
    message32_narrow(&m);
    uint64_t guest = dword32_of((uint64_t)(uintptr_t)&m.guest, "the copy of a message's structure does not fit in 4 bytes");
    messages32 = &m;
    uint64_t result = procedure(window, message, wparam, guest, 0, 0, 0, 0);
    messages32 = m.outer;
    message32_widen(&m);
    return (LRESULT)(int32_t)result;
}

#define RA_WNDPROC32(n) \
    static LRESULT CALLBACK wndproc32_##n(HWND hwnd, UINT message, WPARAM wparam, LPARAM lparam) { \
        return wndproc32_call(n, hwnd, message, wparam, lparam); \
    }
RA_WNDPROC32(0)
RA_WNDPROC32(1)
RA_WNDPROC32(2)
RA_WNDPROC32(3)
RA_WNDPROC32(4)
RA_WNDPROC32(5)
RA_WNDPROC32(6)
RA_WNDPROC32(7)
static const WNDPROC wndproc32_adapters[WNDPROC32_SLOTS] = {wndproc32_0, wndproc32_1, wndproc32_2, wndproc32_3,
                                                             wndproc32_4, wndproc32_5, wndproc32_6, wndproc32_7};

// The adapter user32 calls for the program's procedure: the slot that holds it, or the first free.
static WNDPROC wndproc32_of(uint32_t procedure) {
    if (!procedure) return NULL;
    for (unsigned i = 0; i < WNDPROC32_SLOTS; i++) {
        uint32_t held = 0;
        if (__atomic_compare_exchange_n(&wndprocs32[i], &held, procedure, 0, __ATOMIC_SEQ_CST, __ATOMIC_SEQ_CST) || held == procedure) {
            return wndproc32_adapters[i];
        }
    }
    ra_adapt_failed("the program registers more window procedures than the adapters", procedure);
}

// DefWindowProcA on the lParam a window procedure was handed: user32 reads the host's structure,
// with what the program changed in its copy, and the copy reads what user32 changed.
uint64_t ra_x86_def_window_proc_a(uint64_t hwnd, uint64_t message, uint64_t wparam, uint64_t lparam) {
    for (struct message32 *m = messages32; m; m = m->outer) {
        if (m->message != (UINT)message || (uint32_t)lparam != (uint32_t)(uintptr_t)&m->guest) continue;
        message32_widen(m);
        LRESULT r = DefWindowProcA((HWND)(uintptr_t)hwnd, (UINT)message, (WPARAM)wparam, (LPARAM)m->host);
        message32_narrow(m);
        return (uint64_t)r;
    }
    return (uint64_t)DefWindowProcA((HWND)(uintptr_t)hwnd, (UINT)message, (WPARAM)wparam, (LPARAM)lparam);
}

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
                    wndproc32_of(n->proc),
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
                      wndproc32_of(n->proc),
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
extern _Thread_local uint16_t ra_x87_cw;
extern _Thread_local uint16_t ra_x87_sw;

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
// The words are the Microsoft runtime's: a Windows host answers them, and on another host the C99
// environment, whose rounding modes and exception bits are the x87's own fields, stands in.
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
    unsigned x87 = (ra_crt_control(ra_x87_cw) & ~(unsigned)mask) | ((unsigned)new_bits & (unsigned)mask);
#ifdef _WIN32
    _controlfp_s(&host, (unsigned)new_bits & ~(unsigned)(RA_MCW_PC | RA_MCW_IC), (unsigned)mask & ~(unsigned)(RA_MCW_PC | RA_MCW_IC));
#else
    // The C99 rounding modes are the x87's field in place (FE_DOWNWARD 0x400, ...).
    if (mask & RA_MCW_RC) fesetround((int)((x87 & RA_MCW_RC) << 2));
#endif
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

// The host's raised exceptions in the C runtime's bits. The C99 bits off Windows are the x87's
// order (invalid 1, denormal 2, zero divide 4, overflow 8, underflow 0x10, inexact 0x20).
static unsigned ra_host_raised(unsigned crt) {
#ifdef _WIN32
    return (unsigned)fetestexcept((int)crt);
#else
    return ra_crt_flags((unsigned)fetestexcept((int)ra_x87_flags(crt))) & crt;
#endif
}

static void ra_host_clear(unsigned crt) {
#ifdef _WIN32
    feclearexcept((int)crt);
#else
    feclearexcept((int)ra_x87_flags(crt));
#endif
}

uint64_t ra_x86_statusfp(void) {
#ifdef _WIN32
    unsigned host = _statusfp();
#else
    unsigned host = ra_host_raised(0x8001f);
#endif
    return host | ra_crt_flags(ra_x87_sw & 0x3f);
}

uint64_t ra_x86_clearfp(void) {
    unsigned flags = (unsigned)ra_x86_statusfp();
#ifdef _WIN32
    _clearfp();
#else
    ra_host_clear(0x8001f);
#endif
    ra_x87_sw &= (uint16_t)~0xffu;
    return flags;
}

uint64_t ra_x86_fetestexcept(uint64_t excepts) {
    return (ra_host_raised((unsigned)excepts) | ra_crt_flags(ra_x87_sw & 0x3f)) & (unsigned)excepts;
}

uint64_t ra_x86_feclearexcept(uint64_t excepts) {
    ra_x87_sw &= (uint16_t)~ra_x87_flags((unsigned)excepts);
    ra_host_clear((unsigned)excepts);
    return 0;
}

// The x86 C runtime's square root is the x87's: the result rounded to the precision control, so
// a program that set 24 bits (Direct3D's default) finds a float's mantissa in its double. The
// double's root rounded to a float is the float's root: 53 bits are enough for the second
// rounding to be harmless.
double ra_x86_sqrt(double x) {
    double r = sqrt(x);
    return ((ra_x87_cw >> 8) & 3) == 0 ? (double)(float)r : r;
}

// The x86 C runtime's SSE2 math (`_libm_sse2_pow_precise` and the rest, the operands in xmm0 and
// xmm1), which the 64-bit runtime does not export: its own are the same precise algorithms.
double ra_x86_pow_sse2(double x, double y) { return pow(x, y); }
double ra_x86_sqrt_sse2(double x) { return sqrt(x); }
double ra_x86_sin_sse2(double x) { return sin(x); }
double ra_x86_cos_sse2(double x) { return cos(x); }
double ra_x86_tan_sse2(double x) { return tan(x); }
double ra_x86_asin_sse2(double x) { return asin(x); }
double ra_x86_acos_sse2(double x) { return acos(x); }
double ra_x86_atan_sse2(double x) { return atan(x); }

// The x86 C runtime's x87 `_CIatan2`, which takes its operands on the x87 stack (st0 the x, st1
// the y) and leaves atan2(y, x) in their place: here on the lift's stack of doubles, which a
// program lifted with the double unit keeps in `ra_x87_stack` from `ra_x87_top` up.
extern _Thread_local double ra_x87_stack[8];
extern _Thread_local uint64_t ra_x87_top;
void ra_x86_ci_atan2(void) {
    uint64_t top = ra_x87_top & 7;
    double x = ra_x87_stack[top], y = ra_x87_stack[(top + 1) & 7];
    ra_x87_top = (top + 1) & 7;
    ra_x87_stack[ra_x87_top] = atan2(y, x);
}
// `_CIfmod`, the same way: st0 the divisor, st1 the dividend, fmod(st1, st0) in their place.
void ra_x86_ci_fmod(void) {
    uint64_t top = ra_x87_top & 7;
    double y = ra_x87_stack[top], x = ra_x87_stack[(top + 1) & 7];
    ra_x87_top = (top + 1) & 7;
    ra_x87_stack[ra_x87_top] = fmod(x, y);
}
double ra_x86_exp_sse2(double x) { return exp(x); }
double ra_x86_log_sse2(double x) { return log(x); }
double ra_x86_log10_sse2(double x) { return log10(x); }

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

// fscanf's: a program reading a file with `fscanf(f, "%79s %99[^\n]\n", ...)` passes pointers
// that are dwords, which the host's would read as 8-byte slots.
uint64_t ra_x86_vfscanf(uint64_t options, uint64_t stream, uint64_t format, uint64_t locale, uint64_t args) {
    uint64_t slots[64] = {0};
    widen_scanf_args((const char *)(uintptr_t)format, (const uint32_t *)(uintptr_t)args, slots, 64);
    return (uint64_t)(int64_t)__stdio_common_vfscanf(options, (FILE *)(uintptr_t)stream, (const char *)(uintptr_t)format,
                                                     (_locale_t)(uintptr_t)locale, (va_list)slots);
}

// The addresses of a FILE's buffer fields, which the C++ runtime's filebuf reads the stream
// through: the host writes three pointers of eight bytes where the program's locals are dwords, so
// they go to locals here and their low halves to the program's. The fields themselves are the
// host's, below 4 GB like the rest of the port, and the program reads them as dwords.
uint64_t ra_x86_get_stream_buffer_pointers(uint64_t stream, uint64_t base, uint64_t pointer, uint64_t count) {
    char **b = NULL;
    char **p = NULL;
    int *c = NULL;
    errno_t r = _get_stream_buffer_pointers((FILE *)(uintptr_t)stream, &b, &p, &c);
    if (base) *(uint32_t *)(uintptr_t)base = (uint32_t)(uintptr_t)b;
    if (pointer) *(uint32_t *)(uintptr_t)pointer = (uint32_t)(uintptr_t)p;
    if (count) *(uint32_t *)(uintptr_t)count = (uint32_t)(uintptr_t)c;
    return (uint64_t)(int64_t)r;
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

// argv and the environment as dword pointers to the host's strings, which sit below 4 GB. A POSIX
// kernel hands them over at the top of the address space: the port copies them to its heap, which
// the Win32 layer keeps low.
static int ra_argc;
static char **ra_argv;
static uint32_t *argv32;
static uint32_t argv32_address;
static uint32_t *environment32;

#ifdef _WIN32
#define ra_low_string(s) (s)
#else
#define ra_low_string(s) strdup(s)
#endif

// `WinMain`'s command line: the arguments past the program's name, joined and quoted where they
// hold a space, on the heap the Win32 layer keeps low.
uint64_t ra_win_main_command_line(void) {
    static char *line;
    if (!line) {
        size_t size = 1;
        for (int i = 1; i < ra_argc; i++) size += strlen(ra_argv[i]) + 3;
        line = (char *)calloc(size, 1);
        char *p = line;
        for (int i = 1; i < ra_argc; i++) {
            int quote = strchr(ra_argv[i], ' ') != NULL;
            size_t length = strlen(ra_argv[i]);
            if (i > 1) *p++ = ' ';
            if (quote) *p++ = '"';
            memcpy(p, ra_argv[i], length);
            p += length;
            if (quote) *p++ = '"';
        }
    }
    return (uint64_t)(uintptr_t)line;
}

uint64_t ra_x86_p_argv(void) {
    if (!argv32) {
        argv32 = (uint32_t *)calloc((size_t)ra_argc + 1, sizeof(uint32_t));
        for (int i = 0; i < ra_argc; i++) argv32[i] = (uint32_t)(uintptr_t)ra_low_string(ra_argv[i]);
        argv32_address = (uint32_t)(uintptr_t)argv32;
    }
    return (uint64_t)(uintptr_t)&argv32_address;
}

static uint32_t commode32, fmode32, argc32, errno32;

uint64_t ra_x86_p_commode(void) {
    return (uint64_t)(uintptr_t)&commode32;
}

uint64_t ra_x86_p_fmode(void) {
    return (uint64_t)(uintptr_t)&fmode32;
}

uint64_t ra_x86_p_argc(void) {
    argc32 = (uint32_t)ra_argc;
    return (uint64_t)(uintptr_t)&argc32;
}

uint64_t ra_x86_errno(void) {
    return (uint64_t)(uintptr_t)&errno32;
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
        for (size_t i = 0; i < count; i++) environment32[i] = (uint32_t)(uintptr_t)ra_low_string(environ[i]);
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
    report_entered();
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
    // The copy constructor of each catchable type, zero for a type copied by its bytes (x86).
    uint64_t copies[16];
    uint64_t count;
    // The destructor the end of the exception runs on the object, zero for none (x86), and whether
    // ra_eh_ending_destructor handed it out: the port runs it after ra_eh_leave_catch, on the object.
    uint64_t destructor;
    int destroyed;
    int holds;
};

static struct ra_exception *eh_in_flight, *eh_staged;
static struct ra_exception *eh_caught[64], *eh_suspended[64];
static int eh_caught_depth, eh_suspended_depth;
// Beside each catch being run, the esp the x86 frame handler saved under the registration when the
// catch began, which an unwind out of the catch puts back.
static uint64_t eh_catch_sp[64];

_Noreturn static void eh_fatal(const char *what) {
    fprintf(stderr, "ra: %s\n", what);
    fflush(stderr);
    host_exit(0xE0000006);
}

static void eh_release(struct ra_exception *e) {
    if (--e->holds == 0) {
        // An object whose destructor the port runs after the catch ends outlives the exception: the
        // protocol has no call after the destructor, so its bytes stay allocated.
        if (!e->destroyed) free(e->object);
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
    e->copies[e->count] = 0;
    e->count++;
}

void ra_eh_throw_copy(uint64_t copy) {
    struct ra_exception *e = eh_in_flight;
    if (!e || e->count == 0) eh_fatal("a copy constructor with no catchable type to belong to");
    e->copies[e->count - 1] = copy;
}

// The throw names the destructor by its image address, which the port's call through ra_call at
// the ABI level reaches only as the function's entry in the table the lift wrote: an indirect call
// there is a native one, as the interpreter's alias of the table makes it.
void ra_eh_throw_destructor(uint64_t destructor) {
    struct ra_exception *e = eh_in_flight;
    if (!e) eh_fatal("a destructor with no throw to belong to");
#ifdef RA_ABI
    uint64_t image = destructor;
    destructor = 0;
    for (uint64_t i = 0; i < ra_function_count; i++) {
        if (ra_functions[i].address == image) {
            destructor = (uint64_t)(uintptr_t)ra_functions[i].function;
            break;
        }
    }
    if (!destructor) {
        fprintf(stderr, "ra: the destructor of a thrown object at 0x%llx is no lifted function\n", (unsigned long long)image);
        eh_fatal("a thrown object's destructor cannot be called");
    }
#endif
    e->destructor = destructor;
}

// The exception of the catch that ends, and whether something else still holds it: in flight again,
// staged for a catch, or held by a catch or an unwind further out.
static struct ra_exception *eh_ending(int *held) {
    if (eh_caught_depth == 0) eh_fatal("the end of a catch body with no exception caught");
    struct ra_exception *e = eh_caught[eh_caught_depth - 1];
    *held = eh_in_flight == e || eh_staged == e;
    for (int i = 0; i < eh_caught_depth - 1; i++) *held |= eh_caught[i] == e;
    for (int i = 0; i < eh_suspended_depth; i++) *held |= eh_suspended[i] == e;
    return e;
}

uint64_t ra_eh_ending_object(void) {
    int held;
    return (uint64_t)(uintptr_t)eh_ending(&held)->object;
}

uint64_t ra_eh_ending_destructor(void) {
    int held;
    struct ra_exception *e = eh_ending(&held);
    if (held || !e->destructor) return 0;
    e->destroyed = 1;
    return e->destructor;
}

void ra_eh_catch_sp(uint64_t sp) {
    if (eh_caught_depth == 0) eh_fatal("a saved esp with no catch being run");
    eh_catch_sp[eh_caught_depth - 1] = sp;
}

uint64_t ra_eh_ending_sp(void) {
    if (eh_caught_depth == 0) eh_fatal("the end of a catch body with no exception caught");
    return eh_catch_sp[eh_caught_depth - 1];
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
        if (slot && !(flags & 8) && e->copies[i]) eh_fatal("a catch by value of an object with a copy constructor is not ported yet");
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
    eh_catch_sp[eh_caught_depth] = 0;
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

// The structured exception in flight, in the 32-bit layout the program reads: EXCEPTION_RECORD
// (code, flags, record, address, parameters, information) and EXCEPTION_POINTERS (the record, no
// context: a filter that reads one faults rather than reads made-up registers).
static uint32_t seh_record[20], seh_pointers[2];
static int seh_in_flight;

// Whether the bytes [address, end) can be read, or written with `write`, without a fault: the
// host's answer of its mappings, asked before the access rather than caught after it.
static int seh_accessible(uint64_t address, uint64_t end, uint64_t write) {
#ifdef _WIN32
    MEMORY_BASIC_INFORMATION info;
    DWORD readable = PAGE_READONLY | PAGE_READWRITE | PAGE_WRITECOPY | PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY;
    DWORD writable = PAGE_READWRITE | PAGE_WRITECOPY | PAGE_EXECUTE_READWRITE | PAGE_EXECUTE_WRITECOPY;
    int fits = 1;
    for (uint64_t at = address; fits && at < end;) {
        fits = VirtualQuery((const void *)(uintptr_t)at, &info, sizeof info) == sizeof info && info.State == MEM_COMMIT
            && (info.Protect & (write ? writable : readable)) && !(info.Protect & PAGE_GUARD);
        at = (uint64_t)(uintptr_t)info.BaseAddress + info.RegionSize;
    }
    return fits;
#else
    // The kernel's list of the mappings, in ascending order: a gap or a mapping without the access
    // ends the walk short of `end`.
    FILE *maps = fopen("/proc/self/maps", "r");
    if (!maps) eh_fatal("a protected access with no /proc/self/maps to check it against");
    unsigned long long low, high;
    char perms[5];
    uint64_t at = address;
    while (at < end && fscanf(maps, "%llx-%llx %4s%*[^\n]", &low, &high, perms) == 3) {
        if (at >= high) continue;
        if (at < low || perms[0] != 'r' || (write && perms[1] != 'w')) break;
        at = high;
    }
    fclose(maps);
    return at >= end;
#endif
}

uint64_t ra_eh_probe(uint64_t address, uint64_t size, uint64_t write, uint64_t site) {
    if (seh_accessible(address, address + (size ? size : 1), write)) return 0;
    if (seh_in_flight) eh_fatal("a fault while a structured exception is in flight");
    memset(seh_record, 0, sizeof seh_record);
    seh_record[0] = 0xC0000005u;
    seh_record[3] = (uint32_t)site;
    seh_record[4] = 2;
    seh_record[5] = write ? 1 : 0;
    seh_record[6] = (uint32_t)address;
    seh_pointers[0] = (uint32_t)(uintptr_t)seh_record;
    seh_pointers[1] = 0;
    seh_in_flight = 1;
    return 1;
}

void ra_eh_raise(uint64_t code, uint64_t flags, uint64_t count, uint64_t arguments, uint64_t site) {
    if (seh_in_flight) eh_fatal("RaiseException while a structured exception is in flight");
    memset(seh_record, 0, sizeof seh_record);
    seh_record[0] = (uint32_t)code;
    seh_record[1] = (uint32_t)flags;
    seh_record[3] = (uint32_t)site;
    if (count > 15) count = 15;
    seh_record[4] = (uint32_t)count;
    for (uint64_t n = 0; n < count; n++) seh_record[5 + n] = ((const uint32_t *)(uintptr_t)arguments)[n];
    seh_pointers[0] = (uint32_t)(uintptr_t)seh_record;
    seh_pointers[1] = 0;
    seh_in_flight = 1;
}

uint64_t ra_eh_pointers(void) {
    if (!seh_in_flight) eh_fatal("a filter with no structured exception in flight");
    return (uint64_t)(uintptr_t)seh_pointers;
}

void ra_eh_handled(void) {
    if (!seh_in_flight) eh_fatal("a handler with no structured exception in flight");
    seh_in_flight = 0;
}

void ra_eh_unhandled(uint64_t site) {
    // What the program printed before the fault is its output, as the native crash leaves it.
    fflush(stdout);
    fprintf(stderr, "ra: the exception %#x at %#llx passes a frame the port does not unwind\n", seh_record[0], (unsigned long long)site);
    fflush(stderr);
    host_exit(seh_record[0]);
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
extern _Thread_local uint64_t ra_pops;
static uint64_t *import_addresses;

// The trampolines a 32-bit program reaches a host function through, whose address does not fit
// the program's dword: the import slots' (see bind_imports) and what the program's GetProcAddress
// hands out. Each is `mov rax, imm64; jmp rax` and then, never executed, the bytes a call through
// it pops as an int32 (-1 when the binding table says), 16 bytes in a region below 4 GB with room
// for every import and the functions the program asks for besides.
#define TRAMPOLINE_BYTES 16
#define TRAMPOLINE_COUNT 16384
static uint8_t *slot_trampolines;
static size_t slot_trampolines_used;

// The trampoline of ours at `target`, or NULL.
static uint8_t *trampoline_at(uint64_t target) {
    uint8_t *p = (uint8_t *)(uintptr_t)target;
    if (!slot_trampolines || p < slot_trampolines || p >= slot_trampolines + slot_trampolines_used * TRAMPOLINE_BYTES) return NULL;
    return (size_t)(p - slot_trampolines) % TRAMPOLINE_BYTES == 0 ? p : NULL;
}

// The host function a trampoline of ours jumps to, or `target` itself.
static uint64_t through_trampoline(uint64_t target) {
    uint8_t *p = trampoline_at(target);
    if (!p) return target;
    uint64_t host;
    memcpy(&host, p + 2, 8);
    return host;
}

#ifdef _WIN32
// A trampoline below 4 GB to `f` that pops `pops`: the one handed out before for both, else a new one.
static uint32_t trampoline(FARPROC f, int32_t pops) {
    if (!slot_trampolines) {
        for (uintptr_t base = 0x10000000; base < 0x80000000 && !slot_trampolines; base += 0x01000000) {
            slot_trampolines = VirtualAlloc((void *)base, TRAMPOLINE_BYTES * TRAMPOLINE_COUNT, MEM_RESERVE | MEM_COMMIT, PAGE_EXECUTE_READWRITE);
        }
        if (!slot_trampolines) {
            fprintf(stderr, "ra: no room below 4 GB for the import trampolines\n");
            host_exit(0xE0000004);
        }
    }
    uint64_t host = (uint64_t)(uintptr_t)f;
    for (size_t i = 0; i < slot_trampolines_used; i++) {
        uint8_t *p = slot_trampolines + i * TRAMPOLINE_BYTES;
        uint64_t to;
        int32_t popped;
        memcpy(&to, p + 2, 8);
        memcpy(&popped, p + 12, 4);
        if (to == host && popped == pops) return (uint32_t)(uintptr_t)p;
    }
    if (slot_trampolines_used == TRAMPOLINE_COUNT) {
        fprintf(stderr, "ra: all %d trampolines below 4 GB are handed out\n", TRAMPOLINE_COUNT);
        host_exit(0xE0000004);
    }
    uint8_t *p = slot_trampolines + slot_trampolines_used * TRAMPOLINE_BYTES;
    slot_trampolines_used++;
    p[0] = 0x48;
    p[1] = 0xB8;
    memcpy(p + 2, &host, 8);
    p[10] = 0xFF;
    p[11] = 0xE0;
    memcpy(p + 12, &pops, 4);
    return (uint32_t)(uintptr_t)p;
}
#endif

static int64_t import_pops(uint64_t target) {
    // A function the program asked GetProcAddress for carries its pops in its trampoline.
    uint8_t *own = trampoline_at(target);
    if (own) {
        int32_t pops;
        memcpy(&pops, own + 12, 4);
        if (pops >= 0) return pops;
    }
    target = through_trampoline(target);
    size_t count = 0;
    while (ra_import_pops[3 * count]) count++;
    if (!import_addresses) {
        import_addresses = calloc(count + 1, sizeof(uint64_t));
        // A DLL's own addresses are Windows' to give; elsewhere the Win32 layer links each import
        // by its symbol, which the table's first word already is.
#ifdef _WIN32
        for (size_t i = 0; i < count; i++) {
            const char *dll = (const char *)(uintptr_t)ra_import_pops[3 * i + 1];
            if (!dll) continue;
            const char *name = dll + strlen(dll) + 1;
            HMODULE module = LoadLibraryA(dll);
            if (!module) continue;
            FARPROC f = name[0] == '#' ? GetProcAddress(module, MAKEINTRESOURCEA(atoi(name + 1))) : GetProcAddress(module, name);
            import_addresses[i] = (uint64_t)(uintptr_t)f;
        }
#endif
    }
    for (size_t i = 0; i < count; i++) {
        if (ra_import_pops[3 * i] == target || (import_addresses[i] && import_addresses[i] == target)) return (int64_t)ra_import_pops[3 * i + 2];
    }
    return -1;
}

#ifdef _WIN32
// The module handles a 32-bit program holds: a host module below 4 GB is its own handle, one above
// is the address of the cell here that keeps it, which the program's GetProcAddress reads back.
#define MODULE_CELLS 256
static HMODULE module_cells[MODULE_CELLS];

static uint64_t module_dword(HMODULE module) {
    if ((uint64_t)(uintptr_t)module <= 0xFFFFFFFFu) return (uint64_t)(uintptr_t)module;
    for (int i = 0; i < MODULE_CELLS; i++) {
        if (!module_cells[i]) module_cells[i] = module;
        if (module_cells[i] == module) return (uint32_t)(uintptr_t)&module_cells[i];
    }
    fprintf(stderr, "ra: more than %d modules above 4 GB\n", MODULE_CELLS);
    host_exit(0xE0000004);
}

static HMODULE module_of_dword(uint64_t handle) {
    uintptr_t at = (uint32_t)handle;
    uintptr_t cells = (uintptr_t)module_cells;
    if (at >= cells && at < cells + sizeof module_cells && (at - cells) % sizeof(HMODULE) == 0) return *(HMODULE *)at;
    return (HMODULE)at;
}

uint64_t ra_x86_get_module_handle_a(uint64_t name) {
    HMODULE module = GetModuleHandleA((const char *)(uintptr_t)name);
    return module ? module_dword(module) : 0;
}

uint64_t ra_x86_get_module_handle_w(uint64_t name) {
    HMODULE module = GetModuleHandleW((const wchar_t *)(uintptr_t)name);
    return module ? module_dword(module) : 0;
}

uint64_t ra_x86_load_library_a(uint64_t name) {
    HMODULE module = LoadLibraryA((const char *)(uintptr_t)name);
    return module ? module_dword(module) : 0;
}

uint64_t ra_x86_load_library_w(uint64_t name) {
    HMODULE module = LoadLibraryW((const wchar_t *)(uintptr_t)name);
    return module ? module_dword(module) : 0;
}

uint64_t ra_x86_load_library_ex_a(uint64_t name, uint64_t file, uint64_t flags) {
    HMODULE module = LoadLibraryExA((const char *)(uintptr_t)name, (HANDLE)(uintptr_t)file, (DWORD)flags);
    return module ? module_dword(module) : 0;
}

uint64_t ra_x86_load_library_ex_w(uint64_t name, uint64_t file, uint64_t flags) {
    HMODULE module = LoadLibraryExW((const wchar_t *)(uintptr_t)name, (HANDLE)(uintptr_t)file, (DWORD)flags);
    return module ? module_dword(module) : 0;
}

// The lift's table of the pops of the functions the program may ask for by name (see the lift's
// `ra_proc_pops`): `name\0` and a little-endian u16 each, then an empty name.
extern const uint8_t ra_proc_pops[];

static int32_t proc_pops(const char *name) {
    for (const uint8_t *p = ra_proc_pops; *p;) {
        size_t n = strlen((const char *)p);
        if (strcmp((const char *)p, name) == 0) return (int32_t)(p[n + 1] | (p[n + 2] << 8));
        p += n + 3;
    }
    return -1;
}

// Whether `module` is a C runtime DLL, whose functions are cdecl, by its file name.
static int is_crt_module(HMODULE module) {
    char path[MAX_PATH];
    DWORD n = GetModuleFileNameA(module, path, sizeof path);
    if (n == 0 || n >= sizeof path) return 0;
    const char *base = path;
    for (const char *p = path; *p; p++) {
        if (*p == '\\' || *p == '/') base = p + 1;
    }
    static const char *const prefixes[] = {"msvcr", "msvcp", "ucrtbase", "api-ms-win-crt", "vcruntime"};
    for (size_t i = 0; i < sizeof prefixes / sizeof prefixes[0]; i++) {
        if (_strnicmp(base, prefixes[i], strlen(prefixes[i])) == 0) return 1;
    }
    return _stricmp(base, "crtdll.dll") == 0;
}

// The function the program asks for, as a trampoline below 4 GB that carries its pops: a C
// runtime's pops nothing, a Windows API function by name pops what the lift's table says, and
// one by ordinal or the table does not know leaves them to the binding table.
uint64_t ra_x86_get_proc_address(uint64_t module, uint64_t name) {
    HMODULE host = module_of_dword(module);
    const char *by = (const char *)(uintptr_t)(uint32_t)name;
    FARPROC f = GetProcAddress(host, by);
    if (!f) return 0;
    int32_t pops = -1;
    if ((uint32_t)name > 0xFFFF) pops = is_crt_module(host) ? 0 : proc_pops(by);
    return trampoline(f, pops);
}
#endif
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

#ifdef RA_RESIDUAL
#if !defined(_WIN32) || !defined(RA_STANDALONE) || RA_POINTER_SIZE != 8
#error "the residual engine runs in the standalone port of a 64-bit Windows program, built on Windows"
#endif
// The residual engine, the static library of ra-asm-residual the port links: code of the image
// that no lifted function has runs there, decoded from the image's bytes, which the port maps at
// the image's base without the right to execute them. A call into them, native from the raised
// code or through ra_call from the machine-level code, enters the engine over the caller's state.
// What the engine entered is printed and written to the facts file when the process ends.
int ra_residual_start(const char *image, const char *facts, void *at, uint64_t size);
int ra_residual_call(void *state, uint64_t address);
void ra_residual_finish(void);

// The program's own module is the image at its base, where the engine reads the code: an address
// it computes from its module handle is one of the image (rhipper.h renames the program's calls).
uint64_t ra_residual_module_a(uint64_t name) {
    return name ? (uint64_t)(uintptr_t)GetModuleHandleA((const char *)(uintptr_t)name) : RA_IMAGE_BASE;
}

uint64_t ra_residual_module_w(uint64_t name) {
    return name ? (uint64_t)(uintptr_t)GetModuleHandleW((const wchar_t *)(uintptr_t)name) : RA_IMAGE_BASE;
}

// The program's ExitProcess: the engine reports before the process ends, which the detach below
// does too for an exit by any other way.
void ra_residual_exit(uint64_t code) {
    ra_residual_finish();
    ExitProcess((UINT)code);
}

// The flags of the state, cf pf af zf sf df of, by their bits in EFLAGS.
static const unsigned residual_flag_bits[7] = {0, 2, 4, 6, 7, 10, 11};

// A native call into the image faults on the first byte it executes: the engine runs the function
// over the registers of the call, and the caller goes on at the return address with what the
// function left. The vector registers are the xmm halves; the upper halves of ymm read as zero.
static LONG WINAPI residual_on_exception(EXCEPTION_POINTERS *info) {
    const EXCEPTION_RECORD *r = info->ExceptionRecord;
    CONTEXT *c = info->ContextRecord;
    uint64_t at = (uint64_t)(uintptr_t)r->ExceptionAddress;
    if (r->ExceptionCode != EXCEPTION_ACCESS_VIOLATION || r->NumberParameters < 2 || r->ExceptionInformation[0] != 8 || at != c->Rip
        || at < RA_IMAGE_BASE || at >= RA_IMAGE_BASE + RA_IMAGE_SIZE) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    // A function the port has lifted, called by its image address: the call's registers and
    // return address are the lifted function's own arguments, as a trampoline would pass them.
    for (uint64_t i = 0; i < ra_function_count; i++) {
        if (ra_functions[i].address == at) {
            c->Rip = (DWORD64)(uintptr_t)ra_functions[i].function;
            return EXCEPTION_CONTINUE_EXECUTION;
        }
    }
    struct State state;
    memset(&state, 0, sizeof state);
    DWORD64 *gpr = &c->Rax;  // rax, rcx, rdx, rbx, rsp, rbp, rsi, rdi, r8..r15, as the state has them
    for (int i = 0; i < 16; i++) state.gpr[i] = gpr[i];
    state.rip = at;
    for (int i = 0; i < 7; i++) state.flags[i] = (uint8_t)((c->EFlags >> residual_flag_bits[i]) & 1);
    for (int i = 0; i < 16; i++) memcpy(state.xmm[i], &c->FltSave.XmmRegisters[i], 16);
    uint64_t back = *(const uint64_t *)(uintptr_t)c->Rsp;  // the return address the call pushed
    if (ra_residual_call(&state, at) != 0) {
        fflush(stderr);
        host_exit(0xE0000003);
    }
    for (int i = 0; i < 16; i++) gpr[i] = state.gpr[i];
    for (int i = 0; i < 7; i++) {
        DWORD bit = (DWORD)1 << residual_flag_bits[i];
        c->EFlags = state.flags[i] ? c->EFlags | bit : c->EFlags & ~bit;
    }
    for (int i = 0; i < 16; i++) memcpy(&c->FltSave.XmmRegisters[i], state.xmm[i], 16);
    c->Rip = back;
    return EXCEPTION_CONTINUE_EXECUTION;
}

// The process ends: the loader calls the module's TLS callbacks with DLL_PROCESS_DETACH, on
// ExitProcess as on a return from main.
static void NTAPI residual_detach(PVOID module, DWORD reason, PVOID reserved) {
    (void)module;
    (void)reserved;
    if (reason == DLL_PROCESS_DETACH) ra_residual_finish();
}
#pragma comment(linker, "/INCLUDE:_tls_used")
__attribute__((used, section(".CRT$XLB"))) static const PIMAGE_TLS_CALLBACK residual_tls = residual_detach;

// The image's bytes at its base, readable and writable but not executable, and the engine over
// them, which RA_RESIDUAL_IMAGE (the reference binary) and RA_RESIDUAL_FACTS name.
static void residual_start(void) {
    void *at = VirtualAlloc((void *)(uintptr_t)RA_IMAGE_BASE, RA_IMAGE_SIZE, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    if (at != (void *)(uintptr_t)RA_IMAGE_BASE) {
        fprintf(stderr, "ra: cannot map the image at 0x%llx for the residual engine\n", (unsigned long long)RA_IMAGE_BASE);
        host_exit(0xE0000004);
    }
    if (ra_residual_start(RA_RESIDUAL_IMAGE, RA_RESIDUAL_FACTS, at, RA_IMAGE_SIZE) != 0) {
        fflush(stderr);
        host_exit(0xE0000004);
    }
    AddVectoredExceptionHandler(1, residual_on_exception);
}
#endif

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
#ifdef RA_RESIDUAL
        if (ra_residual_call(state, target) == 0) return;
        fflush(stderr);
        host_exit(0xE0000003);
#endif
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
#elif defined(_WIN32) || defined(RA_WIN32)
    // A Windows program passes four arguments in registers and the rest past the shadow space.
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
        // A host function's address does not fit the slot: the slot holds a trampoline to it,
        // which the binding table sees through.
        *(uint32_t *)(uintptr_t)ra_imports[i].slot = trampoline(f, -1);
#else
        *(uint64_t *)(uintptr_t)ra_imports[i].slot = (uint64_t)(uintptr_t)f;
#endif
    }
}
#else
// A POSIX program imports by symbol name from the libraries it needs, or from the whole process
// when the image names none for the symbol; an ordinal has no meaning there. A Windows program's
// imports are the Win32 layer's and the host C library's, both in the process (the port is linked
// with -rdynamic so dlsym sees the layer).
static void bind_imports(void) {
    for (int i = 0; ra_imports[i].slot != 0; i++) {
#ifdef RA_WIN32
        const char *library = NULL;
#else
        const char *library = ra_imports[i].dll && *ra_imports[i].dll ? ra_imports[i].dll : NULL;
#endif
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

#if !defined(_WIN32) && defined(RA_WIN32)
static void win32_describe_thread(uint8_t *block);
#endif

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
    // the stack guard a Linux program reads at fs:0x28 among it, stays zero. A Windows program
    // finds its process and thread ids there as well, and a 32-bit one the 32-bit layout, whose
    // addresses the Win32 layer keeps low: the thread runs on a low stack, which holds its TLS.
#if RA_POINTER_SIZE == 4
    *(uint32_t *)(teb + 0x18) = (uint32_t)(uintptr_t)teb;
    *(uint32_t *)(teb + 0x2c) = (uint32_t)(uintptr_t)&tls_slot;
#else
    *(uint64_t *)(teb + 0x30) = (uint64_t)(uintptr_t)teb;
    *(uint64_t *)(teb + 0x58) = (uint64_t)(uintptr_t)&tls_slot;
#endif
#ifdef RA_WIN32
    win32_describe_thread(teb);
#endif
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

#if defined(RA_RTTI) && defined(_WIN32)
// The x64 RTTI names its parts by offsets from the image's base, which the host's `__RTtypeid`,
// `__RTDynamicCast` and `__RTCastToVoid` take as the locator's address less its own offset. The
// standalone port's sections are globals the linker places apart, so each field is rewritten to
// the offset of what it names from the port's module: the pairs of a field's address and the
// address it names, then a zero pair. The fields lie in read-only data, open for the rewrite.
extern const uint64_t ra_rtti[];

static void rebase_rtti(void) {
    HMODULE module = NULL;
    if (!GetModuleHandleExA(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, (LPCSTR)ra_rtti, &module)) {
        fprintf(stderr, "ra: the module of the RTTI table is not found\n");
        host_exit(0xE0000006);
    }
    uintptr_t page = 0;
    DWORD protect = 0;
    for (const uint64_t *pair = ra_rtti; pair[0]; pair += 2) {
        uintptr_t field = (uintptr_t)pair[0];
        if ((field & ~(uintptr_t)0xfff) != page) {
            if (page) VirtualProtect((void *)page, 0x1000, protect, &protect);
            page = field & ~(uintptr_t)0xfff;
            VirtualProtect((void *)page, 0x1000, PAGE_READWRITE, &protect);
        }
        *(uint32_t *)field = (uint32_t)(pair[1] - (uintptr_t)module);
    }
    if (page) VirtualProtect((void *)page, 0x1000, protect, &protect);
}
#endif

// The loader's work before the entry: the RTTI's offsets, __tls_index written, the main thread's
// block, the callbacks.
static void attach_process(void) {
#if defined(RA_RTTI) && defined(_WIN32)
    rebase_rtti();
#endif
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
    // Control at an address of the image: a computed call or jump to code the port has no
    // function for (dead code the analysis left out, or a function only an address reached).
    if (r->ExceptionCode == EXCEPTION_ACCESS_VIOLATION && at >= RA_IMAGE_BASE && at < RA_IMAGE_BASE + RA_IMAGE_SIZE) {
        fprintf(stderr, "ra: a computed call or jump to 0x%llx, which is no lifted function\n", (unsigned long long)at);
    }
    fprintf(stderr, "ra: exception 0x%lx at 0x%llx", r->ExceptionCode, (unsigned long long)at);
    if (at >= module) fprintf(stderr, " (module+0x%llx)", (unsigned long long)(at - module));
    if (r->ExceptionCode == EXCEPTION_ACCESS_VIOLATION && r->NumberParameters >= 2) {
        fprintf(stderr, ", %s 0x%llx", r->ExceptionInformation[0] == 1 ? "writing" : "reading", (unsigned long long)r->ExceptionInformation[1]);
    }
    fprintf(stderr, "\n");
    report_entered();
    fflush(stderr);
    return EXCEPTION_CONTINUE_SEARCH;
}
#else
// A crash of the lifted code on a POSIX host names the signal and the address it touched; the
// handler then gives the signal back to its default, so the process ends as the crash would.
static void on_signal(int signal_number, siginfo_t *info, void *context) {
    (void)context;
    fprintf(stderr, "ra: signal %d at 0x%llx\n", signal_number, (unsigned long long)(uintptr_t)info->si_addr);
    report_entered();
    fflush(stderr);
}

// The report runs on a stack of the thread's own beside the one that crashed, so an overflow is
// reported as well; every thread the port runs code on sets one up first.
static RA_THREAD _Alignas(16) uint8_t signal_stack[32768];

static void report_crashes_on_this_thread(void) {
    stack_t s;
    memset(&s, 0, sizeof s);
    s.ss_sp = signal_stack;
    s.ss_size = sizeof signal_stack;
    sigaltstack(&s, NULL);
}

static void report_crashes(void) {
    report_crashes_on_this_thread();
    struct sigaction action;
    memset(&action, 0, sizeof action);
    action.sa_sigaction = on_signal;
    action.sa_flags = SA_SIGINFO | SA_RESETHAND | SA_NODEFER | SA_ONSTACK;
    sigemptyset(&action.sa_mask);
    const int signals[] = {SIGSEGV, SIGBUS, SIGILL, SIGFPE};
    for (size_t i = 0; i < sizeof signals / sizeof signals[0]; i++) sigaction(signals[i], &action, NULL);
}
#endif

#if !defined(_WIN32) && defined(RA_WIN32)  // the Win32 layer
// The Win32 layer: the kernel32, user32 and Microsoft C runtime functions a Windows program
// imports, over the host's. Each takes and returns integers, as the translation declares every
// import, and reads and writes the program's structures at its pointer width. A 32-bit program
// keeps its addresses in 32 bits: the port is linked at its fixed low address (-no-pie), its heap
// is the brk heap right above it, and every stack and mapping the layer makes is MAP_32BIT. A
// handle is an index into the layer's table, times four; the standard ones are the first three.
// What the layer does not cover is not linked: `ras doctor --host <exe>` names it before a build.
#define W_PTR(x) ((void *)(uintptr_t)(x))
#define W_INVALID_HANDLE ((uint64_t)-1)
#define W_INFINITE 0xffffffffu

static RA_THREAD uint32_t win32_last_error;
static RA_THREAD uint32_t win32_thread_id;
static pthread_mutex_t win32_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t win32_changed = PTHREAD_COND_INITIALIZER;

static uint64_t win32_fail(uint32_t error) {
    win32_last_error = error;
    return 0;
}

// The Win32 error of an errno.
static uint32_t win32_error_of(int e) {
    switch (e) {
    case ENOENT: return 2;    // ERROR_FILE_NOT_FOUND
    case ENOTDIR: return 3;   // ERROR_PATH_NOT_FOUND
    case EMFILE: return 4;    // ERROR_TOO_MANY_OPEN_FILES
    case EACCES: case EPERM: case EISDIR: return 5;  // ERROR_ACCESS_DENIED
    case EBADF: return 6;     // ERROR_INVALID_HANDLE
    case ENOMEM: return 8;    // ERROR_NOT_ENOUGH_MEMORY
    case EEXIST: return 80;   // ERROR_FILE_EXISTS
    case ENOSPC: return 112;  // ERROR_DISK_FULL
    case ENOTEMPTY: return 145;  // ERROR_DIR_NOT_EMPTY
    default: return 31;       // ERROR_GEN_FAILURE
    }
}

static void win32_put32(uint64_t at, uint32_t value) {
    if (at) memcpy(W_PTR(at), &value, 4);
}

static void win32_put64(uint64_t at, uint64_t value) {
    if (at) memcpy(W_PTR(at), &value, 8);
}

// A pointer in the program's memory, at its width.
static void win32_put_pointer(uint64_t at, uint64_t value) {
#if RA_POINTER_SIZE == 4
    win32_put32(at, (uint32_t)value);
#else
    win32_put64(at, value);
#endif
}

static uint64_t win32_get_pointer(uint64_t at) {
#if RA_POINTER_SIZE == 4
    uint32_t value;
    memcpy(&value, W_PTR(at), 4);
#else
    uint64_t value;
    memcpy(&value, W_PTR(at), 8);
#endif
    return value;
}

// Anonymous memory at `at`, or anywhere the program's pointers reach when `at` is zero.
static void *win32_map(uintptr_t at, size_t size, int protect) {
    int flags = MAP_PRIVATE | MAP_ANONYMOUS;
#if RA_POINTER_SIZE == 4
    if (!at) flags |= MAP_32BIT;
#endif
    void *p = mmap((void *)at, size, protect, flags, -1, 0);
    if (p == MAP_FAILED) return NULL;
    if ((at && p != (void *)at) || (RA_POINTER_SIZE == 4 && (uint64_t)(uintptr_t)p + size > 0x100000000ull)) {
        munmap(p, size);
        return NULL;
    }
    return p;
}

// A UTF-16 string of the program as UTF-8, NUL-terminated, from malloc; `count` UTF-16 units, or
// to the NUL when it is (size_t)-1.
static char *win32_narrow(uint64_t wide, size_t count) {
    const uint16_t *w = (const uint16_t *)W_PTR(wide);
    if (count == (size_t)-1) for (count = 0; w[count]; count++) {}
    char *out = malloc(count * 3 + 1);
    if (!out) return NULL;
    size_t n = 0;
    for (size_t i = 0; i < count; i++) {
        uint32_t c = w[i];
        if (c >= 0xd800 && c < 0xdc00 && i + 1 < count && w[i + 1] >= 0xdc00 && w[i + 1] < 0xe000) {
            c = 0x10000 + ((c - 0xd800) << 10) + (w[++i] - 0xdc00);
        }
        if (c < 0x80) {
            out[n++] = (char)c;
        } else if (c < 0x800) {
            out[n++] = (char)(0xc0 | (c >> 6));
            out[n++] = (char)(0x80 | (c & 0x3f));
        } else if (c < 0x10000) {
            out[n++] = (char)(0xe0 | (c >> 12));
            out[n++] = (char)(0x80 | ((c >> 6) & 0x3f));
            out[n++] = (char)(0x80 | (c & 0x3f));
        } else {
            out[n++] = (char)(0xf0 | (c >> 18));
            out[n++] = (char)(0x80 | ((c >> 12) & 0x3f));
            out[n++] = (char)(0x80 | ((c >> 6) & 0x3f));
            out[n++] = (char)(0x80 | (c & 0x3f));
        }
    }
    out[n] = 0;
    return out;
}

// A Windows path as the host's: the separators turned, from malloc.
static char *win32_path(const char *name) {
    char *path = strdup(name);
    if (path) for (char *p = path; *p; p++) if (*p == '\\') *p = '/';
    return path;
}

// ---- Handles: files, threads and events.
enum { WIN32_FREE, WIN32_FILE, WIN32_THREAD, WIN32_EVENT };

struct win32_thread {
    uint64_t start, parameter;
    uint32_t id, code;
    int done;
};

struct win32_object {
    int kind;
    int fd;
    struct win32_thread *thread;
    int manual, signaled;
};

static struct win32_object win32_objects[1024] = {{WIN32_FILE, 0, NULL, 0, 0}, {WIN32_FILE, 1, NULL, 0, 0}, {WIN32_FILE, 2, NULL, 0, 0}};

// A new handle of `kind`, with the lock held; 0 when the table is full.
static uint64_t win32_new_handle(int kind) {
    for (size_t i = 3; i < sizeof win32_objects / sizeof win32_objects[0]; i++) {
        if (win32_objects[i].kind != WIN32_FREE) continue;
        memset(&win32_objects[i], 0, sizeof win32_objects[i]);
        win32_objects[i].kind = kind;
        return (uint64_t)(i + 1) * 4;
    }
    return 0;
}

static struct win32_object *win32_object(uint64_t handle) {
    uint32_t h = (uint32_t)handle;
    if (h == 0 || h % 4 || h / 4 > sizeof win32_objects / sizeof win32_objects[0]) return NULL;
    struct win32_object *o = &win32_objects[h / 4 - 1];
    return o->kind == WIN32_FREE ? NULL : o;
}

static int win32_fd(uint64_t handle) {
    struct win32_object *o = win32_object(handle);
    return o && o->kind == WIN32_FILE ? o->fd : -1;
}

uint64_t GetLastError(void) {
    return win32_last_error;
}

uint64_t SetLastError(uint64_t error) {
    win32_last_error = (uint32_t)error;
    return 0;
}

// The length of a narrow string, without its terminator; a null string is empty.
uint64_t lstrlenA(uint64_t text) {
    return text ? (uint32_t)strlen((const char *)W_PTR(text)) : 0;
}

uint64_t GetStdHandle(uint64_t which) {
    switch ((uint32_t)which) {
    case (uint32_t)-10: return 4;   // STD_INPUT_HANDLE
    case (uint32_t)-11: return 8;   // STD_OUTPUT_HANDLE
    case (uint32_t)-12: return 12;  // STD_ERROR_HANDLE
    default: win32_last_error = 6; return W_INVALID_HANDLE;
    }
}

uint64_t CloseHandle(uint64_t handle) {
    pthread_mutex_lock(&win32_lock);
    struct win32_object *o = win32_object(handle);
    if (!o) {
        pthread_mutex_unlock(&win32_lock);
        return win32_fail(6);
    }
    if (o->kind == WIN32_FILE && o->fd > 2) close(o->fd);
    // A thread's record stays: the thread writes its end there whether or not a handle is open.
    if (o > &win32_objects[2]) o->kind = WIN32_FREE;
    pthread_mutex_unlock(&win32_lock);
    return 1;
}

// ---- Files and the console.
static uint64_t win32_open(const char *name, uint64_t access, uint64_t disposition) {
    uint32_t a = (uint32_t)access;
    // GENERIC_READ, GENERIC_ALL or FILE_READ_DATA; GENERIC_WRITE, GENERIC_ALL, FILE_WRITE_DATA or FILE_APPEND_DATA.
    int reads = (a & 0x90000001u) != 0, writes = (a & 0x50000006u) != 0;
    int flags = reads && writes ? O_RDWR : writes ? O_WRONLY : O_RDONLY;
    switch ((uint32_t)disposition) {
    case 1: flags |= O_CREAT | O_EXCL; break;   // CREATE_NEW
    case 2: flags |= O_CREAT | O_TRUNC; break;  // CREATE_ALWAYS
    case 3: break;                              // OPEN_EXISTING
    case 4: flags |= O_CREAT; break;            // OPEN_ALWAYS
    case 5: flags |= O_TRUNC; break;            // TRUNCATE_EXISTING
    default: win32_last_error = 87; return W_INVALID_HANDLE;  // ERROR_INVALID_PARAMETER
    }
    char *path = win32_path(name);
    if (!path) return win32_fail(8), W_INVALID_HANDLE;
    struct stat before;
    int existed = stat(path, &before) == 0;
    int fd = open(path, flags | O_CLOEXEC, 0666);
    int error = errno;
    free(path);
    if (fd < 0) {
        win32_last_error = win32_error_of(error);
        return W_INVALID_HANDLE;
    }
    pthread_mutex_lock(&win32_lock);
    uint64_t handle = win32_new_handle(WIN32_FILE);
    if (handle) win32_object(handle)->fd = fd;
    pthread_mutex_unlock(&win32_lock);
    if (!handle) {
        close(fd);
        win32_last_error = 4;
        return W_INVALID_HANDLE;
    }
    // CREATE_ALWAYS and OPEN_ALWAYS report a file that was there with ERROR_ALREADY_EXISTS.
    uint32_t d = (uint32_t)disposition;
    win32_last_error = existed && (d == 2 || d == 4) ? 183 : 0;
    return handle;
}

uint64_t CreateFileA(uint64_t name, uint64_t access, uint64_t share, uint64_t security, uint64_t disposition, uint64_t flags, uint64_t template_file) {
    (void)share;
    (void)security;
    (void)flags;
    (void)template_file;
    return win32_open((const char *)W_PTR(name), access, disposition);
}

uint64_t CreateFileW(uint64_t name, uint64_t access, uint64_t share, uint64_t security, uint64_t disposition, uint64_t flags, uint64_t template_file) {
    (void)share;
    (void)security;
    (void)flags;
    (void)template_file;
    char *narrow = win32_narrow(name, (size_t)-1);
    if (!narrow) return win32_fail(8), W_INVALID_HANDLE;
    uint64_t handle = win32_open(narrow, access, disposition);
    free(narrow);
    return handle;
}

uint64_t WriteFile(uint64_t handle, uint64_t buffer, uint64_t count, uint64_t written, uint64_t overlapped) {
    (void)overlapped;
    int fd = win32_fd(handle);
    if (fd < 0) return win32_fail(6);
    size_t done = 0, n = (uint32_t)count;
    while (done < n) {
        ssize_t w = write(fd, (const char *)W_PTR(buffer) + done, n - done);
        if (w < 0 && errno == EINTR) continue;
        if (w < 0) {
            win32_put32(written, (uint32_t)done);
            return win32_fail(win32_error_of(errno));
        }
        done += (size_t)w;
    }
    win32_put32(written, (uint32_t)done);
    return 1;
}

uint64_t ReadFile(uint64_t handle, uint64_t buffer, uint64_t count, uint64_t read_count, uint64_t overlapped) {
    (void)overlapped;
    int fd = win32_fd(handle);
    if (fd < 0) return win32_fail(6);
    ssize_t r;
    do r = read(fd, W_PTR(buffer), (uint32_t)count);
    while (r < 0 && errno == EINTR);
    if (r < 0) {
        win32_put32(read_count, 0);
        return win32_fail(win32_error_of(errno));
    }
    win32_put32(read_count, (uint32_t)r);
    return 1;
}

uint64_t WriteConsoleA(uint64_t handle, uint64_t buffer, uint64_t count, uint64_t written, uint64_t reserved) {
    return WriteFile(handle, buffer, count, written, reserved);
}

// The count written is in characters, as the program passed them.
uint64_t WriteConsoleW(uint64_t handle, uint64_t buffer, uint64_t count, uint64_t written, uint64_t reserved) {
    (void)reserved;
    char *narrow = win32_narrow(buffer, (uint32_t)count);
    if (!narrow) return win32_fail(8);
    uint64_t ok = WriteFile(handle, (uint64_t)(uintptr_t)narrow, strlen(narrow), 0, 0);
    free(narrow);
    win32_put32(written, ok ? (uint32_t)count : 0);
    return ok;
}

// A console is a terminal; another handle has no console mode.
uint64_t GetConsoleMode(uint64_t handle, uint64_t mode) {
    int fd = win32_fd(handle);
    if (fd < 0 || !isatty(fd)) return win32_fail(6);
    // ENABLE_PROCESSED_INPUT | ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT, or ENABLE_PROCESSED_OUTPUT | ENABLE_WRAP_AT_EOL_OUTPUT.
    win32_put32(mode, fd == 0 ? 7 : 3);
    return 1;
}

uint64_t GetFileType(uint64_t handle) {
    struct stat s;
    int fd = win32_fd(handle);
    if (fd < 0 || fstat(fd, &s) != 0) return win32_fail(6);
    win32_last_error = 0;
    if (S_ISCHR(s.st_mode)) return 2;                       // FILE_TYPE_CHAR
    if (S_ISFIFO(s.st_mode) || S_ISSOCK(s.st_mode)) return 3;  // FILE_TYPE_PIPE
    return 1;                                               // FILE_TYPE_DISK
}

uint64_t GetFileSize(uint64_t handle, uint64_t high) {
    struct stat s;
    int fd = win32_fd(handle);
    if (fd < 0 || fstat(fd, &s) != 0) {
        win32_last_error = 6;
        return 0xffffffffu;  // INVALID_FILE_SIZE
    }
    win32_put32(high, (uint32_t)((uint64_t)s.st_size >> 32));
    win32_last_error = 0;
    return (uint32_t)s.st_size;
}

uint64_t GetFileSizeEx(uint64_t handle, uint64_t size) {
    struct stat s;
    int fd = win32_fd(handle);
    if (fd < 0 || fstat(fd, &s) != 0) return win32_fail(6);
    win32_put64(size, (uint64_t)s.st_size);
    return 1;
}

// FILE_BEGIN, FILE_CURRENT and FILE_END are SEEK_SET, SEEK_CUR and SEEK_END.
uint64_t SetFilePointer(uint64_t handle, uint64_t low, uint64_t high, uint64_t method) {
    int fd = win32_fd(handle);
    if (fd < 0) return win32_fail(6), 0xffffffffu;
    int64_t distance = (int32_t)low;
    if (high) {
        int32_t h;
        memcpy(&h, W_PTR(high), 4);
        distance = (int64_t)(((uint64_t)(uint32_t)h << 32) | (uint32_t)low);
    }
    off_t at = lseek(fd, (off_t)distance, (int)(uint32_t)method);
    if (at < 0) {
        win32_last_error = win32_error_of(errno);
        return 0xffffffffu;  // INVALID_SET_FILE_POINTER
    }
    win32_put32(high, (uint32_t)((uint64_t)at >> 32));
    win32_last_error = 0;
    return (uint32_t)at;
}

uint64_t FlushFileBuffers(uint64_t handle) {
    int fd = win32_fd(handle);
    if (fd < 0) return win32_fail(6);
    // A terminal or a pipe has nothing to flush to a disk.
    if (fsync(fd) != 0 && errno != EINVAL && errno != EROFS) return win32_fail(win32_error_of(errno));
    return 1;
}

uint64_t DeleteFileA(uint64_t name) {
    char *path = win32_path((const char *)W_PTR(name));
    int ok = path && unlink(path) == 0;
    int error = errno;
    free(path);
    return ok ? 1 : win32_fail(win32_error_of(error));
}

uint64_t CreateDirectoryA(uint64_t name, uint64_t security) {
    (void)security;
    char *path = win32_path((const char *)W_PTR(name));
    int ok = path && mkdir(path, 0777) == 0;
    int error = errno;
    free(path);
    return ok ? 1 : win32_fail(error == EEXIST ? 183 : win32_error_of(error));  // ERROR_ALREADY_EXISTS
}

uint64_t RemoveDirectoryA(uint64_t name) {
    char *path = win32_path((const char *)W_PTR(name));
    int ok = path && rmdir(path) == 0;
    int error = errno;
    free(path);
    return ok ? 1 : win32_fail(win32_error_of(error));
}

uint64_t GetFileAttributesA(uint64_t name) {
    char *path = win32_path((const char *)W_PTR(name));
    struct stat s;
    int ok = path && stat(path, &s) == 0;
    int error = errno;
    free(path);
    if (!ok) {
        win32_last_error = win32_error_of(error);
        return 0xffffffffu;  // INVALID_FILE_ATTRIBUTES
    }
    // FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_READONLY, else FILE_ATTRIBUTE_NORMAL.
    uint32_t attributes = (S_ISDIR(s.st_mode) ? 0x10u : 0) | ((s.st_mode & S_IWUSR) ? 0 : 0x1u);
    return attributes ? attributes : 0x80u;
}

// ---- Memory.
// The regions VirtualAlloc made, for VirtualFree's release of a whole one and VirtualProtect's
// answer of the protection before.
struct win32_region { uintptr_t base; size_t size; uint32_t protect; };
static struct win32_region win32_regions[256];

static int win32_protection(uint32_t protect) {
    switch (protect & 0xff) {
    case 0x01: return PROT_NONE;                           // PAGE_NOACCESS
    case 0x02: return PROT_READ;                           // PAGE_READONLY
    case 0x04: case 0x08: return PROT_READ | PROT_WRITE;   // PAGE_READWRITE, PAGE_WRITECOPY
    case 0x10: return PROT_EXEC;                           // PAGE_EXECUTE
    case 0x20: return PROT_READ | PROT_EXEC;               // PAGE_EXECUTE_READ
    case 0x40: case 0x80: return PROT_READ | PROT_WRITE | PROT_EXEC;
    default: return -1;
    }
}

static struct win32_region *win32_region_of(uintptr_t at) {
    for (size_t i = 0; i < sizeof win32_regions / sizeof win32_regions[0]; i++) {
        struct win32_region *r = &win32_regions[i];
        if (r->size && at >= r->base && at < r->base + r->size) return r;
    }
    return NULL;
}

uint64_t VirtualAlloc(uint64_t address, uint64_t size, uint64_t type, uint64_t protect) {
    int prot = win32_protection((uint32_t)protect);
    size_t page = (size_t)sysconf(_SC_PAGESIZE);
    uintptr_t at = (uintptr_t)address & ~(uintptr_t)(page - 1);
    size_t length = ((uintptr_t)address + (size_t)size - at + page - 1) & ~(page - 1);
    // MEM_COMMIT 0x1000, MEM_RESERVE 0x2000.
    if (prot < 0 || !size || !(type & 0x3000)) return win32_fail(87);
    pthread_mutex_lock(&win32_lock);
    struct win32_region *region = at ? win32_region_of(at) : NULL;
    uint64_t result = 0;
    if (region) {
        // A commit in a reserved region: the pages become accessible.
        if ((type & 0x1000) && mprotect((void *)at, length, prot) == 0) result = at;
    } else {
        void *p = win32_map(at, length, (type & 0x1000) ? prot : PROT_NONE);
        for (size_t i = 0; p && i < sizeof win32_regions / sizeof win32_regions[0]; i++) {
            if (win32_regions[i].size) continue;
            win32_regions[i] = (struct win32_region){(uintptr_t)p, length, (uint32_t)protect};
            result = (uint64_t)(uintptr_t)p;
            break;
        }
        if (p && !result) munmap(p, length);
    }
    pthread_mutex_unlock(&win32_lock);
    return result ? result : win32_fail(8);
}

uint64_t VirtualFree(uint64_t address, uint64_t size, uint64_t type) {
    pthread_mutex_lock(&win32_lock);
    struct win32_region *r = win32_region_of((uintptr_t)address);
    int ok = 0;
    if (r && (type & 0x8000) && r->base == (uintptr_t)address && !size) {  // MEM_RELEASE of the whole
        ok = munmap((void *)r->base, r->size) == 0;
        r->size = 0;
    } else if (r && (type & 0x4000)) {  // MEM_DECOMMIT
        size_t length = size ? (size_t)size : r->size - ((uintptr_t)address - r->base);
        ok = mprotect(W_PTR(address), length, PROT_NONE) == 0 && madvise(W_PTR(address), length, MADV_DONTNEED) == 0;
    }
    pthread_mutex_unlock(&win32_lock);
    return ok ? 1 : win32_fail(487);  // ERROR_INVALID_ADDRESS
}

uint64_t VirtualProtect(uint64_t address, uint64_t size, uint64_t protect, uint64_t old) {
    int prot = win32_protection((uint32_t)protect);
    size_t page = (size_t)sysconf(_SC_PAGESIZE);
    uintptr_t at = (uintptr_t)address & ~(uintptr_t)(page - 1);
    pthread_mutex_lock(&win32_lock);
    struct win32_region *r = win32_region_of(at);
    int ok = r && prot >= 0 && mprotect((void *)at, (uintptr_t)address + (size_t)size - at, prot) == 0;
    if (ok) {
        win32_put32(old, r->protect);
        r->protect = (uint32_t)protect;
    }
    pthread_mutex_unlock(&win32_lock);
    return ok ? 1 : win32_fail(487);
}

// The heap functions: blocks of the C heap, each after its requested size, which HeapSize answers.
// The process heap's handle is no multiple of four, so it never names an object of the handle
// table and CloseHandle refuses it.
#define W_HEAP 0x7ff1
enum { W_HEAP_HEADER = 16 };

static uint64_t win32_alloc(size_t size, int zero) {
    uint8_t *p = zero ? calloc(1, size + W_HEAP_HEADER) : malloc(size + W_HEAP_HEADER);
    if (!p) return win32_fail(8);
    memcpy(p, &size, sizeof size);
    return (uint64_t)(uintptr_t)(p + W_HEAP_HEADER);
}

static size_t win32_block_size(uint64_t block) {
    size_t size;
    memcpy(&size, (uint8_t *)W_PTR(block) - W_HEAP_HEADER, sizeof size);
    return size;
}

uint64_t GetProcessHeap(void) {
    return W_HEAP;
}

uint64_t HeapCreate(uint64_t options, uint64_t initial, uint64_t maximum) {
    (void)options;
    (void)initial;
    (void)maximum;
    return W_HEAP;
}

uint64_t HeapDestroy(uint64_t heap) {
    (void)heap;
    return 1;
}

uint64_t HeapAlloc(uint64_t heap, uint64_t flags, uint64_t size) {
    (void)heap;
    return win32_alloc((size_t)(uint32_t)size, (flags & 8) != 0);  // HEAP_ZERO_MEMORY
}

uint64_t HeapFree(uint64_t heap, uint64_t flags, uint64_t block) {
    (void)heap;
    (void)flags;
    if (block) free((uint8_t *)W_PTR(block) - W_HEAP_HEADER);
    return 1;
}

uint64_t HeapSize(uint64_t heap, uint64_t flags, uint64_t block) {
    (void)heap;
    (void)flags;
    return block ? win32_block_size(block) : (uint64_t)-1;
}

uint64_t HeapReAlloc(uint64_t heap, uint64_t flags, uint64_t block, uint64_t size) {
    (void)heap;
    size_t n = (size_t)(uint32_t)size, old = win32_block_size(block);
    if ((flags & 0x10) && n > old) return win32_fail(8);  // HEAP_REALLOC_IN_PLACE_ONLY
    uint8_t *p = realloc((uint8_t *)W_PTR(block) - W_HEAP_HEADER, n + W_HEAP_HEADER);
    if (!p) return win32_fail(8);
    memcpy(p, &n, sizeof n);
    if ((flags & 8) && n > old) memset(p + W_HEAP_HEADER + old, 0, n - old);
    return (uint64_t)(uintptr_t)(p + W_HEAP_HEADER);
}

// LocalAlloc and GlobalAlloc: a movable block is fixed here, and its lock is its address.
uint64_t LocalAlloc(uint64_t flags, uint64_t size) {
    return win32_alloc((size_t)(uint32_t)size, (flags & 0x40) != 0);  // LMEM_ZEROINIT
}

uint64_t LocalFree(uint64_t block) {
    HeapFree(W_HEAP, 0, block);
    return 0;
}

uint64_t GlobalAlloc(uint64_t flags, uint64_t size) {
    return win32_alloc((size_t)(uint32_t)size, (flags & 0x40) != 0);  // GMEM_ZEROINIT
}

uint64_t GlobalFree(uint64_t block) {
    HeapFree(W_HEAP, 0, block);
    return 0;
}

uint64_t GlobalLock(uint64_t block) {
    return block;
}

uint64_t GlobalUnlock(uint64_t block) {
    (void)block;
    win32_last_error = 0;
    return 0;
}

// ---- Threads and synchronization.
static RA_THREAD struct win32_thread *win32_self;
static uint32_t win32_next_id = 0x100;

uint64_t GetCurrentThreadId(void) {
    if (!win32_thread_id) {
        pthread_mutex_lock(&win32_lock);
        win32_thread_id = win32_next_id += 4;
        pthread_mutex_unlock(&win32_lock);
    }
    return win32_thread_id;
}

uint64_t GetCurrentProcessId(void) {
    return (uint32_t)getpid();
}

uint64_t GetCurrentProcess(void) {
    return (uint64_t)-1;
}

uint64_t GetCurrentThread(void) {
    return (uint64_t)-2;
}

// A thread of the program on a stack its pointers reach.
static int win32_start(pthread_t *thread, size_t stack_size, void *(*main)(void *), void *argument, int detached) {
    pthread_attr_t attr;
    pthread_attr_init(&attr);
    if (detached) pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED);
    if (stack_size < (1 << 20)) stack_size = 1 << 20;
#if RA_POINTER_SIZE == 4
    // A thread's TLS sits at the top of a stack it is given: the thread block lands low as well.
    // A stack the thread is given has no guard page of the host's: its lowest page is one, so an
    // overflow faults and is reported rather than running into the mapping below.
    void *stack = win32_map(0, stack_size, PROT_READ | PROT_WRITE);
    if (!stack) {
        pthread_attr_destroy(&attr);
        return -1;
    }
    mprotect(stack, 4096, PROT_NONE);
    pthread_attr_setstack(&attr, stack, stack_size);
#else
    pthread_attr_setstacksize(&attr, stack_size);
#endif
    int r = pthread_create(thread, &attr, main, argument);
    pthread_attr_destroy(&attr);
    return r;
}

static void win32_thread_end(uint32_t code) {
    if (!win32_self) return;
    pthread_mutex_lock(&win32_lock);
    win32_self->code = code;
    win32_self->done = 1;
    pthread_cond_broadcast(&win32_changed);
    pthread_mutex_unlock(&win32_lock);
}

static void *win32_thread_main(void *argument) {
    report_crashes_on_this_thread();
    win32_self = argument;
    win32_thread_id = win32_self->id;
    uint32_t code = (uint32_t)((native8)(uintptr_t)win32_self->start)(win32_self->parameter, 0, 0, 0, 0, 0, 0, 0);
    win32_thread_end(code);
    return NULL;
}

uint64_t CreateThread(uint64_t security, uint64_t stack_size, uint64_t start, uint64_t parameter, uint64_t flags, uint64_t id) {
    (void)security;
    // A suspended start needs ResumeThread, which a POSIX thread has no counterpart of.
    if (flags & 4) return win32_fail(50);  // CREATE_SUSPENDED: ERROR_NOT_SUPPORTED
    struct win32_thread *t = calloc(1, sizeof *t);
    if (!t) return win32_fail(8);
    t->start = start;
    t->parameter = parameter;
    pthread_mutex_lock(&win32_lock);
    uint64_t handle = win32_new_handle(WIN32_THREAD);
    if (handle) win32_object(handle)->thread = t;
    t->id = win32_next_id += 4;
    pthread_mutex_unlock(&win32_lock);
    if (!handle) {
        free(t);
        return win32_fail(4);
    }
    pthread_t thread;
    if (win32_start(&thread, (size_t)(uint32_t)stack_size, win32_thread_main, t, 1) != 0) {
        CloseHandle(handle);
        free(t);
        return win32_fail(8);
    }
    win32_put32(id, t->id);
    return handle;
}

uint64_t ExitThread(uint64_t code) {
    win32_thread_end((uint32_t)code);
    pthread_exit(NULL);
}

uint64_t GetExitCodeThread(uint64_t handle, uint64_t code) {
    pthread_mutex_lock(&win32_lock);
    struct win32_object *o = win32_object(handle);
    int ok = o && o->kind == WIN32_THREAD;
    if (ok) win32_put32(code, o->thread->done ? o->thread->code : 259);  // STILL_ACTIVE
    pthread_mutex_unlock(&win32_lock);
    return ok ? 1 : win32_fail(6);
}

uint64_t ExitProcess(uint64_t code) {
    exit((int)(uint32_t)code);
}

uint64_t Sleep(uint64_t milliseconds) {
    uint32_t ms = (uint32_t)milliseconds;
    if (ms == 0) {
        sched_yield();
        return 0;
    }
    struct timespec t = {(time_t)(ms / 1000), (long)(ms % 1000) * 1000000L};
    while (nanosleep(&t, &t) != 0 && errno == EINTR) {}
    return 0;
}

uint64_t SwitchToThread(void) {
    sched_yield();
    return 1;
}

// Events. A name would reach another process, which the layer has none of.
static uint64_t win32_event(uint64_t manual, uint64_t initial, uint64_t name) {
    if (name) return win32_fail(50);
    pthread_mutex_lock(&win32_lock);
    uint64_t handle = win32_new_handle(WIN32_EVENT);
    if (handle) {
        win32_object(handle)->manual = (uint32_t)manual != 0;
        win32_object(handle)->signaled = (uint32_t)initial != 0;
    }
    pthread_mutex_unlock(&win32_lock);
    return handle ? handle : win32_fail(4);
}

uint64_t CreateEventA(uint64_t security, uint64_t manual, uint64_t initial, uint64_t name) {
    (void)security;
    return win32_event(manual, initial, name);
}

uint64_t CreateEventW(uint64_t security, uint64_t manual, uint64_t initial, uint64_t name) {
    (void)security;
    return win32_event(manual, initial, name);
}

static uint64_t win32_signal(uint64_t handle, int signaled) {
    pthread_mutex_lock(&win32_lock);
    struct win32_object *o = win32_object(handle);
    int ok = o && o->kind == WIN32_EVENT;
    if (ok) o->signaled = signaled;
    pthread_cond_broadcast(&win32_changed);
    pthread_mutex_unlock(&win32_lock);
    return ok ? 1 : win32_fail(6);
}

uint64_t SetEvent(uint64_t handle) {
    return win32_signal(handle, 1);
}

uint64_t ResetEvent(uint64_t handle) {
    return win32_signal(handle, 0);
}

// WAIT_OBJECT_0 when the thread has ended or the event is set (an automatic one then resets),
// WAIT_TIMEOUT after the time, WAIT_FAILED on a handle that is neither.
uint64_t WaitForSingleObject(uint64_t handle, uint64_t milliseconds) {
    uint32_t ms = (uint32_t)milliseconds;
    struct timespec deadline;
    clock_gettime(CLOCK_REALTIME, &deadline);
    deadline.tv_sec += ms / 1000;
    deadline.tv_nsec += (long)(ms % 1000) * 1000000L;
    if (deadline.tv_nsec >= 1000000000L) {
        deadline.tv_sec++;
        deadline.tv_nsec -= 1000000000L;
    }
    pthread_mutex_lock(&win32_lock);
    uint64_t result = 0;
    for (;;) {
        struct win32_object *o = win32_object(handle);
        if (!o || (o->kind != WIN32_THREAD && o->kind != WIN32_EVENT)) {
            win32_last_error = 6;
            result = 0xffffffffu;
            break;
        }
        if (o->kind == WIN32_THREAD ? o->thread->done : o->signaled) {
            if (o->kind == WIN32_EVENT && !o->manual) o->signaled = 0;
            break;
        }
        if (ms == 0 || (ms != W_INFINITE ? pthread_cond_timedwait(&win32_changed, &win32_lock, &deadline) : pthread_cond_wait(&win32_changed, &win32_lock)) == ETIMEDOUT) {
            result = 0x102;
            break;
        }
    }
    pthread_mutex_unlock(&win32_lock);
    return result;
}

// A critical section holds a recursive mutex of the host in its first field, DebugInfo, which the
// program does not read.
static pthread_mutex_t *win32_mutex_of(uint64_t section) {
    return (pthread_mutex_t *)W_PTR(win32_get_pointer(section));
}

uint64_t InitializeCriticalSection(uint64_t section) {
    memset(W_PTR(section), 0, RA_POINTER_SIZE == 4 ? 24 : 40);
    pthread_mutex_t *m = malloc(sizeof *m);
    pthread_mutexattr_t attr;
    pthread_mutexattr_init(&attr);
    pthread_mutexattr_settype(&attr, PTHREAD_MUTEX_RECURSIVE);
    pthread_mutex_init(m, &attr);
    pthread_mutexattr_destroy(&attr);
    win32_put_pointer(section, (uint64_t)(uintptr_t)m);
    return 0;
}

uint64_t InitializeCriticalSectionAndSpinCount(uint64_t section, uint64_t spin) {
    (void)spin;
    InitializeCriticalSection(section);
    return 1;
}

uint64_t InitializeCriticalSectionEx(uint64_t section, uint64_t spin, uint64_t flags) {
    (void)spin;
    (void)flags;
    InitializeCriticalSection(section);
    return 1;
}

uint64_t EnterCriticalSection(uint64_t section) {
    pthread_mutex_lock(win32_mutex_of(section));
    return 0;
}

uint64_t TryEnterCriticalSection(uint64_t section) {
    return pthread_mutex_trylock(win32_mutex_of(section)) == 0;
}

uint64_t LeaveCriticalSection(uint64_t section) {
    pthread_mutex_unlock(win32_mutex_of(section));
    return 0;
}

uint64_t DeleteCriticalSection(uint64_t section) {
    pthread_mutex_t *m = win32_mutex_of(section);
    if (m) {
        pthread_mutex_destroy(m);
        free(m);
    }
    win32_put_pointer(section, 0);
    return 0;
}

// Thread-local storage: 64 slots, as many as Windows guarantees.
static uint64_t win32_tls_used;
static RA_THREAD uint64_t win32_tls[64];

uint64_t TlsAlloc(void) {
    pthread_mutex_lock(&win32_lock);
    uint64_t index = 0xffffffffu;  // TLS_OUT_OF_INDEXES
    for (unsigned i = 0; i < 64; i++) {
        if (win32_tls_used & (1ull << i)) continue;
        win32_tls_used |= 1ull << i;
        index = i;
        break;
    }
    pthread_mutex_unlock(&win32_lock);
    if (index == 0xffffffffu) win32_last_error = 8;
    return index;
}

uint64_t TlsFree(uint64_t index) {
    if ((uint32_t)index >= 64) return win32_fail(87);
    pthread_mutex_lock(&win32_lock);
    win32_tls_used &= ~(1ull << (uint32_t)index);
    pthread_mutex_unlock(&win32_lock);
    return 1;
}

uint64_t TlsGetValue(uint64_t index) {
    if ((uint32_t)index >= 64) return win32_fail(87);
    win32_last_error = 0;
    return win32_tls[(uint32_t)index];
}

uint64_t TlsSetValue(uint64_t index, uint64_t value) {
    if ((uint32_t)index >= 64) return win32_fail(87);
    win32_tls[(uint32_t)index] = value;
    return 1;
}

// ---- Time.
static uint64_t win32_clock(clockid_t clock) {
    struct timespec t;
    clock_gettime(clock, &t);
    return (uint64_t)t.tv_sec * 1000000000ull + (uint64_t)t.tv_nsec;
}

// 100-nanosecond intervals since 1601, the FILETIME of now.
static uint64_t win32_file_time(void) {
    return win32_clock(CLOCK_REALTIME) / 100 + 116444736000000000ull;
}

uint64_t GetTickCount(void) {
    return (uint32_t)(win32_clock(CLOCK_MONOTONIC) / 1000000);
}

uint64_t GetTickCount64(void) {
    return win32_clock(CLOCK_MONOTONIC) / 1000000;
}

uint64_t timeGetTime(void) {
    return GetTickCount();
}

uint64_t QueryPerformanceCounter(uint64_t counter) {
    win32_put64(counter, win32_clock(CLOCK_MONOTONIC));
    return 1;
}

uint64_t QueryPerformanceFrequency(uint64_t frequency) {
    win32_put64(frequency, 1000000000ull);
    return 1;
}

uint64_t GetSystemTimeAsFileTime(uint64_t file_time) {
    win32_put64(file_time, win32_file_time());
    return 0;
}

uint64_t GetSystemTimePreciseAsFileTime(uint64_t file_time) {
    win32_put64(file_time, win32_file_time());
    return 0;
}

// A SYSTEMTIME: eight words, the year to the milliseconds.
static void win32_system_time(uint64_t at, int local) {
    uint64_t now = win32_clock(CLOCK_REALTIME);
    time_t seconds = (time_t)(now / 1000000000ull);
    struct tm t;
    if (local) localtime_r(&seconds, &t);
    else gmtime_r(&seconds, &t);
    uint16_t words[8] = {(uint16_t)(t.tm_year + 1900), (uint16_t)(t.tm_mon + 1), (uint16_t)t.tm_wday, (uint16_t)t.tm_mday,
                         (uint16_t)t.tm_hour, (uint16_t)t.tm_min, (uint16_t)t.tm_sec, (uint16_t)(now / 1000000 % 1000)};
    memcpy(W_PTR(at), words, sizeof words);
}

uint64_t GetSystemTime(uint64_t at) {
    win32_system_time(at, 0);
    return 0;
}

uint64_t GetLocalTime(uint64_t at) {
    win32_system_time(at, 1);
    return 0;
}

// ---- The process.
static uint64_t win32_unhandled_filter;
static uint8_t win32_module[4096];  // the module's header: zeros, no MZ, so the C runtime finds no managed image

uint64_t SetUnhandledExceptionFilter(uint64_t filter) {
    uint64_t previous = win32_unhandled_filter;
    win32_unhandled_filter = filter;
    return previous;
}

uint64_t UnhandledExceptionFilter(uint64_t pointers) {
    (void)pointers;
    return 0;  // EXCEPTION_CONTINUE_SEARCH
}

uint64_t IsDebuggerPresent(void) {
    return 0;
}

// Nothing listens: a program without a debugger sends its string nowhere.
uint64_t OutputDebugStringA(uint64_t text) {
    (void)text;
    return 0;
}

// The baseline of x86-64 (MMX, SSE, SSE2, cmpxchg8b, rdtsc, NX, fail-fast), and the later
// extensions as the host's processor has them.
uint64_t IsProcessorFeaturePresent(uint64_t feature) {
    switch ((uint32_t)feature) {
    case 2: case 3: case 6: case 8: case 10: case 12: case 23: return 1;
#if defined(__x86_64__)
    case 13: case 36: case 37: case 38: case 39: case 40: {
        uint32_t r1[4], r7[4];
        ra_cpuid(1, 0, r1);
        ra_cpuid(7, 0, r7);
        switch ((uint32_t)feature) {
        case 13: return r1[2] & 1;         // PF_SSE3_INSTRUCTIONS_AVAILABLE
        case 36: return (r1[2] >> 9) & 1;  // PF_SSSE3_INSTRUCTIONS_AVAILABLE
        case 37: return (r1[2] >> 19) & 1; // PF_SSE4_1_INSTRUCTIONS_AVAILABLE
        case 38: return (r1[2] >> 20) & 1; // PF_SSE4_2_INSTRUCTIONS_AVAILABLE
        case 39: return (r1[2] >> 28) & 1; // PF_AVX_INSTRUCTIONS_AVAILABLE
        default: return (r7[1] >> 5) & 1;  // PF_AVX2_INSTRUCTIONS_AVAILABLE
        }
    }
#endif
    default: return 0;
    }
}

#if RA_POINTER_SIZE == 8
uint64_t InitializeSListHead(uint64_t head) {
    memset(W_PTR(head), 0, 16);
    return 0;
}
#endif

uint64_t GetModuleHandleA(uint64_t name) {
    return name ? win32_fail(126) : (uint64_t)(uintptr_t)win32_module;  // ERROR_MOD_NOT_FOUND
}

uint64_t GetModuleHandleW(uint64_t name) {
    return GetModuleHandleA(name);
}

#if RA_POINTER_SIZE == 4
// The layer's one module is the program's, whose handle fits the program's dword.
uint64_t ra_x86_get_module_handle_a(uint64_t name) {
    return GetModuleHandleA(name);
}

uint64_t ra_x86_get_module_handle_w(uint64_t name) {
    return GetModuleHandleA(name);
}
#endif

uint64_t GetModuleFileNameA(uint64_t module, uint64_t buffer, uint64_t size) {
    if (module && module != (uint64_t)(uintptr_t)win32_module) return win32_fail(126);
    char path[4096];
#ifdef __linux__
    ssize_t n = readlink("/proc/self/exe", path, sizeof path - 1);
    if (n < 0) return win32_fail(win32_error_of(errno));
    path[n] = 0;
#else
    if (!realpath(ra_argv[0], path)) return win32_fail(win32_error_of(errno));
#endif
    uint32_t length = (uint32_t)strlen(path), room = (uint32_t)size;
    if (room == 0) return win32_fail(122);
    if (length >= room) {
        memcpy(W_PTR(buffer), path, room - 1);
        ((char *)W_PTR(buffer))[room - 1] = 0;
        win32_last_error = 122;  // ERROR_INSUFFICIENT_BUFFER
        return room;
    }
    memcpy(W_PTR(buffer), path, length + 1);
    return length;
}

// The command line, the arguments joined and quoted where they hold a space.
uint64_t GetCommandLineA(void) {
    static char *line;
    if (!line) {
        size_t size = 1;
        for (int i = 0; i < ra_argc; i++) size += strlen(ra_argv[i]) + 3;
        line = calloc(size, 1);
        if (!line) return 0;
        for (int i = 0; i < ra_argc; i++) {
            int quote = strchr(ra_argv[i], ' ') || strchr(ra_argv[i], '\t') || !*ra_argv[i];
            if (i) strcat(line, " ");
            if (quote) strcat(line, "\"");
            strcat(line, ra_argv[i]);
            if (quote) strcat(line, "\"");
        }
    }
    return (uint64_t)(uintptr_t)line;
}

uint64_t GetEnvironmentVariableA(uint64_t name, uint64_t buffer, uint64_t size) {
    const char *value = getenv((const char *)W_PTR(name));
    if (!value) return win32_fail(203);  // ERROR_ENVVAR_NOT_FOUND
    uint32_t length = (uint32_t)strlen(value);
    if (length >= (uint32_t)size) return length + 1;
    memcpy(W_PTR(buffer), value, length + 1);
    return length;
}

uint64_t SetEnvironmentVariableA(uint64_t name, uint64_t value) {
    int r = value ? setenv((const char *)W_PTR(name), (const char *)W_PTR(value), 1) : unsetenv((const char *)W_PTR(name));
    return r == 0 ? 1 : win32_fail(win32_error_of(errno));
}

// A STARTUPINFO with nothing set: its size and zeros.
static uint64_t win32_startup_info(uint64_t info) {
    uint32_t size = RA_POINTER_SIZE == 4 ? 68 : 104;
    memset(W_PTR(info), 0, size);
    win32_put32(info, size);
    return 0;
}

uint64_t GetStartupInfoA(uint64_t info) {
    return win32_startup_info(info);
}

uint64_t GetStartupInfoW(uint64_t info) {
    return win32_startup_info(info);
}

// ---- The message loop. A window is the host's windowing system's, which the layer does not
// reach: no window exists, and the messages are the threads' own, posted and taken in order.
struct win32_message { uint32_t thread, message; uint64_t wparam, lparam; uint32_t time; };
static struct win32_message win32_queue[256];
static size_t win32_queued;

static uint64_t win32_post(uint32_t thread, uint64_t message, uint64_t wparam, uint64_t lparam) {
    pthread_mutex_lock(&win32_lock);
    int ok = win32_queued < sizeof win32_queue / sizeof win32_queue[0];
    if (ok) win32_queue[win32_queued++] = (struct win32_message){thread, (uint32_t)message, wparam, lparam, (uint32_t)GetTickCount()};
    pthread_cond_broadcast(&win32_changed);
    pthread_mutex_unlock(&win32_lock);
    return ok ? 1 : win32_fail(1816);  // ERROR_NOT_ENOUGH_QUOTA
}

uint64_t PostThreadMessageA(uint64_t thread, uint64_t message, uint64_t wparam, uint64_t lparam) {
    return win32_post((uint32_t)thread, message, wparam, lparam);
}

uint64_t PostQuitMessage(uint64_t code) {
    win32_post((uint32_t)GetCurrentThreadId(), 0x12, (uint32_t)code, 0);  // WM_QUIT
    return 0;
}

// The calling thread's first message in [first, last] (all of them when both are zero) into the
// program's MSG, removed with `remove`; whether there was one. With the lock held.
static int win32_take(uint64_t msg, uint32_t first, uint32_t last, int remove) {
    uint32_t self = (uint32_t)GetCurrentThreadId();
    for (size_t i = 0; i < win32_queued; i++) {
        struct win32_message m = win32_queue[i];
        if (m.thread != self || ((first || last) && (m.message < first || m.message > last))) continue;
        if (remove) memmove(&win32_queue[i], &win32_queue[i + 1], (win32_queued - i - 1) * sizeof m), win32_queued--;
#if RA_POINTER_SIZE == 4
        uint32_t words[7] = {0, m.message, (uint32_t)m.wparam, (uint32_t)m.lparam, m.time, 0, 0};
        memcpy(W_PTR(msg), words, sizeof words);
#else
        uint8_t *out = W_PTR(msg);
        memset(out, 0, 48);
        memcpy(out + 8, &m.message, 4);
        memcpy(out + 16, &m.wparam, 8);
        memcpy(out + 24, &m.lparam, 8);
        memcpy(out + 32, &m.time, 4);
#endif
        return 1;
    }
    return 0;
}

static uint64_t win32_get_message(uint64_t msg, uint64_t hwnd, uint64_t first, uint64_t last) {
    if (hwnd) {
        win32_last_error = 1400;  // ERROR_INVALID_WINDOW_HANDLE
        return 0xffffffffu;
    }
    pthread_mutex_lock(&win32_lock);
    while (!win32_take(msg, (uint32_t)first, (uint32_t)last, 1)) pthread_cond_wait(&win32_changed, &win32_lock);
    pthread_mutex_unlock(&win32_lock);
    uint32_t message;
    memcpy(&message, (uint8_t *)W_PTR(msg) + RA_POINTER_SIZE, 4);
    return message != 0x12;  // WM_QUIT ends the loop
}

static uint64_t win32_peek_message(uint64_t msg, uint64_t hwnd, uint64_t first, uint64_t last, uint64_t remove) {
    if (hwnd) return win32_fail(1400);
    pthread_mutex_lock(&win32_lock);
    int found = win32_take(msg, (uint32_t)first, (uint32_t)last, ((uint32_t)remove & 1) != 0);  // PM_REMOVE
    pthread_mutex_unlock(&win32_lock);
    return (uint64_t)found;
}

uint64_t GetMessageA(uint64_t msg, uint64_t hwnd, uint64_t first, uint64_t last) {
    return win32_get_message(msg, hwnd, first, last);
}

uint64_t PeekMessageA(uint64_t msg, uint64_t hwnd, uint64_t first, uint64_t last, uint64_t remove) {
    return win32_peek_message(msg, hwnd, first, last, remove);
}

// A thread's message has no window to translate for or to dispatch to.
uint64_t TranslateMessage(uint64_t msg) {
    (void)msg;
    return 0;
}

uint64_t DispatchMessageA(uint64_t msg) {
    (void)msg;
    return 0;
}

// The 32-bit program's names for the same (prototypes.rs `x86_shim`): the MSG is the program's
// either way.
uint64_t ra_x86_get_message_a(uint64_t msg, uint64_t hwnd, uint64_t first, uint64_t last) {
    return win32_get_message(msg, hwnd, first, last);
}

uint64_t ra_x86_peek_message_a(uint64_t msg, uint64_t hwnd, uint64_t first, uint64_t last, uint64_t remove) {
    return win32_peek_message(msg, hwnd, first, last, remove);
}

uint64_t ra_x86_translate_message(uint64_t msg) {
    return TranslateMessage(msg);
}

uint64_t ra_x86_dispatch_message_a(uint64_t msg) {
    return DispatchMessageA(msg);
}

// ---- The Microsoft C runtime's own entries; the standard C functions are the host's.
static int win32_commode, win32_new_mode, win32_locale_mode = 2;  // _DISABLE_PER_THREAD_LOCALE
static uint64_t win32_current_exception, win32_current_exception_context;

uint64_t __p___argc(void) {
    return (uint64_t)(uintptr_t)&ra_argc;
}

uint64_t __p__commode(void) {
    return (uint64_t)(uintptr_t)&win32_commode;
}

uint64_t _set_fmode(uint64_t mode) {
    (void)mode;
    return 0;
}

uint64_t _set_app_type(uint64_t type) {
    (void)type;
    return 0;
}

// The host's math library reports its errors through errno, never to a matherr.
uint64_t __setusermatherr(uint64_t handler) {
    (void)handler;
    return 0;
}

uint64_t _configthreadlocale(uint64_t mode) {
    int previous = win32_locale_mode;
    if ((int32_t)mode == 1 || (int32_t)mode == 2) win32_locale_mode = (int)mode;
    else if ((int32_t)mode != 0 && (int32_t)mode != -1) return (uint64_t)-1;
    return (uint64_t)(int64_t)previous;
}

uint64_t _set_new_mode(uint64_t mode) {
    int previous = win32_new_mode;
    win32_new_mode = (int)mode;
    return (uint64_t)(int64_t)previous;
}

uint64_t _configure_narrow_argv(uint64_t mode) {
    (void)mode;
    return 0;
}

uint64_t _initialize_narrow_environment(void) {
    return 0;
}

// No signal handler is installed: every exception goes on to the next handler.
uint64_t _seh_filter_exe(uint64_t code, uint64_t pointers) {
    (void)code;
    (void)pointers;
    return 0;  // EXCEPTION_CONTINUE_SEARCH
}

uint64_t __current_exception(void) {
    return (uint64_t)(uintptr_t)&win32_current_exception;
}

uint64_t __current_exception_context(void) {
    return (uint64_t)(uintptr_t)&win32_current_exception_context;
}

uint64_t terminate(void) {
    abort();
}

// The functions _crt_atexit registers and the thread-local destructors' callback, run at the exit
// as the runtime runs them: the callback first, then the functions in reverse, once.
static uint64_t win32_atexit_functions[256];
static size_t win32_atexit_count;
static uint64_t win32_tls_exit_callback;

static void win32_run_atexit(void) {
    if (win32_tls_exit_callback) {
        uint64_t callback = win32_tls_exit_callback;
        win32_tls_exit_callback = 0;
        ((native8)(uintptr_t)callback)(0, 0, 0, 0, 0, 0, 0, 0);  // (NULL, DLL_PROCESS_DETACH, NULL)
    }
    while (win32_atexit_count) ((native8)(uintptr_t)win32_atexit_functions[--win32_atexit_count])(0, 0, 0, 0, 0, 0, 0, 0);
}

uint64_t _crt_atexit(uint64_t function) {
    static int registered;
    if (!registered) registered = atexit(win32_run_atexit) == 0;
    if (!registered || win32_atexit_count >= sizeof win32_atexit_functions / sizeof win32_atexit_functions[0]) return (uint64_t)-1;
    win32_atexit_functions[win32_atexit_count++] = function;
    return 0;
}

uint64_t _register_thread_local_exe_atexit_callback(uint64_t callback) {
    win32_tls_exit_callback = callback;
    return 0;
}

uint64_t _cexit(void) {
    win32_run_atexit();
    return 0;
}

uint64_t _c_exit(void) {
    return 0;
}

#if RA_POINTER_SIZE == 8
// The 64-bit program's tables of 8-byte pointers, which a 32-bit program's shims read as dwords.
uint64_t __p___argv(void) {
    return (uint64_t)(uintptr_t)&ra_argv;
}

uint64_t _get_initial_narrow_environment(void) {
    extern char **environ;
    return (uint64_t)(uintptr_t)environ;
}

uint64_t _initterm(uint64_t first, uint64_t last) {
    for (uint64_t *p = (uint64_t *)W_PTR(first); (uintptr_t)p < (uintptr_t)last; p++) {
        if (*p) ((native8)(uintptr_t)*p)(0, 0, 0, 0, 0, 0, 0, 0);
    }
    return 0;
}

uint64_t _initterm_e(uint64_t first, uint64_t last) {
    for (uint64_t *p = (uint64_t *)W_PTR(first); (uintptr_t)p < (uintptr_t)last; p++) {
        if (!*p) continue;
        uint32_t result = (uint32_t)((native8)(uintptr_t)*p)(0, 0, 0, 0, 0, 0, 0, 0);
        if (result) return result;
    }
    return 0;
}

uint64_t _initialize_onexit_table(uint64_t table) {
    if (!table) return (uint64_t)-1;
    memset(W_PTR(table), 0, 24);
    return 0;
}

// The table's three pointers are the program's: first, last and end of an array the layer grows.
uint64_t _register_onexit_function(uint64_t table, uint64_t function) {
    uint64_t *t = (uint64_t *)W_PTR(table);
    if (t[1] == t[2]) {
        size_t count = (size_t)(t[1] - t[0]) / 8, room = count ? count * 2 : 32;
        uint64_t *grown = realloc(W_PTR(t[0]), room * 8);
        if (!grown) return (uint64_t)-1;
        t[0] = (uint64_t)(uintptr_t)grown;
        t[1] = t[0] + count * 8;
        t[2] = t[0] + room * 8;
    }
    *(uint64_t *)W_PTR(t[1]) = function;
    t[1] += 8;
    return 0;
}

uint64_t _execute_onexit_table(uint64_t table) {
    uint64_t *t = (uint64_t *)W_PTR(table);
    while (t[1] > t[0]) {
        t[1] -= 8;
        uint64_t function = *(uint64_t *)W_PTR(t[1]);
        if (function) ((native8)(uintptr_t)function)(0, 0, 0, 0, 0, 0, 0, 0);
    }
    free(W_PTR(t[0]));
    memset(t, 0, 24);
    return 0;
}

uint64_t _controlfp_s(uint64_t current, uint64_t new_bits, uint64_t mask) {
    return ra_x86_controlfp_s(current, new_bits, mask);
}
#endif

// The thread block's process and thread ids and its stack, as the program's width lays them out.
static void win32_describe_thread(uint8_t *block) {
    uintptr_t base = 0, limit = 0;
#ifdef __linux__
    pthread_attr_t attr;
    if (pthread_getattr_np(pthread_self(), &attr) == 0) {
        void *low;
        size_t size;
        if (pthread_attr_getstack(&attr, &low, &size) == 0) {
            limit = (uintptr_t)low;
            base = limit + size;
        }
        pthread_attr_destroy(&attr);
    }
#endif
    uint32_t pid = (uint32_t)GetCurrentProcessId(), tid = (uint32_t)GetCurrentThreadId();
#if RA_POINTER_SIZE == 4
    *(uint32_t *)(block + 0x04) = (uint32_t)base;
    *(uint32_t *)(block + 0x08) = (uint32_t)limit;
    *(uint32_t *)(block + 0x20) = pid;
    *(uint32_t *)(block + 0x24) = tid;
#else
    *(uint64_t *)(block + 0x08) = base;
    *(uint64_t *)(block + 0x10) = limit;
    *(uint64_t *)(block + 0x40) = pid;
    *(uint64_t *)(block + 0x48) = tid;
#endif
}

// The heap stays in the brk area right above a non-PIE executable: a 32-bit program's pointers
// reach it. glibc gives a thread other than the first an arena of its own, mapped high, and a large
// block a mapping of its own: one arena for every thread, the program's low one among them, and
// no block mapped, before that thread starts.
static void win32_prepare(void) {
#if defined(__GLIBC__) && RA_POINTER_SIZE == 4
    mallopt(M_MMAP_MAX, 0);
    mallopt(M_ARENA_MAX, 1);
#elif RA_POINTER_SIZE == 4
#error "a 32-bit program's heap stays below 2 GB by glibc's malloc options: its Win32 layer needs glibc"
#endif
}
#endif  // the Win32 layer

#if defined(RA_ABI) && !defined(RA_DLL)
// The entry, and the program's exit code: `main`'s result when the driver's startup stands in for
// the runtime's (`RA_ENTRY_RETURNS`), else nothing the driver reads. Standing in, it first runs the
// program's initializers (`RA_INITIALIZE`), the constructors of its C++ statics among them, and a
// C initializer that fails ends the start with the code the runtime's startup ends it with.
static int ra_entry(void) {
#ifdef RA_INITIALIZE
    if (RA_INITIALIZE()) return 255;
#endif
#ifdef RA_ENTRY_RETURNS
    int code = (int)RA_ENTRY_CALL();
#else
    int code = 0;
    RA_ENTRY_CALL();
#endif
    eh_check_uncaught();
    return code;
}

#if !defined(_WIN32) && defined(RA_WIN32)
#if RA_POINTER_SIZE == 4
// A 32-bit program runs on a thread whose stack and thread block its pointers reach, and exits
// there, so the functions it registered for the exit run on that stack as well.
static void *win32_main_thread(void *unused) {
    (void)unused;
    report_crashes_on_this_thread();
    attach_process();
    exit(ra_entry());
}
#endif

static int win32_main(void) {
    win32_prepare();
#if RA_POINTER_SIZE == 4
    pthread_t thread;
    if (win32_start(&thread, 8u << 20, win32_main_thread, NULL, 0) != 0) {
        fprintf(stderr, "ra: no room below 2 GB for the program's stack\n");
        return 2;
    }
    pthread_join(thread, NULL);
    return 2;
#else
    attach_process();
    return ra_entry();
#endif
}
#endif
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
#ifdef RA_RESIDUAL
    residual_start();
#endif
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
#if !defined(_WIN32) && defined(RA_WIN32)
    return win32_main();
#else
    attach_process();
    return ra_entry();
#endif
#else
    const char *image_path = argc > 1 ? argv[1] : "image.bin";
#ifdef _WIN32
    FILE *f = NULL;
    if (fopen_s(&f, image_path, "rb") != 0) f = NULL;
#else
    FILE *f = fopen(image_path, "rb");
#endif
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
#if !defined(_WIN32) && defined(RA_WIN32)
    return win32_main();
#else
    attach_process();
    return ra_entry();
#endif
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
