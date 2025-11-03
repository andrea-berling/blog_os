#![forbid(clippy::multiple_unsafe_ops_per_block)]
#![forbid(clippy::undocumented_unsafe_blocks)]
#![no_std]
pub mod error;
pub mod macros;
pub mod serial;
pub mod timer;
pub mod vga;
