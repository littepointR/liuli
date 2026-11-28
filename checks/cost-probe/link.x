INCLUDE memory.x

ENTRY(Reset)

_stack_start = ORIGIN(RAM) + LENGTH(RAM);

SECTIONS
{
  .vector_table ORIGIN(FLASH) :
  {
    LONG(_stack_start);
    KEEP(*(.vector_table.reset_vector));
    . = ALIGN(4);
  } > FLASH

  .text : ALIGN(4)
  {
    *(.text .text.*);
    . = ALIGN(4);
  } > FLASH

  .rodata : ALIGN(4)
  {
    *(.rodata .rodata.*);
    . = ALIGN(4);
  } > FLASH

  .ARM.exidx : ALIGN(4)
  {
    *(.ARM.exidx .ARM.exidx.*);
    . = ALIGN(4);
  } > FLASH

  _sidata = LOADADDR(.data);

  .data : ALIGN(4)
  {
    _sdata = .;
    *(.data .data.*);
    . = ALIGN(4);
    _edata = .;
  } > RAM AT > FLASH

  .bss (NOLOAD) : ALIGN(4)
  {
    _sbss = .;
    *(.bss .bss.*);
    *(COMMON);
    . = ALIGN(4);
    _ebss = .;
  } > RAM

  /DISCARD/ :
  {
    *(.ARM.extab .ARM.extab.*);
    *(.note .note.*);
  }
}

_ram_used_end = _ebss;

/* Measurement script records initialized RAM even above the reference demo limit. */
ASSERT((_ram_used_end - ORIGIN(RAM)) <= 131072, "liuli static RAM exceeds 128 KiB");
ASSERT((_stack_start - _ram_used_end) >= 8192, "liuli stack reserve below 8 KiB");
ASSERT((_stack_start % 8) == 0, "liuli stack top must be 8-byte aligned");
