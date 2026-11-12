// Obfuscation in the shapes the common tools leave, written out by hand: a function flattened the
// way OLLVM's pass does it (a dispatcher loop over a state variable, one case per original block,
// the state set to a constant or a select at each block's end), strings held encrypted and run
// through their decryptor at the use, and a bogus branch behind an opaque predicate (x * (x + 1)
// is even) over globals the program never writes. No C runtime.
#include <windows.h>

static HANDLE out;

static void put(const char *s, int n) {
    DWORD written = 0;
    WriteFile(out, s, n, &written, NULL);
}

// The strings, each byte xor its key; the decryptor writes the plaintext into the caller's buffer.
static const unsigned char enc_banner[] = {0x3b, 0x17, 0x14, 0x14, 0x19, 0x0c, 0x02, 0x58, 0x0b, 0x0c, 0x1d, 0x08, 0x0b, 0x75, 0x72};
static const unsigned char enc_steps[] = {0xa3, 0xa4, 0xb5, 0xa0, 0xa3, 0xf0};
static const unsigned char enc_tail[] = {0x60, 0x6b, 0x78, 0x6b, 0x7c, 0x03, 0x04};

__attribute__((noinline)) static void decrypt(char *dst, const unsigned char *src, int n, unsigned char key) {
    for (int i = 0; i < n; i++) {
        dst[i] = (char)(src[i] ^ key);
    }
    dst[n] = 0;
}

// Collatz steps, flattened: the state values are the arbitrary numbers the tools draw.
__attribute__((noinline)) static int steps_of(unsigned n) {
    unsigned state = 0x3a1;
    int steps = 0;
    for (;;) {
        switch (state) {
        case 0x3a1:
            state = (n == 1) ? 0x77 : 0x1f2;
            break;
        case 0x1f2:
            state = (n & 1) ? 0x5c : 0x91;
            break;
        case 0x5c:
            n = 3 * n + 1;
            state = 0x44;
            break;
        case 0x91:
            n >>= 1;
            state = 0x44;
            break;
        case 0x44:
            steps++;
            state = 0x3a1;
            break;
        case 0x77:
            return steps;
        default:
            return -1;
        }
    }
}

// The opaque predicate's globals: never written, so the compiler cannot fold the test itself.
int opaque_x, opaque_y;

__attribute__((noinline)) static void print_int(int v) {
    char buf[12];
    int i = 11;
    buf[i] = '\n';
    buf[--i] = '\r';
    do {
        buf[--i] = (char)('0' + v % 10);
        v /= 10;
    } while (v != 0);
    put(buf + i, 12 - i);
}

int mainCRTStartup(void) {
    char text[32];
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    decrypt(text, enc_banner, sizeof enc_banner, 0x78);
    put(text, sizeof enc_banner);
    for (unsigned n = 6; n <= 9; n++) {
        if (opaque_y < 10 || opaque_x * (opaque_x + 1) % 2 == 0) {
            decrypt(text, enc_steps, sizeof enc_steps, 0xd0);
            put(text, sizeof enc_steps);
            print_int(steps_of(n));
        } else {
            decrypt(text, enc_tail, sizeof enc_tail, 0x0e);
            put(text, sizeof enc_tail);
        }
    }
    ExitProcess(0);
    return 0;
}
