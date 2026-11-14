// <string.h> for a csmith program built without a C library: what csmith.h includes it for and
// what the compiler calls on its own, defined by the platform file.
#ifndef CSMITH_PLATFORM_STRING_H
#define CSMITH_PLATFORM_STRING_H

typedef __SIZE_TYPE__ size_t;

void *memcpy(void *to, const void *from, size_t n);
void *memmove(void *to, const void *from, size_t n);
void *memset(void *to, int byte, size_t n);

#endif
