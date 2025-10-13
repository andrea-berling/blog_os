use std::{
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::Context;
use clap::Parser as _;

const SECTOR_SIZE: u64 = 512;

mod xtasks {
    use clap::{Parser, Subcommand};
    #[derive(Parser, Debug)]
    #[command(author, version, about, long_about = None)]
    pub(crate) struct Cli {
        #[command(subcommand)]
        command: Command,
        #[arg(short, long, default_value_t = String::from("."), env)]
        /// Root directory used to resolve relative references (e.g. $ROOT_DIR/bootloader/Cargo.toml).
        root_dir: String,
    }

    impl Cli {
        pub(crate) fn root_dir(&self) -> &str {
            &self.root_dir
        }

        pub(crate) fn command(&self) -> &Command {
            &self.command
        }
    }

    #[derive(Subcommand, Debug)]
    pub enum Command {
        /// Build an image for qemu to load
        BuildImage {
            #[arg(short, long, default_value_t = false)]
            /// Collect and print extra info during the build process
            verbose: bool,
        },
    }
}

fn build_bootloader(root_dir: &Path, verbose: bool) -> anyhow::Result<()> {
    // Build stage2
    let status = Command::new("cargo")
        .args(["+nightly", "build", "--release"])
        .current_dir(root_dir.join("bootloader"))
        .status()
        .context("building stage2")?;
    if !status.success() {
        anyhow::bail!("build stage2 failed");
    }

    let stage2_elf_path = root_dir.join("target/i686-bootloader/release/bootloader");

    if verbose {
        let status = Command::new("sh")
            .args([
                "-c",
                &format!(
                    r#"readelf -h '{}' | grep Entry"#,
                    stage2_elf_path.to_string_lossy()
                ),
            ])
            .status()
            .context("inspecting stage2 entry point")?;
        if !status.success() {
            anyhow::bail!("inspecting stage2 entry point failed");
        }

        let status = Command::new("sh")
            .args([
                "-c",
                &format!(r#"readelf -S {}"#, stage2_elf_path.to_string_lossy()),
            ])
            .status()
            .context("inspecting stage2 sections")?;
        if !status.success() {
            anyhow::bail!("inspecting stage2 sections failed");
        }

        let status = Command::new("sh")
            .args([
                "-c",
                &format!(r#"nm -v {}"#, stage2_elf_path.to_string_lossy()),
            ])
            .status()
            .context("inspecting stage2 symbols")?;
        if !status.success() {
            anyhow::bail!("inspecting stage2 symbols failed");
        }
    }

    let stage2_path = stage2_elf_path
        .parent()
        .ok_or(anyhow::anyhow!("No parent for stage2 ELF?"))?
        .join("stage2.bin");

    let status = Command::new("objcopy")
        .args([
            "-O",
            "binary",
            "-j",
            ".text",
            "-j",
            ".rodata",
            "-j",
            ".data",
            &stage2_elf_path.to_string_lossy(),
            &stage2_path.to_string_lossy(),
        ])
        .status()
        .context("extracting sections from ELF file to generate stage2")?;
    if !status.success() {
        anyhow::bail!("extracting sections from ELF file to generate stage2 failed");
    }

    let metadata = std::fs::metadata(&stage2_path)
        .context("collecting info about the generated stage2 file")?;

    // Build stage1 to read enough sectors to load stage2
    let stage2_sectors = metadata.size().div_ceil(SECTOR_SIZE);

    let stage1_path = root_dir.join("stage1.bin");

    let status = Command::new("nasm")
        .args([
            &format!("-DSTAGE2_SECTORS={stage2_sectors}"),
            "-fbin",
            "-o",
            &stage1_path.to_string_lossy(),
            &root_dir
                .join("bootloader/stage1/boot.asm")
                .to_string_lossy(),
        ])
        .status()
        .context("building stage1")?;
    if !status.success() {
        anyhow::bail!("building stage1 failed");
    }

    let mut bootloader = std::fs::read(&stage1_path).context("reading stage1 bytes")?;
    let mut stage2 = std::fs::read(&stage2_path).context("reading stage2 bytes")?;

    bootloader.append(&mut stage2);
    let bootloader_path = root_dir.join("bootloader.bin");

    std::fs::write(bootloader_path, bootloader).context("writing bootloader file")?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = xtasks::Cli::parse();
    let root_dir = PathBuf::from(cli.root_dir())
        .canonicalize()
        .context("canonicalising root dir")?;

    match cli.command() {
        &xtasks::Command::BuildImage { verbose } => build_bootloader(&root_dir, verbose)?,
    }

    Ok(())
}
