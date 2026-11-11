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

__declspec(noreturn) void ra_unsupported(uint64_t address, uint64_t code) {
    fprintf(stderr, "ra: an instruction the lift does not cover at 0x%llx (code %llu)\n", (unsigned long long)address, (unsigned long long)code);
    fflush(stderr);
    ExitProcess(0xE0000002);
}

typedef uint64_t (*native8)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);

// An indirect call in the machine model: a lifted function by its address, else native code
// through the universal adapter. The lifted caller pushed the return address; native code does
// not pop it, so this does.
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
    uint64_t *stack = (uint64_t *)(state->gpr[RSP] + 8);  // past the pushed return address
    native8 f = (native8)target;
    state->gpr[RAX] = f(state->gpr[RCX], state->gpr[RDX], state->gpr[R8], state->gpr[R9], stack[4], stack[5], stack[6], stack[7]);
    state->gpr[RSP] += 8;
}

static void bind_imports(void) {
    for (int i = 0; ra_imports[i].slot != 0; i++) {
        HMODULE module = LoadLibraryA(ra_imports[i].dll);
        if (!module) {
            fprintf(stderr, "ra: cannot load %s\n", ra_imports[i].dll);
            ExitProcess(0xE0000004);
        }
        FARPROC f = ra_imports[i].name ? GetProcAddress(module, ra_imports[i].name) : GetProcAddress(module, MAKEINTRESOURCEA(ra_imports[i].ordinal));
        if (!f) {
            fprintf(stderr, "ra: %s has no %s\n", ra_imports[i].dll, ra_imports[i].name ? ra_imports[i].name : "(ordinal)");
            ExitProcess(0xE0000004);
        }
        *(uint64_t *)(uintptr_t)ra_imports[i].slot = (uint64_t)(uintptr_t)f;
    }
}

#ifdef RA_ABI
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

int main(int argc, char **argv) {
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
    uint8_t *stack = VirtualAlloc(NULL, stack_size, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    if (!stack) return 2;
    // The entry sees rsp as after a call: 8 past a 16-byte boundary, with room above it.
    uint64_t top = ((uint64_t)(uintptr_t)(stack + stack_size - 64)) & ~15ULL;
    state.gpr[RSP] = top - 8;
    *(uint64_t *)(uintptr_t)state.gpr[RSP] = 0;  // the return address of the entry
    state.gpr[RCX] = 0;
    state.flags[5] = 0;  // DF clear
    RA_ENTRY(&state);
    return (int)state.gpr[RAX];
#endif
#endif
}
