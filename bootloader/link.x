ENTRY(start)
SECTIONS {
  . = 0x10000;                /* where your loader copies it */

  /DISCARD/ : {
    *(.eh_frame*) *(.gcc_except_table*)
    *(.note*) *(.comment)
    *(.interp) *(.dynamic) *(.dynsym) *(.dynstr)
    *(.got.plt) *(.got) *(.plt) *(.rel*) *(.rela*)
  }

  .text : ALIGN(16) {
    KEEP(*(.text.start))      /* start is literally first bytes */
    *(.text .text.*)
  }

  .rodata : ALIGN(16) { *(.rodata .rodata.*) }  /* include constants! */
  .data   : ALIGN(16) { *(.data .data.*) }
  .bss (NOLOAD) : ALIGN(16) { *(.bss .bss.*) *(COMMON) }
}
