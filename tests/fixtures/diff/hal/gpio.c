#include "hal.h"

#define GPIOA_ODR (*(volatile uint32_t *)0x40020014u)

static uint32_t gpio_state[4];

void gpio_init(void)
{
    for (int i = 0; i < 4; i++) {
        gpio_state[i] = 0;
    }
}

void gpio_toggle(uint32_t pin)
{
    gpio_state[0] ^= 1u << pin;
    GPIOA_ODR = gpio_state[0];
}
