use core::fmt::Write;

// https://refspecs.linuxfoundation.org/elf/gabi4+/ch4.eheader.html#elfid
use thiserror::Error;

#[repr(u8)]
#[allow(clippy::upper_case_acronyms)]
// Identification indexes
enum EI {
    MAG0 = 0,
    MAG1,
    MAG2,
    MAG3,
    CLASS,
    DATA,
    VERSION,
    OSABI,
    ABIVERSION,
    PAD,
    NIDENT = 16,
}

const IDENT_SIZE: u8 = 16;
type Halfword = u16;
type Word = u32;
type Xword = u64;

// TODO: Macro?
#[repr(u8)]
enum HeaderOffsets {
    Type = IDENT_SIZE,
    Machine = HeaderOffsets::Type as u8 + (Halfword::BITS as u8 / 8),
    Version = HeaderOffsets::Machine as u8 + (Halfword::BITS as u8 / 8),
    Entrypoint = HeaderOffsets::Version as u8 + (Word::BITS as u8 / 8),
}

#[repr(u8)]
enum Elf32Offsets {
    ProgramHeader = HeaderOffsets::Entrypoint as u8 + (Word::BITS as u8 / 8),
    SectionHeader = Elf32Offsets::ProgramHeader as u8 + (Word::BITS as u8 / 8),
    Flags = Elf32Offsets::SectionHeader as u8 + (Word::BITS as u8 / 8),
    HeaderSize = Elf32Offsets::Flags as u8 + (Word::BITS as u8 / 8),
    ProgramHeaderEntrySize = Elf32Offsets::HeaderSize as u8 + (Halfword::BITS as u8 / 8),
    ProgramHeaderEntries = Elf32Offsets::ProgramHeaderEntrySize as u8 + (Halfword::BITS as u8 / 8),
    SectionHeaderEntrySize = Elf32Offsets::ProgramHeaderEntries as u8 + (Halfword::BITS as u8 / 8),
    SectionHeaderEntries = Elf32Offsets::SectionHeaderEntrySize as u8 + (Halfword::BITS as u8 / 8),
    StringTable = Elf32Offsets::SectionHeaderEntries as u8 + (Halfword::BITS as u8 / 8),
}

#[repr(u8)]
enum Elf64Offsets {
    ProgramHeader = HeaderOffsets::Entrypoint as u8 + (Xword::BITS as u8 / 8),
    SectionHeader = Elf64Offsets::ProgramHeader as u8 + (Xword::BITS as u8 / 8),
    Flags = Elf64Offsets::SectionHeader as u8 + (Xword::BITS as u8 / 8),
    HeaderSize = Elf64Offsets::Flags as u8 + (Word::BITS as u8 / 8),
    ProgramHeaderEntrySize = Elf64Offsets::HeaderSize as u8 + (Halfword::BITS as u8 / 8),
    ProgramHeaderEntries = Elf64Offsets::ProgramHeaderEntrySize as u8 + (Halfword::BITS as u8 / 8),
    SectionHeaderEntrySize = Elf64Offsets::ProgramHeaderEntries as u8 + (Halfword::BITS as u8 / 8),
    SectionHeaderEntries = Elf64Offsets::SectionHeaderEntrySize as u8 + (Halfword::BITS as u8 / 8),
    StringTable = Elf64Offsets::SectionHeaderEntries as u8 + (Halfword::BITS as u8 / 8),
}

enum Version {
    Current,
}

#[derive(Clone, Copy)]
enum Encoding {
    LittleEndian,
    BigEndian,
}

#[derive(Clone, Copy)]
enum Class {
    Elf32,
    Elf64,
}

enum SizedNumber {
    Elf64(u64),
    Elf32(u32),
}

struct TableHeaderDescriptor {
    offset: SizedNumber,
    entry_size: Halfword,
    n_entries: Halfword,
}

pub(crate) struct Header<'a> {
    bytes: &'a [u8],
    class: Class,
    encoding: Encoding,
    address_size_bytes: usize,
}

#[derive(Error, Debug)]
enum Reason {
    #[error("not enough bytes")]
    NotEnoughBytes,
    #[error("invalid value {0}")]
    InvalidValue(u8),
}

#[derive(Error, Debug)]
enum InvalidHeaderError {
    #[error("can't read '{0}' header field: {1}")]
    CantReadHeaderField(&'static str, Reason),
    #[error("can't read {0}-bit number at '{1:#x}': {2}")]
    CantReadNumber(usize, usize, Reason),
}

#[derive(Error, Debug)]
pub enum Error {
    #[error("invalid header")]
    ParsingError(#[from] InvalidHeaderError),
    #[error("couldn't format to string")]
    FormattingError(#[from] core::fmt::Error),
}

impl Error {
    fn not_enough_bytes_for_header_field(s: &'static str) -> Error {
        use Error::*;
        use InvalidHeaderError::*;
        use Reason::*;
        ParsingError(CantReadHeaderField(s, NotEnoughBytes))
    }

    fn not_enough_bytes_for_number(n_bits: usize, position: &[u8]) -> Error {
        use Error::*;
        use InvalidHeaderError::*;
        use Reason::*;
        ParsingError(CantReadNumber(
            n_bits,
            position.as_ptr() as usize,
            NotEnoughBytes,
        ))
    }
}

type Result<T> = core::result::Result<T, Error>;

impl<'a> Header<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Header<'a>> {
        use Error::*;
        use InvalidHeaderError::*;
        use Reason::*;
        let class = bytes
            .get(EI::CLASS as usize)
            .ok_or(Error::not_enough_bytes_for_header_field("class"))
            .and_then(|&b| match b {
                1 => Ok(Class::Elf32),
                2 => Ok(Class::Elf64),
                _ => Err(ParsingError(CantReadHeaderField("class", InvalidValue(b)))),
            })?;

        let encoding = bytes
            .get(EI::DATA as usize)
            .ok_or(Error::not_enough_bytes_for_header_field("data"))
            .and_then(|&b| match b {
                1 => Ok(Encoding::LittleEndian),
                2 => Ok(Encoding::BigEndian),
                _ => Err(ParsingError(CantReadHeaderField("data", InvalidValue(b)))),
            })?;

        let result = Self {
            bytes,
            class,
            encoding,
            address_size_bytes: match class {
                Class::Elf32 => 4,
                Class::Elf64 => 8,
            },
        };

        if result.get_magic()? != *b"\x7fELF" {
            return Err(ParsingError(CantReadHeaderField("magic", InvalidValue(0))));
        }

        Ok(result)
    }

    pub fn get_magic(&self) -> Result<[u8; 4]> {
        Ok(*self
            .bytes
            .first_chunk()
            .ok_or(Error::not_enough_bytes_for_header_field("magic"))?)
    }

    pub fn get_class(&self) -> Class {
        self.class
    }

    pub fn get_encoding(&self) -> Encoding {
        self.encoding
    }

    pub fn get_elf_version(&self) -> Result<Version> {
        use Error::*;
        use InvalidHeaderError::*;
        use Reason::*;
        match self.bytes.get(EI::VERSION as usize) {
            Some(&n) => match n {
                1 => Ok(Version::Current),
                _ => Err(ParsingError(CantReadHeaderField(
                    "version",
                    InvalidValue(n),
                ))),
            },
            None => Err(Error::not_enough_bytes_for_header_field("version")),
        }
    }

    fn read_number(&self, bytes: &[u8]) -> Result<SizedNumber> {
        match self.encoding {
            Encoding::LittleEndian => match self.class {
                Class::Elf32 => Ok(SizedNumber::Elf32(u32::from_le_bytes(
                    *bytes
                        .first_chunk()
                        .ok_or(Error::not_enough_bytes_for_number(32, bytes))?,
                ))),
                Class::Elf64 => Ok(SizedNumber::Elf64(u64::from_le_bytes(
                    *bytes
                        .first_chunk()
                        .ok_or(Error::not_enough_bytes_for_number(64, bytes))?,
                ))),
            },
            Encoding::BigEndian => match self.class {
                Class::Elf32 => Ok(SizedNumber::Elf32(u32::from_be_bytes(
                    *bytes
                        .first_chunk()
                        .ok_or(Error::not_enough_bytes_for_number(32, bytes))?,
                ))),
                Class::Elf64 => Ok(SizedNumber::Elf64(u64::from_be_bytes(
                    *bytes
                        .first_chunk()
                        .ok_or(Error::not_enough_bytes_for_number(64, bytes))?,
                ))),
            },
        }
    }

    fn read_halfword(&self, bytes: &[u8]) -> Result<Halfword> {
        match self.encoding {
            Encoding::LittleEndian => Ok(u16::from_le_bytes(
                *bytes
                    .first_chunk()
                    .ok_or(Error::not_enough_bytes_for_number(16, bytes))?,
            )),
            Encoding::BigEndian => Ok(u16::from_le_bytes(
                *bytes
                    .first_chunk()
                    .ok_or(Error::not_enough_bytes_for_number(16, bytes))?,
            )),
        }
    }

    pub fn get_entrypoint(&self) -> Result<SizedNumber> {
        match self.bytes.get(
            HeaderOffsets::Entrypoint as usize
                ..HeaderOffsets::Entrypoint as usize + self.address_size_bytes,
        ) {
            Some(bytes) => self.read_number(bytes),
            None => Err(Error::not_enough_bytes_for_header_field("entrypoint")),
        }
    }

    pub fn get_program_header_descriptor(&self) -> Result<TableHeaderDescriptor> {
        let (
            program_header_offset_elfhdr_offset,
            program_header_entry_size_elfhdr_offset,
            program_header_entries_elfhdr_offset,
        ) = match self.class {
            Class::Elf32 => {
                use Elf32Offsets::*;
                (
                    ProgramHeader as usize,
                    ProgramHeaderEntrySize as usize,
                    ProgramHeaderEntries as usize,
                )
            }
            Class::Elf64 => {
                use Elf64Offsets::*;
                (
                    ProgramHeader as usize,
                    ProgramHeaderEntrySize as usize,
                    ProgramHeaderEntries as usize,
                )
            }
        };

        let program_header_offset = self
            .bytes
            .get(
                program_header_offset_elfhdr_offset
                    ..program_header_offset_elfhdr_offset + self.address_size_bytes,
            )
            .ok_or(Error::not_enough_bytes_for_header_field("program header"))
            .and_then(|bytes| self.read_number(bytes))?;

        let program_header_entry_size = self
            .bytes
            .get(
                program_header_entry_size_elfhdr_offset
                    ..program_header_entry_size_elfhdr_offset + self.address_size_bytes,
            )
            .ok_or(Error::not_enough_bytes_for_header_field(
                "program header entry size",
            ))
            .and_then(|bytes| self.read_halfword(bytes))?;

        let program_header_entries = self
            .bytes
            .get(
                program_header_entries_elfhdr_offset
                    ..program_header_entries_elfhdr_offset + self.address_size_bytes,
            )
            .ok_or(Error::not_enough_bytes_for_header_field(
                "program header entries",
            ))
            .and_then(|bytes| self.read_halfword(bytes))?;

        Ok(TableHeaderDescriptor {
            offset: program_header_offset,
            entry_size: program_header_entry_size,
            n_entries: program_header_entries,
        })
    }

    pub fn get_section_header_descriptor(&self) -> Result<TableHeaderDescriptor> {
        let (
            section_header_offset_elfhdr_offset,
            section_header_entry_size_elfhdr_offset,
            section_header_entries_elfhdr_offset,
        ) = match self.class {
            Class::Elf32 => {
                use Elf32Offsets::*;
                (
                    SectionHeader as usize,
                    SectionHeaderEntrySize as usize,
                    SectionHeaderEntries as usize,
                )
            }
            Class::Elf64 => {
                use Elf64Offsets::*;
                (
                    SectionHeader as usize,
                    SectionHeaderEntrySize as usize,
                    SectionHeaderEntries as usize,
                )
            }
        };

        let section_header_offset = self
            .bytes
            .get(
                section_header_offset_elfhdr_offset
                    ..section_header_offset_elfhdr_offset + self.address_size_bytes,
            )
            .ok_or(Error::not_enough_bytes_for_header_field("section header"))
            .and_then(|bytes| self.read_number(bytes))?;

        let section_header_entry_size = self
            .bytes
            .get(
                section_header_entry_size_elfhdr_offset
                    ..section_header_entry_size_elfhdr_offset + Halfword::BITS as usize / 8,
            )
            .ok_or(Error::not_enough_bytes_for_header_field(
                "section header entry size",
            ))
            .and_then(|bytes| self.read_halfword(bytes))?;

        let section_header_entries = self
            .bytes
            .get(
                section_header_entries_elfhdr_offset
                    ..section_header_entries_elfhdr_offset + Halfword::BITS as usize / 8,
            )
            .ok_or(Error::not_enough_bytes_for_header_field(
                "section header entries",
            ))
            .and_then(|bytes| self.read_halfword(bytes))?;

        Ok(TableHeaderDescriptor {
            offset: section_header_offset,
            entry_size: section_header_entry_size,
            n_entries: section_header_entries,
        })
    }

    pub fn get_string_table_index(&self) -> Result<Halfword> {
        let offset = match self.class {
            Class::Elf32 => Elf32Offsets::StringTable as usize,
            Class::Elf64 => Elf64Offsets::StringTable as usize,
        };

        self.bytes
            .get(offset..offset + Halfword::BITS as usize / 8)
            .ok_or(Error::not_enough_bytes_for_number(16, self.bytes))
            .and_then(|bytes| self.read_halfword(bytes))
    }

    pub fn get_size(&self) -> Result<Halfword> {
        let offset = match self.class {
            Class::Elf32 => Elf32Offsets::HeaderSize as usize,
            Class::Elf64 => Elf64Offsets::HeaderSize as usize,
        };

        self.bytes
            .get(offset..offset + Halfword::BITS as usize / 8)
            .ok_or(Error::not_enough_bytes_for_number(16, self.bytes))
            .and_then(|bytes| self.read_halfword(bytes))
    }

    /// Print out the header using the given writer
    /// String formatting is considered infallible,
    pub fn write_to<T: Write>(&self, writer: &mut T) -> Result<()> {
        let magic = self.get_magic()?;

        writeln!(
            writer,
            "Magic: {:#x} {}{}{}",
            magic[0],
            // SAFETY: the magic number was checked in Header::new
            unsafe { char::from_u32_unchecked(magic[1] as u32) },
            // SAFETY: the magic number was checked in Header::new
            unsafe { char::from_u32_unchecked(magic[2] as u32) },
            // SAFETY: the magic number was checked in Header::new
            unsafe { char::from_u32_unchecked(magic[3] as u32) }
        )?;
        writeln!(
            writer,
            "Class: {}",
            match self.class {
                Class::Elf32 => "Elf32",
                Class::Elf64 => "Elf64",
            }
        )?;
        writeln!(
            writer,
            "Data Encoding: {}",
            match self.encoding {
                Encoding::LittleEndian => "Little Endian",
                Encoding::BigEndian => "Big Endian",
            }
        )?;
        writeln!(
            writer,
            "File Version: {}",
            match self.get_elf_version()? {
                Version::Current => "1",
            }
        )?;

        writeln!(
            writer,
            "Entrypoint: {:#x}",
            match self.get_entrypoint()? {
                SizedNumber::Elf64(n) => n,
                SizedNumber::Elf32(n) => n as u64,
            }
        )?;

        writeln!(writer, "Header size: {}", self.get_size()?)?;

        let program_header_descriptor = self.get_program_header_descriptor()?;

        writeln!(
            writer,
            "Program header offset: {}",
            match program_header_descriptor.offset {
                SizedNumber::Elf64(n) => n,
                SizedNumber::Elf32(n) => n as u64,
            }
        )?;
        writeln!(
            writer,
            "Program header entries: {}",
            program_header_descriptor.n_entries
        )?;
        writeln!(
            writer,
            "Program header entry size: {}",
            program_header_descriptor.entry_size
        )?;

        let section_header_descriptor = self.get_section_header_descriptor()?;

        writeln!(
            writer,
            "Section header offset: {}",
            match section_header_descriptor.offset {
                SizedNumber::Elf64(n) => n,
                SizedNumber::Elf32(n) => n as u64,
            }
        )?;
        writeln!(
            writer,
            "Section header entries: {}",
            section_header_descriptor.n_entries
        )?;
        writeln!(
            writer,
            "Section header entry size: {}",
            section_header_descriptor.entry_size
        )?;

        writeln!(
            writer,
            "String table index: {}",
            self.get_string_table_index()?
        )?;

        Ok(())
    }
}
