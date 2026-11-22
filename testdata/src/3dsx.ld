/* The image of a program of the 3DS's system as a 3DSX file holds it: the code where the system
   loads a program, then the read-only data and the data, each from a page's start; what the
   program clears is the end of the data. Linked where it is loaded, the program needs none of
   the file's relocations. */
ENTRY(ctr_start)
SECTIONS
{
    . = 0x00100000;
    .text : {
        KEEP(*(.start))
        *(.text .text.*)
    }
    . = ALIGN(0x1000);
    .rodata : { *(.rodata .rodata.*) }
    . = ALIGN(0x1000);
    .data : { *(.data .data.*) }
    .bss (NOLOAD) : { *(.bss .bss.*) *(COMMON) }
    /DISCARD/ : { *(.ARM.exidx*) *(.ARM.extab*) *(.ARM.attributes) *(.comment) *(.note*) }
}
