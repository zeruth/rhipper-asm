// The Thumb-2 fixture: a Linux program for 32-bit ARM, built with -mthumb, over the C library's
// `write` and `exit` imported through the PLT (whose stubs are ARM code, so every import call
// switches the instruction set). A conditional select becomes an IT block, a loop reads a table
// of the read-only data, and -mno-movt puts the table's address in a literal pool after the code.
// clang builds it for armv7-linux-gnueabihf against the stub `libc.so.6` of libc_stub.c; no ARM
// machine is here, so it runs lifted only, and its expected output is what the source prints.
long write(int fd, const void *buf, unsigned long n);
__attribute__((noreturn)) void exit(int status);

static const int values[5] = {3, 1, 4, 1, 5};

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
    write(1, buf, (unsigned long)(p - buf));
    exit(0);
}
