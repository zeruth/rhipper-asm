/* The memory of a Nintendo DS cartridge's ARM9 program: the console loads it into main RAM,
   where the cartridge's header says, and enters it at its first word. Code, data and what the
   program clears are all RAM there; the fixture has nothing to clear. */
ENTRY(arm9_start)
SECTIONS
{
    . = 0x02000000;
    .arm9 : {
        KEEP(*(.start))
        *(.text .text.*)
        *(.rodata .rodata.*)
        *(.data .data.*)
    }
    .bss (NOLOAD) : { *(.bss .bss.*) *(COMMON) }
    /DISCARD/ : { *(.ARM.exidx*) *(.ARM.extab*) *(.ARM.attributes) *(.comment) *(.note*) }
}
