// The functions of windows.h over fs_write and fs_exit.
#include "windows.h"

HANDLE GetStdHandle(DWORD which) { return (HANDLE)(long)(which == STD_ERROR_HANDLE ? 2 : 1); }

BOOL WriteFile(HANDLE file, LPCVOID buf, DWORD n, LPDWORD written, LPOVERLAPPED overlapped) {
    (void)overlapped;
    long done = fs_write((int)(long)file, buf, n);
    if (done < 0) {
        if (written) *written = 0;
        return 0;
    }
    if (written) *written = (DWORD)done;
    return 1;
}

void ExitProcess(UINT code) { fs_exit((int)code); }

HMODULE GetModuleHandleA(LPCSTR name) {
    (void)name;
    return NULL;
}

FARPROC GetProcAddress(HMODULE module, LPCSTR name) {
    (void)module;
    (void)name;
    return NULL;
}
