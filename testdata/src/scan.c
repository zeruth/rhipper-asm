// A tokenizer's shape: scan skips a line's leading blanks in a loop that may return, then both
// ways reach one loop over the rest, whose head tests for a line break in two compares and which
// leaves only by a return. The structuring places that loop's head as the tail both ways share;
// the head's post-dominator is its second compare, which the loop folds into its head's
// condition, so the way past the head lies out of the loop, not at a label the C never writes.
// No C runtime: output goes through WriteFile.
#include <windows.h>

#define NOINLINE __attribute__((noinline))

static HANDLE out;
static const char *inputs[] = {"plain", "  two\nlines", "\t # comment", "a\r\nb\nc", "\n\n\nx", "x\ry", "  y\0z", " \n lead"};
static const int lengths[] = {5, 11, 11, 6, 4, 3, 5, 0};

NOINLINE static void print_int(int v) {
    char buf[16];
    char *p = buf + sizeof buf;
    unsigned u = v < 0 ? 0u - (unsigned)v : (unsigned)v;
    DWORD n = 0;
    *--p = '\n';
    *--p = '\r';
    do {
        *--p = (char)('0' + u % 10);
        u /= 10;
    } while (u);
    if (v < 0) *--p = '-';
    WriteFile(out, p, (DWORD)(buf + sizeof buf - p), &n, NULL);
}

static int lines;

NOINLINE static void count_line(void) { lines++; }

// A hundred plus the line breaks of s after its leading blanks (none are skipped when n is
// zero), the scan ending at the NUL after a byte that is no line break; -1 for a comment among
// the blanks, with *pos where it starts, -2 for a length n over the limit. The count is
// count_line's, so the loop's head is entered with no set-up of its own.
NOINLINE static int scan(const char *s, int n, int *pos) {
    const char *q = s;
    *pos = 0;
    if (n > 0) {
        while (*q == ' ' || *q == '\t') {
            if (*++q == '#') {
                *pos = (int)(q - s);
                return -1;
            }
        }
    }
    if (n > 64) return -2;
    for (;;) {
        char c = *q;
        if (c == '\n' || c == '\r') {
            count_line();
            q++;
            continue;
        }
        if (*++q == 0) return 100;
    }
}

int mainCRTStartup(void) {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    for (int k = 0; k < (int)(sizeof inputs / sizeof *inputs); k++) {
        int pos = -9;
        lines = 0;
        print_int(scan(inputs[k], lengths[k], &pos));
        print_int(pos);
        print_int(lines);
    }
    int pos = -9;
    print_int(scan("long", 65, &pos));
    print_int(pos);
    ExitProcess(0);
    return 0;
}
