# The PlayStation 2 fixtures, built in WSL with ps2dev:
#   wsl -d Ubuntu-24.04 -- bash -lc "cd /mnt/c/Users/tyler/Documents/code/rhipper-asm && make -C testdata/ps2 hello.elf hello.irx rpc.elf"
# hello.elf and rpc.elf are EE programs, hello.irx the IOP module rpc.elf loads and calls; the three
# are committed, the objects and the IRX's intermediate ELFs under obj/ are the build's and are not.
EE_BIN = hello.elf
EE_OBJS = hello.o

EE_OPTFLAGS = -O1
EE_DBGINFOFLAGS =
# The SDK's libraries carry their DWARF; the symbols stay, the debug sections do not.
EE_LDFLAGS = -Wl,--strip-debug

IOP_BIN = hello.irx
IOP_OBJS = hello_irx.o imports.o exports.o
IOP_DBGINFOFLAGS =

all: $(EE_BIN) $(IOP_BIN) rpc.elf

clean:
	rm -rf $(EE_BIN) $(EE_OBJS) $(IOP_BIN) rpc.elf rpc.o obj

include $(PS2SDK)/samples/Makefile.pref
include $(PS2SDK)/samples/Makefile.eeglobal
include $(PS2SDK)/samples/Makefile.iopglobal

rpc.elf: rpc.o
	$(EE_CC) -T$(EE_LINKFILE) $(EE_OPTFLAGS) -o $@ rpc.o $(EE_LDFLAGS) $(EXTRA_LDFLAGS) $(EE_LIBS)
