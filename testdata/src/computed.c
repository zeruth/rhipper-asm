// Calls whose targets no reading of the image finds: three functions are reached only through
// addresses the program computes at run time, the image's base plus an offset it reads from a
// table and unmasks with a key. Nothing in the image refers to them: no call, no pointer, no
// relocation, no unwind entry. The fixture of the code the static analysis cannot know of, whose
// call is a trap until a run supplies its target. No C runtime: output goes through WriteFile.
#include <windows.h>

#define NOINLINE __attribute__((noinline))
// A function nothing names: kept without a reference, and a leaf without an unwind entry, so
// the x64 exception table does not list it either. minsize drops its alignment: it starts where
// the code before it ends, with no fill between them to mark a function's start.
#define HIDDEN __attribute__((noinline, used, nouwtable, minsize)) static

static HANDLE out;

// Not static, so the compiler emits it where it stands, ahead of the hidden functions: they
// follow a function's last byte, not the section's start.
NOINLINE void print_int(int v) {
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

HIDDEN int h_blend(int a, int b) { return a * 3 + b; }
HIDDEN int h_fold(int a, int b) { return (a ^ b) + (a >> 1); }
HIDDEN int h_weigh(int a, int b) { return a * b - a; }

// The table, written by the assembler since C has no constant for it: each word is a function's
// offset from the image's base plus the mask, a sum the linker computes. The word is neither an
// address nor an offset into the image, and it leaves no base relocation.
#define STR2(x) #x
#define STR(x) STR2(x)
#define SYM(name) STR(__USER_LABEL_PREFIX__) #name
#define MASK 0x5eed0000

__asm__(".section .rdata,\"dr\"\n"
        ".globl " SYM(masked_offsets) "\n"
        ".p2align 2\n"
        SYM(masked_offsets) ":\n"
        ".long " SYM(h_blend) "@IMGREL+" STR(MASK) "\n"
        ".long " SYM(h_fold) "@IMGREL+" STR(MASK) "\n"
        ".long " SYM(h_weigh) "@IMGREL+" STR(MASK) "\n"
        ".text\n");

extern const unsigned masked_offsets[3];

// The key is read at run time, so no constant of the code undoes the mask.
static volatile unsigned key = MASK;

typedef int (*op)(int, int);

NOINLINE static op resolve(int index) {
    char *base = (char *)GetModuleHandleW(NULL);
    return (op)(base + (masked_offsets[index] - key));
}

NOINLINE static int apply(int index, int a, int b) { return resolve(index)(a, b); }

int mainCRTStartup(void) {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    for (int i = 0; i < 3; i++) print_int(apply(i, 40, 2));
    print_int(apply(2, apply(0, 5, 6), apply(1, 9, 4)));
    ExitProcess(0);
    return 0;
}
