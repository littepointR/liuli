MEMORY {
  FLASH : ORIGIN = 0x08000000, LENGTH = 2048K
  REPORT : ORIGIN = 0x24000000, LENGTH = 64
  RAM : ORIGIN = 0x24000040, LENGTH = 512K - 64
}
_stack_end = 0x20000000;
_stack_start = 0x20020000;
SECTIONS { .report (NOLOAD) : { KEEP(*(.report)) } > REPORT } INSERT AFTER .uninit;
ASSERT(SIZEOF(.report) == 64, "J-Link report ABI");
