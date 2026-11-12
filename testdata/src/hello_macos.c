// The macOS fixture: hello over the system library's `write` and `exit`, bound by name through the
// stubs, two leaf functions and the `main` dyld calls, no C runtime of its own. clang builds it for
// x86_64 and arm64 macOS against the stub libSystem.tbd, since no macOS SDK is here: the binaries
// are macOS programs that run on macOS, and what `ras` reads of them is the container.
long write(int fd, const void *buf, unsigned long n);
__attribute__((noreturn)) void exit(int status);

__attribute__((noinline)) static int add(int a, int b) { return a + b; }

__attribute__((noinline)) int square(int x) { return x * x; }

int main(void) {
    char buf[3] = {'0' + (char)(add(1, 2) % 10), '0' + (char)(square(3) % 10), '\n'};
    write(1, buf, 3);
    exit(0);
}
