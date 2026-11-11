// The runtime header of a program ras emit writes as C: the integer and vector types, the
// helpers the lift's intrinsics map to, and the runtime functions the driver provides.
#ifndef RHIPPER_H
#define RHIPPER_H

// Only stdint.h: a C runtime header would declare the functions the translation imports and
// declares itself, with other types.
#include <stdint.h>

// The translation declares every import with integer arguments and result, the ABI it was called
// with; a C runtime import clang knows as a builtin (`exit`, `memcpy`) is then a redeclaration
// with another type, which is the same call on the wire.
#if defined(__clang__)
#pragma clang diagnostic ignored "-Wincompatible-library-redeclaration"
#elif defined(__GNUC__)
#pragma GCC diagnostic ignored "-Wbuiltin-declaration-mismatch"
#endif

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
// The half and quarter vectors: what a conversion of two lanes or a 64-bit move leaves.
typedef uint8_t ra_v8u8 __attribute__((vector_size(8)));
typedef uint16_t ra_v4u16 __attribute__((vector_size(8)));
typedef uint32_t ra_v2u32 __attribute__((vector_size(8)));
typedef int8_t ra_v8i8 __attribute__((vector_size(8)));
typedef int16_t ra_v4i16 __attribute__((vector_size(8)));
typedef int32_t ra_v2i32 __attribute__((vector_size(8)));
typedef float ra_v2f32 __attribute__((vector_size(8)));
typedef uint8_t ra_v4u8 __attribute__((vector_size(4)));
typedef uint16_t ra_v2u16 __attribute__((vector_size(4)));
typedef int8_t ra_v4i8 __attribute__((vector_size(4)));
typedef int16_t ra_v2i16 __attribute__((vector_size(4)));
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
RA_UNALIGNED(ra_v8u8, ra_ua_v8u8)
RA_UNALIGNED(ra_v4u16, ra_ua_v4u16)
RA_UNALIGNED(ra_v2u32, ra_ua_v2u32)
RA_UNALIGNED(ra_v8i8, ra_ua_v8i8)
RA_UNALIGNED(ra_v4i16, ra_ua_v4i16)
RA_UNALIGNED(ra_v2i32, ra_ua_v2i32)
RA_UNALIGNED(ra_v2f32, ra_ua_v2f32)
RA_UNALIGNED(ra_v4u8, ra_ua_v4u8)
RA_UNALIGNED(ra_v2u16, ra_ua_v2u16)
RA_UNALIGNED(ra_v4i8, ra_ua_v4i8)
RA_UNALIGNED(ra_v2i16, ra_ua_v2i16)

struct ra_opaque { uint8_t byte; };
struct ra_function_entry { uint64_t address; void *function; };

// The entry hook of every function: nothing, unless the port is built with RA_TRACE, when each
// function names itself on stderr as it starts, for finding where a port goes wrong.
#ifdef RA_TRACE
#define RA_ENTER(name) ra_enter(name)
#else
#define RA_ENTER(name) do { } while (0)
#endif

// The runtime functions the driver provides.
RA_NORETURN void ra_trap(uint64_t address, uint64_t code);
RA_NORETURN void ra_unsupported(uint64_t address, uint64_t code);
void ra_call(void *state, uint64_t target);
void ra_enter(const char *name);
// The processor's answers, from the host.
void ra_cpuid(uint64_t leaf, uint64_t subleaf, void *out);
uint64_t ra_xgetbv(uint64_t which);
uint64_t ra_rdtsc(void);
// The thread environment block of the calling thread, which fs: and gs: accesses read.
uint64_t ra_teb(void);
// The C runtime functions a 32-bit program calls that walk or hand back a table of its own
// pointers: the runtime's versions keep the program's pointer width.
uint64_t ra_x86_initterm(uint64_t first, uint64_t last);
uint64_t ra_x86_initterm_e(uint64_t first, uint64_t last);
uint64_t ra_x86_initialize_onexit_table(uint64_t table);
uint64_t ra_x86_register_onexit_function(uint64_t table, uint64_t function);
uint64_t ra_x86_execute_onexit_table(uint64_t table);
uint64_t ra_x86_p_argv(void);
uint64_t ra_x86_initial_narrow_environment(void);
uint64_t ra_x86_initialize_slist_head(uint64_t head);
uint64_t ra_x86_controlfp(uint64_t new_bits, uint64_t mask);
// The printf and scanf family through their va_list, the program's dword arguments widened to the
// host's slots by the format.
uint64_t ra_x86_vsprintf(uint64_t options, uint64_t buffer, uint64_t count, uint64_t format, uint64_t locale, uint64_t args);
uint64_t ra_x86_vsprintf_s(uint64_t options, uint64_t buffer, uint64_t count, uint64_t format, uint64_t locale, uint64_t args);
uint64_t ra_x86_vsnprintf_s(uint64_t options, uint64_t buffer, uint64_t count, uint64_t max, uint64_t format, uint64_t locale, uint64_t args);
uint64_t ra_x86_vfprintf(uint64_t options, uint64_t stream, uint64_t format, uint64_t locale, uint64_t args);
uint64_t ra_x86_vsscanf(uint64_t options, uint64_t buffer, uint64_t count, uint64_t format, uint64_t locale, uint64_t args);
// The strto* family, their end pointer written at the program's width.
uint64_t ra_x86_strtol(uint64_t s, uint64_t end, uint64_t base);
uint64_t ra_x86_strtoul(uint64_t s, uint64_t end, uint64_t base);
uint64_t ra_x86_strtoll(uint64_t s, uint64_t end, uint64_t base);
uint64_t ra_x86_strtoull(uint64_t s, uint64_t end, uint64_t base);
double ra_x86_strtod(uint64_t s, uint64_t end);
// The exact x87 unit's software f80. A slot is 16 bytes: the 80-bit value as the x87 stores a
// tbyte (the 64-bit significand with its integer bit, then the sign and the 15-bit exponent) and
// six bytes of padding. An operation reads its operands before it writes its result, so the
// result may be one of them; `cw` is the control word, whose rounding mode (bits 10 and 11) and
// precision (bits 8 and 9) the operation rounds by. `ra_f80_cmp` answers with less in bit 0,
// equal in bit 1 and unordered in bit 2 (then all three, as the hardware sets its codes).
void ra_f80_load(void *slot, const void *memory);
void ra_f80_store(void *memory, const void *slot);
void ra_f80_pack(void *slot, uint64_t significand, uint64_t sign_exponent);
void ra_f80_from_f64(void *slot, double value);
void ra_f80_from_i64(void *slot, uint64_t value);
double ra_f80_to_f64(const void *slot, uint64_t cw);
float ra_f80_to_f32(const void *slot, uint64_t cw);
uint64_t ra_f80_to_int(const void *slot, uint64_t rounding, uint64_t bits);
void ra_f80_add(void *result, const void *a, const void *b, uint64_t cw);
void ra_f80_sub(void *result, const void *a, const void *b, uint64_t cw);
void ra_f80_mul(void *result, const void *a, const void *b, uint64_t cw);
void ra_f80_div(void *result, const void *a, const void *b, uint64_t cw);
void ra_f80_sqrt(void *result, const void *a, uint64_t cw);
void ra_f80_rndint(void *result, const void *a, uint64_t cw);
void ra_f80_chs(void *result, const void *a);
void ra_f80_abs(void *result, const void *a);
uint64_t ra_f80_cmp(const void *a, const void *b);
// An import by ordinal, resolved at startup.
void *ra_resolve_ordinal(const char *dll, unsigned ordinal);
// setjmp and longjmp: the program's jmp_buf is a key to a host buffer, and setjmp runs in the
// lifted function itself, so the frame it saves is the one longjmp comes back to.
#include <setjmp.h>
jmp_buf *ra_jmp_slot(uint64_t buffer);
#define ra_x86_setjmp3(buffer, ...) ((uint64_t)(uint32_t)setjmp(*ra_jmp_slot(buffer)))
uint64_t (ra_x86_setjmp3)(uint64_t buffer, uint64_t count);
RA_NORETURN uint64_t ra_x86_longjmp(uint64_t buffer, uint64_t value);

// Float constants from their bits.
static inline float ra_f32(uint32_t bits) { float f; __builtin_memcpy(&f, &bits, 4); return f; }
static inline double ra_f64(uint64_t bits) { double d; __builtin_memcpy(&d, &bits, 8); return d; }
static inline int ra_isnan(double v) { return v != v; }
static inline double ra_frem(double a, double b) { return __builtin_fmod(a, b); }
/* A float to a signed integer as cvttsd2si and fist make it: truncated, and the integer indefinite
   (the minimum) when it does not fit or is NaN; C leaves that undefined. */
static inline int16_t ra_fptosi_16(double v) { return (v > -32769.0 && v < 32768.0) ? (int16_t)v : (int16_t)-0x8000; }
static inline int32_t ra_fptosi_32(double v) { return (v > -2147483649.0 && v < 2147483648.0) ? (int32_t)v : (int32_t)-0x80000000; }
static inline int64_t ra_fptosi_64(double v) {
    return (v >= -9223372036854775808.0 && v < 9223372036854775808.0) ? (int64_t)v : (int64_t)0x8000000000000000ull;
}

// Bit casts between the widths the lift uses.
#define RA_BITCAST(name, from, to) static inline to ra_bitcast_##name(from v) { to r; __builtin_memcpy(&r, &v, sizeof r); return r; }
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
