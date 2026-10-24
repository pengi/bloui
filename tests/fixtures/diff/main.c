#include <stdint.h>
#include "uart.h"
#include "hal/hal.h"

volatile uint32_t ticks;
uint32_t boot_count = 1;
#ifdef V2
static uint8_t scratch[512];
#else
static uint8_t scratch[256];
#endif

#ifdef V2
static const char banner[] = "bloui test firmware v2.0, now with reports\r\n";
#else
static const char banner[] = "bloui test firmware v1.0\r\n";
#endif

const uint16_t crc_table[64] = {
    0x0000, 0x1021, 0x2042, 0x3063, 0x4084, 0x50a5, 0x60c6, 0x70e7,
    0x8108, 0x9129, 0xa14a, 0xb16b, 0xc18c, 0xd1ad, 0xe1ce, 0xf1ef,
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
    17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32,
    33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48,
};

void unused_function(void)
{
    /* Removed by --gc-sections. */
    uart_puts("never");
}

void SysTick_Handler(void)
{
    ticks++;
}

static uint16_t crc16(const uint8_t *data, uint32_t len)
{
    uint16_t crc = 0xffff;
    for (uint32_t i = 0; i < len; i++) {
        crc = (uint16_t)((crc << 4) ^ crc_table[((crc >> 12) ^ data[i]) & 0x3f]);
    }
    return crc;
}

#ifdef V2
/* New in v2. */
__attribute__((noinline)) static void report(uint32_t n, uint32_t rate)
{
    uart_puts(n > rate ? "busy\r\n" : "idle\r\n");
    if (n == 0)
        uart_puts("no data\r\n");
}
#endif

int main(void)
{
    clock_init(boot_count * 8000000u);
    gpio_init();
    uart_init(115200);
    uart_puts(banner);
    for (;;) {
        uint32_t n = uart_read(scratch, sizeof(scratch));
        /* 64-bit division pulls __aeabi_uldivmod from libgcc. */
        uint32_t rate = (uint32_t)(((uint64_t)ticks << 20) / (n + 1u));
#ifdef V2
        report(crc16(scratch, n), rate);
#else
        uart_put_hex(crc16(scratch, n) ^ rate);
#endif
        gpio_toggle(5);
    }
}
