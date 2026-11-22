// The runtime of an MSP430 program written as C from its lift (crates/ra-asm-msp430): what the
// source calls and does not define. The source is the program's code as `msp430_run` over the
// processor's state, and `msp430_main`, which hands this runtime each segment of the program
// and runs the code from its entry.
//
// The machine's memory is sixty-four kilobytes, which the runtime keeps as one block:
// `msp430_memory` says where the machine's address zero is in it, and the source adds that to
// every address it reads or writes. The source reads and writes a word as the host has it, so
// the block holds the machine's memory in the machine's order on a little-endian host.
//
// The state is sixteen slots of eight bytes, a register in the low two: r0 to r15, the status
// register r2 with its flags as the machine's bits.
//
// A microcontroller has no system under it. The program prints by storing a byte to a serial
// port's transmit register (0x060e, where the F5 series has its second port's), and it ends
// by leaving its code in the first output port's register (0x0202) and stopping the processor,
// the bit CPUOFF of the status register set: the source calls `msp430_out` for a store to a
// peripheral's register and `msp430_status` after it writes the status register.

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// Where the machine's address zero is: the source's own eight bytes, which the runtime fills.
extern uint8_t msp430_memory[8];

extern void msp430_main(uint8_t *state);

#define SPACE 0x10000u
#define STACK_TOP 0x4000u
#define SLOTS 16
#define TRANSMIT 0x060eu
#define PORT 0x0202u
#define CPUOFF 0x10u

static uint8_t memory[SPACE];
// What the output port holds.
static uint8_t port;

// A segment of the program: its bytes go where the program has them, the rest of it zero.
void msp430_segment(uint64_t address, uint8_t *bytes, uint64_t in_file, uint64_t size) {
    uint64_t whole = size > in_file ? size : in_file;
    if (address >= SPACE || whole > SPACE - address) {
        fprintf(stderr, "msp430: a segment at %#llx is outside the machine's memory\n", (unsigned long long)address);
        exit(70);
    }
    memcpy(memory + address, bytes, in_file);
}

void msp430_out(uint8_t *state, uint64_t address, uint64_t value) {
    (void)state;
    switch (address) {
    case TRANSMIT:
        fputc((int)(value & 0xff), stdout);
        break;
    case PORT:
        port = (uint8_t)value;
        break;
    default:
        fflush(stdout);
        fprintf(stderr, "msp430: a store to the peripheral register at %#llx, which the board has not\n", (unsigned long long)address);
        exit(70);
    }
}

void msp430_status(uint8_t *state) {
    uint64_t *slot = (uint64_t *)state;
    // The status register is r2: a processor stopped with nothing to wake it has ended.
    if (slot[2] & CPUOFF) {
        fflush(stdout);
        exit(port);
    }
}

void msp430_trap(uint64_t address) {
    fflush(stdout);
    fprintf(stderr, "msp430: no instruction at %#llx\n", (unsigned long long)address);
    exit(70);
}

int main(void) {
    uint64_t where = (uint64_t)(uintptr_t)memory;
    memcpy(msp430_memory, &where, sizeof where);
    uint64_t state[SLOTS] = {0};
    // The stack pointer is r1.
    state[1] = STACK_TOP;
    msp430_main((uint8_t *)state);
    return 0;
}
