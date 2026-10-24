// Reset handler and vector table.
#include <stdint.h>

extern "C" {
extern uint32_t _estack, _sidata, _sdata, _edata, _sbss, _ebss;
int main();
void SysTick_Handler();
}

namespace board {
[[noreturn]] __attribute__((noinline, used)) void default_handler() noexcept
{
    for (;;) {
    }
}
} // namespace board

extern "C" [[noreturn]] void Reset_Handler()
{
    uint32_t *src = &_sidata;
    for (uint32_t *dst = &_sdata; dst < &_edata;)
        *dst++ = *src++;
    for (uint32_t *dst = &_sbss; dst < &_ebss;)
        *dst++ = 0;
    main();
    board::default_handler();
}

extern "C" [[noreturn]] void HardFault_Handler() noexcept __attribute__((weak, alias("_ZN5board15default_handlerEv")));

__attribute__((section(".isr_vector"), used))
const void *const vector_table[] = {
    &_estack,
    reinterpret_cast<const void *>(Reset_Handler),
    nullptr,
    reinterpret_cast<const void *>(HardFault_Handler),
    nullptr, nullptr, nullptr, nullptr, nullptr, nullptr, nullptr,
    nullptr, nullptr, nullptr, nullptr,
    reinterpret_cast<const void *>(SysTick_Handler),
};
