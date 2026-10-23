#ifndef HAL_H
#define HAL_H
#include <stdint.h>
void clock_init(uint32_t hz);
void gpio_init(void);
void gpio_toggle(uint32_t pin);
#endif
