// The Nintendo 64's drawing fixture: a libdragon program that draws into the frame buffers the
// video interface shows, moved by the first controller. Each frame is a band of the background,
// a white line across the top and a yellow square at (x, y), drawn by the processor into the
// buffer display_get hands out and shown at the next vertical blank. The square starts at
// (144, 104) and moves two pixels a frame while a direction is held; the A button prints the
// square's place once each time it goes down.
//
// display_get waits for the video interface's interrupt and the joypad module speaks to the PIF
// through the serial interface's, so one run crosses the VR4300's exception, the MIPS interface,
// the video interface and the PIF with its controller. Built by the Makefile beside it; its
// output is testdata/n64/draw.txt.
#include <libdragon.h>

int main(void) {
    debug_init_emulog();
    display_init(RESOLUTION_320x240, DEPTH_16_BPP, 2, GAMMA_NONE, FILTERS_RESAMPLE);
    joypad_init();
    debugf("draw: ready\n");

    int x = 144, y = 104, held = 0;
    for (;;) {
        joypad_poll();
        joypad_buttons_t buttons = joypad_get_buttons(JOYPAD_PORT_1);
        if (buttons.d_left) x -= 2;
        if (buttons.d_right) x += 2;
        if (buttons.d_up) y -= 2;
        if (buttons.d_down) y += 2;
        if (buttons.a && !held) debugf("draw: A at %d %d\n", x, y);
        held = buttons.a;

        surface_t *frame = display_get();
        graphics_draw_box(frame, 0, 96, 320, 64, graphics_make_color(0, 32, 64, 255));
        graphics_draw_box(frame, 10, 10, 300, 1, graphics_make_color(255, 255, 255, 255));
        graphics_draw_box(frame, x, y, 32, 32, graphics_make_color(255, 255, 0, 255));
        display_show(frame);
    }
}
