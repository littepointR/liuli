/* Eight aligned RGB565 pixels per block. Q0 holds the input throughout.
 * For each channel compute (bg * inv_alpha + fg * alpha + 7) / 15.
 * The largest numerator is 952; multiplying by 4370 then shifting 16
 * is exact division by 15 throughout this range.
 */
    .section .iram1,"ax"
    .align 4
    .global liuli_blend_blocks
    .type liuli_blend_blocks,@function
liuli_blend_blocks:
    entry a1, 32
    ee.vld.128.ip q4, a4, 16
    ee.vld.128.ip q7, a4, 16
    ee.vld.128.ip q1, a4, 16
    ee.vld.128.ip q2, a4, 16
    addi a5,a4,16
    addi a6,a4,32
.Lblock:
    ee.vld.128.ip q0,a2,0

    ssai 11
    ee.vsr.32 q3,q0
    ee.andq q3,q3,q4
    ssai 0
    ee.vmul.u16 q3,q3,q1
    ee.vld.128.ip q6,a4,0
    ee.vadds.s16 q3,q3,q6
    ssai 16
    ee.vmul.u16 q3,q3,q2
    ssai 11
    ee.vsl.32 q3,q3
    mv.qr q5,q3

    ssai 5
    ee.vsr.32 q3,q0
    ee.andq q3,q3,q7
    ssai 0
    ee.vmul.u16 q3,q3,q1
    ee.vld.128.ip q6,a5,0
    ee.vadds.s16 q3,q3,q6
    ssai 16
    ee.vmul.u16 q3,q3,q2
    ssai 5
    ee.vsl.32 q3,q3
    ee.orq q5,q5,q3

    ssai 0
    ee.vsr.32 q3,q0
    ee.andq q3,q3,q4
    ssai 0
    ee.vmul.u16 q3,q3,q1
    ee.vld.128.ip q6,a6,0
    ee.vadds.s16 q3,q3,q6
    ssai 16
    ee.vmul.u16 q3,q3,q2
    ssai 0
    ee.vsl.32 q3,q3
    ee.orq q5,q5,q3
    ee.vst.128.ip q5,a2,16
    addi a3,a3,-1
    bnez a3,.Lblock
    retw
