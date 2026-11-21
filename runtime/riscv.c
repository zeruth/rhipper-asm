// The runtime of a RISC-V program written as C from its lift (crates/ra-asm-riscv): what the
// source calls and does not define. The source is the program's code as `rv_run` over the
// machine's registers, and `rv_main`, which hands this runtime each segment of the program and
// runs the code from its entry.
//
// The program's addresses are its own, and a host cannot give it them: the runtime keeps the
// program's memory as one block, `rv_memory` says where the program's address zero is in it, and
// the source adds that to every address it reads or writes. The block is the low 2 GiB of the
// program's address space, which is where a static program and its stack are.
//
// The system under the program is Linux's, as far as a program without a C library needs it to
// print and to end: `write` and `exit`.

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Where the program's address zero is: the source's own eight bytes, which the runtime fills.
extern uint8_t rv_memory[8];

extern void rv_main(uint8_t *state);

#define SPACE ((uint64_t)1 << 31)
#define STACK_TOP ((uint64_t)0x7ff00000 + 0x100000)

static uint8_t *memory;

static uint8_t *at(uint64_t address, uint64_t size) {
    if (address >= SPACE || size > SPACE - address) {
        fprintf(stderr, "riscv: the program's address %#llx is outside the memory the runtime gives it\n", (unsigned long long)address);
        exit(70);
    }
    return memory + address;
}

// A segment of the program: its bytes go where the program has them, the rest of it zero.
void rv_segment(uint64_t address, uint8_t *bytes, uint64_t in_file, uint64_t size) {
    memcpy(at(address, size > in_file ? size : in_file), bytes, in_file);
}

// An `ecall`: the call's number in a7, its arguments from a0, its result in a0.
void rv_ecall(uint8_t *state) {
    uint64_t *x = (uint64_t *)state;
    switch (x[17]) {
    case 64: // write
        if (x[10] == 1 || x[10] == 2) {
            fwrite(at(x[11], x[12]), 1, x[12], x[10] == 2 ? stderr : stdout);
            x[10] = x[12];
        } else {
            x[10] = (uint64_t)-9; // -EBADF
        }
        break;
    case 93: // exit
    case 94: // exit_group
        fflush(stdout);
        exit((int)x[10]);
    default:
        fprintf(stderr, "riscv: the system call %llu is not answered\n", (unsigned long long)x[17]);
        exit(70);
    }
}

void rv_trap(uint64_t address) {
    fflush(stdout);
    fprintf(stderr, "riscv: no instruction at %#llx\n", (unsigned long long)address);
    exit(70);
}

int main(void) {
    // The block is asked for zeroed and never touched whole: the system gives its pages as the
    // program uses them.
    memory = calloc(1, SPACE);
    if (!memory) {
        fputs("riscv: no memory for the program\n", stderr);
        return 70;
    }
    uint64_t where = (uint64_t)(uintptr_t)memory;
    memcpy(rv_memory, &where, sizeof where);
    uint64_t state[32] = {0};
    state[2] = STACK_TOP - 64;
    rv_main((uint8_t *)state);
    return 0;
}
