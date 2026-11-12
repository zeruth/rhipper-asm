// Files through the Win32 API, without a C runtime: a file created and written, reopened and read
// back, its attributes asked, removed, and the error of opening what is not there. The calls take
// the SDK's named constants (GENERIC_READ, CREATE_ALWAYS, ERROR_FILE_NOT_FOUND), which the emit
// names again from the import's prototype. The file is named by the thread, so the oracles can run
// the program side by side in one directory.
#include <windows.h>

#define NOINLINE __attribute__((noinline))

static HANDLE out;
static char path[32];

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

NOINLINE static DWORD write_text(const char *name, const char *text) {
    HANDLE f = CreateFileA(name, GENERIC_READ | GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
    if (f == INVALID_HANDLE_VALUE) return 0;
    const char *p = text;
    while (*p) p++;
    DWORD n = 0;
    WriteFile(f, text, (DWORD)(p - text), &n, NULL);
    CloseHandle(f);
    return n;
}

NOINLINE static DWORD read_text(const char *name, char *buf, DWORD cap) {
    HANDLE f = CreateFileA(name, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL);
    if (f == INVALID_HANDLE_VALUE) return 0;
    DWORD n = 0;
    ReadFile(f, buf, cap, &n, NULL);
    CloseHandle(f);
    return n;
}

NOINLINE static const char *open_error(const char *name) {
    HANDLE f = CreateFileA(name, GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, 0, NULL);
    if (f != INVALID_HANDLE_VALUE) {
        CloseHandle(f);
        return "opened";
    }
    switch (GetLastError()) {
    case ERROR_FILE_NOT_FOUND: return "not found";
    case ERROR_PATH_NOT_FOUND: return "no path";
    case ERROR_ACCESS_DENIED: return "denied";
    default: return "other";
    }
}

NOINLINE static int is_plain_file(const char *name) {
    DWORD attrs = GetFileAttributesA(name);
    return attrs != INVALID_FILE_ATTRIBUTES && (attrs & FILE_ATTRIBUTE_DIRECTORY) == 0;
}

int mainCRTStartup(void) {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    // files-<thread id>.tmp
    char *p = path + sizeof path - 1;
    *p = 0;
    *--p = 'p', *--p = 'm', *--p = 't', *--p = '.';
    DWORD id = GetCurrentThreadId();
    do {
        *--p = (char)('0' + id % 10);
        id /= 10;
    } while (id);
    *--p = '-', *--p = 's', *--p = 'e', *--p = 'l', *--p = 'i', *--p = 'f';
    print_int((int)write_text(p, "hello, file\r\n"));
    char buf[64];
    DWORD n = read_text(p, buf, sizeof buf - 1);
    buf[n] = 0;
    print_str(buf);
    print_int((int)n);
    print_int(is_plain_file(p));
    print_int(DeleteFileA(p) != 0);
    print_str(open_error(p));
    print_str("\r\n");
    print_str(open_error("no-such-dir\\x.tmp"));
    print_str("\r\n");
    ExitProcess(0);
    return 0;
}
