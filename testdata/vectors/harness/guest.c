// The guest of the vector harness: a freestanding program with no C library, built by clang in
// WSL for AArch64 Linux and run under qemu-aarch64-static. It maps the case page and the data
// region at the addresses the vectors name, reads cases from a file named on the command line (or
// standard input), runs each through the trampoline of aarch64.S and writes the machine's state
// after it to the output file (or standard output). Every case ends in a signal: the `brk` after
// the instruction, the `brk` fill a branch lands on, or the fault the instruction itself raises.
// The handler copies the registers out of the signal context, so the state is the machine's at
// the moment the case stopped, and resumes the trampoline with the host's registers.
//
// Built and run by `write_arm64_vectors` in crates/ra-asm-lift/tests/vectors.rs:
//   clang --target=aarch64-linux-gnu -ffreestanding -fno-builtin -nostdlib -static
//         -mgeneral-regs-only -fno-stack-protector -O1 -fuse-ld=lld -Wl,--image-base=0x0a000000
//         guest.c aarch64.S -o guest
//   qemu-aarch64-static -cpu max,pauth=off guest cases.bin results.bin
// `pauth=off` keeps the pointer-authentication hints the no-ops the lift takes them for; the keys
// of a process are random, so with it on `paciasp` would sign x30 differently on every run.

typedef unsigned long u64;
typedef unsigned int u32;
typedef unsigned char u8;

// The addresses the vectors name: the page the case runs in and the region its loads and stores
// touch. The replay maps the same addresses in its own process.
#define CODE_MAP 0x30000000ul
#define CODE_MAP_LEN 0x10000ul
#define CODE_BASE 0x30001000ul
#define CODE_LEN 0x2000ul
#define CASE_IP 0x30002000ul
#define DATA_BASE 0x40000000ul
#define DATA_LEN 0x30000ul

#define BRK 0xd4200000u
// `ldr x9, [x0, #72]` and `ldr x0, [x0]`: the prologue before the case, see aarch64.S.
#define PROLOGUE_0 0xf9402409u
#define PROLOGUE_1 0xf9400000u

#define WINDOW 64

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

#define SIGILL 4
#define SIGTRAP 5
#define SIGBUS 7
#define SIGFPE 8
#define SIGSEGV 11

#define SA_SIGINFO 4ul
#define SA_ONSTACK 0x08000000ul
#define SA_NODEFER 0x40000000ul

long sys_call(long number, long a0, long a1, long a2, long a3, long a4, long a5);
void flush_code(u64 begin, u64 end);
void run_case(const struct case_in *in);
void run_case_done(void);
extern u64 saved[13];

static struct case_in g_in;
static struct case_out g_out;
static u8 altstack[65536] __attribute__((aligned(16)));

// The freestanding build still needs these names for a struct copy or a cleared array.
void *memcpy(void *to, const void *from, u64 len) {
    u8 *t = to;
    const u8 *f = from;
    while (len--) {
        *t++ = *f++;
    }
    return to;
}

void *memset(void *to, int byte, u64 len) {
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
static long read_all(int fd, u8 *buf, u64 len) {
    u64 got = 0;
    while (got < len) {
        long n = sys_call(SYS_read, fd, (long)(buf + got), (long)(len - got), 0, 0, 0);
        if (n <= 0) {
            break;
        }
        got += (u64)n;
    }
    if (got == 0) {
        return 0;
    }
    return got == len ? (long)len : -1;
}

static int write_all(int fd, const u8 *buf, u64 len) {
    u64 done = 0;
    while (done < len) {
        long n = sys_call(SYS_write, fd, (long)(buf + done), (long)(len - done), 0, 0, 0);
        if (n <= 0) {
            return -1;
        }
        done += (u64)n;
    }
    return 0;
}

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

static void install(int sig) {
    struct kernel_sigaction act;
    act.handler = handler;
    act.flags = SA_SIGINFO | SA_ONSTACK | SA_NODEFER;
    act.mask = 0;
    if (sys_call(SYS_rt_sigaction, sig, (long)&act, 0, 8, 0, 0) != 0) {
        exit_with(6);
    }
}

static void map_fixed(u64 at, u64 len, long prot) {
    long got = sys_call(SYS_mmap, (long)at, (long)len, prot, 0x2 | 0x20 | 0x10, -1, 0);
    if ((u64)got != at) {
        exit_with(4);
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
    for (u64 i = 0; i < CODE_LEN / 4; i++) {
        page[i] = BRK;
    }
    page[(CASE_IP - CODE_BASE) / 4 - 2] = PROLOGUE_0;
    page[(CASE_IP - CODE_BASE) / 4 - 1] = PROLOGUE_1;
    flush_code(CODE_BASE, CODE_BASE + CODE_LEN);

    volatile u64 *data = (volatile u64 *)DATA_BASE;
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
        page[(CASE_IP - CODE_BASE) / 4] = g_in.code;
        flush_code(CASE_IP & ~63ul, (CASE_IP & ~63ul) + 64);
        volatile u8 *window = (volatile u8 *)g_in.window_at;
        for (int i = 0; i < WINDOW; i++) {
            window[i] = g_in.window[i];
        }

        run_case(&g_in);

        for (int i = 0; i < WINDOW; i++) {
            g_out.window[i] = window[i];
            window[i] = 0;
        }
        g_out.stray = 0;
        for (u64 i = 0; i < DATA_LEN / 8; i++) {
            if (data[i] != 0) {
                if (g_out.stray == 0) {
                    g_out.stray = DATA_BASE + i * 8 + 1;
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
