#include "stm32h7xx_hal.h"
#include "stm32h7xx_ll_bus.h"
#include "stm32h7xx_ll_gpio.h"

static LTDC_HandleTypeDef display;
static SDRAM_HandleTypeDef memory;

int fk_st_clocks(void) {
    if (HAL_Init() != HAL_OK || HAL_PWREx_ConfigSupply(PWR_LDO_SUPPLY) != HAL_OK) return 1;
    __HAL_PWR_VOLTAGESCALING_CONFIG(PWR_REGULATOR_VOLTAGE_SCALE1);
    uint32_t start = HAL_GetTick();
    while (!__HAL_PWR_GET_FLAG(PWR_FLAG_VOSRDY)) {
        if (HAL_GetTick() - start > 100) return 2;
    }
    RCC_OscInitTypeDef osc = {
        .OscillatorType = RCC_OSCILLATORTYPE_HSE, .HSEState = RCC_HSE_ON,
        .PLL = {.PLLState = RCC_PLL_ON, .PLLSource = RCC_PLLSOURCE_HSE,
                .PLLM = 5, .PLLN = 160, .PLLP = 2, .PLLQ = 5, .PLLR = 2,
                .PLLRGE = RCC_PLL1VCIRANGE_2, .PLLVCOSEL = RCC_PLL1VCOWIDE}
    };
    if (HAL_RCC_OscConfig(&osc) != HAL_OK) return 3;
    RCC_ClkInitTypeDef clocks = {
        .ClockType = RCC_CLOCKTYPE_SYSCLK | RCC_CLOCKTYPE_HCLK | RCC_CLOCKTYPE_D1PCLK1 |
                     RCC_CLOCKTYPE_PCLK1 | RCC_CLOCKTYPE_PCLK2 | RCC_CLOCKTYPE_D3PCLK1,
        .SYSCLKSource = RCC_SYSCLKSOURCE_PLLCLK, .SYSCLKDivider = RCC_SYSCLK_DIV1,
        .AHBCLKDivider = RCC_HCLK_DIV2, .APB1CLKDivider = RCC_APB1_DIV2,
        .APB2CLKDivider = RCC_APB2_DIV2, .APB3CLKDivider = RCC_APB3_DIV2, .APB4CLKDivider = RCC_APB4_DIV2
    };
    if (HAL_RCC_ClockConfig(&clocks, FLASH_LATENCY_4) != HAL_OK) return 4;
    RCC_PeriphCLKInitTypeDef periph = {
        .PeriphClockSelection = RCC_PERIPHCLK_LTDC | RCC_PERIPHCLK_FMC,
        .FmcClockSelection = RCC_FMCCLKSOURCE_PLL,
        .PLL3 = {.PLL3M = 25, .PLL3N = 330, .PLL3P = 2, .PLL3Q = 2, .PLL3R = 10,
                 .PLL3RGE = RCC_PLL3VCIRANGE_0, .PLL3VCOSEL = RCC_PLL3VCOMEDIUM}
    };
    return HAL_RCCEx_PeriphCLKConfig(&periph) == HAL_OK ? 0 : 5;
}

void fk_core_init(void) {
    CoreDebug->DEMCR |= CoreDebug_DEMCR_TRCENA_Msk;
    DWT->LAR = 0xC5ACCE55;
    DWT->CYCCNT = 0;
    DWT->CTRL |= DWT_CTRL_CYCCNTENA_Msk;
    // Start with the 32 MiB SDRAM aperture Non-Cacheable until `fk_sdram_init()` completes
    // FMC setup and physical write-read verification; `.report` stays Non-Cacheable for SWD polling.
    MPU_Region_InitTypeDef sdram_nc = {
        .Enable = MPU_REGION_ENABLE, .Number = MPU_REGION_NUMBER0, .BaseAddress = 0xc0000000,
        .Size = MPU_REGION_SIZE_32MB, .AccessPermission = MPU_REGION_FULL_ACCESS,
        .DisableExec = MPU_INSTRUCTION_ACCESS_DISABLE, .TypeExtField = MPU_TEX_LEVEL1,
        .IsShareable = MPU_ACCESS_NOT_SHAREABLE, .IsCacheable = MPU_ACCESS_NOT_CACHEABLE,
        .IsBufferable = MPU_ACCESS_NOT_BUFFERABLE
    };
#ifdef LIULI_DIAGNOSTICS
    MPU_Region_InitTypeDef report_sram = {
        .Enable = MPU_REGION_ENABLE, .Number = MPU_REGION_NUMBER2, .BaseAddress = 0x24000000,
        .Size = MPU_REGION_SIZE_64B, .AccessPermission = MPU_REGION_FULL_ACCESS,
        .DisableExec = MPU_INSTRUCTION_ACCESS_DISABLE, .TypeExtField = MPU_TEX_LEVEL1,
        .IsShareable = MPU_ACCESS_NOT_SHAREABLE, .IsCacheable = MPU_ACCESS_NOT_CACHEABLE,
        .IsBufferable = MPU_ACCESS_NOT_BUFFERABLE
    };
#endif
    HAL_MPU_Disable();
    HAL_MPU_ConfigRegion(&sdram_nc);
#ifdef LIULI_DIAGNOSTICS
    HAL_MPU_ConfigRegion(&report_sram);
#endif
    HAL_MPU_Enable(MPU_PRIVILEGED_DEFAULT);
    SCB_EnableICache();
    LL_AHB4_GRP1_EnableClock(LL_AHB4_GRP1_PERIPH_GPIOA | LL_AHB4_GRP1_PERIPH_GPIOD |
        LL_AHB4_GRP1_PERIPH_GPIOE | LL_AHB4_GRP1_PERIPH_GPIOF | LL_AHB4_GRP1_PERIPH_GPIOG |
        LL_AHB4_GRP1_PERIPH_GPIOH | LL_AHB4_GRP1_PERIPH_GPIOI);
    RCC->AHB3ENR |= RCC_AHB3ENR_DMA2DEN;
    __DSB();
}

void fk_dcache_clean(uint32_t addr, uint32_t len) {
    if (len == 0) return;
    uint32_t base = addr & ~31u;
    uint32_t end = (addr + len + 31u) & ~31u;
    SCB_CleanDCache_by_Addr((uint32_t *)base, (int32_t)(end - base));
    __DSB();
}

void fk_dcache_clean_invalidate(uint32_t addr, uint32_t len) {
    if (len == 0) return;
    uint32_t base = addr & ~31u;
    uint32_t end = (addr + len + 31u) & ~31u;
    SCB_CleanInvalidateDCache_by_Addr((uint32_t *)base, (int32_t)(end - base));
    __DSB();
}

void fk_dma2d_wait(void) {
    while (DMA2D->CR & DMA2D_CR_START) {
    }
    DMA2D->IFCR = DMA2D_IFCR_CTEIF | DMA2D_IFCR_CTCIF | DMA2D_IFCR_CCTCIF;
    __DSB();
}

void fk_dma2d_fill(uint32_t dst, uint32_t w, uint32_t h, uint32_t off, uint32_t color) {
    if (w == 0 || h == 0) return;
    fk_dma2d_wait();
    DMA2D->CR = DMA2D_CR_MODE_0 | DMA2D_CR_MODE_1;
    DMA2D->OPFCCR = 2u;
    DMA2D->OCOLR = color;
    DMA2D->OMAR = dst;
    DMA2D->OOR = off;
    DMA2D->NLR = ((w & 0x3FFFU) << DMA2D_NLR_PL_Pos) | (h & 0xFFFFU);
    __DSB();
    DMA2D->CR |= DMA2D_CR_START;
    fk_dma2d_wait();
}

void fk_dma2d_copy_async(uint32_t src, uint32_t dst, uint32_t w, uint32_t h, uint32_t off) {
    if (w == 0 || h == 0) return;
    fk_dma2d_wait();
    DMA2D->CR = 0u;
    DMA2D->FGPFCCR = 2u;
    DMA2D->OPFCCR = 2u;
    DMA2D->FGMAR = src;
    DMA2D->FGOR = off;
    DMA2D->OMAR = dst;
    DMA2D->OOR = off;
    DMA2D->NLR = ((w & 0x3FFFU) << DMA2D_NLR_PL_Pos) | (h & 0xFFFFU);
    __DSB();
    DMA2D->CR |= DMA2D_CR_START;
}

uint32_t fk_cycles(void) { return DWT->CYCCNT; }

static void alternate(GPIO_TypeDef *port, uint32_t pins, uint32_t af) {
    GPIO_InitTypeDef config = {.Pin = pins, .Mode = GPIO_MODE_AF_PP, .Pull = GPIO_NOPULL,
                               .Speed = GPIO_SPEED_FREQ_VERY_HIGH, .Alternate = af};
    HAL_GPIO_Init(port, &config);
}

int fk_sdram_init(void) {
    alternate(GPIOD, GPIO_PIN_0|GPIO_PIN_1|GPIO_PIN_8|GPIO_PIN_9|GPIO_PIN_10|GPIO_PIN_14|GPIO_PIN_15, GPIO_AF12_FMC);
    alternate(GPIOE, GPIO_PIN_0|GPIO_PIN_1|GPIO_PIN_7|GPIO_PIN_8|GPIO_PIN_9|GPIO_PIN_10|GPIO_PIN_11|GPIO_PIN_12|GPIO_PIN_13|GPIO_PIN_14|GPIO_PIN_15, GPIO_AF12_FMC);
    alternate(GPIOF, GPIO_PIN_0|GPIO_PIN_1|GPIO_PIN_2|GPIO_PIN_3|GPIO_PIN_4|GPIO_PIN_5|GPIO_PIN_11|GPIO_PIN_12|GPIO_PIN_13|GPIO_PIN_14|GPIO_PIN_15, GPIO_AF12_FMC);
    alternate(GPIOG, GPIO_PIN_0|GPIO_PIN_1|GPIO_PIN_2|GPIO_PIN_4|GPIO_PIN_5|GPIO_PIN_8|GPIO_PIN_15, GPIO_AF12_FMC);
    alternate(GPIOH, GPIO_PIN_2|GPIO_PIN_3|GPIO_PIN_5, GPIO_AF12_FMC);
    __HAL_RCC_FMC_CLK_ENABLE();
    memory.Instance = FMC_SDRAM_DEVICE;
    memory.Init = (FMC_SDRAM_InitTypeDef){
        .SDBank = FMC_SDRAM_BANK1, .ColumnBitsNumber = FMC_SDRAM_COLUMN_BITS_NUM_9,
        .RowBitsNumber = FMC_SDRAM_ROW_BITS_NUM_13, .MemoryDataWidth = FMC_SDRAM_MEM_BUS_WIDTH_16,
        .InternalBankNumber = FMC_SDRAM_INTERN_BANKS_NUM_4, .CASLatency = FMC_SDRAM_CAS_LATENCY_3,
        .WriteProtection = FMC_SDRAM_WRITE_PROTECTION_DISABLE, .SDClockPeriod = FMC_SDRAM_CLOCK_PERIOD_2,
        .ReadBurst = FMC_SDRAM_RBURST_ENABLE, .ReadPipeDelay = FMC_SDRAM_RPIPE_DELAY_1
    };
    FMC_SDRAM_TimingTypeDef timing = {
        .LoadToActiveDelay = 2, .ExitSelfRefreshDelay = 8, .SelfRefreshTime = 5,
        .RowCycleDelay = 7, .WriteRecoveryTime = 3, .RPDelay = 2, .RCDDelay = 2
    };
    if (HAL_SDRAM_Init(&memory, &timing) != HAL_OK) return 6;
    const uint32_t modes[] = {FMC_SDRAM_CMD_CLK_ENABLE, FMC_SDRAM_CMD_PALL,
                              FMC_SDRAM_CMD_AUTOREFRESH_MODE, FMC_SDRAM_CMD_LOAD_MODE};
    for (unsigned i = 0; i < 4; ++i) {
        FMC_SDRAM_CommandTypeDef cmd = {.CommandMode = modes[i], .CommandTarget = FMC_SDRAM_CMD_TARGET_BANK1,
            .AutoRefreshNumber = i == 2 ? 8 : 1, .ModeRegisterDefinition = i == 3 ? 0x231 : 0};
        if (HAL_SDRAM_SendCommand(&memory, &cmd, 100) != HAL_OK) return 6;
        if (i == 0) HAL_Delay(1);
    }
    if (HAL_SDRAM_ProgramRefreshRate(&memory, 605) != HAL_OK) return 6;
    volatile uint32_t *a = (volatile uint32_t *)0xc0000000, *b = (volatile uint32_t *)0xc1000000;
    *a = 0x97a52648; *b = 0x9c64ba75;
    __DSB();
    if (*a != 0x97a52648 || *b != 0x9c64ba75) return 7;
    __disable_irq();
    HAL_MPU_Disable();
    MPU_Region_InitTypeDef heap_sdram = {
        .Enable = MPU_REGION_ENABLE, .Number = MPU_REGION_NUMBER0, .BaseAddress = 0xc0000000,
        .Size = MPU_REGION_SIZE_32MB, .AccessPermission = MPU_REGION_FULL_ACCESS,
        .DisableExec = MPU_INSTRUCTION_ACCESS_DISABLE, .TypeExtField = MPU_TEX_LEVEL1,
        .IsShareable = MPU_ACCESS_NOT_SHAREABLE, .IsCacheable = MPU_ACCESS_CACHEABLE,
        .IsBufferable = MPU_ACCESS_BUFFERABLE
    };
    HAL_MPU_ConfigRegion(&heap_sdram);
    HAL_MPU_Enable(MPU_PRIVILEGED_DEFAULT);
    __DSB();
    __ISB();
    SCB_EnableDCache();
    __enable_irq();
    return 0;
}

int fk_panel_init(uint32_t front) {
    LL_GPIO_ResetOutputPin(GPIOH, LL_GPIO_PIN_6);
    LL_GPIO_SetPinMode(GPIOH, LL_GPIO_PIN_6, LL_GPIO_MODE_OUTPUT);
    alternate(GPIOA, GPIO_PIN_2, GPIO_AF14_LTDC);
    alternate(GPIOA, GPIO_PIN_8, GPIO_AF13_LTDC);
    alternate(GPIOD, GPIO_PIN_6, GPIO_AF14_LTDC);
    alternate(GPIOE, GPIO_PIN_5|GPIO_PIN_6, GPIO_AF14_LTDC);
    alternate(GPIOF, GPIO_PIN_10, GPIO_AF14_LTDC);
    alternate(GPIOG, GPIO_PIN_6|GPIO_PIN_7|GPIO_PIN_12|GPIO_PIN_13|GPIO_PIN_14, GPIO_AF14_LTDC);
    alternate(GPIOH, GPIO_PIN_8|GPIO_PIN_9|GPIO_PIN_10|GPIO_PIN_11|GPIO_PIN_12|GPIO_PIN_13|GPIO_PIN_14|GPIO_PIN_15, GPIO_AF14_LTDC);
    alternate(GPIOI, GPIO_PIN_0|GPIO_PIN_1|GPIO_PIN_2|GPIO_PIN_4|GPIO_PIN_5|GPIO_PIN_6|GPIO_PIN_7|GPIO_PIN_9|GPIO_PIN_10, GPIO_AF14_LTDC);
    __HAL_RCC_LTDC_FORCE_RESET();
    __HAL_RCC_LTDC_CLK_ENABLE();
    __HAL_RCC_LTDC_RELEASE_RESET();
    // RM0433: INI6 is LTDC. Give scanout reads priority over default-priority
    // CPU and DMA2D transactions competing for the same SDRAM target.
    WRITE_REG(GPV->AXI_INI6_READ_QOS, 1U);
    display.Instance = LTDC;
    display.Init = (LTDC_InitTypeDef){
        .HSPolarity = LTDC_HSPOLARITY_AL, .VSPolarity = LTDC_VSPOLARITY_AL,
        .DEPolarity = LTDC_DEPOLARITY_AL, .PCPolarity = LTDC_PCPOLARITY_IPC,
        .HorizontalSync = 0, .VerticalSync = 0, .AccumulatedHBP = 80, .AccumulatedVBP = 40,
        .AccumulatedActiveW = 880, .AccumulatedActiveH = 520, .TotalWidth = 1080, .TotalHeigh = 542
    };
    LTDC_LayerCfgTypeDef layer = {
        .WindowX1 = 800, .WindowY1 = 480, .PixelFormat = LTDC_PIXEL_FORMAT_RGB565,
        .Alpha = 255, .BlendingFactor1 = LTDC_BLENDING_FACTOR1_CA, .BlendingFactor2 = LTDC_BLENDING_FACTOR2_CA,
        .FBStartAdress = front, .ImageWidth = 800, .ImageHeight = 480
    };
    if (HAL_LTDC_Init(&display) != HAL_OK || HAL_LTDC_ConfigLayer(&display, &layer, 0) != HAL_OK) return 8;
    return 0;
}

int fk_panel_swap(uint32_t addr) {
    __DSB();
    if (HAL_LTDC_SetAddress_NoReload(&display, addr, 0) != HAL_OK) return 8;
    __HAL_LTDC_CLEAR_FLAG(&display, LTDC_FLAG_RR);
    if (HAL_LTDC_Reload(&display, LTDC_RELOAD_VERTICAL_BLANKING) != HAL_OK) return 8;
    uint32_t start = fk_cycles();
    while (!__HAL_LTDC_GET_FLAG(&display, LTDC_FLAG_RR)) {
        if (fk_cycles() - start > 40000000) return 8;
    }
    return 0;
}

void fk_backlight(void) { LL_GPIO_SetOutputPin(GPIOH, LL_GPIO_PIN_6); }
uint32_t fk_display_status(void) { return display.Instance->ISR; }

// PI11 and PI8 have no common hardware I2C peripheral on this board.
static GPIO_TypeDef *const touch_ports[] = {GPIOI, GPIOI, GPIOG, GPIOH};
static const uint32_t touch_pins[] = {GPIO_PIN_11, GPIO_PIN_8, GPIO_PIN_3, GPIO_PIN_4};
void fk_pin_mode(unsigned i, unsigned output, unsigned od) {
    GPIO_InitTypeDef pin = {.Pin=touch_pins[i], .Mode=output ? (od ? GPIO_MODE_OUTPUT_OD : GPIO_MODE_OUTPUT_PP) : GPIO_MODE_INPUT,
                            .Pull=od ? GPIO_PULLUP : GPIO_NOPULL, .Speed=GPIO_SPEED_FREQ_VERY_HIGH};
    HAL_GPIO_Init(touch_ports[i], &pin);
}
void fk_pin_write(unsigned i, unsigned high) {
    if (high) LL_GPIO_SetOutputPin(touch_ports[i],touch_pins[i]);
    else LL_GPIO_ResetOutputPin(touch_ports[i],touch_pins[i]);
}
unsigned fk_pin_read(unsigned i) { return LL_GPIO_IsInputPinSet(touch_ports[i],touch_pins[i]); }

#ifdef LIULI_TOUCH_IRQ
void fk_touch_irq_init(void) {
    __HAL_RCC_SYSCFG_CLK_ENABLE();
    GPIO_InitTypeDef gpio = {.Pin=GPIO_PIN_3, .Mode=GPIO_MODE_IT_RISING_FALLING, .Pull=GPIO_NOPULL};
    HAL_GPIO_Init(GPIOG, &gpio);
    __HAL_GPIO_EXTI_CLEAR_IT(GPIO_PIN_3);
    HAL_NVIC_SetPriority(EXTI3_IRQn, 6, 0);
    HAL_NVIC_EnableIRQ(EXTI3_IRQn);
}
void fk_touch_irq_ack(void) { __HAL_GPIO_EXTI_CLEAR_IT(GPIO_PIN_3); }
#endif
