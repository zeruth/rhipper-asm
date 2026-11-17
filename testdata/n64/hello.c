// The Nintendo 64 loader's fixture: a libdragon program that prints through the emulator log, the
// ISViewer buffer at 0xb3ff0020 that `debugf` writes to once `debug_init_emulog` hooks stderr.
// Built by the Makefile beside it.
#include <libdragon.h>

// A count the compiler cannot fold into a constant, so `sum` stays a call.
static volatile int limit = 10;

__attribute__((noinline)) static int sum(int n) {
    int total = 0;
    for (int i = 1; i <= n; i++) {
        total += i;
    }
    return total;
}

int main(void) {
    debug_init_emulog();
    debugf("hello from the vr4300\n");
    debugf("sum 1..10 = %d\n", sum(limit));
    return 0;
}
