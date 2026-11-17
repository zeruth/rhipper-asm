// The session of a port: the imports a port built with RA_SESSION calls go through the wrappers
// here (rhipper.h renames them to `ra_session_<name>`). With RA_RECORD naming a file, each wrapper
// calls the host and writes the call to the file; with RA_REPLAY naming one, it answers from the
// file and the host is not called, so a window program's port runs without a window, the same way
// every time; with neither, it calls the host alone.
//
// The session's text has one event to a line, in the order the run made them:
//
//   ra-port-session 1
//   call GetStdHandle 0xfffffff5 = 0x54
//   back i+0x1180 0x30c1a 0x81 0x0 s
//   call DefWindowProcA 0x30c1a 0x81 0x0 s = 0x1
//   ret 0x1
//   call CreateWindowExA 0x0 i+0x2040 i+0x204e 0x80000000 0x0 0x0 0xc8 0x64 0x0 0x0 i+0x0 0x0 = 0x30c1a
//     write 0+0x8 0f000000
//   call ExitProcess 0x0 ! exit
//
// A `call` is written when it returns, after the window procedure calls the host made inside it
// (`back` with the procedure and its arguments, then `ret` with what it answered). An argument is
// read at the width the function's prototype gives it: an integer in hex, `i+<offset>` an address
// in the port's own image, `s` an address in the thread's stack, which differs between a run in
// the host's callback and the replay's, and is compared as a stack address alone. A `write` after
// a call is the bytes the host left at an argument (by its number from zero) and an offset: the
// whole structure an output parameter documents, and the bytes that changed in the stretch of
// memory after any other argument in the image or the stack.
//
// A replay calls the window procedures the session's `back` lines name and stops, with exit code
// RA_SESSION_DIFFERS, at the first call or return that is not the session's next event, naming
// the event, what the port made instead and the address it was made from. The console is the
// replay's own: a WriteFile to a standard handle writes the port's bytes. A procedure's argument
// that pointed into the host's stack is handed a zeroed block in the replay.

#ifdef _WIN32
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <windows.h>

#define HEADER "ra-port-session 1"
#define ARGS 12
// The stretch after an argument whose changed bytes a call records.
#define WINDOW 256
// The window procedures a recording interposes on.
#define PROCS 8
#define RA_SESSION_DIFFERS 3

typedef uint64_t (*host_fn)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                            uint64_t);
typedef uint64_t (*proc_fn)(uint64_t, uint64_t, uint64_t, uint64_t);

enum mode { UNSET, HOST, RECORD, REPLAY };
static enum mode mode;
static uint64_t image_base, image_size;

// A line of the session as it is made.
struct text {
    char bytes[2048];
    size_t length;
};

__attribute__((format(printf, 2, 3))) static void add(struct text *t, const char *format, ...) {
    if (t->length >= sizeof t->bytes - 1) return;
    va_list args;
    va_start(args, format);
    int n = vsnprintf(t->bytes + t->length, sizeof t->bytes - t->length, format, args);
    va_end(args);
    if (n > 0) t->length = t->length + (size_t)n < sizeof t->bytes ? t->length + (size_t)n : sizeof t->bytes - 1;
}

static int in_image(uint64_t v) { return v >= image_base && v - image_base < image_size; }

static int on_stack(uint64_t v) {
    ULONG_PTR low, high;
    GetCurrentThreadStackLimits(&low, &high);
    return v >= low && v < high;
}

static uint64_t stack_high(void) {
    ULONG_PTR low, high;
    GetCurrentThreadStackLimits(&low, &high);
    return high;
}

// A value at the width its letter gives: `d` a dword, `h` a word, `p` a quadword that may be an
// address, `v` and `x` nothing (a void function, one that does not return).
static uint64_t masked(uint64_t v, char kind) {
    switch (kind) {
    case 'd': return (uint32_t)v;
    case 'h': return (uint16_t)v;
    case 'v':
    case 'x': return 0;
    default: return v;
    }
}

static void add_value(struct text *t, uint64_t v, char kind, int stack) {
    v = masked(v, kind);
    if (kind == 'p' && in_image(v)) {
        add(t, " i+0x%llx", (unsigned long long)(v - image_base));
    } else if (kind == 'p' && stack && on_stack(v)) {
        add(t, " s");
    } else {
        add(t, " 0x%llx", (unsigned long long)v);
    }
}

// --- The replay's session ---

struct event {
    char kind;  // `c` a call, `b` a window procedure called back, `r` its return
    int line;
    char *text;    // the line, a call's without its answer
    char *answer;  // a call's answer; none for one that ends the process
    int first_write, writes;
};

struct write {
    int arg;
    uint64_t offset;
    const uint8_t *bytes;
    size_t size;
};

static const char *session_path;
static struct event *events;
static int event_count, next_event;
static struct write *writes;
static int write_count;

static void *grow(void *array, int count, size_t size) {
    // The capacity doubles from 16 at each power of two the count reaches.
    if (count != 0 && (count < 16 || (count & (count - 1)))) return array;
    void *grown = realloc(array, (size_t)(count ? count * 2 : 16) * size);
    if (!grown) {
        fputs("ra: no memory for the session\n", stderr);
        ExitProcess(RA_SESSION_DIFFERS);
    }
    return grown;
}

__attribute__((noreturn)) static void unreadable(int line, const char *why) {
    fprintf(stderr, "ra: %s, line %d: %s\n", session_path, line, why);
    fflush(stderr);
    ExitProcess(RA_SESSION_DIFFERS);
}

static int hex_digit(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

// A write line after its `  write `: `<arg>+0x<offset> <bytes in hex>`, its bytes decoded in place.
static void read_write(char *rest, int line) {
    if (event_count == 0 || events[event_count - 1].kind != 'c') unreadable(line, "a write that follows no call");
    char *end;
    long arg = strtol(rest, &end, 10);
    if (end == rest || arg < 0 || arg >= ARGS || strncmp(end, "+0x", 3) != 0) unreadable(line, "a write is `<arg>+0x<offset> <bytes>`");
    uint64_t offset = strtoull(end + 3, &end, 16);
    if (*end != ' ') unreadable(line, "a write is `<arg>+0x<offset> <bytes>`");
    char *hex = end + 1;
    size_t digits = strlen(hex);
    if (digits == 0 || digits % 2) unreadable(line, "a write's bytes are pairs of hex digits");
    uint8_t *bytes = (uint8_t *)hex;
    for (size_t i = 0; i < digits / 2; i++) {
        int high = hex_digit(hex[2 * i]), low = hex_digit(hex[2 * i + 1]);
        if (high < 0 || low < 0) unreadable(line, "a write's bytes are pairs of hex digits");
        bytes[i] = (uint8_t)(high << 4 | low);
    }
    writes = grow(writes, write_count, sizeof *writes);
    writes[write_count++] = (struct write){(int)arg, offset, bytes, digits / 2};
    events[event_count - 1].writes++;
}

static void load(const char *path) {
    session_path = path;
    FILE *f = NULL;
    if (fopen_s(&f, path, "rb") != 0 || !f) {
        fprintf(stderr, "ra: cannot open the session %s\n", path);
        ExitProcess(RA_SESSION_DIFFERS);
    }
    fseek(f, 0, SEEK_END);
    long size = ftell(f);
    fseek(f, 0, SEEK_SET);
    char *text = malloc((size_t)size + 1);
    if (!text || fread(text, 1, (size_t)size, f) != (size_t)size) unreadable(0, "the file does not read");
    fclose(f);
    text[size] = 0;
    int line = 0;
    for (char *at = text; *at;) {
        char *end = strchr(at, '\n');
        char *after = end ? end + 1 : at + strlen(at);
        if (end) *end = 0;
        size_t length = strlen(at);
        if (length && at[length - 1] == '\r') at[length - 1] = 0;
        line++;
        if (line == 1) {
            if (strcmp(at, HEADER) != 0) unreadable(line, "the first line is not `" HEADER "`");
        } else if (strncmp(at, "  write ", 8) == 0) {
            read_write(at + 8, line);
        } else if (*at) {
            struct event e = {0, line, at, NULL, write_count, 0};
            if (strncmp(at, "call ", 5) == 0) {
                e.kind = 'c';
                char *answer = strstr(at, " = ");
                char *ends = strstr(at, " ! exit");
                if (answer) {
                    *answer = 0;
                    e.answer = answer + 3;
                } else if (ends) {
                    *ends = 0;
                } else {
                    unreadable(line, "a call has no answer");
                }
            } else if (strncmp(at, "back ", 5) == 0) {
                e.kind = 'b';
            } else if (strncmp(at, "ret ", 4) == 0) {
                e.kind = 'r';
            } else {
                unreadable(line, "neither a call, a write, a callback nor a return");
            }
            events = grow(events, event_count, sizeof *events);
            events[event_count++] = e;
        }
        at = after;
    }
}

// A value of the session: an integer, or an address in the port's image.
static uint64_t value_of(const char *token) {
    if (strncmp(token, "i+0x", 4) == 0) return image_base + strtoull(token + 4, NULL, 16);
    if (strncmp(token, "0x", 2) == 0) return strtoull(token + 2, NULL, 16);
    return 0;
}

__attribute__((noreturn)) static void differs(const char *made, const void *from) {
    const struct event *expected = next_event < event_count ? &events[next_event] : NULL;
    fprintf(stderr, "ra: the replay of %s differs at event %d: the port made `%s`", session_path, next_event + 1, made);
    uint64_t at = (uint64_t)(uintptr_t)from;
    if (in_image(at)) fprintf(stderr, " from 0x%llx (module+0x%llx)", (unsigned long long)at, (unsigned long long)(at - image_base));
    if (expected) {
        fprintf(stderr, ", where the session has `%s` (line %d)\n", expected->text, expected->line);
    } else {
        fputs(", past the session's last event\n", stderr);
    }
    fflush(stderr);
    ExitProcess(RA_SESSION_DIFFERS);
}

// The standard handles the session answered with, and the replay's own for each.
static struct {
    uint64_t recorded;
    HANDLE own;
} standard[8];
static int standard_count;

static void replay_back(const struct event *e) {
    // The procedure and its four arguments, after `back`.
    uint64_t values[5] = {0};
    uint8_t blocks[5][WINDOW];
    const char *at = e->text + 4;
    for (int i = 0; i < 5; i++) {
        while (*at == ' ') at++;
        if (!*at) unreadable(e->line, "a callback is `back <procedure> <hwnd> <message> <wparam> <lparam>`");
        if (*at == 's' && (at[1] == ' ' || at[1] == 0)) {
            memset(blocks[i], 0, WINDOW);
            values[i] = (uint64_t)(uintptr_t)blocks[i];
        } else {
            values[i] = value_of(at);
        }
        while (*at && *at != ' ') at++;
    }
    uint64_t result = ((proc_fn)(uintptr_t)values[0])(values[1], values[2], values[3], values[4]);
    struct text made = {.length = 0};
    add(&made, "ret");
    add_value(&made, result, 'p', 0);
    if (next_event >= event_count || events[next_event].kind != 'r' || strcmp(events[next_event].text, made.bytes) != 0) {
        differs(made.bytes, (const void *)(uintptr_t)values[0]);
    }
    next_event++;
}

static uint64_t replay_call(const char *name, const char *kinds, const uint64_t *a, const char *head, const void *from) {
    for (;;) {
        if (next_event >= event_count) differs(head, from);
        const struct event *e = &events[next_event];
        if (e->kind == 'b') {
            next_event++;
            replay_back(e);
            continue;
        }
        if (e->kind != 'c' || strcmp(e->text, head) != 0) differs(head, from);
        next_event++;
        for (int i = 0; i < e->writes; i++) {
            const struct write *w = &writes[e->first_write + i];
            if (a[w->arg]) memcpy((uint8_t *)(uintptr_t)a[w->arg] + w->offset, w->bytes, w->size);
        }
        if (!e->answer) {
            if (next_event != event_count) {
                fprintf(stderr, "ra: the replay of %s ends at event %d of %d: the port exits there\n", session_path, next_event, event_count);
                fflush(stderr);
                ExitProcess(RA_SESSION_DIFFERS);
            }
            fprintf(stderr, "ra: replayed the %d events of %s\n", event_count, session_path);
            fflush(stderr);
            ExitProcess((UINT)a[0]);
        }
        uint64_t answer = value_of(e->answer);
        if (strcmp(name, "GetStdHandle") == 0 && standard_count < 8) {
            standard[standard_count].recorded = answer;
            standard[standard_count++].own = GetStdHandle((DWORD)a[0]);
        }
        if (strcmp(name, "WriteFile") == 0) {
            for (int i = 0; i < standard_count; i++) {
                if (standard[i].recorded != a[0]) continue;
                DWORD written;
                WriteFile(standard[i].own, (const void *)(uintptr_t)a[1], (DWORD)a[2], &written, NULL);
                break;
            }
        }
        return masked(answer, kinds[0]);
    }
}

// --- The recording ---

static FILE *record_file;

// The window procedures a recording handed the host in their place, by trampoline.
static uint64_t procs[PROCS];
static int proc_count;

static void record_line(const struct text *line) {
    if (!record_file) return;
    fputs(line->bytes, record_file);
    fputc('\n', record_file);
    fflush(record_file);
}

static uint64_t back(int k, uint64_t hwnd, uint64_t message, uint64_t wparam, uint64_t lparam) {
    DWORD error = GetLastError();
    struct text line = {.length = 0};
    add(&line, "back");
    add_value(&line, procs[k], 'p', 1);
    add_value(&line, hwnd, 'p', 1);
    add_value(&line, message, 'd', 1);
    add_value(&line, wparam, 'p', 1);
    add_value(&line, lparam, 'p', 1);
    record_line(&line);
    SetLastError(error);
    uint64_t result = ((proc_fn)(uintptr_t)procs[k])(hwnd, message, wparam, lparam);
    error = GetLastError();
    line.length = 0;
    add(&line, "ret");
    add_value(&line, result, 'p', 0);
    record_line(&line);
    SetLastError(error);
    return result;
}

#define PROC(k) \
    static LRESULT CALLBACK proc_##k(HWND hwnd, UINT message, WPARAM wparam, LPARAM lparam) { \
        return (LRESULT)back(k, (uint64_t)(uintptr_t)hwnd, message, wparam, (uint64_t)lparam); \
    }
PROC(0)
PROC(1)
PROC(2)
PROC(3)
PROC(4)
PROC(5)
PROC(6)
PROC(7)
static const WNDPROC trampolines[PROCS] = {proc_0, proc_1, proc_2, proc_3, proc_4, proc_5, proc_6, proc_7};

static WNDPROC trampoline_for(WNDPROC proc) {
    uint64_t original = (uint64_t)(uintptr_t)proc;
    if (!original) return proc;
    for (int k = 0; k < proc_count; k++) {
        if (procs[k] == original) return trampolines[k];
    }
    if (proc_count == PROCS) {
        fprintf(stderr, "ra: a session records at most %d window procedures\n", PROCS);
        ExitProcess(RA_SESSION_DIFFERS);
    }
    procs[proc_count] = original;
    return trampolines[proc_count++];
}

// The output parameters whose whole structure the host fills, by function and argument.
static const struct {
    const char *name;
    int arg;
    size_t size;
} outputs[] = {
    {"GetMessageA", 0, sizeof(MSG)},          {"PeekMessageA", 0, sizeof(MSG)}, {"BeginPaint", 1, sizeof(PAINTSTRUCT)},
    {"GetClientRect", 1, sizeof(RECT)},       {"GetWindowRect", 1, sizeof(RECT)}, {"WriteFile", 3, sizeof(DWORD)},
};

// Memory an argument points at, as it was before the call.
struct region {
    int arg;
    int whole;
    uint8_t *at;
    size_t size;
    uint8_t before[WINDOW];
};

static size_t output_size(const char *name, int arg) {
    for (size_t i = 0; i < sizeof outputs / sizeof *outputs; i++) {
        if (outputs[i].arg == arg && strcmp(outputs[i].name, name) == 0) return outputs[i].size;
    }
    return 0;
}

// The regions of a call's arguments, the whole output structures first.
static int regions_of(const char *name, const char *kinds, const uint64_t *a, int n, struct region *regions) {
    int count = 0;
    for (int whole = 1; whole >= 0; whole--) {
        for (int i = 0; i < n; i++) {
            uint64_t v = a[i];
            if (kinds[i + 1] != 'p' || !v) continue;
            size_t output = output_size(name, i);
            size_t size = 0;
            if (whole) {
                size = output;
            } else if (!output && on_stack(v)) {
                size = stack_high() - v < WINDOW ? (size_t)(stack_high() - v) : WINDOW;
            } else if (!output && in_image(v)) {
                size = image_base + image_size - v < WINDOW ? (size_t)(image_base + image_size - v) : WINDOW;
            }
            if (!size) continue;
            struct region *r = &regions[count++];
            r->arg = i;
            r->whole = whole;
            r->at = (uint8_t *)(uintptr_t)v;
            r->size = size;
            memcpy(r->before, r->at, size);
        }
    }
    return count;
}

// The bytes the call left in each region: all of an output structure, the changed ones elsewhere,
// each byte once, under the first region that holds it.
static void record_writes(const struct region *regions, int count) {
    for (int r = 0; r < count; r++) {
        const struct region *region = &regions[r];
        size_t i = 0;
        while (i < region->size) {
            size_t start = i;
            while (i < region->size && i - start < 64) {
                const uint8_t *at = region->at + i;
                int held = 0;
                for (int e = 0; e < r && !held; e++) held = at >= regions[e].at && at < regions[e].at + regions[e].size;
                if (held || (!region->whole && *at == region->before[i])) break;
                i++;
            }
            if (i == start) {
                i++;
                continue;
            }
            struct text line = {.length = 0};
            add(&line, "  write %d+0x%llx ", region->arg, (unsigned long long)start);
            for (size_t b = start; b < i; b++) add(&line, "%02x", region->at[b]);
            record_line(&line);
        }
    }
}

static uint64_t record_call(const char *name, const char *kinds, const uint64_t *a, int n, struct text *head, host_fn host) {
    if (kinds[0] == 'x') {
        add(head, " ! exit");
        record_line(head);
        fclose(record_file);
        record_file = NULL;
        return host(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7], a[8], a[9], a[10], a[11]);
    }
    static struct region regions[16][ARGS];
    static int depth;
    if (depth == 16) {
        fputs("ra: a session records calls nested at most 16 deep\n", stderr);
        ExitProcess(RA_SESSION_DIFFERS);
    }
    struct region *mine = regions[depth++];
    int count = regions_of(name, kinds, a, n, mine);
    // A window class goes to the host with a trampoline for its procedure, which records each
    // message the host calls it back with.
    uint64_t passed[ARGS];
    memcpy(passed, a, sizeof passed);
    WNDCLASSA wndclass;
    WNDCLASSEXA wndclass_ex;
    if (strcmp(name, "RegisterClassA") == 0 && a[0]) {
        memcpy(&wndclass, (const void *)(uintptr_t)a[0], sizeof wndclass);
        wndclass.lpfnWndProc = trampoline_for(wndclass.lpfnWndProc);
        passed[0] = (uint64_t)(uintptr_t)&wndclass;
    } else if (strcmp(name, "RegisterClassExA") == 0 && a[0]) {
        memcpy(&wndclass_ex, (const void *)(uintptr_t)a[0], sizeof wndclass_ex);
        wndclass_ex.lpfnWndProc = trampoline_for(wndclass_ex.lpfnWndProc);
        passed[0] = (uint64_t)(uintptr_t)&wndclass_ex;
    }
    uint64_t result = host(passed[0], passed[1], passed[2], passed[3], passed[4], passed[5], passed[6], passed[7], passed[8], passed[9],
                           passed[10], passed[11]);
    DWORD error = GetLastError();
    add(head, " =");
    add_value(head, result, kinds[0], 0);
    record_line(head);
    record_writes(mine, count);
    depth--;
    SetLastError(error);
    return masked(result, kinds[0]);
}

// --- The wrappers ---

static void start(void) {
    image_base = (uint64_t)(uintptr_t)GetModuleHandleA(NULL);
    const IMAGE_DOS_HEADER *dos = (const IMAGE_DOS_HEADER *)(uintptr_t)image_base;
    const IMAGE_NT_HEADERS *nt = (const IMAGE_NT_HEADERS *)(uintptr_t)(image_base + (uint64_t)dos->e_lfanew);
    image_size = nt->OptionalHeader.SizeOfImage;
    static char path[MAX_PATH];
    DWORD n = GetEnvironmentVariableA("RA_REPLAY", path, sizeof path);
    if (n > 0 && n < sizeof path) {
        load(path);
        mode = REPLAY;
        return;
    }
    n = GetEnvironmentVariableA("RA_RECORD", path, sizeof path);
    if (n > 0 && n < sizeof path) {
        if (fopen_s(&record_file, path, "wb") != 0 || !record_file) {
            fprintf(stderr, "ra: cannot write the session %s\n", path);
            ExitProcess(RA_SESSION_DIFFERS);
        }
        fputs(HEADER "\n", record_file);
        mode = RECORD;
        return;
    }
    mode = HOST;
}

// One import: `kinds` has the letter of its result, then one per argument (see `masked`).
static uint64_t call(const char *name, const char *kinds, const uint64_t *a, host_fn host, const void *from) {
    DWORD error = GetLastError();
    if (mode == UNSET) start();
    if (mode == HOST) {
        SetLastError(error);
        return host(a[0], a[1], a[2], a[3], a[4], a[5], a[6], a[7], a[8], a[9], a[10], a[11]);
    }
    int n = (int)strlen(kinds) - 1;
    struct text head = {.length = 0};
    add(&head, "call %s", name);
    for (int i = 0; i < n; i++) add_value(&head, a[i], kinds[i + 1], 1);
    if (mode == REPLAY) return replay_call(name, kinds, a, head.bytes, from);
    SetLastError(error);
    return record_call(name, kinds, a, n, &head, host);
}

#define P1 uint64_t a0
#define P2 P1, uint64_t a1
#define P3 P2, uint64_t a2
#define P4 P3, uint64_t a3
#define P5 P4, uint64_t a4
#define P6 P5, uint64_t a5
#define P7 P6, uint64_t a6
#define P8 P7, uint64_t a7
#define P9 P8, uint64_t a8
#define P10 P9, uint64_t a9
#define P11 P10, uint64_t a10
#define P12 P11, uint64_t a11
#define A1 a0
#define A2 A1, a1
#define A3 A2, a2
#define A4 A3, a3
#define A5 A4, a4
#define A6 A5, a5
#define A7 A6, a6
#define A8 A7, a7
#define A9 A8, a8
#define A10 A9, a9
#define A11 A10, a10
#define A12 A11, a11
#define WRAP(n, name, kinds) \
    uint64_t ra_session_##name(P##n); \
    uint64_t ra_session_##name(P##n) { \
        uint64_t a[ARGS] = {A##n}; \
        return call(#name, kinds, a, (host_fn)(void *)&name, __builtin_return_address(0)); \
    }

// The list rhipper.h renames under RA_SESSION, with each prototype's widths.
WRAP(1, GetStdHandle, "pd")
WRAP(5, WriteFile, "dppdpp")
WRAP(1, ExitProcess, "xd")
WRAP(1, GetModuleHandleA, "pp")
WRAP(1, RegisterClassA, "hp")
WRAP(1, RegisterClassExA, "hp")
WRAP(12, CreateWindowExA, "pdppdddddpppp")
WRAP(4, DefWindowProcA, "ppdpp")
WRAP(1, DestroyWindow, "dp")
WRAP(2, ShowWindow, "dpd")
WRAP(1, UpdateWindow, "dp")
WRAP(3, InvalidateRect, "dppd")
WRAP(4, GetMessageA, "dppdd")
WRAP(5, PeekMessageA, "dppddd")
WRAP(1, TranslateMessage, "dp")
WRAP(1, DispatchMessageA, "pp")
WRAP(4, PostMessageA, "dpdpp")
WRAP(4, SendMessageA, "ppdpp")
WRAP(1, PostQuitMessage, "vd")
WRAP(2, BeginPaint, "ppp")
WRAP(2, EndPaint, "dpp")
WRAP(3, FillRect, "dppp")
WRAP(2, GetClientRect, "dpp")
WRAP(2, GetWindowRect, "dpp")

#else
// The session wrappers stand in for the Windows imports of a port on a Windows host.
typedef int ra_replay_unit;
#endif
