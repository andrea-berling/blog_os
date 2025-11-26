#![deny(clippy::multiple_unsafe_ops_per_block)]
#![forbid(clippy::undocumented_unsafe_blocks)]
#![deny(clippy::missing_panics_doc)]
#![deny(clippy::unwrap_used)]
#![no_std]
pub mod ata;
pub mod control_registers;
pub mod elf;
pub mod error;
pub mod gdt;
pub mod idt;
pub mod ioport;
pub mod macros;
pub mod paging;
pub mod pci;
pub mod protection;
pub mod serial;
pub mod timer;
pub mod tss;
pub mod usb;
pub mod vga;
