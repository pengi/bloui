#include "drivers.h"

namespace hw::uart {

void Port::write(uint8_t byte)
{
    *reinterpret_cast<volatile uint32_t *>(base_ + 4) = byte;
}

void Port::write(const char *text)
{
    while (*text)
        write(static_cast<uint8_t>(*text++));
}

Port &Port::operator<<(const char *text)
{
    write(text);
    return *this;
}

} // namespace hw::uart
