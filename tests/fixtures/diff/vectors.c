/* Vector table. */
#include <stdint.h>

extern uint32_t _estack;
void Reset_Handler(void);
void SysTick_Handler(void);
void USART1_IRQHandler(void);

static void Default_Handler(void)
{
    for (;;) {
    }
}

void NMI_Handler(void) __attribute__((weak, alias("Default_Handler")));
void HardFault_Handler(void) __attribute__((weak, alias("Default_Handler")));

__attribute__((section(".isr_vector"), used))
const void *const vector_table[] = {
    &_estack,
    Reset_Handler,
    NMI_Handler,
    HardFault_Handler,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    SysTick_Handler,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0,
    USART1_IRQHandler,
};
