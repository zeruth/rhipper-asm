# The PlayStation 2 fixture, built in WSL with ps2dev:
#   wsl -d Ubuntu-24.04 -- bash -lc "cd /mnt/c/Users/tyler/Documents/code/rhipper-asm && make -C testdata/ps2"
# The ELF is committed as hello.elf; hello.o is the build's and is not.
EE_BIN = hello.elf
EE_OBJS = hello.o

EE_OPTFLAGS = -O1
EE_DBGINFOFLAGS =
# The SDK's libraries carry their DWARF; the symbols stay, the debug sections do not.
EE_LDFLAGS = -Wl,--strip-debug

all: $(EE_BIN)

clean:
	rm -f $(EE_BIN) $(EE_OBJS)

include $(PS2SDK)/samples/Makefile.pref
include $(PS2SDK)/samples/Makefile.eeglobal
