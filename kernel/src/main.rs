#![no_std]
#![no_main]

use core::fmt::Write;
use core::panic::PanicInfo;

use common::vga;

/// This function is called on panic.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut vga = vga::Writer::new();
    writeln!(vga, "{info:#?}").unwrap();
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let mut vga = vga::Writer::new();
    writeln!(vga, "Hello from the kernel!").unwrap();
    loop {}
}
