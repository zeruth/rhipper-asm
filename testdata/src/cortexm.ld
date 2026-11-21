/* The memory of the Cortex-M fixture: flash at 0x08000000, where the image is and the vector
   table starts it, and RAM at 0x20000000, where the data is copied to and the stack ends. */
MEMORY {
    FLASH (rx) : ORIGIN = 0x08000000, LENGTH = 64K
    RAM (rw) : ORIGIN = 0x20000000, LENGTH = 16K
}

ENTRY(reset)

SECTIONS {
    .text : {
        KEEP(*(.vectors))
        *(.text*)
        *(.rodata*)
        . = ALIGN(4);
    } > FLASH
    __data_load = LOADADDR(.data);
    .data : {
        __data_start = .;
        *(.data*)
        . = ALIGN(4);
        __data_end = .;
    } > RAM AT > FLASH
    .bss : {
        __bss_start = .;
        *(.bss*)
        *(COMMON)
        . = ALIGN(4);
        __bss_end = .;
    } > RAM
    __stack_top = ORIGIN(RAM) + LENGTH(RAM);
    /DISCARD/ : { *(.ARM.exidx*) *(.comment) *(.ARM.attributes) }
}
