#include "stm32h7xx_hal.h"

enum { SECTOR_BYTES = 128 * 1024, BANK_BYTES = 1024 * 1024, IMAGE_BYTES = 2 * BANK_BYTES };
enum { PROGRAM_SECTOR = 1, VERIFY_IMAGE = 2, DONE = 2, WRITE_FAILED = 3 };
struct Request {
    uint32_t magic, operation, offset, bytes, expected_crc;
    uint32_t state, actual_crc, error;
};
static volatile struct Request *const request = (void *)0x2407f000;
static const uint8_t *const payload = (void *)0x24010000;
extern uint32_t begin_bss[], end_bss[];

static uint32_t crc32(const uint8_t *data, uint32_t length) {
    uint32_t crc = UINT32_MAX;
    while (length--) {
        crc ^= *data++;
        for (unsigned k = 0; k < 8; k++) crc = (crc >> 1) ^ (0xedb88320u & -(crc & 1u));
    }
    return ~crc;
}

// Each request starts after a system reset with the 64 MHz HSI clock.
uint32_t HAL_GetTick(void) { return DWT->CYCCNT / 64000; }

static uint32_t program(uint32_t offset, uint32_t bytes) {
    if ((offset % SECTOR_BYTES) || bytes > SECTOR_BYTES) return 2;
    if (crc32(payload, bytes) != request->expected_crc) return 3;
    if (HAL_FLASH_Unlock() != HAL_OK) return 4;
    uint32_t bank = offset < BANK_BYTES ? FLASH_BANK_1 : FLASH_BANK_2;
    if (bank == FLASH_BANK_1) __HAL_FLASH_CLEAR_FLAG_BANK1(FLASH_FLAG_ALL_ERRORS_BANK1);
    else __HAL_FLASH_CLEAR_FLAG_BANK2(FLASH_FLAG_ALL_ERRORS_BANK2);
    uint32_t bad_sector;
    FLASH_EraseInitTypeDef erase = {
        .TypeErase = FLASH_TYPEERASE_SECTORS, .Banks = bank, .Sector = (offset % BANK_BYTES) / SECTOR_BYTES,
        .NbSectors = 1, .VoltageRange = FLASH_VOLTAGE_RANGE_3
    };
    HAL_StatusTypeDef result = HAL_FLASHEx_Erase(&erase, &bad_sector);
    for (uint32_t i = 0; result == HAL_OK && i < bytes; i += 32) {
        // The host supplies an erased-value padded final flash word.
        result = HAL_FLASH_Program(FLASH_TYPEPROGRAM_FLASHWORD, FLASH_BASE + offset + i, (uint32_t)(payload + i));
    }
    uint32_t error = result == HAL_OK ? 0 : (0x100 | HAL_FLASH_GetError());
    if (HAL_FLASH_Lock() != HAL_OK && !error) error = 5;
    __DSB();
    return error;
}

__attribute__((noreturn)) static void stop(uint32_t error) {
    request->error = error;
    __DSB();
    request->state = error ? WRITE_FAILED : DONE;
    __DSB();
    // The probe reads the result over the AXI bus before issuing the next reset.
    // Keep the dedicated RAM writer awake until the host has acknowledged it.
    for (;;) __NOP();
}

void fk_write(void) {
    __disable_irq();
    SCB->CPACR |= 0xfu << 20;
    __DSB(); __ISB();
    for (uint32_t *p = begin_bss; p < end_bss; ++p) *p = 0;
    CoreDebug->DEMCR |= CoreDebug_DEMCR_TRCENA_Msk;
    DWT->LAR = 0xc5acce55;
    DWT->CYCCNT = 0;
    DWT->CTRL |= DWT_CTRL_CYCCNTENA_Msk;
    uint32_t bytes = request->bytes, offset = request->offset;
    if (request->magic != 0x464b3734 || request->state || !bytes || bytes > IMAGE_BYTES ||
        offset > IMAGE_BYTES - bytes || (SCB->CCR & SCB_CCR_DC_Msk)) stop(1);
    request->state = 1;
    if (request->operation == PROGRAM_SECTOR) {
        uint32_t error = program(offset, bytes);
        if (error) stop(error);
    } else if (request->operation != VERIFY_IMAGE) stop(6);
    request->actual_crc = crc32((const uint8_t *)(FLASH_BASE + offset), bytes);
    stop(request->actual_crc != request->expected_crc ? 7 : 0);
}
static void fault(void) { stop(0x80000000 | SCB->CFSR); }
__attribute__((section(".vectors"), used))
static const uintptr_t vectors[16] = {
    [0] = 0x20020000, [1] = (uintptr_t)fk_write, [2] = (uintptr_t)fault, [3] = (uintptr_t)fault,
    [4] = (uintptr_t)fault, [5] = (uintptr_t)fault, [6] = (uintptr_t)fault
};
