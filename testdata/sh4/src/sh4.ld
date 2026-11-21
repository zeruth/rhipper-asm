/* The layout of the SH-4 fixture: one segment of code and read-only data that starts with the
   file's own headers, so the file maps page for page, and one of data after it. The toolchain's
   own script is a board's, with a stack section at a fixed address no system would map. */
ENTRY(__start)

SECTIONS {
    . = 0x400000 + SIZEOF_HEADERS;
    .text : { *(.text*) }
    .rodata : { *(.rodata*) }
    . = ALIGN(0x1000);
    .data : { *(.data*) }
    .bss : { *(.bss*) *(COMMON) }
    /DISCARD/ : { *(.stack) *(.comment) }
}
