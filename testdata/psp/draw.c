// The PSP's drawing fixture: a frame buffer in the video memory the display shows, moved by the
// controller. The program draws a white line across the top, three blocks of the primaries and a
// yellow square at (x, y) into a 32-bit buffer it names with sceDisplaySetFrameBuf, and waits for
// the vertical blank each frame. The square starts at (224, 120) and moves two pixels a frame
// while a direction is held; the cross button prints the square's place once each time it goes
// down. One run crosses the display, the controller and the blank's wait of the platform runtime.
// Its output is testdata/psp/draw.txt.
#include <pspkernel.h>
#include <pspctrl.h>
#include <pspdisplay.h>
#include <pspge.h>
#include <pspiofilemgr.h>

PSP_MODULE_INFO("draw", PSP_MODULE_USER, 1, 0);
PSP_MAIN_THREAD_ATTR(PSP_THREAD_ATTR_USER);

#define STRIDE 512
#define BACKGROUND 0xff402000u
#define YELLOW 0xff00ffffu

static void fill(unsigned int *frame, int x, int y, int w, int h, unsigned int colour) {
    for (int row = y; row < y + h; row++) {
        unsigned int *at = frame + row * STRIDE + x;
        for (int i = 0; i < w; i++) {
            at[i] = colour;
        }
    }
}

static int put_number(char *out, int value) {
    char digits[12];
    int at = sizeof digits, n = 0;
    do {
        digits[--at] = (char)('0' + value % 10);
        value /= 10;
    } while (value != 0);
    while (at < (int)sizeof digits) {
        out[n++] = digits[at++];
    }
    return n;
}

static void say(const char *text) {
    int n = 0;
    while (text[n] != 0) {
        n++;
    }
    sceIoWrite(1, text, n);
}

int main(void) {
    // Through the uncached mirror, as a program that draws for the display does.
    unsigned int *frame = (unsigned int *)(0x40000000 | (unsigned int)sceGeEdramGetAddr());
    sceDisplaySetMode(0, 480, 272);
    sceDisplaySetFrameBuf(frame, STRIDE, PSP_DISPLAY_PIXEL_FORMAT_8888, PSP_DISPLAY_SETBUF_NEXTFRAME);
    sceCtrlSetSamplingCycle(0);
    sceCtrlSetSamplingMode(PSP_CTRL_MODE_ANALOG);

    fill(frame, 10, 10, 460, 1, 0xffffffffu);
    fill(frame, 16, 200, 48, 48, 0xff0000ffu);
    fill(frame, 80, 200, 48, 48, 0xff00ff00u);
    fill(frame, 144, 200, 48, 48, 0xffff0000u);
    say("draw: ready\n");

    int x = 224, y = 120, held = 0;
    for (;;) {
        SceCtrlData pad;
        sceCtrlReadBufferPositive(&pad, 1);
        fill(frame, x, y, 32, 32, BACKGROUND);
        if (pad.Buttons & PSP_CTRL_LEFT) x -= 2;
        if (pad.Buttons & PSP_CTRL_RIGHT) x += 2;
        if (pad.Buttons & PSP_CTRL_UP) y -= 2;
        if (pad.Buttons & PSP_CTRL_DOWN) y += 2;
        int cross = (pad.Buttons & PSP_CTRL_CROSS) != 0;
        if (cross && !held) {
            char line[48] = "draw: cross at ";
            int n = 15;
            n += put_number(line + n, x);
            line[n++] = ' ';
            n += put_number(line + n, y);
            line[n++] = '\n';
            sceIoWrite(1, line, n);
        }
        held = cross;
        fill(frame, x, y, 32, 32, YELLOW);
        sceDisplayWaitVblankStart();
    }
    return 0;
}
