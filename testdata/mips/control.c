// The control fixture of testdata/src/control.c as a static MIPS32 little-endian Linux executable
// (the o32 ABI, ISA II): the same loops, switch, recursion, tables and globals, over a `_start` of
// its own and `write` and `exit` by `syscall`, no C library. Built without PIC and abicalls, so a
// call is a `jal` (`R_MIPS_26`), an address a `lui`/`addiu` pair (`R_MIPS_HI16`, `R_MIPS_LO16`),
// a pointer in data an `R_MIPS_32`, and the small globals sit in `.sdata`/`.sbss` at `$gp`, which
// `_start` sets from `_gp` before anything reads them. `--emit-relocs` keeps the relocations in the
// file. In WSL, from the repository's root:
//
//   clang --target=mipsel-linux-gnu -march=mips2 -O1 -static -nostdlib -fno-pic -mno-abicalls
//     -fuse-ld=lld -Wl,--entry=_start -Wl,--emit-relocs -Wl,--build-id=none
//     testdata/mips/control.c -o testdata/mips/control
//
// It prints what the Windows program prints, with LF line ends: testdata/mips/control.txt.

#define NOINLINE __attribute__((noinline))

// The o32 syscall numbers: Linux's `asm/unistd_o32.h`.
#define SYS_WRITE 4004
#define SYS_EXIT_GROUP 4246

// The kernel returns the result in $v0 and an error flag in $a3; a set flag makes $v0 the errno.
static long syscall3(long number, long a, long b, long c) {
    register long v0 __asm__("$2") = number;
    register long a0 __asm__("$4") = a;
    register long a1 __asm__("$5") = b;
    register long a2 __asm__("$6") = c;
    register long a3 __asm__("$7");
    __asm__ volatile("syscall"
                     : "+r"(v0), "=r"(a3)
                     : "r"(a0), "r"(a1), "r"(a2)
                     : "$1", "$3", "$8", "$9", "$10", "$11", "$12", "$13", "$14", "$15", "$24", "$25", "hi", "lo", "memory");
    return a3 ? -v0 : v0;
}

NOINLINE long write(int fd, const void *buf, unsigned long n) { return syscall3(SYS_WRITE, fd, (long)buf, (long)n); }

NOINLINE __attribute__((noreturn)) void exit(int status) {
    for (;;) syscall3(SYS_EXIT_GROUP, status, 0, 0);
}

static int out = 1;

static int table[10] = {5, 3, 8, 1, 9, 2, 7, 4, 6, 0};
static volatile int sink;
static const char *names[] = {"zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"};

NOINLINE static void print_str(const char *s) {
    const char *p = s;
    while (*p) p++;
    write(out, s, (unsigned long)(p - s));
}

NOINLINE static void print_int(int v) {
    char buf[16];
    char *p = buf + sizeof buf;
    unsigned u = v < 0 ? (unsigned)(-v) : (unsigned)v;
    *--p = '\n';
    do {
        *--p = (char)('0' + u % 10);
        u /= 10;
    } while (u);
    if (v < 0) *--p = '-';
    write(out, p, (unsigned long)(buf + sizeof buf - p));
}

NOINLINE __attribute__((noreturn)) static void fail(int code) {
    print_str("fail\n");
    exit(code);
}

NOINLINE static int collatz(int n) {
    int steps = 0;
    while (n != 1) {
        n = (n & 1) ? 3 * n + 1 : n / 2;
        steps++;
    }
    return steps;
}

NOINLINE static int fib(int n) { return n < 2 ? n : fib(n - 1) + fib(n - 2); }

NOINLINE static int classify(int x) {
    // Each case does different work and calls out, so the compiler keeps a jump table.
    switch (x) {
    case 0: print_str("A"); return 10;
    case 1: print_str("B"); sink += 2; return 11;
    case 2: print_str("C"); sink *= 3; return 12;
    case 3: print_str("D"); return collatz(27);
    case 4: print_str("E"); sink ^= 7; return 14;
    case 5: print_str("F"); return fib(10);
    case 6: print_str("G"); sink -= 1; return 16;
    case 7: print_str("H"); return table[x];
    case 8: print_str("I"); sink |= 9; return 18;
    default: print_str("?"); return -1;
    }
}

NOINLINE static int op_add(int a, int b) { return a + b; }
NOINLINE static int op_sub(int a, int b) { return a - b; }
NOINLINE static int op_mul(int a, int b) { return a * b; }
NOINLINE static int op_div(int a, int b) {
    if (b == 0) fail(3);
    return a / b;
}

static int (*const ops[4])(int, int) = {op_add, op_sub, op_mul, op_div};

NOINLINE static int sum_table(void) {
    int s = 0;
    for (int i = 0; i < 10; i++) s += table[i];
    return s;
}

NOINLINE static void sort_table(void) {
    for (int i = 1; i < 10; i++) {
        int v = table[i], j = i - 1;
        while (j >= 0 && table[j] > v) {
            table[j + 1] = table[j];
            j--;
        }
        table[j + 1] = v;
    }
}

NOINLINE static unsigned mix(unsigned a, unsigned b) {
    unsigned h = a * 2654435761u;
    h ^= b >> 3;
    h = (h << 7) | (h >> 25);
    return h ^ (h >> 16);
}

// `write` called through a pointer kept in data, twice: once through the word stored from the
// function's address, once through a second word copied from the first. Volatile, so the compiler
// keeps the loads and the calls through them.
typedef long (*write_fn)(int, const void *, unsigned long);
static write_fn volatile write_slot;
static write_fn volatile write_proc;

NOINLINE static void print_slot(const char *s) {
    const char *p = s;
    while (*p) p++;
    write_slot(out, s, (unsigned long)(p - s));
}

NOINLINE static void print_proc(const char *s) {
    const char *p = s;
    while (*p) p++;
    write_proc(out, s, (unsigned long)(p - s));
}

NOINLINE __attribute__((noreturn)) void control(void) {
    write_slot = write;
    write_proc = write_slot;
    print_slot("slot\n");
    print_proc("proc\n");
    print_int(collatz(27));
    print_int(fib(15));
    int total = 0;
    for (int i = 0; i <= 9; i++) total += classify(i);
    print_str("\n");
    print_int(total);
    for (int i = 0; i < 4; i++) print_int(ops[i](42, 6));
    print_int(sum_table());
    sort_table();
    for (int i = 0; i < 10; i++) print_str(names[table[i]]), print_str(i == 9 ? "\n" : " ");
    print_int((int)(mix(123456789u, 987654321u) % 100000u));
    if (sink == 0x7fffffff) fail(2);
    print_int(sink);
    exit(0);
}

// The entry: the kernel leaves $gp unset and the stack 8-byte aligned with the arguments on it;
// o32 wants the caller to reserve 16 bytes of argument space before a call. Each `jal` carries its
// own `nop` in the delay slot rather than leaving the assembler to place one.
__asm__(".text\n"
        ".globl _start\n"
        ".type _start, @function\n"
        ".set noreorder\n"
        "_start:\n"
        "    lui $gp, %hi(_gp)\n"
        "    addiu $gp, $gp, %lo(_gp)\n"
        "    addiu $sp, $sp, -16\n"
        "    jal control\n"
        "    nop\n"
        ".set reorder\n"
        ".size _start, .-_start\n");
