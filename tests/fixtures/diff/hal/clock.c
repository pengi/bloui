#include "hal.h"

uint32_t system_core_clock = 16000000u;

void clock_init(uint32_t hz)
{
    system_core_clock = hz;
}
