# STM32H743 driver sources

`stm32h743-drivers.zip` contains the STM32H7xx HAL/LL 1.11.1 and CMSIS-Device files distributed in Keil.STM32H7xx_DFP 3.1.1, plus the required Arm CMSIS-Core headers from ARM.CMSIS 5.9.0. Source files retain their upstream notices. The `licenses/` directory inside the archive contains the STM32CubeH7 package terms and the CMSIS licenses. STM32Cube identifies HAL/LL as BSD-3-Clause. The board configuration is adapted from the HAL configuration template.

Both H743 runtimes build the same C peripheral drivers. The build extracts the pinned subset into Cargo’s output directory. Clang supplies the freestanding C environment and the Cortex-M7 hard-float ABI. The local math header provides the two C floating-point evaluation types used by RCC. [STM32H7xx HAL driver](https://github.com/STMicroelectronics/stm32h7xx-hal-driver) · [Arm CMSIS 5](https://github.com/ARM-software/CMSIS_5)
