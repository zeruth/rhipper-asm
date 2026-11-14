// What a fixture of a foreign Linux machine gets instead of a C library: a write and an exit that
// are the kernel's syscalls (sys_<machine>.c), a start routine that calls `main` and exits with
// its result (the same file), and a printf of %d %u %x %s %c (printf.c). Nothing here allocates,
// reads, or depends on the machine beyond the syscall file.
#ifndef FREESTANDING_H
#define FREESTANDING_H

// `write(2)`: the bytes written, or a negative errno.
long fs_write(int fd, const void *buf, unsigned long n);

// `exit_group(2)`.
__attribute__((noreturn)) void fs_exit(int status);

unsigned long fs_strlen(const char *s);

// %d %u %x %s %c and %%, no width or flags, at most 255 characters per call, to the standard
// output. Returns the count written.
int fs_printf(const char *fmt, ...);

// The fixture's entry, called by the machine's start routine; its result is the exit status.
int main(void);

#endif
