/* Test linker script for a small Cortex-M4 part. */
ENTRY(Reset_Handler)

MEMORY
{
    FLASH (rx)  : ORIGIN = 0x08000000, LENGTH = 64K
    RAM   (rwx) : ORIGIN = 0x20000000, LENGTH = 16K
}

_estack = ORIGIN(RAM) + LENGTH(RAM);
_stack_size = 0x400;

SECTIONS
{
    .isr_vector : {
        KEEP(*(.isr_vector))
    } > FLASH

    .text : {
        *(.text .text.*)
        *(.glue_7) *(.glue_7t)
    } > FLASH

    .rodata : {
        . = ALIGN(4);
        *(.rodata .rodata.*)
        . = ALIGN(4);
    } > FLASH

    .ARM.exidx : {
        *(.ARM.exidx* .gnu.linkonce.armexidx.*)
    } > FLASH

    .data : {
        . = ALIGN(4);
        _sdata = .;
        *(.data .data.*)
        . = ALIGN(4);
        _edata = .;
    } > RAM AT > FLASH
    _sidata = LOADADDR(.data);

    .bss (NOLOAD) : {
        . = ALIGN(4);
        _sbss = .;
        *(.bss .bss.* COMMON)
        . = ALIGN(4);
        _ebss = .;
    } > RAM

    .stack (NOLOAD) : {
        . = ALIGN(8);
        . = . + _stack_size;
    } > RAM
}
