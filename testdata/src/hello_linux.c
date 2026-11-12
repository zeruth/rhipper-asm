// The Linux fixture: hello over the C library's `write` and `exit`, imported by name through the
// PLT, two leaf functions and one caller, no startup code of a C runtime. clang builds it for
// x86_64-linux-gnu against the stub `libc.so.6` of libc_stub.c, since no Linux C library is here:
// the binary is a Linux program that runs on Linux, and its expected output is what it prints there.
long write(int fd, const void *buf, unsigned long n);
__attribute__((noreturn)) void exit(int status);

__attribute__((noinline)) static int add(int a, int b) { return a + b; }

__attribute__((noinline)) int square(int x) { return x * x; }

__attribute__((noreturn)) void _start(void) {
    char buf[3] = {'0' + (char)(add(1, 2) % 10), '0' + (char)(square(3) % 10), '\n'};
    write(1, buf, 3);
    exit(0);
}
