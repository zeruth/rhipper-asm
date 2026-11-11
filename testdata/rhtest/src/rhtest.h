/* rhtest.h: what every rhtest program shares. Each program is small and on its own (one exe, one
 * Rhipper project per build variant, rhtest.py), so a corpus stays ~100 functions and a phase
 * iterates in a minute. A program defines rhtest_run(); rhtest_main.c, linked into each, holds
 * main(), the seed and the output.
 *
 * A check prints its value and folds it into an FNV-1a checksum, the exit code's low 7 bits.
 * Every line is written out as it is made (WriteFile, kernel32 only: the CRT is a program of its
 * own), so a port that crashes still shows each check before the one that failed.
 *
 * Every input comes from seed(), a volatile's value, so /O2 cannot fold a check to its answer;
 * every check is a NOINLINE function of its own, so each is a function in the corpus.
 */
#pragma once

#define NOINLINE __declspec(noinline)

/* Every program builds with each toolset rhtest.py knows, the newest MSVC and Visual Studio 2010
 * (msvc-16.00-x86, the compiler a VS2010 /MT game client was built with), with what each lacks
 * filled in here; nothing here changes what a newer compiler makes of the source. */
#if defined(_MSC_VER) && _MSC_VER < 1700
#ifdef __cplusplus
/* (VS2010: `override` an extension, warned at /W4 and refused on a destructor; no noexcept) */
#define override
#define noexcept throw()
#else
/* (VS2010's C is C89: a non-constant aggregate initializer, and a bitfield of a type other than
 * int, are extensions it warns of) */
#pragma warning(disable: 4204 4214)
#endif
#endif
/* storage aligned for a type (VS2010 has no alignas: 8, enough for every rhtest type) */
#if defined(_MSC_VER) && _MSC_VER < 1900
#define RH_ALIGNAS(T) __declspec(align(8))
#else
#define RH_ALIGNAS(T) alignas(T)
#endif

typedef unsigned char u8;
typedef unsigned short u16;
typedef unsigned int u32;
typedef unsigned long long u64;
typedef signed char i8;
typedef short i16;
typedef int i32;
typedef long long i64;

#ifdef __cplusplus
extern "C" {
#endif

/* 7, from a volatile */
u32 seed(void);
/* name = value (hex), one line, written now */
void check(const char *name, u32 v);
/* a 64-bit value: its halves in hex, high first */
void check64(const char *name, u64 v);
/* the program's checks, in order */
void rhtest_run(void);

#ifdef __cplusplus
}
#endif
