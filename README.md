# blog_os

[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/andrea-berling/blog_os)

**Note:** This project is for the x86 architecture.

This project is a hobby operating system created by following the excellent blog series [Writing an OS in Rust](https://os.phil-opp.com) by Philipp Oppermann.

## Project Structure

The project is organized into the following main components:

*   `kernel/`: This directory contains the source code for the operating system kernel itself.
*   `bootloader/`: This directory contains the source code for the BIOS-compatible bootloader. The bootloader is divided into two stages:
    *   **Stage 1:** A small assembly program (`boot.asm`) that is responsible for loading the second stage of the bootloader.
    *   **Stage 2:** A Rust program that is responsible for parsing the ELF file of the kernel, loading it into memory, switching the processor to long mode, and finally handing over control to the kernel.
*   `xtasks/`: This directory contains a helper crate for building the bootloader and kernel.

## Prerequisites

This project requires the following tools to be installed:

*   `nasm`: An assembler for the x86 architecture.
*   `qemu`: A generic and open source machine emulator and virtualizer.

This project uses a custom target for the bootloader, so you don't need to add any additional targets to your toolchain.

## Toolchain Requirements

This project requires the Rust nightly toolchain to be installed. You can install it with the following command:

```bash
rustup toolchain install nightly
```

You will also need the `rust-src` component, which is required for building the standard library from source. You can install it with the following command:

```bash
rustup component add rust-src --toolchain nightly
```

## Building and Running

To build the bootloader and kernel, you can use the `xtasks` crate. The following command will build the bootloader and create a `bootloader.bin` file in the root of the project:

```bash
cargo run --manifest-path xtasks/Cargo.toml -- build-image
```

Once the `bootloader.bin` file is created, you can run it in QEMU with the following command:

```bash
qemu-system-x86_64 -drive format=raw,file=./bootloader.bin
```

## License

This project is licensed under the MIT License.
