// The Android fixture: a native library in the shape of a game's libmain.so, built for
// aarch64-linux-android against the stub bionic libc.so of libc_stub.c. Everything is hidden but
// the exports, which a host program loads the library for and calls; a table of numbers and a table
// of pointers (each pointer a RELATIVE relocation the loader fills) in its data, a helper that is
// no export, and a call into the C library through the PLT. The expected results of the calls are
// testdata/expected/libmain.txt.
unsigned long strlen(const char *s);

static const int weights[4] = {3, 5, 7, 11};
// Written by ra_rename, so the compiler keeps it a table of pointers rather than of offsets.
static const char *names[4] = {"zero", "one", "two", "three"};

__attribute__((noinline)) static int weigh(int i) { return weights[i & 3] * i; }

__attribute__((visibility("default"))) int ra_add(int a, int b) { return a + b; }

__attribute__((visibility("default"))) int ra_score(int n) {
    int s = 0;
    for (int i = 0; i < n; i++) {
        s += weigh(i);
    }
    return s;
}

__attribute__((visibility("default"))) int ra_name_length(int i) { return (int)strlen(names[i & 3]); }

__attribute__((visibility("default"))) void ra_rename(int i, const char *name) { names[i & 3] = name; }
