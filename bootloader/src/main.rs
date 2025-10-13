// Bless be thee:
// https://stackoverflow.com/questions/67902309/how-to-compile-rust-code-to-bare-metal-32-bit-x86-i686-code-what-compile-targ#67902310
#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.start")]
pub extern "C" fn start() -> ! {
    print_msg("Hello from stage2!");

    loop {}
}

#[cfg(target_os = "none")]
use core::panic::PanicInfo;

/// This function is called on panic.
#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[cfg(target_os = "none")]
use core::ptr::write_volatile;

#[cfg(target_os = "none")]
const VGA_BUF: *mut u16 = 0xb8000 as *mut u16;
#[cfg(target_os = "none")]
const WHITE_ON_BLACK: u8 = 0x0F;

#[cfg(target_os = "none")]
fn print_msg(s: &str) {
    for (i, b) in s.bytes().enumerate() {
        unsafe {
            let cell: u16 = (WHITE_ON_BLACK as u16) << 8 | b as u16;
            write_volatile(VGA_BUF.add(i), cell);
        }
    }
}
