ENTRY(Reset)
MEMORY {
  FLASH (rx) : ORIGIN = 0x08000000, LENGTH = 2048K
  AXI (rw) : ORIGIN = 0x24000000, LENGTH = 512K
  DTCM (rw) : ORIGIN = 0x20000000, LENGTH = 128K
}
SECTIONS {
  .vectors ORIGIN(FLASH) : { KEEP(*(.vectors)) } > FLASH
  .text : { *(.text .text.*) *(.rodata .rodata.*) . = ALIGN(4); } > FLASH
  .ARM.exidx : { *(.ARM.exidx*) } > FLASH
  .report ORIGIN(AXI) (NOLOAD) : { KEEP(*(.report)) } > AXI
  .data : ALIGN(4) { _sdata = .; *(.data .data.*) . = ALIGN(4); _edata = .; } > AXI AT > FLASH
  _sidata = LOADADDR(.data);
  .bss (NOLOAD) : ALIGN(8) { _sbss = .; *(.bss .bss.* COMMON) . = ALIGN(8); _ebss = .; } > AXI
  _stack = ORIGIN(DTCM) + LENGTH(DTCM);
  /DISCARD/ : { *(.ARM.extab*) *(.comment) }
}
ASSERT(SIZEOF(.report) == 64, "J-Link report ABI")
