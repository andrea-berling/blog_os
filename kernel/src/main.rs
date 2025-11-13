#![no_std]
#![no_main]

use core::panic::PanicInfo;

use common::vga;

/// This function is called on panic.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    vga::writeln_no_sync!("{info:#?}").unwrap();
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    vga::writeln_no_sync!("Hello from the kernel!").unwrap();
    loop {}
}
