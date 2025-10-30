use thiserror::Error;

use crate::elf::Halfword;

#[derive(Error, Debug, Clone, Copy)]
pub(crate) enum Facility {
    #[error("ELF file")]
    File,
    #[error("ELF header")]
    Header,
    #[error("section header")]
    SectionHeader,
    #[error("program header")]
    ProgramHeader,
    #[error("section header entry {0}")]
    SectionHeaderEntry(Halfword),
    #[error("program header entry {0}")]
    ProgramHeaderEntry(Halfword),
}
