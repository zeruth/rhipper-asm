# The Dreamcast fixture, built in WSL with KallistiOS:
#   wsl -d Ubuntu-24.04 -- bash -lc "cd /mnt/c/Users/tyler/Documents/code/rhipper-asm && source $KOS_BASE/environ.sh && make -C testdata/dc"
# The boot program is committed as 1ST_READ.BIN and its scrambled form, the one a disc of the
# CD-ROM kind carries, as 1ST_READ.scrambled; hello.elf and hello.o are kept beside them for
# their symbols, which name the boot program's functions.
TARGET = hello.elf
OBJS = hello.o
KOS_CFLAGS += -O1

all: 1ST_READ.BIN 1ST_READ.scrambled

include $(KOS_BASE)/Makefile.rules

$(TARGET): $(OBJS)
	kos-cc -o $(TARGET) $(OBJS)

1ST_READ.BIN: $(TARGET)
	$(KOS_OBJCOPY) -R .stack -O binary $(TARGET) 1ST_READ.BIN

1ST_READ.scrambled: 1ST_READ.BIN
	$(KOS_BASE)/utils/scramble/scramble 1ST_READ.BIN 1ST_READ.scrambled

clean:
	rm -f $(TARGET) $(OBJS) 1ST_READ.BIN 1ST_READ.scrambled
