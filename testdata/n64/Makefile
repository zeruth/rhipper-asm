# The Nintendo 64 fixture, built in WSL with libdragon:
#   wsl -d Ubuntu-24.04 -- bash -lc "cd /mnt/c/Users/tyler/Documents/code/rhipper-asm && make -C testdata/n64"
# The ROM is committed as hello.z64 and the linked program as hello.elf, its symbols kept and its
# debug sections dropped; build/ is the build's and is not.
BUILD_DIR = build
# The program goes into the ROM as a plain ELF, which the loader maps by its segments.
N64_ROM_ELFCOMPRESS = 0

all: hello.z64 hello.elf

include $(N64_INST)/include/n64.mk

hello.z64: N64_ROM_TITLE = "rhipper hello"
hello.z64: $(BUILD_DIR)/hello.elf
$(BUILD_DIR)/hello.elf: $(BUILD_DIR)/hello.o

hello.elf: $(BUILD_DIR)/hello.elf
	$(N64_OBJCOPY) --strip-debug $< $@

clean:
	rm -rf $(BUILD_DIR) hello.z64 hello.elf

.PHONY: all clean
