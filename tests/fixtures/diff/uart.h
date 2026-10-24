#ifndef UART_H
#define UART_H
#include <stdint.h>
void uart_init(uint32_t baud);
void uart_puts(const char *s);
void uart_put_hex(uint32_t v);
uint32_t uart_read(uint8_t *buf, uint32_t max);
#endif
