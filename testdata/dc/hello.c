// The Dreamcast loader's fixture: a KallistiOS program that prints through `printf`, which KOS's
// dbgio sends to the serial port (or to dcload, when the program is loaded by it). Built by the
// Makefile beside it into the boot program `1ST_READ.BIN` and its scrambled form.
#include <kos.h>

static int sum(int n) {
    int total = 0;
    for (int i = 1; i <= n; i++) {
        total += i;
    }
    return total;
}

int main(void) {
    printf("hello from the sh-4\n");
    printf("sum 1..10 = %d\n", sum(10));
    return 0;
}
