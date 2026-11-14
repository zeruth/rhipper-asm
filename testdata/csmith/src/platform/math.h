// <math.h> for a csmith program built without a C library: csmith.h includes it for its
// floating-point wrappers, which the seeds here (generated with --no-float) never call. The
// wrappers are static, so the declarations below let them compile and an unused one is never
// emitted; nothing here needs a definition at link time.
#ifndef CSMITH_PLATFORM_MATH_H
#define CSMITH_PLATFORM_MATH_H
double fabs(double x);
float fabsf(float x);
double ldexp(double x, int exp);
float ldexpf(float x, int exp);
#endif
