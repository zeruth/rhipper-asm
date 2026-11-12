// The Thumb-2 fixture: a static Linux program for 32-bit ARM, built with -mthumb, that talks to
// the kernel through `svc` alone, so no C library and no PLT is needed. A conditional select
// becomes an IT block, a loop reads a table of the read-only data, -mno-movt puts the table's
// address in a literal pool after the code, and a division by ten becomes a long multiply. No ARM
// machine is here, so it runs lifted only, and its expected output is what the source prints.

static const int values[5] = {3, 1, 4, 1, 5};

// The EABI system call: the number in r7, the arguments from r0, the result in r0.
static long syscall3(long number, long a, long b, long c) {
    register long r7 __asm__("r7") = number;
    register long r0 __asm__("r0") = a;
    register long r1 __asm__("r1") = b;
    register long r2 __asm__("r2") = c;
    __asm__ volatile("svc #0" : "+r"(r0) : "r"(r7), "r"(r1), "r"(r2) : "memory");
    return r0;
}

__attribute__((noinline)) int square(int x) { return x * x; }

__attribute__((noinline)) int larger(int a, int b) { return a > b ? a : b; }

__attribute__((noinline)) int sum(const int *v, int n) {
    int s = 0;
    for (int i = 0; i < n; i++) {
        s += v[i];
    }
    return s;
}

__attribute__((noinline)) char *digits(char *out, unsigned value) {
    char tmp[10];
    int n = 0;
    do {
        tmp[n++] = (char)('0' + value % 10);
        value /= 10;
    } while (value != 0);
    while (n > 0) {
        *out++ = tmp[--n];
    }
    return out;
}

__attribute__((noreturn)) void _start(void) {
    char buf[32];
    char *p = buf;
    p = digits(p, (unsigned)larger(square(3), 7));
    *p++ = ' ';
    p = digits(p, (unsigned)larger(5, square(2)));
    *p++ = ' ';
    p = digits(p, (unsigned)sum(values, 5));
    *p++ = '\n';
    syscall3(4, 1, (long)buf, p - buf);
    for (;;) {
        syscall3(1, 0, 0, 0);
    }
}
