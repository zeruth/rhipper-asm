// A window without a C runtime: a class whose procedure the OS calls back, a popup window of a
// known client size, one paint, and a message loop that ends with the quit message. The log names
// the messages whose order is the window's own (create, the first paint, close, destroy) and the
// quit code; the ones the shell adds vary with the desktop and go to DefWindowProc unlogged.
#include <windows.h>

#define NOINLINE __attribute__((noinline))

static HANDLE out;
static int paints;

NOINLINE static void print_str(const char *s) {
    DWORD n = 0;
    const char *p = s;
    while (*p) p++;
    WriteFile(out, s, (DWORD)(p - s), &n, NULL);
}

NOINLINE static void print_int(int v) {
    char buf[16];
    char *p = buf + sizeof buf;
    unsigned u = v < 0 ? (unsigned)(-v) : (unsigned)v;
    *--p = '\n';
    *--p = '\r';
    do {
        *--p = (char)('0' + u % 10);
        u /= 10;
    } while (u);
    if (v < 0) *--p = '-';
    DWORD n = 0;
    WriteFile(out, p, (DWORD)(buf + sizeof buf - p), &n, NULL);
}

static LRESULT CALLBACK window_proc(HWND hwnd, UINT msg, WPARAM wp, LPARAM lp) {
    switch (msg) {
    case WM_CREATE:
        print_str("create\r\n");
        return 0;
    case WM_PAINT: {
        PAINTSTRUCT ps;
        RECT client;
        HDC dc = BeginPaint(hwnd, &ps);
        FillRect(dc, &ps.rcPaint, (HBRUSH)(COLOR_WINDOW + 1));
        EndPaint(hwnd, &ps);
        if (paints++ == 0) {
            GetClientRect(hwnd, &client);
            print_str("paint\r\nwidth ");
            print_int(client.right - client.left);
            print_str("height ");
            print_int(client.bottom - client.top);
            PostMessageA(hwnd, WM_CLOSE, 0, 0);
        }
        return 0;
    }
    case WM_CLOSE:
        print_str("close\r\n");
        DestroyWindow(hwnd);
        return 0;
    case WM_DESTROY:
        print_str("destroy\r\n");
        PostQuitMessage(7);
        return 0;
    default:
        return DefWindowProcA(hwnd, msg, wp, lp);
    }
}

// The message loop, to the quit message, whose code it answers with.
NOINLINE static int message_loop(void) {
    MSG m;
    while (GetMessageA(&m, NULL, 0, 0) > 0) {
        TranslateMessage(&m);
        DispatchMessageA(&m);
    }
    return (int)m.wParam;
}

int mainCRTStartup(void) {
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    WNDCLASSA wc;
    wc.style = 0;
    wc.lpfnWndProc = window_proc;
    wc.cbClsExtra = 0;
    wc.cbWndExtra = 0;
    wc.hInstance = GetModuleHandleA(NULL);
    wc.hIcon = NULL;
    wc.hCursor = NULL;
    wc.hbrBackground = NULL;
    wc.lpszMenuName = NULL;
    wc.lpszClassName = "RhipperWindow";
    if (!RegisterClassA(&wc)) {
        print_str("no class\r\n");
        ExitProcess(1);
    }
    print_str("class\r\n");
    HWND hwnd = CreateWindowExA(0, "RhipperWindow", "Rhipper", WS_POPUP, 0, 0, 200, 100, NULL, NULL, wc.hInstance, NULL);
    if (!hwnd) {
        print_str("no window\r\n");
        ExitProcess(2);
    }
    print_str("window\r\n");
    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    UpdateWindow(hwnd);
    int quit = message_loop();
    print_str("quit ");
    print_int(quit);
    print_str("paints ");
    print_int(paints > 0);
    ExitProcess(0);
    return 0;
}
