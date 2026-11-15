// The PlayStation 2 loader's fixture: an EE program of ps2sdk that prints through the EE's serial
// port (`sio_puts`, the transmit FIFO at 0x1000f180) rather than `printf`, which goes through the
// IOP's fileio RPC. Built by the Makefile beside it.
#include <sio.h>

static int sum(int n) {
    int total = 0;
    for (int i = 1; i <= n; i++) {
        total += i;
    }
    return total;
}

// The decimal digits of `value` after `prefix`, into `out`.
static void format(char *out, const char *prefix, int value) {
    while (*prefix) {
        *out++ = *prefix++;
    }
    char digits[12];
    int count = 0;
    do {
        digits[count++] = (char)('0' + value % 10);
        value /= 10;
    } while (value > 0);
    while (count > 0) {
        *out++ = digits[--count];
    }
    *out = 0;
}

int main(void) {
    char line[32];
    sio_puts("hello from the emotion engine");
    format(line, "sum 1..10 = ", sum(10));
    sio_puts(line);
    return 0;
}
