// The drawing fixture: a frame of every kind of primitive the platform runtime renders, moved by
// the first controller. Each frame is a shaded triangle, a line across the top, a flat square at
// (x, y) and the square's place as text in the debug font (textured sprites through a 4-bit lookup
// table); the two buffers swap at the vertical blank. The square starts at (144, 104) and moves two
// pixels a frame while a direction is held; the cross button prints the square's place on the TTY
// once each time it goes down.
//
// The BIOS's pad driver fills the buffers (InitPAD, StartPAD), the ordering table is cleared and
// sent by DMA (ClearOTagR, DrawOTag), and VSync waits for the vertical blank's interrupt, so one
// run of the program crosses the GPU, the DMA controller, the interrupt controller, the BIOS and
// the controller port.
#include <stdint.h>
#include <stdio.h>
#include <psxapi.h>
#include <psxgpu.h>
#include <psxpad.h>

#define OT_LEN 4

static DISPENV disp[2];
static DRAWENV draw[2];
static uint32_t ot[2][OT_LEN];
static uint8_t prims[2][256];
static uint8_t pad[2][34];

int main(void) {
    ResetGraph(0);
    SetDefDispEnv(&disp[0], 0, 0, 320, 240);
    SetDefDispEnv(&disp[1], 0, 240, 320, 240);
    SetDefDrawEnv(&draw[0], 0, 240, 320, 240);
    SetDefDrawEnv(&draw[1], 0, 0, 320, 240);
    for (int i = 0; i < 2; i++) {
        setRGB0(&draw[i], 0, 32, 64);
        draw[i].isbg = 1;
    }
    FntLoad(960, 0);
    FntOpen(8, 216, 304, 16, 0, 64);
    InitPAD(pad[0], 34, pad[1], 34);
    StartPAD();
    ChangeClearPAD(0);
    SetDispMask(1);
    printf("draw: ready\n");

    int x = 144, y = 104, db = 0, held = 0;
    for (;;) {
        PADTYPE *p = (PADTYPE *)pad[0];
        if (p->stat == 0) {
            if (!(p->btn & PAD_LEFT)) x -= 2;
            if (!(p->btn & PAD_RIGHT)) x += 2;
            if (!(p->btn & PAD_UP)) y -= 2;
            if (!(p->btn & PAD_DOWN)) y += 2;
            int cross = !(p->btn & PAD_CROSS);
            if (cross && !held) printf("draw: cross at %d %d\n", x, y);
            held = cross;
        }

        ClearOTagR(ot[db], OT_LEN);
        uint8_t *next = prims[db];

        POLY_G3 *triangle = (POLY_G3 *)next;
        setPolyG3(triangle);
        setXY3(triangle, 160, 24, 60, 200, 260, 200);
        setRGB0(triangle, 255, 0, 0);
        setRGB1(triangle, 0, 255, 0);
        setRGB2(triangle, 0, 0, 255);
        addPrim(&ot[db][2], triangle);
        next += sizeof(POLY_G3);

        TILE *square = (TILE *)next;
        setTile(square);
        setXY0(square, x, y);
        setWH(square, 32, 32);
        setRGB0(square, 255, 255, 0);
        addPrim(&ot[db][1], square);
        next += sizeof(TILE);

        LINE_F2 *line = (LINE_F2 *)next;
        setLineF2(line);
        setXY2(line, 10, 10, 309, 10);
        setRGB0(line, 255, 255, 255);
        addPrim(&ot[db][0], line);

        FntPrint(-1, "X=%d Y=%d", x, y);
        FntFlush(-1);

        DrawSync(0);
        VSync(0);
        PutDispEnv(&disp[db]);
        PutDrawEnv(&draw[db]);
        DrawOTag(&ot[db][OT_LEN - 1]);
        db ^= 1;
    }
}
