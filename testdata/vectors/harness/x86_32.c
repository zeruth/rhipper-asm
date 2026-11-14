// The 32-bit x86 harness of the instruction vectors: a Windows program with no C runtime, built
// by clang for i686 and run under WOW64 on the x64 machine, where the 64-bit instruction oracle
// cannot run the 32-bit-only forms (pushad, the x87 unit, the 16-bit stack). It maps the arena and
// the code page at the addresses the vectors name, reads cases from the file named first on the
// command line (or standard input), runs each through a prologue in the code page and writes the
// machine's state after it to the file named second (or standard output). Every case ends in an
// exception: the breakpoint after the instruction, the breakpoint fill a branch lands on, or the
// fault the instruction itself raises. A vectored handler copies the registers out of the
// exception's context, so the state is the machine's at the moment the case stopped, and resumes
// the host with its own registers.
//
// Built by build.ps1 beside this file and by `harness_runs_three_cases` in
// crates/ra-asm-lift/tests/vectors_gen.rs:
//   clang -target i686-pc-windows-msvc -O1 -nostdlib -fno-builtin -fuse-ld=lld x86_32.c
//         -o x86_32.exe -lkernel32 -Wl,/entry:mainCRTStartup -Wl,/subsystem:console
//   x86_32.exe cases.bin results.bin
//
// Windows delivers an exception on the stack the case left: the dispatcher writes its context
// below the case's final esp, so the arena below that esp (up to DISPATCH_LEN) is not the case's
// and is zeroed uncounted; a window must lie at or above the final esp, or away from the stack.
//
// The x87 unit goes along as a 108-byte fsave image: the prologue loads the case's with frstor,
// and once the handler has resumed the host, fnsave takes the unit as the case left it (the
// exception's round trip through the kernel keeps the thread's floating state, and nothing on the
// way touches the unit) and reinitializes it for the next case.

#include <windows.h>

typedef unsigned int u32;
typedef unsigned char u8;

// The addresses the vectors name: the arena the loads, stores and the stack use, and the page
// the case runs in. The replay maps the same arena in its own process.
#define ARENA 0x20000000u
#define ARENA_LEN 0x10000u
#define CODE_MAP 0x30000000u
#define CODE_MAP_LEN 0x10000u
#define CASE_IP 0x30002000u
#define DISPATCH_LEN 0x2000u

#define WINDOW 64
#define CODE_MAX 16
// The 32-bit protected-mode fsave image: the control, status and tag words and the pointers in
// seven dwords, then the eight registers of ten bytes, st(0) first.
#define FSAVE_LEN 108

// The flags a case sets and reads: CF, PF, AF, ZF, SF, DF and OF. The rest of EFLAGS is the host's.
#define FLAG_MASK 0x0cd5u
#define HOST_EFLAGS 0x202u

// One case in: the registers (eax, ecx, edx, ebx, esp, ebp, esi, edi), the flags, the length of
// the encoding, the window's address, the encoding's bytes, the window of data bytes and the x87
// unit's fsave image.
struct case_in {
    u32 r[8];
    u32 eflags;
    u32 len;
    u32 window_at;
    u32 pad;
    u8 code[CODE_MAX];
    u8 window[WINDOW];
    u8 fpu[FSAVE_LEN];
    u32 pad2;
};

// One case out: the exception that ended it, the address it ended at (the breakpoint the case ran
// into, or the faulting instruction), the address a faulting access touched, the flags and the
// registers as the context held them, the first address outside the window a store reached (plus
// one, zero for none), the window as the case left it and the x87 unit's fsave image after it.
struct case_out {
    u32 code;
    u32 eip;
    u32 fault_at;
    u32 eflags;
    u32 r[8];
    u32 stray;
    u32 pad;
    u8 window[WINDOW];
    u8 fpu[FSAVE_LEN];
    u32 pad2;
};

_Static_assert(sizeof(struct case_in) == 240, "the layout the generator writes");
_Static_assert(__builtin_offsetof(struct case_in, eflags) == 32, "the prologue's flags load");
_Static_assert(__builtin_offsetof(struct case_in, fpu) == 128, "the prologue's frstor");
_Static_assert(sizeof(struct case_out) == 232, "the layout the generator reads");

static struct case_in g_in;
static struct case_out g_out;
// The host's ebx, esi, edi, ebp and esp at the call into the prologue; the handler resumes with them.
static u32 saved[5];
static volatile int running;

// The freestanding build still needs these names for a struct copy or a cleared array.
void *memcpy(void *to, const void *from, size_t len) {
    u8 *t = to;
    const u8 *f = from;
    while (len--) {
        *t++ = *f++;
    }
    return to;
}

void *memset(void *to, int byte, size_t len) {
    u8 *t = to;
    while (len--) {
        *t++ = (u8)byte;
    }
    return to;
}

static void exit_with(int code) {
    ExitProcess((UINT)code);
}

// Reads `len` bytes, or fewer at the end of the input: 0 at a clean end, -1 on a short record.
static int read_all(HANDLE in, u8 *buf, u32 len) {
    u32 got = 0;
    while (got < len) {
        DWORD n = 0;
        if (!ReadFile(in, buf + got, len - got, &n, NULL) || n == 0) {
            break;
        }
        got += n;
    }
    if (got == 0) {
        return 0;
    }
    return got == len ? (int)len : -1;
}

static int write_all(HANDLE out, const u8 *buf, u32 len) {
    u32 done = 0;
    while (done < len) {
        DWORD n = 0;
        if (!WriteFile(out, buf + done, len - done, &n, NULL) || n == 0) {
            return -1;
        }
        done += n;
    }
    return 0;
}

// The case stopped: its state out of the context, the host's registers back in, and the host
// resumed at the prologue's return address with clean flags.
static LONG CALLBACK handler(EXCEPTION_POINTERS *info) {
    if (!running) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    running = 0;
    EXCEPTION_RECORD *record = info->ExceptionRecord;
    CONTEXT *c = info->ContextRecord;
    g_out.code = (u32)record->ExceptionCode;
    g_out.eip = (u32)(ULONG_PTR)record->ExceptionAddress;
    g_out.fault_at = record->NumberParameters >= 2 ? (u32)record->ExceptionInformation[1] : 0;
    g_out.eflags = c->EFlags & FLAG_MASK;
    g_out.r[0] = c->Eax;
    g_out.r[1] = c->Ecx;
    g_out.r[2] = c->Edx;
    g_out.r[3] = c->Ebx;
    g_out.r[4] = c->Esp;
    g_out.r[5] = c->Ebp;
    g_out.r[6] = c->Esi;
    g_out.r[7] = c->Edi;
    c->Ebx = saved[0];
    c->Esi = saved[1];
    c->Edi = saved[2];
    c->Ebp = saved[3];
    c->Esp = saved[4] + 4;
    c->Eip = *(u32 *)saved[4];
    c->EFlags = HOST_EFLAGS;
    return EXCEPTION_CONTINUE_EXECUTION;
}

static void map_fixed(u32 at, u32 len, DWORD protect) {
    void *got = VirtualAlloc((void *)at, len, MEM_RESERVE | MEM_COMMIT, protect);
    if ((u32)got != at) {
        exit_with(4);
    }
}

// region The prologue

// The machine code before the case, hand-assembled into the page: the host's callee-saved
// registers and esp into `saved` (the return address of the call sits at that esp), the x87 unit
// from the case's fsave image, the flags through popfd while the host stack is still in place,
// then every register from the case, eax last since it holds the case's address.
static u8 *emit;

static void emit_u32(u32 v) {
    for (int i = 0; i < 4; i++) {
        *emit++ = (u8)(v >> (8 * i));
    }
}

// `mov [abs32], r32`
static void emit_store_abs(u32 reg, u32 at) {
    *emit++ = 0x89;
    *emit++ = (u8)(reg << 3 | 5);
    emit_u32(at);
}

// `mov r32, [eax + disp8]`
static void emit_load_eax(u32 reg, u8 disp) {
    *emit++ = 0x8b;
    *emit++ = (u8)(0x40 | reg << 3);
    *emit++ = disp;
}

#define EAX 0
#define ECX 1
#define EDX 2
#define EBX 3
#define ESP 4
#define EBP 5
#define ESI 6
#define EDI 7

#define PROLOGUE_LEN 68

static void write_prologue(void) {
    emit = (u8 *)(CASE_IP - PROLOGUE_LEN);
    emit_store_abs(EBX, (u32)&saved[0]);
    emit_store_abs(ESI, (u32)&saved[1]);
    emit_store_abs(EDI, (u32)&saved[2]);
    emit_store_abs(EBP, (u32)&saved[3]);
    emit_store_abs(ESP, (u32)&saved[4]);
    // frstor [g_in.fpu]
    *emit++ = 0xdd;
    *emit++ = 0x25;
    emit_u32((u32)g_in.fpu);
    // mov eax, g_in
    *emit++ = 0xb8;
    emit_u32((u32)&g_in);
    // push dword [eax + 32]; popfd
    *emit++ = 0xff;
    *emit++ = 0x70;
    *emit++ = 32;
    *emit++ = 0x9d;
    for (u32 reg = ECX; reg <= EDI; reg++) {
        emit_load_eax(reg, (u8)(4 * reg));
    }
    // mov eax, [eax]
    *emit++ = 0x8b;
    *emit++ = 0x00;
    if ((u32)emit != CASE_IP) {
        exit_with(8);
    }
}

// The page: breakpoints throughout, the prologue before the case.
static void fill_page(void) {
    memset((void *)CODE_MAP, 0xcc, CODE_MAP_LEN);
    write_prologue();
}

// The case's bytes into the page, the fill restored after a longer case before.
static void place_case(void) {
    u8 *at = (u8 *)CASE_IP;
    memset(at, 0xcc, CODE_MAX);
    for (u32 i = 0; i < g_in.len && i < CODE_MAX; i++) {
        at[i] = g_in.code[i];
    }
}

// endregion

// region The command line

// The next token of the command line into `buf`: a quoted run or a run without spaces. Whether
// there was one.
static int next_token(const char **line, char *buf, u32 size) {
    const char *p = *line;
    while (*p == ' ' || *p == '\t') {
        p++;
    }
    if (*p == 0) {
        return 0;
    }
    u32 n = 0;
    if (*p == '"') {
        p++;
        while (*p != 0 && *p != '"') {
            if (n + 1 < size) {
                buf[n++] = *p;
            }
            p++;
        }
        if (*p == '"') {
            p++;
        }
    } else {
        while (*p != 0 && *p != ' ' && *p != '\t') {
            if (n + 1 < size) {
                buf[n++] = *p;
            }
            p++;
        }
    }
    buf[n] = 0;
    *line = p;
    return 1;
}

// endregion

int mainCRTStartup(void) {
    HANDLE in = GetStdHandle(STD_INPUT_HANDLE);
    HANDLE out = GetStdHandle(STD_OUTPUT_HANDLE);
    static char in_name[MAX_PATH];
    static char out_name[MAX_PATH];
    const char *line = GetCommandLineA();
    next_token(&line, in_name, sizeof in_name);
    if (next_token(&line, in_name, sizeof in_name) && next_token(&line, out_name, sizeof out_name)) {
        in = CreateFileA(in_name, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
        out = CreateFileA(out_name, GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
        if (in == INVALID_HANDLE_VALUE || out == INVALID_HANDLE_VALUE) {
            exit_with(2);
        }
    }
    map_fixed(ARENA, ARENA_LEN, PAGE_READWRITE);
    map_fixed(CODE_MAP, CODE_MAP_LEN, PAGE_EXECUTE_READWRITE);
    if (AddVectoredExceptionHandler(1, handler) == NULL) {
        exit_with(6);
    }
    fill_page();

    void (*prologue)(void) = (void (*)(void))(CASE_IP - PROLOGUE_LEN);
    volatile u32 *arena = (volatile u32 *)ARENA;
    for (;;) {
        int n = read_all(in, (u8 *)&g_in, sizeof g_in);
        if (n == 0) {
            break;
        }
        if (n < 0) {
            exit_with(3);
        }
        if (g_in.window_at < ARENA || g_in.window_at + WINDOW > ARENA + ARENA_LEN) {
            exit_with(7);
        }
        g_in.eflags = (g_in.eflags & FLAG_MASK) | HOST_EFLAGS;
        place_case();
        volatile u8 *window = (volatile u8 *)g_in.window_at;
        for (int i = 0; i < WINDOW; i++) {
            window[i] = g_in.window[i];
        }

        running = 1;
        prologue();
        // The unit as the case left it; fnsave then reinitializes it, so nothing pends into the
        // next case's frstor.
        __asm__ __volatile__("fnsave %0" : "=m"(g_out.fpu));

        for (int i = 0; i < WINDOW; i++) {
            g_out.window[i] = window[i];
            window[i] = 0;
        }
        // The exception dispatcher's frames below the case's final esp are not the case's stores.
        u32 esp = g_out.r[4];
        if (esp > ARENA && esp <= ARENA + ARENA_LEN) {
            u32 from = esp - ARENA > DISPATCH_LEN ? esp - DISPATCH_LEN : ARENA;
            memset((void *)from, 0, esp - from);
        }
        g_out.stray = 0;
        for (u32 i = 0; i < ARENA_LEN / 4; i++) {
            if (arena[i] != 0) {
                if (g_out.stray == 0) {
                    g_out.stray = ARENA + i * 4 + 1;
                }
                arena[i] = 0;
            }
        }
        if (write_all(out, (const u8 *)&g_out, sizeof g_out) != 0) {
            exit_with(5);
        }
    }
    exit_with(0);
    return 0;
}
