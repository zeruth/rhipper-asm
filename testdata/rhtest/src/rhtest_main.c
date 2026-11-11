/* rhtest_main.c: main(), the seed and the output every rhtest program links (rhtest.h). */
#define WIN32_LEAN_AND_MEAN
#include <windows.h>

#include "rhtest.h"

static volatile u32 g_seed = 7;

static char g_out[512];
static u32 g_outLen;
static u32 g_sum = 2166136261u;   /* FNV-1a over every checked value */

u32 seed(void) {
    return g_seed;
}

static void flush(void) {
    DWORD n;
    WriteFile(GetStdHandle(STD_OUTPUT_HANDLE), g_out, g_outLen, &n, 0);
    g_outLen = 0;
}

static void put(const char *s) {
    while (*s) {
        if (g_outLen == sizeof g_out) flush();
        g_out[g_outLen++] = *s++;
    }
}

static void put_u(u32 v) {
    char b[12];
    int k = 11;
    b[k] = 0;
    do {
        b[--k] = (char)('0' + v % 10);
        v /= 10;
    } while (v);
    put(b + k);
}

static void put_i(i32 v) {
    if (v < 0) {
        put("-");
        put_u(0u - (u32)v);
    } else {
        put_u((u32)v);
    }
}

static void put_x(u32 v) {
    char b[11];
    int k;
    b[0] = '0';
    b[1] = 'x';
    b[10] = 0;
    for (k = 9; k >= 2; k--, v >>= 4) b[k] = "0123456789abcdef"[v & 15];
    put(b);
}

static void fold(u32 v) {
    int k;
    for (k = 0; k < 4; k++, v >>= 8) g_sum = (g_sum ^ (v & 0xff)) * 16777619u;
}

void check(const char *name, u32 v) {
    put(name);
    put(" = ");
    put_i((i32)v);
    put(" (");
    put_x(v);
    put(")\n");
    fold(v);
    flush();
}

void check64(const char *name, u64 v) {
    put(name);
    put(" = ");
    put_x((u32)(v >> 32));
    put(":");
    put_x((u32)v);
    put("\n");
    fold((u32)v);
    fold((u32)(v >> 32));
    flush();
}

int main(void) {
    rhtest_run();
    put("checksum = ");
    put_x(g_sum);
    put("\n");
    flush();
    return (int)(g_sum & 0x7f);
}
