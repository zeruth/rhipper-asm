// The PSP fixture: a user-mode PRX that prints through the kernel's file manager. Its imports are
// NIDs the loader names from the SDK's stub table; its output is testdata/psp/hello.txt.
#include <pspkernel.h>
#include <pspiofilemgr.h>

PSP_MODULE_INFO("hello", PSP_MODULE_USER, 1, 0);
PSP_MAIN_THREAD_ATTR(PSP_THREAD_ATTR_USER);

static const char greeting[] = "hello from the psp\n";

static int sum(int n) {
    int total = 0;
    for (int i = 1; i <= n; i++) {
        total += i;
    }
    return total;
}

static void put_number(int value) {
    char digits[12];
    int at = sizeof digits;
    digits[--at] = '\n';
    do {
        digits[--at] = (char)('0' + value % 10);
        value /= 10;
    } while (value != 0);
    sceIoWrite(1, digits + at, sizeof digits - at);
}

int main(void) {
    sceIoWrite(1, greeting, sizeof greeting - 1);
    put_number(sum(10));
    sceKernelExitGame();
    return 0;
}
