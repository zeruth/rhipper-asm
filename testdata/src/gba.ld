/* The memory of a Game Boy Advance cartridge's program: the ROM where the console maps the
   cartridge, its header first, the code and the read-only data after it; what the program
   keeps in RAM in the console's fast RAM. The fixture has nothing there to copy or to clear. */
ENTRY(rom_start)
SECTIONS
{
    . = 0x08000000;
    .rom : {
        KEEP(*(.header))
        *(.text .text.*)
        *(.rodata .rodata.*)
        *(.data .data.*)
    }
    . = 0x03000000;
    .bss (NOLOAD) : { *(.bss .bss.*) *(COMMON) }
    /DISCARD/ : { *(.ARM.exidx*) *(.ARM.extab*) *(.ARM.attributes) *(.comment) *(.note*) }
}
