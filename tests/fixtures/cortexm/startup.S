/* Reset handler in assembly: built without -g, so it has no DWARF. */
    .syntax unified
    .cpu cortex-m4
    .thumb

    .section .text.Reset_Handler,"ax",%progbits
    .global Reset_Handler
    .type Reset_Handler, %function
Reset_Handler:
    ldr r0, =_sidata
    ldr r1, =_sdata
    ldr r2, =_edata
1:  cmp r1, r2
    bhs 2f
    ldr r3, [r0], #4
    str r3, [r1], #4
    b 1b
2:  ldr r1, =_sbss
    ldr r2, =_ebss
    movs r3, #0
3:  cmp r1, r2
    bhs 4f
    str r3, [r1], #4
    b 3b
4:  bl main
    b .
    .size Reset_Handler, .-Reset_Handler
