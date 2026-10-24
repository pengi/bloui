// A tiny driver layer with namespaces, a class and a template.
#pragma once
#include <stddef.h>
#include <stdint.h>

namespace hw {
namespace uart {

class Port {
public:
    explicit constexpr Port(uintptr_t base) : base_(base) {}
    void write(uint8_t byte);
    void write(const char *text);
    Port &operator<<(const char *text);

private:
    uintptr_t base_;
};

} // namespace uart

template <typename T, size_t N>
class RingBuffer {
public:
    __attribute__((noinline)) bool push(T value)
    {
        size_t next = (head_ + 1) % N;
        if (next == tail_)
            return false;
        items_[head_] = value;
        head_ = next;
        return true;
    }
    __attribute__((noinline)) bool pop(T &value)
    {
        if (head_ == tail_)
            return false;
        value = items_[tail_];
        tail_ = (tail_ + 1) % N;
        return true;
    }

private:
    T items_[N];
    size_t head_ = 0;
    size_t tail_ = 0;
};

} // namespace hw

namespace app {
void tick();
uint32_t uptime();
} // namespace app
