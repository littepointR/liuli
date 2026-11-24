ENTRY(fk_write)
MEMORY { CODE (rwx) : ORIGIN = 0x24000000, LENGTH = 64K }
SECTIONS {
  .vectors : { KEEP(*(.vectors)) } > CODE
  .text : { *(.text .text.*) *(.rodata .rodata.*) } > CODE
  .data : { *(.data .data.*) } > CODE
  .bss (NOLOAD) : ALIGN(4) { begin_bss = .; *(.bss .bss.* COMMON) . = ALIGN(4); end_bss = .; } > CODE
  /DISCARD/ : { *(.ARM.exidx*) *(.ARM.extab*) *(.comment) }
}
