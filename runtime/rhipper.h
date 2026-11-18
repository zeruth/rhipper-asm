// The runtime header of a program ras emit writes as C: the integer and vector types, the
// helpers the lift's intrinsics map to, and the runtime functions the driver provides.
#ifndef RHIPPER_H
#define RHIPPER_H

// Only stdint.h: a C runtime header would declare the functions the translation imports and
// declares itself, with other types.
#include <stdint.h>

// The translation declares every import with integer arguments and result, the ABI it was called
// with; a C runtime import clang knows as a builtin (`exit`, `memcpy`) is then a redeclaration
// with another type, which is the same call on the wire. One whose own prototype names a type of
// a header (`fread`, `fwrite`: `FILE *`) is declared without that header, which is the same.
#if defined(__clang__)
#pragma clang diagnostic ignored "-Wincompatible-library-redeclaration"
#pragma clang diagnostic ignored "-Wbuiltin-requires-header"
#elif defined(__GNUC__)
#pragma GCC diagnostic ignored "-Wbuiltin-declaration-mismatch"
#endif

#if defined(_MSC_VER) && !defined(__clang__)
#define RA_NORETURN __declspec(noreturn)
#define RA_ALIGNED(n) __declspec(align(n))
#define RA_UNUSED
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
// A parameter the convention passes and the function ignores.
#define RA_UNUSED __attribute__((unused))
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
// The scalar at an integer address of the program, as an lvalue: `RA_U32(m + 40u)` reads the
// dword there, `RA_U32(m + 40u) = 1u;` writes it. A 32-bit program's address widens through
// uintptr_t as the 32-bit model's zero extension does.
#define RA_U8(a) (*(uint8_t *)(uintptr_t)(a))
#define RA_U16(a) (*(ra_ua_uint16 *)(uintptr_t)(a))
#define RA_U32(a) (*(ra_ua_uint32 *)(uintptr_t)(a))
#define RA_U64(a) (*(ra_ua_uint64 *)(uintptr_t)(a))
#define RA_F32(a) (*(ra_ua_float *)(uintptr_t)(a))
#define RA_F64(a) (*(ra_ua_double *)(uintptr_t)(a))
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
// One open assumption of a port emitted checked: the function it was made in, what was assumed
// and the address of the instruction it was made at.
struct ra_assumption { const char *function; const char *kind; uint64_t site; };

// The entry hook of every function: nothing, unless the port is built with RA_TRACE, when each
// function names itself on stderr as it starts, or with RA_TRACE_LAST, when a crash report lists
// the last functions entered, for finding where a port goes wrong.
#if defined(RA_TRACE) || defined(RA_TRACE_LAST)
#define RA_ENTER(name) ra_enter(name)
#else
#define RA_ENTER(name) do { } while (0)
#endif

// The runtime functions the driver provides. The driver is C: a C++ port names them with C linkage.
#ifdef __cplusplus
extern "C" {
#endif
RA_NORETURN void ra_trap(uint64_t address, uint64_t code);
RA_NORETURN void ra_unsupported(uint64_t address, uint64_t code);
// The stop of a 32-bit program's import wrapper (`adapters.c`) whose structure does not convert.
RA_NORETURN void ra_adapt_failed(const char *what, uint64_t value);
// The checks of a port emitted checked. What the raise relied on and nothing proved is tested
// where it was relied on: `ra_assume(id, condition)` goes on when the condition holds, and
// otherwise prints the function, what was assumed and the site, and stops. The translation
// defines the table, one row per assumption at the index that is its number; a port emitted
// unchecked names neither the table nor the check.
extern const struct ra_assumption ra_assumptions[];
RA_NORETURN void ra_assume_failed(uint64_t id, const char *function, const char *kind, uint64_t site);
#define ra_assume(id, condition) \
    ((condition) ? (void)0 \
                 : ra_assume_failed((id), ra_assumptions[(id)].function, ra_assumptions[(id)].kind, ra_assumptions[(id)].site))
void ra_call(void *state, uint64_t target);
// A port linked with the residual engine (RA_RESIDUAL) holds the image's bytes at its base, where
// the engine runs the code no lifted function has: the program's own module handle is that base,
// so an address it computes from the handle is one of the image, as in the reference. Its exit
// reports what the engine entered first. The translation's own declarations of the imports
// declare the driver's functions under their new names.
#ifdef RA_RESIDUAL
#define GetModuleHandleA ra_residual_module_a
#define GetModuleHandleW ra_residual_module_w
#define ExitProcess ra_residual_exit
#endif
// What the x86 function a pointer named popped beyond its return address.
uint64_t ra_pops_of(uint64_t target);
// A call through a pointer of a 32-bit program that hands on stack words past the six of the
// universal call, as one expression: every argument is evaluated first, then the `n` words after
// the count go to ra_args, where the callee's universal entry point reads them, and then the call
// is made, the order the stores and the call had. The translation defines ra_args when one of its
// calls takes this form.
static inline uint64_t ra_args_call_into(uint64_t *words, void *fp, uint64_t a0, uint64_t a1, uint64_t a2, uint64_t a3, uint64_t a4,
                                         uint64_t a5, uint64_t a6, uint64_t a7, unsigned n, ...) {
    __builtin_va_list extra;
    __builtin_va_start(extra, n);
    for (unsigned k = 0; k < n; k++) words[k] = __builtin_va_arg(extra, uint64_t);
    __builtin_va_end(extra);
    return ((uint64_t (*)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t))fp)(a0, a1, a2, a3, a4, a5, a6, a7);
}
#define ra_args_call(...) ra_args_call_into((uint64_t *)(void *)ra_args, __VA_ARGS__)
void ra_enter(const char *name);
// The trace of a port emitted with tracing (`run.trace` of the project): every function records
// its arguments as it starts and its result as it returns, one line per event, to the file the
// RHIPPER_TRACE environment variable names, else to stderr. `kinds` has one letter per argument,
// each passed as a uint64_t: `u` an integer, `p` a pointer, `f` and `g` the bits of a double or a
// float, `?` a value of another type (passed as 0). The pipeline records the reference's trace in
// the same lines and compares the two event by event.
void ra_trace_enter(const char *name, const char *kinds, ...);
uint64_t ra_trace_ret(const char *name, uint64_t value, unsigned bits);
uint64_t ra_trace_ret_ptr(const char *name, uint64_t value);
double ra_trace_ret_f64(const char *name, double value);
float ra_trace_ret_f32(const char *name, float value);
void ra_trace_leave(const char *name);
uint64_t ra_f64_bits(double value);
uint64_t ra_f32_bits(float value);
// The R5900's FPU on the bits of its singles, which IEEE arithmetic is not: a denormal operand is a
// zero, exponent 255 a number (no infinity, no NaN), every result rounded toward zero, clamped to
// the largest magnitude 0x7fffffff and flushed to a zero of its sign below the smallest normal; a
// division by zero gives the largest magnitude, a square root takes its operand's magnitude.
uint32_t ra_f32_ftz_add(uint32_t a, uint32_t b);
uint32_t ra_f32_ftz_sub(uint32_t a, uint32_t b);
uint32_t ra_f32_ftz_mul(uint32_t a, uint32_t b);
uint32_t ra_f32_ftz_div(uint32_t a, uint32_t b);
uint32_t ra_f32_ftz_rsqrt(uint32_t a, uint32_t b);
uint32_t ra_f32_ftz_sqrt(uint32_t a);
uint32_t ra_f32_ftz_cvt_s_w(uint32_t a);
uint32_t ra_f32_ftz_cvt_w_s(uint32_t a);
// The platform interface of a console's port. Every access of a device register the lift saw at its
// address (`ra_<device>_read<bits>(register)`, `ra_<device>_write<bits>(register, value)`, the
// register at the address the memory map folds it to) and every call on the console's kernel
// (`ra_ee_<name>`) goes to the runtime bound here: the host's graphics, sound and input, or the
// stub. `ras emit` writes the program's calls on the interface into `platform.c` with the stub
// runtime, which logs each call on stderr in the order the program makes it and answers zero (the
// serial port's transmit FIFO is also the console on stdout, and the kernel's exits end the run);
// the port runs over the stub until its `main` binds another runtime with `ra_platform_bind`.
typedef struct ra_platform {
    uint64_t (*read)(const char *device, uint64_t reg, unsigned bits);
    void (*write)(const char *device, uint64_t reg, unsigned bits, uint64_t value);
    uint64_t (*call)(const char *name, unsigned count, const uint64_t *args);
} ra_platform;
extern const ra_platform ra_platform_stub;
extern const ra_platform *ra_platform_bound;
void ra_platform_bind(const ra_platform *platform);
// The EE's serial port by its registers (ps2sdk's <sio.h>), for a runtime to read the log by.
#define RA_SIO_LCR 0x1000f100ull
#define RA_SIO_LSR 0x1000f110ull
#define RA_SIO_IER 0x1000f120ull
#define RA_SIO_ISR 0x1000f130ull
#define RA_SIO_FCR 0x1000f140ull
#define RA_SIO_BGR 0x1000f150ull
#define RA_SIO_TXFIFO 0x1000f180ull
#define RA_SIO_RXFIFO 0x1000f1c0ull
// The prototypes of the program's calls on the interface, which `ras emit` writes beside a console's
// port in `platform.h`; a program of an operating system has none.
#if defined(__has_include)
#if __has_include("platform.h")
#include "platform.h"
#endif
#endif
// The processor's answers, from the host.
void ra_cpuid(uint64_t leaf, uint64_t subleaf, void *out);
uint64_t ra_xgetbv(uint64_t which);
uint64_t ra_rdtsc(void);
// The thread environment block of the calling thread, which fs: and gs: accesses read.
uint64_t ra_teb(void);
// A MIPS `syscall` of an o32 Linux program: the number and the four argument registers, the
// kernel's result back, negative with the error number on a failure. `write` and `exit` are
// answered by the host's C library.
uint64_t ra_syscall(uint64_t number, uint64_t a0, uint64_t a1, uint64_t a2, uint64_t a3);
// The C runtime functions a 32-bit program calls that walk or hand back a table of its own
// pointers: the runtime's versions keep the program's pointer width.
uint64_t ra_x86_initterm(uint64_t first, uint64_t last);
uint64_t ra_x86_initterm_e(uint64_t first, uint64_t last);
uint64_t ra_x86_initialize_onexit_table(uint64_t table);
uint64_t ra_x86_register_onexit_function(uint64_t table, uint64_t function);
uint64_t ra_x86_execute_onexit_table(uint64_t table);
uint64_t ra_x86_p_argv(void);
// The command line `WinMain` takes when the driver's startup stands in for the runtime's.
uint64_t ra_win_main_command_line(void);
// The C runtime's globals behind `__p__commode`, `__p__fmode`, `__p___argc` and `_errno`, as
// dwords at a 32-bit address.
uint64_t ra_x86_p_commode(void);
uint64_t ra_x86_p_fmode(void);
uint64_t ra_x86_p_argc(void);
uint64_t ra_x86_errno(void);
uint64_t ra_x86_initial_narrow_environment(void);
uint64_t ra_x86_initialize_slist_head(uint64_t head);
// The interlocked list over the program's 8-byte head and dword links.
uint64_t ra_x86_interlocked_push_entry_slist(uint64_t head, uint64_t entry);
uint64_t ra_x86_interlocked_pop_entry_slist(uint64_t head);
uint64_t ra_x86_interlocked_flush_slist(uint64_t head);
uint64_t ra_x86_query_depth_slist(uint64_t head);
// EncodePointer and DecodePointer within the program's dword: the 64-bit kernel32's take 64 bits.
uint64_t ra_x86_encode_pointer(uint64_t pointer);
uint64_t ra_x86_decode_pointer(uint64_t pointer);
// CommandLineToArgvW with a table of dword pointers, in one LocalAlloc block.
uint64_t ra_x86_command_line_to_argv_w(uint64_t line, uint64_t count);
// The interlocked operations of the 32-bit kernel32 on a 32-bit value, which the 64-bit one does
// not export.
uint64_t ra_x86_interlocked_increment(uint64_t target);
uint64_t ra_x86_interlocked_decrement(uint64_t target);
uint64_t ra_x86_interlocked_exchange(uint64_t target, uint64_t value);
uint64_t ra_x86_interlocked_exchange_add(uint64_t target, uint64_t value);
uint64_t ra_x86_interlocked_compare_exchange(uint64_t target, uint64_t exchange, uint64_t comparand);
// The module handles and the functions of the 64-bit host, which do not fit a 32-bit program's
// dword: a handle above 4 GB comes back as a dword that stands for it, a function as a trampoline
// below 4 GB that carries what a call through it pops.
uint64_t ra_x86_get_module_handle_a(uint64_t name);
uint64_t ra_x86_get_module_handle_w(uint64_t name);
uint64_t ra_x86_load_library_a(uint64_t name);
uint64_t ra_x86_load_library_w(uint64_t name);
uint64_t ra_x86_load_library_ex_a(uint64_t name, uint64_t file, uint64_t flags);
uint64_t ra_x86_load_library_ex_w(uint64_t name, uint64_t file, uint64_t flags);
uint64_t ra_x86_get_proc_address(uint64_t module, uint64_t name);
// The C++ runtime's `operator new` and `operator delete` a 32-bit program links statically,
// over the host's heap.
uint64_t ra_x86_operator_new(uint64_t size);
uint64_t ra_x86_operator_delete(uint64_t block);
// The run-time type information of a 32-bit program, whose structures hold absolute dwords where
// the host's `__RTDynamicCast`, `__RTCastToVoid` and `__RTtypeid` read the 64-bit layout.
uint64_t ra_x86_rt_dynamic_cast(uint64_t object, uint64_t vf_delta, uint64_t source, uint64_t target, uint64_t is_reference);
uint64_t ra_x86_rt_cast_to_void(uint64_t object);
uint64_t ra_x86_rt_typeid(uint64_t object);
// `type_info::name()` of a 32-bit program, whose descriptor caches the undecorated name in a dword
// where the host's `__std_type_info_name` reads eight bytes.
uint64_t ra_x86_std_type_info_name(uint64_t data, uint64_t root);
// The window structures of user32 (WNDCLASSA, WNDCLASSEXA, MSG, PAINTSTRUCT), which a 32-bit
// program lays out with 4-byte pointers: the runtime widens them for the host and narrows what
// comes back.
uint64_t ra_x86_register_class_a(uint64_t wndclass);
uint64_t ra_x86_register_class_ex_a(uint64_t wndclass);
uint64_t ra_x86_get_message_a(uint64_t msg, uint64_t hwnd, uint64_t first, uint64_t last);
uint64_t ra_x86_peek_message_a(uint64_t msg, uint64_t hwnd, uint64_t first, uint64_t last, uint64_t remove);
uint64_t ra_x86_translate_message(uint64_t msg);
uint64_t ra_x86_dispatch_message_a(uint64_t msg);
uint64_t ra_x86_begin_paint(uint64_t hwnd, uint64_t paint);
uint64_t ra_x86_end_paint(uint64_t hwnd, uint64_t paint);
// DefWindowProcA on the program's copy of a message's structure (CREATESTRUCTA, WINDOWPOS,
// NCCALCSIZE_PARAMS), which its window procedure was handed for the host's.
uint64_t ra_x86_def_window_proc_a(uint64_t hwnd, uint64_t message, uint64_t wparam, uint64_t lparam);
// The timer queue's handle, a dword of the program's, and its negative INVALID_HANDLE_VALUE.
uint64_t ra_x86_create_timer_queue_timer(uint64_t timer, uint64_t queue, uint64_t callback, uint64_t parameter, uint64_t due,
                                         uint64_t period, uint64_t flags);
uint64_t ra_x86_delete_timer_queue_timer(uint64_t queue, uint64_t timer, uint64_t event);
uint64_t ra_x86_init_once_execute_once(uint64_t once, uint64_t fn, uint64_t parameter, uint64_t context);
uint64_t ra_x86_wait_for_multiple_objects(uint64_t count, uint64_t handles, uint64_t all, uint64_t milliseconds);
uint64_t ra_x86_initialize_critical_section(uint64_t program);
uint64_t ra_x86_initialize_critical_section_and_spin_count(uint64_t program, uint64_t spin);
uint64_t ra_x86_initialize_critical_section_ex(uint64_t program, uint64_t spin, uint64_t flags);
uint64_t ra_x86_enter_critical_section(uint64_t program);
uint64_t ra_x86_leave_critical_section(uint64_t program);
uint64_t ra_x86_try_enter_critical_section(uint64_t program);
uint64_t ra_x86_delete_critical_section(uint64_t program);
// The floating-point environment, which on x86 is the x87 control and status words as much as the
// SSE register: the runtime keeps the lifted words and the host's in step, and the x86 C runtime's
// square root rounds by the precision control as the x87's does.
uint64_t ra_x86_controlfp(uint64_t new_bits, uint64_t mask);
uint64_t ra_x86_controlfp_s(uint64_t current, uint64_t new_bits, uint64_t mask);
uint64_t ra_x86_fesetround(uint64_t mode);
uint64_t ra_x86_statusfp(void);
uint64_t ra_x86_clearfp(void);
uint64_t ra_x86_fetestexcept(uint64_t excepts);
uint64_t ra_x86_feclearexcept(uint64_t excepts);
double ra_x86_sqrt(double x);
double ra_x86_pow_sse2(double x, double y);
double ra_x86_sqrt_sse2(double x);
double ra_x86_sin_sse2(double x);
double ra_x86_cos_sse2(double x);
double ra_x86_tan_sse2(double x);
double ra_x86_asin_sse2(double x);
double ra_x86_acos_sse2(double x);
double ra_x86_atan_sse2(double x);
void ra_x86_ci_atan2(void);
double ra_x86_exp_sse2(double x);
double ra_x86_log_sse2(double x);
double ra_x86_log10_sse2(double x);
// The fail-fast reports of the x86 /GS checks: the program ends as it would natively, with
// STATUS_STACK_BUFFER_OVERRUN.
RA_NORETURN void ra_x86_report_gsfailure(void);
RA_NORETURN void ra_x86_report_rangecheckfailure(void);
// The printf and scanf family through their va_list, the program's dword arguments widened to the
// host's slots by the format.
uint64_t ra_x86_vsprintf(uint64_t options, uint64_t buffer, uint64_t count, uint64_t format, uint64_t locale, uint64_t args);
uint64_t ra_x86_vsprintf_s(uint64_t options, uint64_t buffer, uint64_t count, uint64_t format, uint64_t locale, uint64_t args);
uint64_t ra_x86_vsnprintf_s(uint64_t options, uint64_t buffer, uint64_t count, uint64_t max, uint64_t format, uint64_t locale, uint64_t args);
uint64_t ra_x86_vfprintf(uint64_t options, uint64_t stream, uint64_t format, uint64_t locale, uint64_t args);
uint64_t ra_x86_vfscanf(uint64_t options, uint64_t stream, uint64_t format, uint64_t locale, uint64_t args);
uint64_t ra_x86_vsscanf(uint64_t options, uint64_t buffer, uint64_t count, uint64_t format, uint64_t locale, uint64_t args);
// The strto* family, their end pointer written at the program's width.
uint64_t ra_x86_strtol(uint64_t s, uint64_t end, uint64_t base);
uint64_t ra_x86_strtoul(uint64_t s, uint64_t end, uint64_t base);
uint64_t ra_x86_strtoll(uint64_t s, uint64_t end, uint64_t base);
uint64_t ra_x86_strtoull(uint64_t s, uint64_t end, uint64_t base);
double ra_x86_strtod(uint64_t s, uint64_t end);
// The addresses of a FILE's buffer fields (`_get_stream_buffer_pointers`), written at the program's width.
uint64_t ra_x86_get_stream_buffer_pointers(uint64_t stream, uint64_t base, uint64_t pointer, uint64_t count);
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

// C++ exceptions. A throw site hands the object and the types it can be caught as to the runtime,
// which keeps a copy in flight; a lifted function asks ra_eh_pending after each call and unwinds
// its own frame when one is in flight; a landing matches a handler by the key of its type (zero
// for a catch-all), which copies the object into the catch's slot (flags: the handler's
// adjectives, bit 3 for a catch by reference, and the pointer width in bits 8 and up); the catch
// body runs between ra_eh_enter_catch and ra_eh_leave_catch; destructors on the way out run with
// the exception suspended.
void ra_eh_throw(uint64_t object, uint64_t size);
void ra_eh_throw_type(uint64_t key, uint64_t displacement, uint64_t size);
void ra_eh_rethrow(void);
uint64_t ra_eh_pending(void);
uint64_t ra_eh_catches(uint64_t key, uint64_t slot, uint64_t flags);
void ra_eh_enter_catch(void);
void ra_eh_leave_catch(void);
void ra_eh_suspend(void);
void ra_eh_resume(void);
// What the x86 frame handler's protocol adds: the thrown object's destructor and the copy
// constructor of the catchable type just named; at the end of a catch, before ra_eh_leave_catch, the
// object and the destructor to run on it when the end of the catch ends the exception (zero when
// something still holds it); and the esp the handler saved when the catch began, which an unwind
// out of the catch puts back under the registration.
void ra_eh_throw_destructor(uint64_t destructor);
void ra_eh_throw_copy(uint64_t copy);
uint64_t ra_eh_ending_object(void);
uint64_t ra_eh_ending_destructor(void);
void ra_eh_catch_sp(uint64_t sp);
uint64_t ra_eh_ending_sp(void);
// The structured exceptions of a 32-bit __try: ra_eh_probe(address, size, write, site) puts the
// access violation in flight and answers 1 when the access would fault; ra_eh_pointers gives the
// EXCEPTION_POINTERS its filter reads, in the program's 32-bit layout; ra_eh_handled ends it, and
// ra_eh_unhandled(site) ends the program when no scope of the function takes it. ra_eh_raise is
// RaiseException(code, flags, count, arguments) at a site a scope protects: its exception in flight.
uint64_t ra_eh_probe(uint64_t address, uint64_t size, uint64_t write, uint64_t site);
void ra_eh_raise(uint64_t code, uint64_t flags, uint64_t count, uint64_t arguments, uint64_t site);
uint64_t ra_eh_pointers(void);
void ra_eh_handled(void);
void ra_eh_unhandled(uint64_t site);
#ifdef __cplusplus
}
#endif

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

// Bit casts between the widths the lift uses. A cast between two widths (an MMX value the lift
// keeps in a 64-bit integer, read as an SSE vector) takes the low bytes and zeroes the rest.
#define RA_BITCAST(name, from, to)                                                     \
    static inline to ra_bitcast_##name(from v) {                                       \
        to r;                                                                          \
        __builtin_memset(&r, 0, sizeof r);                                             \
        __builtin_memcpy(&r, &v, sizeof r < sizeof v ? sizeof r : sizeof v);           \
        return r;                                                                      \
    }
RA_BITCAST(u64_v4u32, uint64_t, ra_v4u32)
RA_BITCAST(v8u16_u64, ra_v8u16, uint64_t)
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
    static inline type ra_fshr_##name(type a, type b, type c) { c &= (bits - 1); return c ? (type)((a << (bits - c)) | (b >> c)) : b; } \
    static inline type ra_rotl_##name(type v, type c) { return ra_fshl_##name(v, v, c); } \
    static inline type ra_rotr_##name(type v, type c) { return ra_fshr_##name(v, v, c); }
RA_FUNNEL(i8, uint8_t, 8)
RA_FUNNEL(i16, uint16_t, 16)
RA_FUNNEL(i32, uint32_t, 32)
RA_FUNNEL(i64, uint64_t, 64)
// The idioms the emitter reads back as calls: the minimum and maximum a compare-and-select
// computes, and the walk to a string's end.
#define RA_MINMAX(name, type, stype) \
    static inline type ra_umin_##name(type a, type b) { return a < b ? a : b; } \
    static inline type ra_umax_##name(type a, type b) { return a > b ? a : b; } \
    static inline type ra_smin_##name(type a, type b) { return (stype)a < (stype)b ? a : b; } \
    static inline type ra_smax_##name(type a, type b) { return (stype)a > (stype)b ? a : b; }
RA_MINMAX(i8, uint8_t, int8_t)
RA_MINMAX(i16, uint16_t, int16_t)
RA_MINMAX(i32, uint32_t, int32_t)
RA_MINMAX(i64, uint64_t, int64_t)
static inline uint64_t ra_strlen(const char *s) { uint64_t n = 0; while (s[n]) n++; return n; }
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

// A port built with RA_SESSION and runtime/replay.c calls these imports through the session's
// wrappers, which record the run to the file RA_RECORD names or answer it from the one RA_REPLAY
// names, without the host: the translation's declarations, calls and import slots all take the
// wrapper's name.
#ifdef RA_SESSION
#define GetStdHandle ra_session_GetStdHandle
#define WriteFile ra_session_WriteFile
#define ExitProcess ra_session_ExitProcess
#define GetModuleHandleA ra_session_GetModuleHandleA
#define RegisterClassA ra_session_RegisterClassA
#define RegisterClassExA ra_session_RegisterClassExA
#define CreateWindowExA ra_session_CreateWindowExA
#define DefWindowProcA ra_session_DefWindowProcA
#define DestroyWindow ra_session_DestroyWindow
#define ShowWindow ra_session_ShowWindow
#define UpdateWindow ra_session_UpdateWindow
#define InvalidateRect ra_session_InvalidateRect
#define GetMessageA ra_session_GetMessageA
#define PeekMessageA ra_session_PeekMessageA
#define TranslateMessage ra_session_TranslateMessage
#define DispatchMessageA ra_session_DispatchMessageA
#define PostMessageA ra_session_PostMessageA
#define SendMessageA ra_session_SendMessageA
#define PostQuitMessage ra_session_PostQuitMessage
#define BeginPaint ra_session_BeginPaint
#define EndPaint ra_session_EndPaint
#define FillRect ra_session_FillRect
#define GetClientRect ra_session_GetClientRect
#define GetWindowRect ra_session_GetWindowRect
#endif

#endif
