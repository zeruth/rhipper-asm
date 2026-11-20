# The Nintendo 64 fixtures, built in WSL with libdragon:
#   wsl -d Ubuntu-24.04 -- bash -lc "cd /mnt/c/Users/tyler/Documents/code/rhipper-asm && N64_INST=/opt/libdragon make -C testdata/n64"
# Each ROM is committed as <name>.z64 and its linked program as <name>.elf, its symbols kept and
# its debug sections dropped; build/ is the build's and is not.
BUILD_DIR = build
# The program goes into the ROM as a plain ELF, which the loader maps by its segments.
N64_ROM_ELFCOMPRESS = 0
PROGRAMS = hello draw

all: $(PROGRAMS:%=%.z64) $(PROGRAMS:%=%.elf)

include $(N64_INST)/include/n64.mk

hello.z64: N64_ROM_TITLE = "rhipper hello"
draw.z64: N64_ROM_TITLE = "rhipper draw"
hello.z64: $(BUILD_DIR)/hello.elf
$(BUILD_DIR)/hello.elf: $(BUILD_DIR)/hello.o
draw.z64: $(BUILD_DIR)/draw.elf
$(BUILD_DIR)/draw.elf: $(BUILD_DIR)/draw.o

$(PROGRAMS:%=%.elf): %.elf: $(BUILD_DIR)/%.elf
	$(N64_OBJCOPY) --strip-debug $< $@

clean:
	rm -rf $(BUILD_DIR) $(PROGRAMS:%=%.z64) $(PROGRAMS:%=%.elf)

.PHONY: all clean
