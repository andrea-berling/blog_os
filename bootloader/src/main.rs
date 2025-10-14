// Bless be thee:
// https://stackoverflow.com/questions/67902309/how-to-compile-rust-code-to-bare-metal-32-bit-x86-i686-code-what-compile-targ#67902310
#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(clippy::multiple_unsafe_ops_per_block)]
#![forbid(clippy::undocumented_unsafe_blocks)]

mod elf;
mod vga;

#[cfg(target_os = "none")]
use core::panic::PanicInfo;

/// This function is called on panic.
#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.start")]
pub extern "C" fn start() -> ! {
    let mut vga_writer = vga::Writer::new();
    vga_writer.write_string("Hello from stage2!");

    loop {}
}

#[cfg(not(target_os = "none"))]
fn main() {
    let mut args = std::env::args();
    args.next().unwrap();
    let bytes = std::fs::read(args.next().unwrap()).unwrap();
    let header = elf::Header::new(&bytes).unwrap();

    let mut s = String::new();
    header.write_to(&mut s).unwrap();
    print!("{s}");
}
