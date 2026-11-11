/* fpenv.c: the floating-point environment a game sets and reads. _controlfp's precision control
 * (Direct3D 9 puts the x87 unit in 24-bit precision for the process unless asked not to, and
 * everything computed after it rounds so), its rounding control under conversions and the CRT's
 * rint/lrint, the masked exceptions' sticky flags (_clearfp, _statusfp, fetestexcept), denormals,
 * and the CRT's math across its ranges, printed as rounded integers or exact bits only where the
 * result is exact. Each variant: SSE2 by default, x87 with /arch:IA32. */
#define _CRT_SECURE_NO_WARNINGS   /* (_controlfp, not _controlfp_s: the call games make) */
#include <float.h>
#include <math.h>
#include "rhtest.h"
/* (VS2010 has no C99 fenv.h, rint, lrint or nearbyint: its rounding checks round by the mode
 * through its own conversion helper instead) */
#if _MSC_VER >= 1800
#include <fenv.h>
#define HAVE_C99_FP 1
#else
#define HAVE_C99_FP 0
#endif

static volatile double g_third_num = 1.0, g_third_den = 3.0;

NOINLINE static u32 bits_hi(double d)
{
    union { double d; u64 u; } x;
    x.d = d;
    return (u32)(x.u >> 32);
}

NOINLINE static u32 bits_lo(double d)
{
    union { double d; u64 u; } x;
    x.d = d;
    return (u32)x.u;
}

NOINLINE static double third(void)
{
    return g_third_num / (g_third_den + (double)(seed() - 7));
}

/* ---- precision control: 24, 53 and 64 bits ------------------------------------------------ */

NOINLINE static void precision(void)
{
    unsigned int saved = _controlfp(0, 0), now;
    double d;
    check("pc default", saved & _MCW_PC);
    d = third();
    check("third hi", bits_hi(d));
    check("third lo", bits_lo(d));
#if !defined(_M_X64)
    /* (what Direct3D does: 24 bits, the x87 unit's results rounded to a float's mantissa) */
    _controlfp(_PC_24, _MCW_PC);
    now = _controlfp(0, 0);
    check("pc set 24", now & _MCW_PC);
    d = third();
    check("third pc24 hi", bits_hi(d));
    check("third pc24 lo", bits_lo(d));
    d = sqrt(g_third_den + (double)seed());
    check("sqrt pc24 lo", bits_lo(d));
    _controlfp(_PC_53, _MCW_PC);
    d = third();
    check("third pc53 lo", bits_lo(d));
#else
    (void)now;
#endif
    _controlfp(saved, _MCW_PC);
    check("pc restored", _controlfp(0, 0) & _MCW_PC);
}

/* ---- rounding control under conversions --------------------------------------------------- */

static volatile double g_vals[6] = {2.5, -2.5, 3.5, -0.5, 1.75, -7.25};

#if !HAVE_C99_FP
/* (rounding by the current mode: the x87 unit's fistp, which honours the control word) */
NOINLINE static i32 mode_round(double d)
{
    i32 r;
    __asm {
        fld d
        fistp r
    }
    return r;
}
#endif

NOINLINE static u32 round_all(void)
{
    u32 acc = 0, i;
    for (i = 0; i < 6; i++) {
#if HAVE_C99_FP
        acc = acc * 13 + (u32)(i32)lrint(g_vals[i]);
        acc = acc * 13 + (u32)(i32)rint(g_vals[i]);
        acc = acc * 13 + (u32)(i32)nearbyint(g_vals[i] * 3);
#else
        acc = acc * 13 + (u32)mode_round(g_vals[i]);
        acc = acc * 13 + (u32)mode_round(g_vals[i] * 3);
#endif
    }
    return acc;
}

NOINLINE static void rounding(void)
{
    unsigned int saved = _controlfp(0, 0);
    check("rc default", saved & _MCW_RC);
    check("round nearest", round_all());
    _controlfp(_RC_DOWN, _MCW_RC);
    check("round down", round_all());
    _controlfp(_RC_UP, _MCW_RC);
    check("round up", round_all());
    _controlfp(_RC_CHOP, _MCW_RC);
    check("round chop", round_all());
    /* (a C cast truncates whatever the mode) */
    check("cast under chop", (u32)(i32)(g_vals[5] * 2) + (u32)(i32)g_vals[4] * 100);
    _controlfp(saved, _MCW_RC);
    check("rc restored", _controlfp(0, 0) & _MCW_RC);
#if HAVE_C99_FP
    _controlfp(_RC_UP, _MCW_RC);
    check("fegetround up", fegetround() == FE_UPWARD);
    fesetround(FE_TONEAREST);
    check("fesetround", (_controlfp(0, 0) & _MCW_RC) == _RC_NEAR);
#endif
}

/* ---- the masked exceptions' flags -------------------------------------------------------- */

static volatile double g_zero = 0.0, g_big = 1e308, g_tiny = 1e-308;

NOINLINE static void exceptions(void)
{
    volatile double r;
    u32 st;
    _clearfp();
    r = 1.0 / g_zero;
    st = _statusfp();
    check("div by zero flag", (st & _SW_ZERODIVIDE) != 0);
    check("div by zero inf", r > g_big);
    _clearfp();
    r = g_big * 10;
    check("overflow flag", (_statusfp() & _SW_OVERFLOW) != 0);
    _clearfp();
    r = g_tiny / 1e10;
    check("underflow flag", (_statusfp() & _SW_UNDERFLOW) != 0);
    check("denormal kept", r > 0 && r < g_tiny);
    _clearfp();
    r = g_zero / g_zero;
    check("invalid flag", (_statusfp() & _SW_INVALID) != 0);
    check("nan", r != r);
#if HAVE_C99_FP
    feclearexcept(FE_ALL_EXCEPT);
    r = 2.0 / 3.0 + g_zero;
    check("inexact", fetestexcept(FE_INEXACT) != 0);
    check("not divbyzero", fetestexcept(FE_DIVBYZERO) == 0);
#endif
    _clearfp();
}

/* ---- the CRT's math ---------------------------------------------------------------------- */

static volatile double g_x[8] = {0.1, 0.5, 1.0, 2.0, 3.14159, 10.0, 100.5, -0.75};

NOINLINE static void crt_math(void)
{
    u32 i, a = 0, b = 0, c = 0;
    for (i = 0; i < 8; i++) {
        double x = g_x[i] + (double)(seed() - 7);
        a = a * 31 + (u32)(i32)(sin(x) * 1e6);
        a = a * 31 + (u32)(i32)(cos(x) * 1e6);
        b = b * 31 + (u32)(i32)(atan2(x, 1.5) * 1e6);
        b = b * 31 + (u32)(i32)(exp(x / 10) * 1e6);
        c = c * 31 + (u32)(i32)(fmod(x * 7, 2.5) * 1e6);
        c = c * 31 + (u32)(i32)(floor(x * 3) + ceil(x * 3));
        if (x > 0) {
            b = b * 31 + (u32)(i32)(log(x) * 1e6);
            c = c * 31 + (u32)(i32)(pow(x, 1.5) * 1e3);
        }
    }
    check("trig", a);
    check("exp log atan", b);
    check("fmod floor pow", c);
    check("hypot", (u32)(i32)(_hypot(3.0 + seed() - 7, 4.0) * 1000));
    check("ldexp", bits_hi(ldexp(1.5, 20 + (int)seed())));
    {
        int e = 0;
        double m = frexp(96.0 + seed(), &e);
        check("frexp", (u32)e * 1000 + (u32)(i32)(m * 1000));
    }
    check("modf", (u32)(i32)(modf(-7.75, (double *)&g_x[0]) * 100) + (u32)(i32)g_x[0] * 1000);
}

/* ---- floats: conversions both ways, at the edges ------------------------------------------ */

static volatile float g_f[6] = {3.4e38f, -1.0e-40f, 16777217.0f, 0.333333f, -2147483648.0f, 4294967040.0f};

NOINLINE static void conversions(void)
{
    u32 acc = 0, i;
    for (i = 0; i < 6; i++) {
        double d = g_f[i];
        acc = acc * 31 + bits_hi(d) + bits_lo(d);
        acc = acc * 31 + (u32)(i64)(d / 2);
    }
    check("float to double", acc);
    check("u32 from float", (u32)g_f[5]);
    check("i32 from float", (u32)(i32)g_f[4]);
    check("u64 from double", (u32)((u64)(g_f[5] * 3.0) >> 4));
    check("float from u32", bits_hi((double)(float)(0xfffffff0u - seed())));
    check("double from u64", bits_lo((double)(0x8000000000000001ull + seed())));
}

void rhtest_run(void)
{
    precision();
    rounding();
    exceptions();
    crt_math();
    conversions();
}
