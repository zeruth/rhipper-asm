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
// lifted or native code, reaches the host function.
//
// The runtime functions the lifted code calls: ra_trap, ra_unsupported and ra_call.
#include <intrin.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <windows.h>

#include "image.h"

// The machine state, as %State in the IR: 16 general registers, rip, 8 flag bytes, 16 vectors.
struct State {
    uint64_t gpr[16];
    uint64_t rip;
    uint8_t flags[8];
    __declspec(align(16)) uint8_t xmm[16][16];
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
    case 4: return "division by zero";
    case 5: return "128-bit division";
    case 6: return "__fastfail";
    case 7: return "a switch to an address outside its table";
    case 8: return "bytes that are no instruction";
    case 9: return "a software interrupt";
    default: return "a trap";
    }
}

__declspec(noreturn) void ra_trap(uint64_t address, uint64_t code) {
    fprintf(stderr, "ra: %s at 0x%llx\n", trap_name(code), (unsigned long long)address);
    fflush(stderr);
    ExitProcess(0xE0000001);
}

// The entry hook of a port built with RA_TRACE: each function names itself as it starts.
void ra_enter(const char *name) {
    fputs("> ", stderr);
    fputs(name, stderr);
    fputs("\n", stderr);
    fflush(stderr);
}

__declspec(noreturn) void ra_unsupported(uint64_t address, uint64_t code) {
    fprintf(stderr, "ra: an instruction the lift does not cover at 0x%llx (code %llu)\n", (unsigned long long)address, (unsigned long long)code);
    fflush(stderr);
    ExitProcess(0xE0000002);
}

// The processor's answers, from the host: the port runs on a processor at least as capable as
// the one the program asks about.
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
        char *block = GetEnvironmentStringsA();
        size_t count = 0;
        for (char *p = block; *p; p += strlen(p) + 1) count++;
        environment32 = (uint32_t *)calloc(count + 1, sizeof(uint32_t));
        size_t i = 0;
        for (char *p = block; *p; p += strlen(p) + 1) environment32[i++] = (uint32_t)(uintptr_t)p;
    }
    return (uint64_t)(uintptr_t)environment32;
}

// An indirect call in the machine model: a lifted function by its address, else native code
// through the universal adapter. The lifted caller pushed the return address; native code does
// not pop it, so this does. A 32-bit program passes every argument on the stack as a dword and
// takes the result in edx:eax; a port of one runs in a process whose addresses all fit in 32 bits
// (the image is linked /LARGEADDRESSAWARE:NO), so a dword is a pointer the native code can use.
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
        ExitProcess(0xE0000003);
    }
    native8 f = (native8)target;
#if RA_POINTER_SIZE == 4
    uint32_t *stack = (uint32_t *)(uintptr_t)((uint32_t)state->gpr[RSP] + 4);  // past the pushed return address
    uint64_t result = f(stack[0], stack[1], stack[2], stack[3], stack[4], stack[5], stack[6], stack[7]);
    state->gpr[RAX] = (uint32_t)result;
    state->gpr[RDX] = (uint32_t)(result >> 32);
    state->gpr[RSP] = (uint32_t)state->gpr[RSP] + 4;
#else
    uint64_t *stack = (uint64_t *)(state->gpr[RSP] + 8);  // past the pushed return address
    state->gpr[RAX] = f(state->gpr[RCX], state->gpr[RDX], state->gpr[R8], state->gpr[R9], stack[4], stack[5], stack[6], stack[7]);
    state->gpr[RSP] += 8;
#endif
}

#ifndef RA_STANDALONE
static void bind_imports(void) {
    for (int i = 0; ra_imports[i].slot != 0; i++) {
        HMODULE module = LoadLibraryA(ra_imports[i].dll);
        if (!module) {
            fprintf(stderr, "ra: cannot load %s\n", ra_imports[i].dll);
            ExitProcess(0xE0000004);
        }
        FARPROC f = ra_imports[i].name ? GetProcAddress(module, ra_imports[i].name) : GetProcAddress(module, MAKEINTRESOURCEA(ra_imports[i].ordinal));
        if (!f) {
            // The host's DLL lacks it (the x86 exception handling of the C runtime, say): the
            // slot stays empty, and a call through it would fault at address zero.
            fprintf(stderr, "ra: %s has no %s; its slot stays empty\n", ra_imports[i].dll, ra_imports[i].name ? ra_imports[i].name : "(ordinal)");
            continue;
        }
#if RA_POINTER_SIZE == 4
        *(uint32_t *)(uintptr_t)ra_imports[i].slot = (uint32_t)(uintptr_t)f;
#else
        *(uint64_t *)(uintptr_t)ra_imports[i].slot = (uint64_t)(uintptr_t)f;
#endif
    }
}

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
    FlushInstructionCache(GetCurrentProcess(), (void *)(uintptr_t)RA_IMAGE_BASE, RA_IMAGE_SIZE);
}
#endif
#endif  // RA_STANDALONE

// The thread environment block the lifted code reads through fs: (x86) or gs: (x64): the fields
// a program and its C runtime look at, in the layout of the program's pointer width, from the
// host's own thread. The stack bounds and the block's own address fit the width: a 32-bit port
// runs with every address below 4 GB. The environment block of the process stays zero.
extern uint8_t ra_teb[];

static void init_teb(void) {
    NT_TIB *tib = (NT_TIB *)NtCurrentTeb();
    uintptr_t self = (uintptr_t)ra_teb;
    uintptr_t stack_base = (uintptr_t)tib->StackBase;
    uintptr_t stack_limit = (uintptr_t)tib->StackLimit;
    uint32_t pid = GetCurrentProcessId();
    uint32_t tid = GetCurrentThreadId();
#if RA_POINTER_SIZE == 4
    *(uint32_t *)(ra_teb + 0x04) = (uint32_t)stack_base;
    *(uint32_t *)(ra_teb + 0x08) = (uint32_t)stack_limit;
    *(uint32_t *)(ra_teb + 0x18) = (uint32_t)self;
    *(uint32_t *)(ra_teb + 0x20) = pid;
    *(uint32_t *)(ra_teb + 0x24) = tid;
#else
    *(uint64_t *)(ra_teb + 0x08) = stack_base;
    *(uint64_t *)(ra_teb + 0x10) = stack_limit;
    *(uint64_t *)(ra_teb + 0x30) = self;
    *(uint64_t *)(ra_teb + 0x40) = pid;
    *(uint64_t *)(ra_teb + 0x48) = tid;
#endif
}

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
    uintptr_t module = (uintptr_t)GetModuleHandleA(NULL);
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

int main(int argc, char **argv) {
    AddVectoredExceptionHandler(1, on_exception);
    init_teb();
    ra_argc = argc;
    ra_argv = argv;
#ifdef RA_STANDALONE
    // The data sections are globals of the program and the imports are linked by name: there is
    // no image to map and nothing to bind.
    (void)argc;
    (void)argv;
    RA_ENTRY_CALL();
    return 0;
#else
    const char *image_path = argc > 1 ? argv[1] : "image.bin";
    FILE *f = fopen(image_path, "rb");
    if (!f) {
        fprintf(stderr, "ra: cannot open %s\n", image_path);
        return 2;
    }
    void *image = VirtualAlloc((void *)(uintptr_t)RA_IMAGE_BASE, RA_IMAGE_SIZE, MEM_RESERVE | MEM_COMMIT, PAGE_EXECUTE_READWRITE);
    if (image != (void *)(uintptr_t)RA_IMAGE_BASE) {
        fprintf(stderr, "ra: cannot map the image at 0x%llx (%lu)\n", (unsigned long long)RA_IMAGE_BASE, GetLastError());
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
    RA_ENTRY_CALL();
    return 0;
#else
    static struct State state;
    const size_t stack_size = 1 << 20;
    uint8_t *stack = NULL;
#if RA_POINTER_SIZE == 4
    // A 32-bit program keeps its stack pointer in 32 bits: the stack must sit below 4 GB.
    for (uintptr_t base = 0x20000000; base < 0x80000000 && !stack; base += 0x10000000) {
        stack = VirtualAlloc((void *)base, stack_size, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    }
#else
    stack = VirtualAlloc(NULL, stack_size, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
#endif
    if (!stack) return 2;
    // The entry sees the stack pointer as after a call: a pointer past a 16-byte boundary, with
    // room above it.
    uint64_t top = ((uint64_t)(uintptr_t)(stack + stack_size - 64)) & ~15ULL;
    state.gpr[RSP] = top - RA_POINTER_SIZE;
#if RA_POINTER_SIZE == 4
    *(uint32_t *)(uintptr_t)state.gpr[RSP] = 0;  // the return address of the entry
#else
    *(uint64_t *)(uintptr_t)state.gpr[RSP] = 0;  // the return address of the entry
#endif
    state.gpr[RCX] = 0;
    state.flags[5] = 0;  // DF clear
    RA_ENTRY(&state);
    return (int)state.gpr[RAX];
#endif
#endif
}
