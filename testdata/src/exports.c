// A DLL fixture: named exports, an export by ordinal only (through the .def file), and a forwarder
// (through the linker command line). No C runtime.
#include <windows.h>

__declspec(dllexport) int ra_add(int a, int b) { return a + b; }

__declspec(dllexport) int ra_twice(int a) { return ra_add(a, a); }

__declspec(dllexport) int ra_counter = 7;

int ra_hidden(int a) { return a - 1; }

BOOL WINAPI DllMainCRTStartup(HINSTANCE instance, DWORD reason, LPVOID reserved) {
    (void)instance;
    (void)reason;
    (void)reserved;
    return TRUE;
}
