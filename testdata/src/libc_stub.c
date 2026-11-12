// The stand-in for the Linux and Android C library that hello_linux and libmain link against: the
// same names and the soname of the real one (`libc.so.6`, bionic's `libc.so`), so the program's
// dynamic section imports them as from the real one. Only the link reads it; the program runs over
// the real library.
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

unsigned long strlen(const char *s) {
    unsigned long n = 0;
    while (s[n]) {
        n++;
    }
    return n;
}
