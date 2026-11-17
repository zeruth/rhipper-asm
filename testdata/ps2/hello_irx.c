// The PlayStation 2 IOP module fixture: an IRX of ps2sdk that registers one SIF RPC server, whose
// dispatcher calls the function the EE's `SifCallRpc` names by its number out of a table, and
// exports a library of its own. `rpc.c` beside it is the EE program that calls it. Built by the
// Makefile beside it, with `imports.lst` and `exports.tab`.
#include "irx_imports.h"

#define HELLO_RPC 0x48454c4f

IRX_ID("hello", 1, 1);

extern struct irx_export_table _exp_hello;

static SifRpcDataQueue_t queue;
static SifRpcServerData_t server;
static int buffer[16];

// RPC 0: the sum of 1 to the first word of the buffer, written over it.
static void *hello_sum(void *data, int size) {
    int *words = data;
    int total = 0;
    for (int i = 1; i <= words[0]; i++) {
        total += i;
    }
    words[0] = total;
    return data;
}

// RPC 1: a line on the IOP's console.
static void *hello_greet(void *data, int size) {
    printf("hello from the iop\n");
    return data;
}

// RPC 2: the first word of the buffer times the second.
static void *hello_scale(void *data, int size) {
    int *words = data;
    words[0] *= words[1];
    return data;
}

// The functions the RPC numbers name, in their order.
static void *(*const functions[])(void *, int) = {hello_sum, hello_greet, hello_scale};

static void *dispatch(int fno, void *data, int size) {
    if ((unsigned)fno >= sizeof functions / sizeof *functions) {
        return NULL;
    }
    return functions[fno](data, size);
}

// The queue comes in as the thread's argument: iopfixup refuses two low halves of one `lui`, which
// the compiler makes of an address it uses more than once.
static void serve(void *arg) {
    SifRpcDataQueue_t *rpcs = arg;
    sceSifInitRpc(0);
    sceSifSetRpcQueue(rpcs, GetThreadId());
    sceSifRegisterRpc(&server, HELLO_RPC, dispatch, buffer, NULL, NULL, rpcs);
    sceSifRpcLoop(rpcs);
}

// Export 4 of the module's library: its version, for another module to ask.
int hello_version(void) {
    return 0x101;
}

int _start(int argc, char *argv[]) {
    iop_thread_t thread = {.attr = TH_C, .thread = serve, .priority = 40, .stacksize = 0x800};
    if (RegisterLibraryEntries(&_exp_hello) != 0) {
        return MODULE_NO_RESIDENT_END;
    }
    int id = CreateThread(&thread);
    if (id <= 0) {
        return MODULE_NO_RESIDENT_END;
    }
    StartThread(id, &queue);
    return MODULE_RESIDENT_END;
}
