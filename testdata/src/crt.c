// A program over the statically linked C runtime: `main` reached through the runtime's own
// startup, and the string, memory and stdio functions linked into the image. The port calls the
// host's C runtime for them and lifts only the program's code; the PDB beside the binary names
// every function and the library each came from.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

struct item {
    const char *name;
    int count;
};

__attribute__((noinline)) static int total(const struct item *items, int n) {
    int sum = 0;
    for (int i = 0; i < n; i++) sum += items[i].count;
    return sum;
}

__attribute__((noinline)) static char *label(const char *kind, int count) {
    char *text = malloc(64);
    strcpy(text, kind);
    strcat(text, count == 1 ? " item" : " items");
    return text;
}

int main(void) {
    struct item items[3] = {{"nut", 4}, {"bolt", 1}, {"washer", 7}};
    printf("total %d\n", total(items, 3));
    for (int i = 0; i < 3; i++) {
        char *text = label(items[i].name, items[i].count);
        printf("%s: %zu chars, %d %s\n", text, strlen(text), items[i].count, items[i].count == atoi("1") ? "one" : "many");
        free(text);
    }
    printf("%s\n", strcmp("nut", "bolt") > 0 ? "sorted after" : "sorted before");
    return 0;
}
