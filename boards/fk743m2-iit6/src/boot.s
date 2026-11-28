.syntax unified
.thumb
.section .vectors,"a",%progbits
.word _stack, Reset
.rept 13
.word Fault
.endr
.word HAL_IncTick
.rept 9
.word Fault
.endr
.if {touch_irq}
.word EXTI3
.else
.word Fault
.endif
.rept 141
.word Fault
.endr

.section .text.Reset,"ax",%progbits
.global Reset
.thumb_func
Reset:
    cpsid i
    bl SystemInit
    ldr r0, =_sdata
    ldr r1, =_edata
    ldr r2, =_sidata
1:  cmp r0, r1
    bhs 2f
    ldr r3, [r2], #4
    str r3, [r0], #4
    b 1b
2:  ldr r0, =_sbss
    ldr r1, =_ebss
    movs r2, #0
3:  cmp r0, r1
    bhs 4f
    str r2, [r0], #4
    b 3b
4:  cpsie i
    bl firmware_main
.thumb_func
Fault:
.if {diagnostics}
    ldr r0, =0x24000000
    ldr r1, =0xBAD00001
    str r1, [r0, #4]
    ldr r2, =0xE000ED28
    ldr r3, [r2]
    str r3, [r0, #60]
.endif
5:  b 5b
