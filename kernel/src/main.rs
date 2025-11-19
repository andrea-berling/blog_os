#![no_std]
#![no_main]
#![deny(clippy::missing_panics_doc)]
#![deny(clippy::unwrap_used)]

use core::panic::PanicInfo;

use common::vga;

/// This function is called on panic.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    vga::writeln_no_sync!("{info:#?}");
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    vga::writeln_no_sync!("Hello from the kernel!");
    loop {}
}
