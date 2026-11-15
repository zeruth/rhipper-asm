# The PSP fixture, built in WSL with pspdev:
#   wsl -d Ubuntu-24.04 -- bash -lc "cd /mnt/c/Users/tyler/Documents/code/rhipper-asm && make -C testdata/psp"
# The PRX is committed as hello.prx; hello.elf and hello.o are the build's and are not.
TARGET = hello
OBJS = hello.o

BUILD_PRX = 1

CFLAGS = -O1 -G0 -Wall
LDFLAGS =
LIBS =

PSPSDK = $(shell psp-config --pspsdk-path)
include $(PSPSDK)/lib/build.mak
