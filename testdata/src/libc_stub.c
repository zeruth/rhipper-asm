// The stand-in for the Linux C library that hello_linux links against: the same names and the
// soname `libc.so.6`, so the program's dynamic section imports them as from the real one. Only the
// link reads it; the program runs over the real library on Linux.
long write(int fd, const void *buf, unsigned long n) {
    (void)fd;
    (void)buf;
    return (long)n;
}

__attribute__((noreturn)) void exit(int status) {
    (void)status;
    for (;;) {
    }
}
