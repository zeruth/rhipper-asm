// A reader of a settings text in place: the handlers are told first, each of which may refuse, then
// each line is cut at its end and handed on, the empty lines and line ends skipped at the head of
// the line loop. Both ways out of the handler loop reach that loop's head, whose test of '\r' is
// the block every way on passes. No C runtime: output goes through WriteFile.
#include <windows.h>

#define NOINLINE __attribute__((noinline))

static HANDLE out;
static volatile int refuse_at = -1;
static int sections, entries, comments, told;

static char text[] = "[Window][Main]\r\nPos=60,60\r\nSize=400,300\r\n\r\n; a comment\n\n[Docking][Data]\nCollapsed=0\r\n\n";
static char second[] = "\n\r\n;only\r\nKey=1\n[Table][T]";
// Read at each call, so the compiler knows no size and keeps the loop's guard.
static volatile int text_size = sizeof text - 1, second_size = sizeof second - 1, empty_size = 0;

NOINLINE static void print_str(const char *s) {
    DWORD n = 0;
    const char *p = s;
    while (*p) p++;
    WriteFile(out, s, (DWORD)(p - s), &n, NULL);
}

NOINLINE static void print_int(int v) {
    char buf[16];
    char *p = buf + sizeof buf;
    unsigned u = v < 0 ? (unsigned)(-v) : (unsigned)v;
    *--p = '\n';
    *--p = '\r';
    do {
        *--p = (char)('0' + u % 10);
        u /= 10;
    } while (u);
    if (v < 0) *--p = '-';
    DWORD n = 0;
    WriteFile(out, p, (DWORD)(buf + sizeof buf - p), &n, NULL);
}

NOINLINE static int tell(int handler) {
    told++;
    return handler == refuse_at ? -1 : 0;
}

NOINLINE static void handle(const char *line) {
    if (*line == '[') {
        sections++;
    } else {
        entries++;
    }
}

// The line from `line` cut at its end and counted; returns its end.
NOINLINE static char *take(char *line, char *end) {
    char *le = line;
    while (le < end && *le != '\n' && *le != '\r') le++;
    *le = 0;
    if (*line == ';') {
        comments++;
    } else if (*line != 0) {
        handle(line);
    }
    return le;
}

NOINLINE static int load(char *buf, int size, int handlers) {
    for (int i = 0; i < handlers; i++) {
        if (tell(i) < 0) return -1;
    }
    char *end = buf + size;
    char *line = buf;
    while (line < end) {
        while (*line == '\n' || *line == '\r') line++;
        line = take(line, end) + 1;
    }
    return sections * 100 + entries * 10 + comments;
}

void mainCRTStartup(void) {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    print_str("lines\r\n");
    print_int(load(text, text_size, 3));
    print_int(told);
    refuse_at = 1;
    print_int(load(second, second_size, 2));
    print_int(told);
    refuse_at = -1;
    print_int(load(second, second_size, 0));
    print_int(load(text, empty_size, 1));
    print_int(sections);
    print_int(entries);
    print_int(comments);
    ExitProcess(0);
}
