// The runtime of a port's harness. `ras verify --built` writes the harness beside the port as
// `verify.c`, which includes this file: a loader for each vector a run of the program recorded
// and a mock for each callee the vectors call.
//
// A loader puts the bytes the function read back where the image has them, hands the mocks the
// calls the function made in the order it made them, calls the port's function with the
// arguments of its entry, and compares the result and the bytes the function wrote with the
// record. A mock takes the next call of the record, compares the arguments the record holds,
// writes the callee's effects and returns its result. A call the record does not hold, or holds
// of another callee, stops the vector where it is. `ra_verify` runs every loader and prints a
// line for each vector and a last line that counts them; it returns the vectors that differ.

#include <setjmp.h>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

// The bits of an argument a mock compares: the port of a 32-bit program hands its integers in
// the low half of a 64-bit register, whose high half is no part of the value.
#ifndef RA_VERIFY_WORD
#define RA_VERIFY_WORD UINT64_MAX
#endif

// Where the port has the byte of the image at `address`: at the address itself, unless the
// build of the harness lays the image out elsewhere and says so.
#ifndef RA_VERIFY_AT
#define RA_VERIFY_AT(address) ((uint8_t *)(uintptr_t)(address))
#endif

// Adjacent bytes of the image and the address of the first.
struct ra_verify_run {
    uint64_t address;
    const uint8_t *bytes;
    size_t size;
};

// One call the function made, as the record answers it.
struct ra_verify_call {
    const char *callee;
    // The arguments by their low 64 bits; none for a call the record kept over the state.
    size_t arg_count;
    const uint64_t *args;
    // Whether the record holds the callee's result, and the result's bits.
    int answered;
    uint64_t result;
    const struct ra_verify_run *effects;
    size_t effect_count;
};

struct ra_verify_vector {
    const char *function;
    int (*load)(void);
};

// A callee and the mock that answers in its place.
struct ra_verify_mock {
    const char *callee;
    void (*mock)(void);
};

enum ra_verify_outcome { RA_VERIFY_OK, RA_VERIFY_DIFFERS, RA_VERIFY_UNCHECKED, RA_VERIFY_SKIPPED };

// The vector being loaded.
static struct {
    const char *function;
    unsigned number;
    const struct ra_verify_call *calls;
    size_t call_count;
    size_t next;
    char why[512];
    jmp_buf stop;
} ra_verify_now;

static inline uint64_t ra_verify_bits_f64(double value) {
    uint64_t bits;
    memcpy(&bits, &value, sizeof bits);
    return bits;
}

static inline uint64_t ra_verify_bits_f32(float value) {
    uint32_t bits;
    memcpy(&bits, &value, sizeof bits);
    return bits;
}

static inline double ra_verify_f64(uint64_t bits) {
    double value;
    memcpy(&value, &bits, sizeof value);
    return value;
}

static inline float ra_verify_f32(uint64_t bits) {
    uint32_t low = (uint32_t)bits;
    float value;
    memcpy(&value, &low, sizeof value);
    return value;
}

__attribute__((format(printf, 1, 2))) static void ra_verify_note(const char *format, ...) {
    va_list args;
    va_start(args, format);
    vsnprintf(ra_verify_now.why, sizeof ra_verify_now.why, format, args);
    va_end(args);
}

// Ends the vector inside the function, with `outcome` and the reason in `why`.
__attribute__((noreturn)) static void ra_verify_stop(int outcome) { longjmp(ra_verify_now.stop, outcome); }

static void ra_verify_write(const struct ra_verify_run *runs, size_t count) {
    for (size_t i = 0; i < count; i++) {
        uint8_t *at = RA_VERIFY_AT(runs[i].address);
        // Bytes the port holds already are not written again: its read-only data holds its own.
        if (memcmp(at, runs[i].bytes, runs[i].size) != 0) {
            memcpy(at, runs[i].bytes, runs[i].size);
        }
    }
}

static void ra_verify_begin(unsigned number, const char *function, const struct ra_verify_run *in, size_t in_count,
                            const struct ra_verify_call *calls, size_t call_count) {
    ra_verify_now.function = function;
    ra_verify_now.number = number;
    ra_verify_now.calls = calls;
    ra_verify_now.call_count = call_count;
    ra_verify_now.next = 0;
    ra_verify_now.why[0] = 0;
    ra_verify_write(in, in_count);
}

// Starts the vector in the loader, whose frame a mock that stops the vector returns to.
#define RA_VERIFY_BEGIN(number, function, in, in_count, calls, call_count) \
    do { \
        ra_verify_begin((number), (function), (in), (in_count), (calls), (call_count)); \
        switch (setjmp(ra_verify_now.stop)) { \
        case 0: break; \
        case RA_VERIFY_UNCHECKED: return RA_VERIFY_UNCHECKED; \
        default: return RA_VERIFY_DIFFERS; \
        } \
    } while (0)

// The answer of the next call, which a mock of `callee` asks for with the arguments it was
// handed, each by its low 64 bits.
static uint64_t ra_verify_answer(const char *callee, size_t count, const uint64_t *args) {
    size_t n = ra_verify_now.next + 1;
    if (ra_verify_now.next == ra_verify_now.call_count) {
        ra_verify_note("call %zu is of %s, past the %zu calls of the record", n, callee, ra_verify_now.call_count);
        ra_verify_stop(RA_VERIFY_DIFFERS);
    }
    const struct ra_verify_call *call = &ra_verify_now.calls[ra_verify_now.next++];
    if (strcmp(call->callee, callee) != 0) {
        ra_verify_note("call %zu is of %s, the record's of %s", n, callee, call->callee);
        ra_verify_stop(RA_VERIFY_DIFFERS);
    }
    if (call->arg_count != 0) {
        if (call->arg_count != count) {
            ra_verify_note("call %zu of %s hands %zu arguments, the record's %zu", n, callee, count, call->arg_count);
            ra_verify_stop(RA_VERIFY_DIFFERS);
        }
        for (size_t i = 0; i < count; i++) {
            if (((args[i] ^ call->args[i]) & RA_VERIFY_WORD) != 0) {
                ra_verify_note("call %zu of %s: argument %zu is 0x%llx, the record's 0x%llx", n, callee, i + 1,
                               (unsigned long long)args[i], (unsigned long long)call->args[i]);
                ra_verify_stop(RA_VERIFY_DIFFERS);
            }
        }
    }
    if (!call->answered) {
        ra_verify_note("call %zu of %s: the record holds no result of it", n, callee);
        ra_verify_stop(RA_VERIFY_UNCHECKED);
    }
    ra_verify_write(call->effects, call->effect_count);
    return call->result;
}

// The end of a vector whose function returned: every call made, every byte it wrote as the
// record has it.
static int ra_verify_end(const struct ra_verify_run *out, size_t count) {
    if (ra_verify_now.next != ra_verify_now.call_count) {
        ra_verify_note("made %zu of the %zu calls of the record", ra_verify_now.next, ra_verify_now.call_count);
        return RA_VERIFY_DIFFERS;
    }
    for (size_t i = 0; i < count; i++) {
        const uint8_t *at = RA_VERIFY_AT(out[i].address);
        for (size_t j = 0; j < out[i].size; j++) {
            if (at[j] != out[i].bytes[j]) {
                ra_verify_note("byte 0x%llx is 0x%02x, the record's 0x%02x", (unsigned long long)(out[i].address + j), at[j],
                               out[i].bytes[j]);
                return RA_VERIFY_DIFFERS;
            }
        }
    }
    return RA_VERIFY_OK;
}

static int ra_verify_result(uint64_t result, uint64_t expected, const struct ra_verify_run *out, size_t count) {
    if (result != expected) {
        ra_verify_note("returned 0x%llx, the record's 0x%llx", (unsigned long long)result, (unsigned long long)expected);
        return RA_VERIFY_DIFFERS;
    }
    return ra_verify_end(out, count);
}

// The end of a vector whose function returns a value the record does not hold.
static int ra_verify_unknown(const struct ra_verify_run *out, size_t count) {
    int outcome = ra_verify_end(out, count);
    if (outcome != RA_VERIFY_OK) {
        return outcome;
    }
    ra_verify_note("the record holds no result of it");
    return RA_VERIFY_UNCHECKED;
}

// A vector the port cannot take as it was recorded, and why.
static int ra_verify_skip(unsigned number, const char *function, const char *why) {
    ra_verify_now.function = function;
    ra_verify_now.number = number;
    ra_verify_note("%s", why);
    return RA_VERIFY_SKIPPED;
}

static int ra_verify_all(const struct ra_verify_vector *vectors, size_t count) {
    static const char *const words[] = {"ok", "differs", "unchecked", "skipped"};
    size_t outcomes[4] = {0};
    for (size_t i = 0; i < count; i++) {
        ra_verify_now.why[0] = 0;
        int outcome = vectors[i].load();
        outcomes[outcome]++;
        printf("%s vector %u %s%s%s\n", vectors[i].function, ra_verify_now.number, words[outcome], ra_verify_now.why[0] ? ": " : "",
               ra_verify_now.why);
    }
    printf("verified %zu of %zu vectors, %zu differ, %zu unchecked, %zu skipped\n", outcomes[RA_VERIFY_OK], count,
           outcomes[RA_VERIFY_DIFFERS], outcomes[RA_VERIFY_UNCHECKED], outcomes[RA_VERIFY_SKIPPED]);
    return (int)outcomes[RA_VERIFY_DIFFERS];
}
