// The guest of the vector harness: a freestanding program with no C library, built by clang in
// WSL for AArch64 Linux (run under qemu-aarch64-static) or for 32-bit ARM Linux (run under
// qemu-arm-static). It maps the case page and the data region at the addresses the vectors name,
// reads cases from a file named on the command line (or standard input), runs each through the
// trampoline of aarch64.S or arm.S and writes the machine's state after it to the output file (or
// standard output). Every case ends in a signal: the breakpoint after the instruction, the
// breakpoint fill a branch lands on, or the fault the instruction itself raises. The handler
// copies the registers out of the signal context, so the state is the machine's at the moment the
// case stopped, and resumes the trampoline with the host's registers.
//
// Built and run by `write_arm64_vectors` in crates/ra-asm-lift/tests/vectors.rs:
//   clang --target=aarch64-linux-gnu -ffreestanding -fno-builtin -nostdlib -static
//         -mgeneral-regs-only -fno-stack-protector -O1 -fuse-ld=lld -Wl,--image-base=0x0a000000
//         guest.c aarch64.S -o guest
//   qemu-aarch64-static -cpu max,pauth=off guest cases.bin results.bin
// `pauth=off` keeps the pointer-authentication hints the no-ops the lift takes them for; the keys
// of a process are random, so with it on `paciasp` would sign x30 differently on every run.
// And by `write_arm32_vectors` in crates/ra-asm-lift/tests/vectors/arm32.rs:
//   clang --target=armv7-linux-gnueabi -marm -mfloat-abi=soft -ffreestanding -fno-builtin
//         -nostdlib -static -fno-stack-protector -O1 -fuse-ld=lld -Wl,--image-base=0x0a000000
//         guest.c arm.S -o guest32
//   qemu-arm-static -cpu max guest32 cases.bin results.bin

typedef unsigned long long u64;
typedef unsigned int u32;
typedef unsigned short u16;
typedef unsigned char u8;
// The machine word: the width of a register and of a pointer.
typedef unsigned long uw;

// The addresses the vectors name: the page the case runs in and the region its loads and stores
// touch. The replay maps the same addresses in its own process.
#define CODE_MAP 0x30000000ul
#define CODE_MAP_LEN 0x10000ul
#define CODE_BASE 0x30001000ul
#define CODE_LEN 0x2000ul
#define CASE_IP 0x30002000ul
#define DATA_BASE 0x40000000ul
#define DATA_LEN 0x30000ul

#define WINDOW 64

#if defined(__aarch64__)

#define BRK 0xd4200000u
// `ldr x9, [x0, #72]` and `ldr x0, [x0]`: the prologue before the case, see aarch64.S.
#define PROLOGUE_0 0xf9402409u
#define PROLOGUE_1 0xf9400000u

// One case in: the registers (x0..x30, then sp), the flags as four bits, the vector registers as
// pairs of words (low first), the instruction word, and the window of data bytes with its address.
struct case_in {
    u64 x[32];
    u64 nzcv;
    u64 pad;
    u64 v[64];
    u32 code;
    u32 pad2;
    u64 window_at;
    u8 window[WINDOW];
};

// One case out: the signal that ended it and the pc it ended at, the registers, the flags and the
// vectors as the context held them, the first address outside the window a store reached (plus
// one, zero for none) and the window as the case left it.
struct case_out {
    u64 sig;
    u64 pc;
    u64 x[32];
    u64 nzcv;
    u64 v[64];
    u64 stray;
    u8 window[WINDOW];
};

_Static_assert(sizeof(struct case_in) == 864, "the layout aarch64.S and the generator share");
_Static_assert(__builtin_offsetof(struct case_in, nzcv) == 256, "IN_NZCV");
_Static_assert(__builtin_offsetof(struct case_in, v) == 272, "IN_V");
_Static_assert(sizeof(struct case_out) == 864, "the layout the generator reads");

// The signal context of AArch64 Linux, as the kernel and qemu lay it out.
struct kernel_stack {
    void *sp;
    int flags;
    u64 size;
};

struct kernel_sigaction {
    void (*handler)(int, void *, void *);
    u64 flags;
    u64 mask;
};

struct sigcontext {
    u64 fault_address;
    u64 regs[31];
    u64 sp;
    u64 pc;
    u64 pstate;
    u8 reserved[4096] __attribute__((aligned(16)));
};

struct ucontext {
    u64 flags;
    void *link;
    struct kernel_stack stack;
    u64 sigmask;
    u8 unused[120];
    struct sigcontext mc;
};

_Static_assert(__builtin_offsetof(struct ucontext, mc) == 176, "uc_mcontext");
_Static_assert(__builtin_offsetof(struct ucontext, mc.pc) == 440, "uc_mcontext.pc");
_Static_assert(__builtin_offsetof(struct ucontext, mc.reserved) == 464, "uc_mcontext.__reserved");

#define FPSIMD_MAGIC 0x46508001u

#define SYS_openat 56
#define SYS_read 63
#define SYS_write 64
#define SYS_exit_group 94
#define SYS_sigaltstack 132
#define SYS_rt_sigaction 134
#define SYS_mmap 222

#define SAVED_WORDS 13

#elif defined(__arm__)

// The breakpoints and the prologue `ldr r12, [r0, #48]; ldr r0, [r0]` of each state, as words of
// the page: the Thumb ones are halfword pairs in memory order, see arm.S.
#define BKPT_ARM 0xe1200070u
#define BKPT_THUMB 0xbe00be00u
#define PROLOGUE_ARM_0 0xe590c030u
#define PROLOGUE_ARM_1 0xe5900000u
#define PROLOGUE_THUMB_0 0xc030f8d0u
#define PROLOGUE_THUMB_1 0x0000f8d0u

// One case in: the registers (r0..r15, r15 unused), the flags as four bits, whether the case is
// Thumb code, the length of its encoding, the window's address, the encoding's bytes in memory
// order (an IT block with four 16-bit instructions is ten) and the window of data bytes.
struct case_in {
    u32 r[16];
    u32 nzcv;
    u32 thumb;
    u32 len;
    u32 window_at;
    u8 code[16];
    u8 window[WINDOW];
};

// One case out: the signal that ended it and the pc it ended at, the registers (r15 the pc) and
// the flags as the context held them, the first address outside the window a store reached (plus
// one, zero for none) and the window as the case left it.
struct case_out {
    u32 sig;
    u32 pc;
    u32 r[16];
    u32 nzcv;
    u32 stray;
    u8 window[WINDOW];
};

_Static_assert(sizeof(struct case_in) == 160, "the layout arm.S and the generator share");
_Static_assert(__builtin_offsetof(struct case_in, nzcv) == 64, "IN_NZCV");
_Static_assert(__builtin_offsetof(struct case_in, thumb) == 68, "IN_THUMB");
_Static_assert(__builtin_offsetof(struct case_in, code) == 80, "the encoding");
_Static_assert(sizeof(struct case_out) == 144, "the layout the generator reads");

// The signal context of 32-bit ARM Linux, as the kernel and qemu lay it out.
struct kernel_stack {
    void *sp;
    int flags;
    u32 size;
};

struct kernel_sigaction {
    void (*handler)(int, void *, void *);
    u32 flags;
    void (*restorer)(void);
    u64 mask;
};

struct sigcontext {
    u32 trap_no;
    u32 error_code;
    u32 oldmask;
    u32 regs[16];
    u32 cpsr;
    u32 fault_address;
};

struct ucontext {
    u32 flags;
    void *link;
    struct kernel_stack stack;
    struct sigcontext mc;
};

_Static_assert(__builtin_offsetof(struct ucontext, mc) == 20, "uc_mcontext");
_Static_assert(__builtin_offsetof(struct ucontext, mc.regs) == 32, "uc_mcontext.arm_r0");
_Static_assert(__builtin_offsetof(struct ucontext, mc.cpsr) == 96, "uc_mcontext.arm_cpsr");

// The Thumb state bit and the IT-block state bits of the CPSR, which a stopped Thumb case leaves
// set and ARM code cannot resume under.
#define CPSR_THUMB 0x20u
#define CPSR_IT 0x0600fc00u

#define SYS_read 3
#define SYS_write 4
#define SYS_rt_sigaction 174
#define SYS_sigaltstack 186
#define SYS_mmap2 192
#define SYS_exit_group 248
#define SYS_openat 322

#define SAVED_WORDS 10

#else
#error "the guest is AArch64 or 32-bit ARM"
#endif

#define SIGILL 4
#define SIGTRAP 5
#define SIGBUS 7
#define SIGFPE 8
#define SIGSEGV 11

#define SA_SIGINFO 4ul
#define SA_ONSTACK 0x08000000ul
#define SA_NODEFER 0x40000000ul

long sys_call(long number, long a0, long a1, long a2, long a3, long a4, long a5);
void flush_code(uw begin, uw end);
void run_case(const struct case_in *in);
void run_case_done(void);
extern uw saved[SAVED_WORDS];

static struct case_in g_in;
static struct case_out g_out;
static u8 altstack[65536] __attribute__((aligned(16)));

// The freestanding build still needs these names for a struct copy or a cleared array.
void *memcpy(void *to, const void *from, uw len) {
    u8 *t = to;
    const u8 *f = from;
    while (len--) {
        *t++ = *f++;
    }
    return to;
}

void *memset(void *to, int byte, uw len) {
    u8 *t = to;
    while (len--) {
        *t++ = (u8)byte;
    }
    return to;
}

static void exit_with(int code) {
    sys_call(SYS_exit_group, code, 0, 0, 0, 0, 0);
    for (;;) {
    }
}

// Reads `len` bytes, or fewer at the end of the input: 0 at a clean end, -1 on a short record.
static long read_all(int fd, u8 *buf, uw len) {
    uw got = 0;
    while (got < len) {
        long n = sys_call(SYS_read, fd, (long)(buf + got), (long)(len - got), 0, 0, 0);
        if (n <= 0) {
            break;
        }
        got += (uw)n;
    }
    if (got == 0) {
        return 0;
    }
    return got == len ? (long)len : -1;
}

static int write_all(int fd, const u8 *buf, uw len) {
    uw done = 0;
    while (done < len) {
        long n = sys_call(SYS_write, fd, (long)(buf + done), (long)(len - done), 0, 0, 0);
        if (n <= 0) {
            return -1;
        }
        done += (uw)n;
    }
    return 0;
}

#if defined(__aarch64__)

// The case stopped: its state out of the context, the host's registers back in, and the
// trampoline resumed at run_case_done.
static void handler(int sig, void *info, void *context) {
    struct ucontext *uc = context;
    (void)info;
    g_out.sig = (u64)sig;
    g_out.pc = uc->mc.pc;
    for (int i = 0; i < 31; i++) {
        g_out.x[i] = uc->mc.regs[i];
    }
    g_out.x[31] = uc->mc.sp;
    g_out.nzcv = (uc->mc.pstate >> 28) & 15;
    for (int i = 0; i < 64; i++) {
        g_out.v[i] = 0;
    }
    u8 *record = uc->mc.reserved;
    for (;;) {
        u32 magic = *(u32 *)record;
        u32 size = *(u32 *)(record + 4);
        if (magic == 0 || size == 0 || record + size > uc->mc.reserved + sizeof uc->mc.reserved) {
            break;
        }
        if (magic == FPSIMD_MAGIC) {
            const u64 *vregs = (const u64 *)(record + 16);
            for (int i = 0; i < 64; i++) {
                g_out.v[i] = vregs[i];
            }
            break;
        }
        record += size;
    }
    for (int i = 0; i < 12; i++) {
        uc->mc.regs[19 + i] = saved[i];
    }
    uc->mc.sp = saved[12];
    uc->mc.pc = (u64)run_case_done;
}

static void map_fixed(uw at, uw len, long prot) {
    long got = sys_call(SYS_mmap, (long)at, (long)len, prot, 0x2 | 0x20 | 0x10, -1, 0);
    if ((uw)got != at) {
        exit_with(4);
    }
}

// The page: the fill with the prologue before the case.
static void fill_page(volatile u32 *page) {
    for (uw i = 0; i < CODE_LEN / 4; i++) {
        page[i] = BRK;
    }
    page[(CASE_IP - CODE_BASE) / 4 - 2] = PROLOGUE_0;
    page[(CASE_IP - CODE_BASE) / 4 - 1] = PROLOGUE_1;
    flush_code(CODE_BASE, CODE_BASE + CODE_LEN);
}

// The case's instruction into the page.
static void place_case(volatile u32 *page) {
    page[(CASE_IP - CODE_BASE) / 4] = g_in.code;
    flush_code(CASE_IP & ~63ul, (CASE_IP & ~63ul) + 64);
}

#elif defined(__arm__)

// The case stopped: its state out of the context, the host's registers back in, and the
// trampoline resumed at run_case_done in ARM state, with no IT block pending.
static void handler(int sig, void *info, void *context) {
    struct ucontext *uc = context;
    (void)info;
    g_out.sig = (u32)sig;
    g_out.pc = uc->mc.regs[15];
    for (int i = 0; i < 16; i++) {
        g_out.r[i] = uc->mc.regs[i];
    }
    g_out.nzcv = (uc->mc.cpsr >> 28) & 15;
    for (int i = 0; i < 8; i++) {
        uc->mc.regs[4 + i] = saved[i];
    }
    uc->mc.regs[13] = saved[8];
    uc->mc.regs[14] = saved[9];
    uc->mc.regs[15] = (u32)run_case_done;
    uc->mc.cpsr &= ~(CPSR_THUMB | CPSR_IT);
}

static void map_fixed(uw at, uw len, long prot) {
    long got = sys_call(SYS_mmap2, (long)at, (long)len, prot, 0x2 | 0x20 | 0x10, -1, 0);
    if ((uw)got != at) {
        exit_with(4);
    }
}

// The state the page is filled for: the breakpoints and the prologue are of one instruction set,
// and the page is refilled when a case of the other comes.
static int page_thumb = -1;

static void fill_page(volatile u32 *page) {
    int thumb = g_in.thumb != 0;
    if (thumb == page_thumb) {
        return;
    }
    page_thumb = thumb;
    for (uw i = 0; i < CODE_LEN / 4; i++) {
        page[i] = thumb ? BKPT_THUMB : BKPT_ARM;
    }
    page[(CASE_IP - CODE_BASE) / 4 - 2] = thumb ? PROLOGUE_THUMB_0 : PROLOGUE_ARM_0;
    page[(CASE_IP - CODE_BASE) / 4 - 1] = thumb ? PROLOGUE_THUMB_1 : PROLOGUE_ARM_1;
    flush_code(CODE_BASE, CODE_BASE + CODE_LEN);
}

// The case's bytes into the page, the fill restored after a longer case before.
static void place_case(volatile u32 *page) {
    uw at = (CASE_IP - CODE_BASE) / 4;
    for (uw i = 0; i < sizeof g_in.code / 4; i++) {
        page[at + i] = page_thumb ? BKPT_THUMB : BKPT_ARM;
    }
    volatile u8 *bytes = (volatile u8 *)CASE_IP;
    for (u32 i = 0; i < g_in.len && i < sizeof g_in.code; i++) {
        bytes[i] = g_in.code[i];
    }
    flush_code(CASE_IP & ~63ul, (CASE_IP & ~63ul) + 64);
}

#endif

static void install(int sig) {
    struct kernel_sigaction act;
    memset(&act, 0, sizeof act);
    act.handler = handler;
    act.flags = SA_SIGINFO | SA_ONSTACK | SA_NODEFER;
    act.mask = 0;
    if (sys_call(SYS_rt_sigaction, sig, (long)&act, 0, 8, 0, 0) != 0) {
        exit_with(6);
    }
}

int guest_main(long argc, char **argv) {
    int in_fd = 0;
    int out_fd = 1;
    if (argc >= 3) {
        in_fd = (int)sys_call(SYS_openat, -100, (long)argv[1], 0, 0, 0, 0);
        out_fd = (int)sys_call(SYS_openat, -100, (long)argv[2], 0x241, 0644, 0, 0);
        if (in_fd < 0 || out_fd < 0) {
            exit_with(2);
        }
    }
    map_fixed(CODE_MAP, CODE_MAP_LEN, 1 | 2 | 4);
    map_fixed(DATA_BASE, DATA_LEN, 1 | 2);

    struct kernel_stack ss;
    ss.sp = altstack;
    ss.flags = 0;
    ss.size = sizeof altstack;
    if (sys_call(SYS_sigaltstack, (long)&ss, 0, 0, 0, 0, 0) != 0) {
        exit_with(6);
    }
    install(SIGILL);
    install(SIGTRAP);
    install(SIGBUS);
    install(SIGFPE);
    install(SIGSEGV);

    volatile u32 *page = (volatile u32 *)CODE_BASE;
#if defined(__aarch64__)
    fill_page(page);
#endif

    volatile uw *data = (volatile uw *)DATA_BASE;
    for (;;) {
        long n = read_all(in_fd, (u8 *)&g_in, sizeof g_in);
        if (n == 0) {
            break;
        }
        if (n < 0) {
            exit_with(3);
        }
        if (g_in.window_at < DATA_BASE || g_in.window_at + WINDOW > DATA_BASE + DATA_LEN) {
            exit_with(7);
        }
#if defined(__arm__)
        fill_page(page);
#endif
        place_case(page);
        volatile u8 *window = (volatile u8 *)(uw)g_in.window_at;
        for (int i = 0; i < WINDOW; i++) {
            window[i] = g_in.window[i];
        }

        run_case(&g_in);

        for (int i = 0; i < WINDOW; i++) {
            g_out.window[i] = window[i];
            window[i] = 0;
        }
        g_out.stray = 0;
        for (uw i = 0; i < DATA_LEN / sizeof(uw); i++) {
            if (data[i] != 0) {
                if (g_out.stray == 0) {
                    g_out.stray = DATA_BASE + i * sizeof(uw) + 1;
                }
                data[i] = 0;
            }
        }
        if (write_all(out_fd, (const u8 *)&g_out, sizeof g_out) != 0) {
            exit_with(5);
        }
    }
    return 0;
}
