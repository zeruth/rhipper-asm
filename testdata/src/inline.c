// A helper the compiler inlines at -O2: score, a loop over a string, written once and called from
// three functions, with a constant seed in two of them and a variable one in the third. The
// fixture of the outlining pass, which reads the three copies back as one function with the seed
// as its parameter. No C runtime: output goes through WriteFile.
#include <windows.h>

#define NOINLINE __attribute__((noinline))

static HANDLE out;
static const char *names[] = {"zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "x-ray"};

NOINLINE static void print_int(unsigned u) {
    char buf[16];
    char *p = buf + sizeof buf;
    DWORD n = 0;
    *--p = '\n';
    *--p = '\r';
    do {
        *--p = (char)('0' + u % 10);
        u /= 10;
    } while (u);
    WriteFile(out, p, (DWORD)(buf + sizeof buf - p), &n, NULL);
}

// The letter score of a string over a seed: the alphabet position of each lowercase letter, a
// hundred for any other byte.
static inline unsigned score(const char *s, unsigned seed) {
    unsigned total = seed;
    while (*s) {
        unsigned c = (unsigned char)*s++;
        total += c - 'a' < 26u ? c - 'a' + 1 : 100u;
    }
    return total;
}

NOINLINE static void show_low(const char *s) { print_int(score(s, 1000u)); }
NOINLINE static void show_high(const char *s) { print_int(score(s, 2000u)); }
NOINLINE static void show_from(const char *s, unsigned seed) { print_int(score(s, seed)); }

int mainCRTStartup(void) {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    for (int i = 0; i < 11; i++) {
        show_low(names[i]);
        show_high(names[i]);
        show_from(names[i], (unsigned)i * 10u);
    }
    ExitProcess(0);
    return 0;
}
