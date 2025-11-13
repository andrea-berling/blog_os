use thiserror::Error;

use crate::elf::Halfword;

#[derive(Error, Debug, Clone, Copy)]
pub enum Facility {
    #[error("ELF file")]
    File,
    #[error("ELF header")]
    Header,
    #[error("ELF section header")]
    SectionHeader,
    #[error("ELF program header")]
    ProgramHeader,
    #[error("ELF section header entry {0}")]
    SectionHeaderEntry(Halfword),
    #[error("ELF program header entry {0}")]
    ProgramHeaderEntry(Halfword),
}

pub type Error = crate::error::Error<Facility>;
