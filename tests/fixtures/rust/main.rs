//! A small no_std firmware: two modules, a generic, a trait impl, and
//! core::fmt, so the binary has symbols from the app and from core.
#![no_std]
#![no_main]

use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr;

mod uart {
    use core::fmt;

    pub struct Port {
        base: usize,
    }

    impl Port {
        pub const fn new(base: usize) -> Self {
            Port { base }
        }

        #[inline(never)]
        pub fn write_byte(&mut self, byte: u8) {
            unsafe { core::ptr::write_volatile((self.base + 4) as *mut u32, byte as u32) }
        }
    }

    impl fmt::Write for Port {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            for b in s.bytes() {
                self.write_byte(b);
            }
            Ok(())
        }
    }
}

mod queue {
    pub struct Ring<T: Copy, const N: usize> {
        items: [T; N],
        head: usize,
        tail: usize,
    }

    impl<T: Copy, const N: usize> Ring<T, N> {
        pub const fn new(fill: T) -> Self {
            Ring { items: [fill; N], head: 0, tail: 0 }
        }

        #[inline(never)]
        pub fn push(&mut self, value: T) -> bool {
            let next = (self.head + 1) % N;
            if next == self.tail {
                return false;
            }
            self.items[self.head] = value;
            self.head = next;
            true
        }

        #[inline(never)]
        pub fn pop(&mut self) -> Option<T> {
            if self.head == self.tail {
                return None;
            }
            let v = self.items[self.tail];
            self.tail = (self.tail + 1) % N;
            Some(v)
        }
    }
}

static mut TICKS: u32 = 0;
static mut EVENTS: queue::Ring<u32, 16> = queue::Ring::new(0);
static mut RX: queue::Ring<u8, 64> = queue::Ring::new(0);
static mut GREETING: [u8; 8] = *b"bloui rs";

extern "C" {
    static mut _sidata: u32;
    static mut _sdata: u32;
    static mut _edata: u32;
    static mut _sbss: u32;
    static mut _ebss: u32;
    static _estack: u32;
}

#[no_mangle]
pub extern "C" fn SysTick_Handler() {
    unsafe {
        TICKS += 1;
        (*ptr::addr_of_mut!(EVENTS)).push(TICKS);
    }
}

#[no_mangle]
pub unsafe extern "C" fn Reset_Handler() -> ! {
    let mut src = ptr::addr_of_mut!(_sidata);
    let mut dst = ptr::addr_of_mut!(_sdata);
    while dst < ptr::addr_of_mut!(_edata) {
        ptr::write_volatile(dst, ptr::read(src));
        dst = dst.add(1);
        src = src.add(1);
    }
    let mut dst = ptr::addr_of_mut!(_sbss);
    while dst < ptr::addr_of_mut!(_ebss) {
        ptr::write_volatile(dst, 0);
        dst = dst.add(1);
    }
    main()
}

fn main() -> ! {
    let mut port = uart::Port::new(0x4001_1000);
    let greeting = unsafe { &*ptr::addr_of!(GREETING) };
    let _ = port.write_str(core::str::from_utf8(greeting).unwrap_or("?"));
    loop {
        if let Some(e) = unsafe { (*ptr::addr_of_mut!(EVENTS)).pop() } {
            unsafe { (*ptr::addr_of_mut!(RX)).push(e as u8) };
            let _ = writeln!(port, "tick {}", e);
        }
        if let Some(c) = unsafe { (*ptr::addr_of_mut!(RX)).pop() } {
            port.write_byte(c);
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[repr(C)]
pub struct Vectors {
    stack_pointer: &'static u32,
    reset: unsafe extern "C" fn() -> !,
    exceptions: [Option<extern "C" fn()>; 14],
}

#[link_section = ".isr_vector"]
#[used]
static VECTOR_TABLE: Vectors = Vectors {
    stack_pointer: unsafe { &*ptr::addr_of!(_estack) },
    reset: Reset_Handler,
    exceptions: {
        let mut v: [Option<extern "C" fn()>; 14] = [None; 14];
        v[13] = Some(SysTick_Handler);
        v
    },
};
