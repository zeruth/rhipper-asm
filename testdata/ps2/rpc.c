// The EE caller of the IOP module fixture: loads `hello.irx`, binds its RPC server and calls two of
// its functions through `SifCallRpc` by their numbers, then prints the sum on the EE's serial port.
// Built by the Makefile beside it.
#include <kernel.h>
#include <loadfile.h>
#include <sifrpc.h>
#include <sio.h>

#define HELLO_RPC 0x48454c4f

static SifRpcClientData_t client __attribute__((aligned(64)));
static int buffer[16] __attribute__((aligned(64)));

int main(void) {
    SifInitRpc(0);
    if (SifLoadModule("host:hello.irx", 0, NULL) < 0) {
        sio_puts("hello.irx did not load");
        return 1;
    }
    while (SifBindRpc(&client, HELLO_RPC, 0) >= 0 && client.server == NULL) {
        nopdelay();
    }
    buffer[0] = 10;
    SifCallRpc(&client, 0, 0, buffer, 4, buffer, 4, NULL, NULL);
    SifCallRpc(&client, 1, 0, buffer, 0, NULL, 0, NULL, NULL);
    sio_puts(buffer[0] == 55 ? "sum 1..10 = 55" : "the sum is wrong");
    return 0;
}
