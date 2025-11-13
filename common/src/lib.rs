#![deny(clippy::multiple_unsafe_ops_per_block)]
#![forbid(clippy::undocumented_unsafe_blocks)]
#![no_std]
pub mod ata;
pub mod control_registers;
pub mod elf;
pub mod error;
pub mod gdt;
pub mod idt;
pub mod macros;
pub mod paging;
pub mod protection;
pub mod serial;
pub mod timer;
pub mod tss;
pub mod vga;
