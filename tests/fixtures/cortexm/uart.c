#include "uart.h"

#define USART1_DR (*(volatile uint32_t *)0x40011004u)
#define USART1_SR (*(volatile uint32_t *)0x40011000u)

static uint8_t rx_buffer[128];
static volatile uint32_t rx_head;
static uint32_t rx_tail;
static uint32_t uart_baud = 9600;

void uart_init(uint32_t baud)
{
    uart_baud = baud;
    rx_head = 0;
    rx_tail = 0;
}

static void uart_putc(char c)
{
    while ((USART1_SR & 0x80u) == 0) {
    }
    USART1_DR = (uint32_t)c;
}

void uart_puts(const char *s)
{
    while (*s) {
        uart_putc(*s++);
    }
}

void uart_put_hex(uint32_t v)
{
    static const char digits[] = "0123456789abcdef";
    for (int i = 28; i >= 0; i -= 4) {
        uart_putc(digits[(v >> i) & 0xf]);
    }
}

uint32_t uart_read(uint8_t *buf, uint32_t max)
{
    uint32_t n = 0;
    while (n < max && rx_tail != rx_head) {
        buf[n++] = rx_buffer[rx_tail++ % sizeof(rx_buffer)];
    }
    return n;
}

void USART1_IRQHandler(void)
{
    rx_buffer[rx_head++ % sizeof(rx_buffer)] = (uint8_t)USART1_DR;
}
