// The name source of `ras`'s plugin ABI 1: names the functions of the image from `names.txt`
// beside the plugin, one `0x<address> <name>` line each; a blank line and a `#` line are
// skipped. A line for no function of the image, or one that is not an address and a name, is
// logged and left out. The host checks each name as it checks an annotation's.
//
// The engine's own `names` pass moved out, as the proof of the interface: the request is the
// host's text (`file <name> <bytes>` with the file after it, `function 0x<address> <name>` per
// function in address order), the reply is `0x<address> <name>` lines.
//
// Built with no C library into the `names.wasm` beside it:
//   clang --target=wasm32 -O2 -nostdlib -fno-builtin -Wl,--no-entry -o names.wasm names.c

typedef unsigned long long u64;
typedef unsigned int u32;

__attribute__((import_module("ra"), import_name("log"))) void ra_log(const char *text, int length);

extern unsigned char __heap_base;
static u32 heap;

__attribute__((export_name("ra_abi"))) int ra_abi(void) {
    return 1;
}

// The host writes its request into what this returns; the reply comes from here too.
__attribute__((export_name("ra_alloc"))) void *ra_alloc(int size) {
    if (heap == 0)
        heap = (u32)&__heap_base;
    u32 at = (heap + 15u) & ~15u;
    u32 end = at + (u32)size;
    u32 have = (u32)__builtin_wasm_memory_size(0) * 65536u;
    if (end > have && __builtin_wasm_memory_grow(0, (end - have + 65535u) / 65536u) == (__SIZE_TYPE__)-1)
        return 0;
    heap = end;
    return (void *)at;
}

struct text {
    const char *at;
    const char *end;
};

// The next line of `t`, without its line end; 0 when `t` is spent.
static int next_line(struct text *t, struct text *line) {
    if (t->at >= t->end)
        return 0;
    line->at = t->at;
    while (t->at < t->end && *t->at != '\n')
        t->at++;
    line->end = t->at;
    if (t->at < t->end)
        t->at++;
    if (line->end > line->at && line->end[-1] == '\r')
        line->end--;
    return 1;
}

// Whether `line` starts with `word` and a space, which it then skips.
static int take_word(struct text *line, const char *word) {
    const char *p = line->at;
    while (*word) {
        if (p >= line->end || *p != *word)
            return 0;
        p++;
        word++;
    }
    if (p >= line->end || *p != ' ')
        return 0;
    line->at = p + 1;
    return 1;
}

static void skip_spaces(struct text *line) {
    while (line->at < line->end && (*line->at == ' ' || *line->at == '\t'))
        line->at++;
}

// The run of characters up to the next space.
static struct text take_token(struct text *line) {
    skip_spaces(line);
    struct text token = {line->at, line->at};
    while (token.end < line->end && *token.end != ' ' && *token.end != '\t')
        token.end++;
    line->at = token.end;
    return token;
}

// `0x` and hexadecimal digits; 0 when `token` is not.
static int address_of(struct text token, u64 *value) {
    if (token.end - token.at < 3 || token.at[0] != '0' || (token.at[1] != 'x' && token.at[1] != 'X'))
        return 0;
    u64 v = 0;
    for (const char *p = token.at + 2; p < token.end; p++) {
        char c = *p;
        if (c >= '0' && c <= '9')
            v = v * 16 + (u64)(c - '0');
        else if (c >= 'a' && c <= 'f')
            v = v * 16 + (u64)(c - 'a' + 10);
        else if (c >= 'A' && c <= 'F')
            v = v * 16 + (u64)(c - 'A' + 10);
        else
            return 0;
    }
    *value = v;
    return 1;
}

static u32 decimal(struct text token) {
    u32 v = 0;
    for (const char *p = token.at; p < token.end && *p >= '0' && *p <= '9'; p++)
        v = v * 10 + (u32)(*p - '0');
    return v;
}

static int same(struct text token, const char *word) {
    const char *p = token.at;
    while (*word && p < token.end && *p == *word) {
        p++;
        word++;
    }
    return *word == 0 && p == token.end;
}

static int is_function(const u64 *functions, u32 count, u64 address) {
    u32 lo = 0, hi = count;
    while (lo < hi) {
        u32 mid = lo + (hi - lo) / 2;
        if (functions[mid] == address)
            return 1;
        if (functions[mid] < address)
            lo = mid + 1;
        else
            hi = mid;
    }
    return 0;
}

static void log_line(const char *why, struct text line) {
    u32 a = 0, b = 0;
    while (why[a])
        a++;
    u32 n = (u32)(line.end - line.at);
    char *text = ra_alloc((int)(a + n + 1));
    if (!text)
        return;
    for (u32 i = 0; i < a; i++)
        text[b++] = why[i];
    for (u32 i = 0; i < n; i++)
        text[b++] = line.at[i];
    ra_log(text, (int)b);
}

__attribute__((export_name("ra_names"))) u64 ra_names(const char *request, int length) {
    struct text all = {request, request + length};
    struct text names = {0, 0};
    u32 count = 0;
    struct text t = all, line;
    while (next_line(&t, &line)) {
        if (take_word(&line, "file")) {
            struct text name = take_token(&line);
            u32 size = decimal(take_token(&line));
            if ((u32)(t.end - t.at) < size)
                size = (u32)(t.end - t.at);
            if (same(name, "names.txt")) {
                names.at = t.at;
                names.end = t.at + size;
            }
            t.at += size;
        } else if (take_word(&line, "function")) {
            count++;
        }
    }
    u64 *functions = ra_alloc((int)(count * sizeof(u64) + 1));
    u32 known = 0;
    t = all;
    while (functions && next_line(&t, &line)) {
        if (take_word(&line, "file")) {
            take_token(&line);
            u32 size = decimal(take_token(&line));
            t.at += (u32)(t.end - t.at) < size ? (u32)(t.end - t.at) : size;
        } else if (take_word(&line, "function") && address_of(take_token(&line), &functions[known])) {
            known++;
        }
    }
    u32 capacity = (u32)(names.end - names.at) * 2 + 16;
    char *out = ra_alloc((int)capacity);
    if (!out)
        return 0;
    u32 used = 0;
    while (next_line(&names, &line)) {
        struct text rest = line;
        skip_spaces(&rest);
        if (rest.at == rest.end || *rest.at == '#')
            continue;
        struct text address = take_token(&rest);
        struct text name = take_token(&rest);
        u64 va;
        if (!address_of(address, &va) || name.at == name.end) {
            log_line("names.txt: not an address and a name: ", line);
            continue;
        }
        if (!is_function(functions, known, va)) {
            log_line("names.txt: no function starts at ", address);
            continue;
        }
        for (const char *p = address.at; p < address.end; p++)
            out[used++] = *p;
        out[used++] = ' ';
        for (const char *p = name.at; p < name.end; p++)
            out[used++] = *p;
        out[used++] = '\n';
    }
    return ((u64)(u32)out << 32) | used;
}
