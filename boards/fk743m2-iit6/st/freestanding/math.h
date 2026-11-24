// STM32Cube RCC needs the C floating-point evaluation types on this hard-float target.
#pragma once
#if __FLT_EVAL_METHOD__ != 0
#error "The H743 BSP expects native float and double evaluation"
#endif
typedef float float_t;
typedef double double_t;
