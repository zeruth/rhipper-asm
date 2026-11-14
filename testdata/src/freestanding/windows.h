// The Windows API that hello.c and control.c call, over the freestanding syscalls, so the two
// sources build for a Linux machine unchanged and print what their Windows builds print: a build
// with `-nostdlibinc -I testdata/src/freestanding` finds this file where it looks for <windows.h>.
// Only what the two fixtures use is here; the functions are real ones (windows.c), so a call stays
// a call and a function pointer to one stays a pointer. There is no loader: GetModuleHandleA and
// GetProcAddress answer NULL, the way the fixtures already allow for.
#ifndef FREESTANDING_WINDOWS_H
#define FREESTANDING_WINDOWS_H

#include "freestanding.h"

#define NULL ((void *)0)
#define WINAPI
#define STD_OUTPUT_HANDLE ((DWORD)-11)
#define STD_ERROR_HANDLE ((DWORD)-12)

typedef void *HANDLE;
typedef void *HMODULE;
typedef unsigned int DWORD;
typedef unsigned int UINT;
typedef int BOOL;
typedef const void *LPCVOID;
typedef DWORD *LPDWORD;
typedef void *LPOVERLAPPED;
typedef const char *LPCSTR;
typedef void (*FARPROC)(void);

// The standard output is file descriptor 1 and the error output 2, carried in the handle.
HANDLE GetStdHandle(DWORD which);
BOOL WriteFile(HANDLE file, LPCVOID buf, DWORD n, LPDWORD written, LPOVERLAPPED overlapped);
__attribute__((noreturn)) void ExitProcess(UINT code);
HMODULE GetModuleHandleA(LPCSTR name);
FARPROC GetProcAddress(HMODULE module, LPCSTR name);

// The fixtures name their entry as the Windows linker's default; the start routine calls `main`.
#define mainCRTStartup main

#endif
