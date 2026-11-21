// The runtime of a 32-bit PowerPC program written as C from its lift (crates/ra-asm-ppc): what
// the source calls and does not define. The source is the program's code as `ppc_run` over the
// processor's state, and `ppc_main`, which hands this runtime each segment of the program and
// runs the code from its entry.
//
// The program's addresses are its own, and a host cannot give it them: the runtime keeps the
// program's memory as one block, `ppc_memory` says where the program's address zero is in it,
// and the source adds that to every address it reads or writes. The block is the low 2 GiB of
// the program's address space, which is where a static program and its stack are. The source
// turns the bytes of what it loads and stores itself, so the block holds the program's memory in
// the program's own byte order whatever the host's.
//
// The state is slots of eight bytes, a register in the low four: r0 to r31, lr, ctr, the
// condition register and xer. The system under the program is Linux's, as far as a program
// without a C library needs it to print and to end: `write` and `exit`, by `sc`, the call's
// number in r0, its arguments from r3 and its result in r3.

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Where the program's address zero is: the source's own eight bytes, which the runtime fills.
extern uint8_t ppc_memory[8];

extern void ppc_main(uint8_t *state);

#define SPACE ((uint64_t)1 << 31)
#define STACK_TOP ((uint64_t)0x7ff00000 + 0x100000)
#define SLOTS 36

static uint8_t *memory;

static uint8_t *at(uint64_t address, uint64_t size) {
    if (address >= SPACE || size > SPACE - address) {
        fprintf(stderr, "ppc: the program's address %#llx is outside the memory the runtime gives it\n", (unsigned long long)address);
        exit(70);
    }
    return memory + address;
}

// A segment of the program: its bytes go where the program has them, the rest of it zero.
void ppc_segment(uint64_t address, uint8_t *bytes, uint64_t in_file, uint64_t size) {
    memcpy(at(address, size > in_file ? size : in_file), bytes, in_file);
}

void ppc_sc(uint8_t *state) {
    uint64_t *slot = (uint64_t *)state;
    uint32_t call = (uint32_t)slot[0], a = (uint32_t)slot[3], b = (uint32_t)slot[4], c = (uint32_t)slot[5];
    uint32_t *result = (uint32_t *)&slot[3];
    switch (call) {
    case 4: // write
        if (a == 1 || a == 2) {
            fwrite(at(b, c), 1, c, a == 2 ? stderr : stdout);
            *result = c;
        } else {
            *result = 9; // EBADF, which the kernel says by the summary bit; no program here looks
        }
        break;
    case 1:   // exit
    case 234: // exit_group
        fflush(stdout);
        exit((int)a);
    default:
        fprintf(stderr, "ppc: the system call %u is not answered\n", call);
        exit(70);
    }
}

void ppc_trap(uint64_t address) {
    fflush(stdout);
    fprintf(stderr, "ppc: no instruction at %#llx\n", (unsigned long long)address);
    exit(70);
}

int main(void) {
    // The block is asked for zeroed and never touched whole: the system gives its pages as the
    // program uses them.
    memory = calloc(1, SPACE);
    if (!memory) {
        fputs("ppc: no memory for the program\n", stderr);
        return 70;
    }
    uint64_t where = (uint64_t)(uintptr_t)memory;
    memcpy(ppc_memory, &where, sizeof where);
    uint64_t state[SLOTS] = {0};
    // The stack pointer is r1.
    state[1] = STACK_TOP - 256;
    ppc_main((uint8_t *)state);
    return 0;
}
