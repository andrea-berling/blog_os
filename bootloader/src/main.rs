// Bless be thee:
// https://stackoverflow.com/questions/67902309/how-to-compile-rust-code-to-bare-metal-32-bit-x86-i686-code-what-compile-targ#67902310
#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(clippy::multiple_unsafe_ops_per_block)]
#![forbid(clippy::undocumented_unsafe_blocks)]

use core::{arch::asm, fmt::Write as _, panic};

mod edd;
mod elf;
mod error;
mod macros;
mod vga;

#[cfg(target_os = "none")]
use core::panic::PanicInfo;

/// This function is called on panic.
#[cfg(target_os = "none")]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut vga_writer = vga::Writer::new();
    writeln!(vga_writer, "{info:#?}");
    loop {}
}

#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.start")]
//#[cfg(target_os = "none")]
pub extern "C" fn start(
    drive_parameters_pointer: *const u8,
    edd_version: u32,
    extensions_bitmap: u32,
) -> ! {
    let mut vga_writer = vga::Writer::new();
    vga_writer.write_string("Hello from stage2!\n");

    writeln!(vga_writer, "x = {drive_parameters_pointer:#x?}").unwrap();
    writeln!(vga_writer, "edd_version = {edd_version:#x?}").unwrap();
    writeln!(vga_writer, "extensions_bitmap = {extensions_bitmap:#x?}").unwrap();

    // SAFETY: The call to BIOS interrupt 13h with AH=48h returned without error in stage1 if we
    // got to stage2, and the drive_parameters_pointer, passed during stage1 to start, points to a
    // buffer of 30 bytes containing the result
    let drive_parameters_bytes = unsafe {
        core::ptr::slice_from_raw_parts(drive_parameters_pointer, 66)
            .as_ref()
            .unwrap()
    };

    let drive_parameters = edd::DriveParameters::from_bytes(drive_parameters_bytes, true)
        .inspect_err(|err| {
            writeln!(vga_writer, "{err:#}").unwrap();
        })
        .unwrap();

    writeln!(vga_writer, "{:x?}", drive_parameters_bytes).unwrap();
    writeln!(vga_writer, "{}", drive_parameters).unwrap();

    loop {}
}

#[cfg(not(target_os = "none"))]
fn main() {
    let mut args = std::env::args();
    args.next().unwrap();
    let bytes = std::fs::read(args.next().unwrap()).unwrap();
    let elf_file = match elf::File::try_from(bytes.as_slice()) {
        Ok(elf_file) => elf_file,
        Err(err) => {
            println!("{err}");
            return;
        }
    };
    let mut s = String::new();
    elf_file.header().write_to(&mut s).unwrap();
    print!("{s}");

    let string_table = elf_file
        .get_section_by_index(elf_file.header().string_table_index().into())
        .unwrap()
        .unwrap()
        .downcast_to_string_table()
        .unwrap();

    println!("--------");
    println!("SECTIONS");
    println!("--------");
    for section in elf_file.sections() {
        use core::fmt::Write as _;

        let section = section.unwrap();

        let mut s = String::new();
        let section_name = string_table
            .get_string(section.name_index() as usize)
            .unwrap()
            .unwrap();
        s.write_fmt(format_args!("Section name: {section_name}\n"))
            .unwrap();
        section.write_to(&mut s).unwrap();
        println!("--------");
        print!("{s}");
        println!("--------");
    }

    println!("--------");
    println!("SEGMENTS");
    println!("--------");
    for header in elf_file.program_headers() {
        let header = header.unwrap();

        let mut s = String::new();
        header.write_to(&mut s).unwrap();
        println!("--------");
        print!("{s}");
        println!("--------");
    }
}
