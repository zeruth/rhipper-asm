// The GTE fixture: a cube's eight vertices through RTPT and RTPS, the normal clip and the average
// depth of each triple, then two vertices that overflow the perspective division and saturate the
// screen coordinates; the screen coordinates, the depths and the flag register print through the
// BIOS's printf.
//
// Every input is chosen so the GTE's documented arithmetic gives an exact answer by hand, since no
// emulator ran it: the rotation is the identity, the translation puts the cube's faces at depths
// 512 and 1024 (powers of two, where the Newton-Raphson division of H by SZ3 is exact), H is 256,
// the screen offset (160, 120), ZSF3 a quarter, and depth cueing off (DQA = DQB = 0). The third
// vertex pair sits at depth 68, under H / 2, which the division refuses with flag bits 17 and 31.
#include <stdint.h>
#include <stdio.h>
#include <psxgte.h>
#include <inline_c.h>

// The control registers inline_c.h has no macro for: depth cueing and the Z averaging scale.
#define gte_SetDQA(r0) __asm__ volatile("ctc2 %0, $27;" : : "r"(r0))
#define gte_SetDQB(r0) __asm__ volatile("ctc2 %0, $28;" : : "r"(r0))
#define gte_SetZSF3(r0) __asm__ volatile("ctc2 %0, $29;" : : "r"(r0))

static SVECTOR cube[8] = {
    {-256, -256, -256, 0}, {256, -256, -256, 0}, {-256, 256, -256, 0}, {256, 256, -256, 0},
    {256, -256, 256, 0},   {-256, -256, 256, 0}, {256, 256, 256, 0},   {-256, 256, 256, 0},
};
static SVECTOR near_vertex = {256, 256, -700, 0};
static SVECTOR wide_vertex = {1000, -1000, -700, 0};

static void print_vertex(const char *name, DVECTOR *sxy, uint32_t sz) { printf("%s sx=%d sy=%d sz=%d", name, sxy->vx, sxy->vy, sz); }

// Three vertices at once: their screen coordinates and depths, the flag the triple left, the
// winding of the triangle they make and the average depth of the three.
static void triple(int first) {
    DVECTOR sxy[3];
    uint32_t sz[3], flag, otz;
    int32_t nclip;
    static const char *names[8] = {"v0", "v1", "v2", "v3", "v4", "v5", "v6", "v7"};
    gte_ldv3(&cube[first], &cube[first + 1], &cube[first + 2]);
    gte_rtpt();
    gte_stsxy3(&sxy[0], &sxy[1], &sxy[2]);
    gte_stsz3(&sz[0], &sz[1], &sz[2]);
    gte_stflg(&flag);
    gte_nclip();
    gte_stopz(&nclip);
    gte_avsz3();
    gte_stotz(&otz);
    for (int i = 0; i < 3; i++) {
        print_vertex(names[first + i], &sxy[i], sz[i]);
        printf("\n");
    }
    printf("flag=%x nclip=%d otz=%d\n", flag, nclip, otz);
}

static void single(const char *name, SVECTOR *v) {
    DVECTOR sxy;
    uint32_t sz, flag;
    gte_ldv0(v);
    gte_rtps();
    gte_stsxy(&sxy);
    gte_stsz(&sz);
    gte_stflg(&flag);
    print_vertex(name, &sxy, sz);
    printf(" flag=%x\n", flag);
}

int main(void) {
    MATRIX view = {{{ONE, 0, 0}, {0, ONE, 0}, {0, 0, ONE}}, {0, 0, 768}};
    InitGeom();
    gte_SetRotMatrix(&view);
    gte_SetTransMatrix(&view);
    gte_SetGeomOffset(160, 120);
    gte_SetGeomScreen(256);
    gte_SetDQA(0);
    gte_SetDQB(0);
    gte_SetZSF3(ONE / 4);
    triple(0);
    triple(3);
    single("v6", &cube[6]);
    single("v7", &cube[7]);
    single("near", &near_vertex);
    single("wide", &wide_vertex);
    return 0;
}
