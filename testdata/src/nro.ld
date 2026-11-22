/* The image of a program of the Switch's system as an NRO file holds it: three segments, each
   on a page of its own, the code first with the file's header in its first bytes, then the
   read-only data, then the data; what the program clears follows them. The system loads the
   image where it pleases, so the program is linked from zero and names nothing by address. */
ENTRY(nro_start)
SECTIONS
{
    . = 0;
    .text : {
        KEEP(*(.nro))
        *(.text .text.*)
    }
    . = ALIGN(0x1000);
    .rodata : { *(.rodata .rodata.*) }
    . = ALIGN(0x1000);
    .data : { *(.data .data.*) }
    .bss (NOLOAD) : { *(.bss .bss.*) *(COMMON) }
    /DISCARD/ : { *(.comment) *(.note*) *(.eh_frame*) *(.dynamic) *(.dynsym) *(.dynstr) *(.hash) *(.gnu.hash) *(.interp) }
}
