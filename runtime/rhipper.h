// The runtime header of a program ras emit writes as C: the integer and vector types, the
// helpers the lift's intrinsics map to, and the runtime functions the driver provides.
#ifndef RHIPPER_H
#define RHIPPER_H

#include <stdint.h>
#include <string.h>

#if defined(_MSC_VER) && !defined(__clang__)
#define RA_NORETURN __declspec(noreturn)
#define RA_ALIGNED(n) __declspec(align(n))
#define RA_PACKED_BEGIN __pragma(pack(push, 1))
#define RA_PACKED_END __pragma(pack(pop))
#define RA_UNREACHABLE() __assume(0)
// A function that runs before main: a pointer to it in the C runtime's initializer section.
#define RA_CONSTRUCTOR(name) \
    static void name(void); \
    __pragma(section(".CRT$XCU", read)) __declspec(allocate(".CRT$XCU")) static void (*name##_ptr)(void) = name; \
    static void name(void)
// A type read or written at any address: the program's own alignment is not known.
#define RA_UNALIGNED(type, name) typedef __unaligned type name;
#else
#define RA_NORETURN __attribute__((noreturn))
#define RA_ALIGNED(n) __attribute__((aligned(n)))
#define RA_PACKED_BEGIN
#define RA_PACKED_END __attribute__((packed))
#define RA_UNREACHABLE() __builtin_unreachable()
#define RA_CONSTRUCTOR(name) __attribute__((constructor)) static void name(void)
#define RA_UNALIGNED(type, name) typedef type __attribute__((aligned(1))) name;
#endif

typedef unsigned __int128 ra_u128;
typedef __int128 ra_i128;

typedef uint8_t ra_v16u8 __attribute__((vector_size(16)));
typedef uint16_t ra_v8u16 __attribute__((vector_size(16)));
typedef uint32_t ra_v4u32 __attribute__((vector_size(16)));
typedef uint64_t ra_v2u64 __attribute__((vector_size(16)));
typedef int8_t ra_v16i8 __attribute__((vector_size(16)));
typedef int16_t ra_v8i16 __attribute__((vector_size(16)));
typedef int32_t ra_v4i32 __attribute__((vector_size(16)));
typedef int64_t ra_v2i64 __attribute__((vector_size(16)));
typedef float ra_v4f32 __attribute__((vector_size(16)));
typedef double ra_v2f64 __attribute__((vector_size(16)));
typedef uint8_t ra_v1u1 __attribute__((vector_size(1)));
typedef uint8_t ra_v2u1 __attribute__((vector_size(2)));
typedef uint8_t ra_v4u1 __attribute__((vector_size(4)));
typedef uint8_t ra_v8u1 __attribute__((vector_size(8)));
typedef uint8_t ra_v16u1 __attribute__((vector_size(16)));

// The types as the translation reads and writes them in memory: a machine access has whatever
// alignment the program gave it, and a `movups` must not become a `movaps`.
RA_UNALIGNED(uint16_t, ra_ua_uint16)
RA_UNALIGNED(uint32_t, ra_ua_uint32)
RA_UNALIGNED(uint64_t, ra_ua_uint64)
RA_UNALIGNED(ra_u128, ra_ua_u128)
RA_UNALIGNED(float, ra_ua_float)
RA_UNALIGNED(double, ra_ua_double)
RA_UNALIGNED(ra_v16u8, ra_ua_v16u8)
RA_UNALIGNED(ra_v8u16, ra_ua_v8u16)
RA_UNALIGNED(ra_v4u32, ra_ua_v4u32)
RA_UNALIGNED(ra_v2u64, ra_ua_v2u64)
RA_UNALIGNED(ra_v16i8, ra_ua_v16i8)
RA_UNALIGNED(ra_v8i16, ra_ua_v8i16)
RA_UNALIGNED(ra_v4i32, ra_ua_v4i32)
RA_UNALIGNED(ra_v2i64, ra_ua_v2i64)
RA_UNALIGNED(ra_v4f32, ra_ua_v4f32)
RA_UNALIGNED(ra_v2f64, ra_ua_v2f64)

struct ra_opaque { uint8_t byte; };
struct ra_function_entry { uint64_t address; void *function; };

// The runtime functions the driver provides.
RA_NORETURN void ra_trap(uint64_t address, uint64_t code);
RA_NORETURN void ra_unsupported(uint64_t address, uint64_t code);
void ra_call(void *state, uint64_t target);

// Float constants from their bits.
static inline float ra_f32(uint32_t bits) { float f; memcpy(&f, &bits, 4); return f; }
static inline double ra_f64(uint64_t bits) { double d; memcpy(&d, &bits, 8); return d; }
static inline int ra_isnan(double v) { return v != v; }
static inline double ra_frem(double a, double b) { return __builtin_fmod(a, b); }

// Bit casts between the widths the lift uses.
#define RA_BITCAST(name, from, to) static inline to ra_bitcast_##name(from v) { to r; memcpy(&r, &v, sizeof r); return r; }
RA_BITCAST(u32_f32, uint32_t, float)
RA_BITCAST(f32_u32, float, uint32_t)
RA_BITCAST(u64_f64, uint64_t, double)
RA_BITCAST(f64_u64, double, uint64_t)
RA_BITCAST(u128_v4u32, ra_u128, ra_v4u32)
RA_BITCAST(v4u32_u128, ra_v4u32, ra_u128)
RA_BITCAST(u128_v2u64, ra_u128, ra_v2u64)
RA_BITCAST(v2u64_u128, ra_v2u64, ra_u128)
RA_BITCAST(u32_v4u32, uint32_t, ra_v4u32)
RA_BITCAST(u64_v2u64, uint64_t, ra_v2u64)
RA_BITCAST(v4u32_u32, ra_v4u32, uint32_t)
RA_BITCAST(v2u64_u64, ra_v2u64, uint64_t)
RA_BITCAST(v4u1_v4u32, ra_v4u1, ra_v4u32)
RA_BITCAST(v2u1_v2u64, ra_v2u1, ra_v2u64)
RA_BITCAST(v16u1_v16u8, ra_v16u1, ra_v16u8)
RA_BITCAST(v8u1_v8u16, ra_v8u1, ra_v8u16)
#define ra_insert(vector, index, element) ({ __typeof__(vector) ra_v = (vector); ra_v[(index)] = (element); ra_v; })

// The intrinsics.
static inline uint8_t ra_ctpop_i8(uint8_t v) { return (uint8_t)__builtin_popcount(v); }
static inline uint16_t ra_ctpop_i16(uint16_t v) { return (uint16_t)__builtin_popcount(v); }
static inline uint32_t ra_ctpop_i32(uint32_t v) { return (uint32_t)__builtin_popcount(v); }
static inline uint64_t ra_ctpop_i64(uint64_t v) { return (uint64_t)__builtin_popcountll(v); }
static inline uint8_t ra_cttz_i8(uint8_t v, uint8_t z) { (void)z; return v ? (uint8_t)__builtin_ctz(v) : 8; }
static inline uint16_t ra_cttz_i16(uint16_t v, uint8_t z) { (void)z; return v ? (uint16_t)__builtin_ctz(v) : 16; }
static inline uint32_t ra_cttz_i32(uint32_t v, uint8_t z) { (void)z; return v ? (uint32_t)__builtin_ctz(v) : 32; }
static inline uint64_t ra_cttz_i64(uint64_t v, uint8_t z) { (void)z; return v ? (uint64_t)__builtin_ctzll(v) : 64; }
static inline uint8_t ra_ctlz_i8(uint8_t v, uint8_t z) { (void)z; return v ? (uint8_t)(__builtin_clz(v) - 24) : 8; }
static inline uint16_t ra_ctlz_i16(uint16_t v, uint8_t z) { (void)z; return v ? (uint16_t)(__builtin_clz(v) - 16) : 16; }
static inline uint32_t ra_ctlz_i32(uint32_t v, uint8_t z) { (void)z; return v ? (uint32_t)__builtin_clz(v) : 32; }
static inline uint64_t ra_ctlz_i64(uint64_t v, uint8_t z) { (void)z; return v ? (uint64_t)__builtin_clzll(v) : 64; }
static inline uint8_t ra_bswap_i8(uint8_t v) { return v; }
static inline uint16_t ra_bswap_i16(uint16_t v) { return __builtin_bswap16(v); }
static inline uint32_t ra_bswap_i32(uint32_t v) { return __builtin_bswap32(v); }
static inline uint64_t ra_bswap_i64(uint64_t v) { return __builtin_bswap64(v); }
#define RA_FUNNEL(name, type, bits) \
    static inline type ra_fshl_##name(type a, type b, type c) { c &= (bits - 1); return c ? (type)((a << c) | (b >> (bits - c))) : a; } \
    static inline type ra_fshr_##name(type a, type b, type c) { c &= (bits - 1); return c ? (type)((a << (bits - c)) | (b >> c)) : b; }
RA_FUNNEL(i8, uint8_t, 8)
RA_FUNNEL(i16, uint16_t, 16)
RA_FUNNEL(i32, uint32_t, 32)
RA_FUNNEL(i64, uint64_t, 64)
static inline float ra_sqrt_f32(float v) { return __builtin_sqrtf(v); }
static inline double ra_sqrt_f64(double v) { return __builtin_sqrt(v); }
static inline float ra_fabs_f32(float v) { return __builtin_fabsf(v); }
static inline double ra_fabs_f64(double v) { return __builtin_fabs(v); }
static inline float ra_minnum_f32(float a, float b) { return __builtin_fminf(a, b); }
static inline double ra_minnum_f64(double a, double b) { return __builtin_fmin(a, b); }
static inline float ra_maxnum_f32(float a, float b) { return __builtin_fmaxf(a, b); }
static inline double ra_maxnum_f64(double a, double b) { return __builtin_fmax(a, b); }
static inline float ra_trunc_f32(float v) { return __builtin_truncf(v); }
static inline double ra_trunc_f64(double v) { return __builtin_trunc(v); }
static inline float ra_floor_f32(float v) { return __builtin_floorf(v); }
static inline double ra_floor_f64(double v) { return __builtin_floor(v); }
static inline float ra_ceil_f32(float v) { return __builtin_ceilf(v); }
static inline double ra_ceil_f64(double v) { return __builtin_ceil(v); }
// rint rounds to the nearest even under the default rounding mode, and unlike roundeven it is in
// every C runtime.
static inline float ra_rint_f32(float v) { return __builtin_rintf(v); }
static inline double ra_rint_f64(double v) { return __builtin_rint(v); }

#endif
