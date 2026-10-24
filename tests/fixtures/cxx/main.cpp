#include "drivers.h"

namespace app {

namespace {
uint32_t ticks;
hw::RingBuffer<uint8_t, 64> rx;
hw::RingBuffer<uint32_t, 16> events;
const char banner[] = "bloui C++ test firmware\r\n";
} // namespace

void tick()
{
    ++ticks;
    events.push(ticks);
}

uint32_t uptime()
{
    return ticks / 1000;
}

} // namespace app

extern "C" void SysTick_Handler()
{
    app::tick();
}

extern "C" int main()
{
    hw::uart::Port port(0x40011000);
    port << app::banner;
    for (;;) {
        uint32_t e;
        if (app::events.pop(e)) {
            app::rx.push(static_cast<uint8_t>(e));
            port << "tick\r\n";
        }
        uint8_t c;
        if (app::rx.pop(c) && app::uptime() > 10)
            port.write(c);
    }
}
