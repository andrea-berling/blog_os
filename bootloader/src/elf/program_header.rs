use crate::elf::{
    Halfword,
    error::{self, Facility, InternalError, Kind, Reason, try_read_error},
    header,
};

mod inner {
    use zerocopy::{LE, TryFromBytes, U32, U64};

    #[derive(Debug, TryFromBytes)]
    #[repr(C)]
    pub(super) struct Elf32HeaderEntry {
        pub(super) r#type: U32<LE>,
        pub(super) offset: U32<LE>,
        pub(super) virtual_address: U32<LE>,
        pub(super) physical_address: U32<LE>,
        pub(super) segment_size_on_file: U32<LE>,
        pub(super) segment_size_in_memory: U32<LE>,
        pub(super) flags: U32<LE>,
        pub(super) alignment: U32<LE>,
    }

    #[derive(Debug, TryFromBytes)]
    #[repr(C)]
    pub(super) struct Elf64HeaderEntry {
        pub(super) r#type: U32<LE>,
        pub(super) flags: U32<LE>,
        pub(super) offset: U64<LE>,
        pub(super) virtual_address: U64<LE>,
        pub(super) physical_address: U64<LE>,
        pub(super) segment_size_on_file: U64<LE>,
        pub(super) segment_size_in_memory: U64<LE>,
        pub(super) alignment: U64<LE>,
    }
}

pub const ELF32_ENTRY_SIZE: usize = size_of::<inner::Elf32HeaderEntry>();
pub const ELF64_ENTRY_SIZE: usize = size_of::<inner::Elf64HeaderEntry>();

#[derive(Debug)]
pub enum HeaderEntry {
    Elf32(inner::Elf32HeaderEntry),
    Elf64(inner::Elf64HeaderEntry),
}

impl HeaderEntry {
    fn error(kind: Kind, facility: Facility) -> error::Error {
        error::Error::InternalError(InternalError::new(facility, kind, error::Context::Parsing))
    }

    pub fn try_from_bytes(
        bytes: &[u8],
        class: header::Class,
        facility: Facility,
    ) -> error::Result<Self> {
        match class {
            header::Class::Elf32 => inner::Elf32HeaderEntry::try_read_from_prefix(bytes)
                .map_err(|err| try_read_error(facility, err))
                .and_then(|(header_entry, _rest)| {
                    let type_halfword = header_entry.r#type.get();

                    match ProgramHeaderEntryType::try_from(type_halfword) {
                        Ok(_) => Ok(header_entry),
                        Err(err) => Err(Self::error(CantReadField("type", err), facility)),
                    }
                })
                .map(HeaderEntry::Elf32),

            header::Class::Elf64 => inner::Elf64HeaderEntry::try_read_from_prefix(bytes)
                .map_err(|err| try_read_error(facility, err))
                .and_then(|(header_entry, _rest)| {
                    let type_halfword = header_entry.r#type.get();

                    match ProgramHeaderEntryType::try_from(type_halfword) {
                        Ok(_) => Ok(header_entry),
                        Err(err) => Err(Self::error(CantReadField("type", err), facility)),
                    }
                })
                .map(HeaderEntry::Elf64),
        }
    }

    pub fn r#type(&self) -> ProgramHeaderEntryType {
        let r#type_word = match self {
            HeaderEntry::Elf32(elf32_header_entry) => elf32_header_entry.r#type.get(),
            HeaderEntry::Elf64(elf64_header_entry) => elf64_header_entry.r#type.get(),
        };

        match r#type_word {
            0 => ProgramHeaderEntryType::Null,
            1 => ProgramHeaderEntryType::Load,
            2 => ProgramHeaderEntryType::Dynamic,
            3 => ProgramHeaderEntryType::Interpreter,
            4 => ProgramHeaderEntryType::Note,
            5 => ProgramHeaderEntryType::SharedLibrary,
            6 => ProgramHeaderEntryType::ProgramHeader,
            7 => ProgramHeaderEntryType::ThreadLocalStorage,
            t if (8..=0x5FFFFFFF).contains(&t) => ProgramHeaderEntryType::OsSpecific(t),
            t if (0x60000000..=0xFFFFFFFF).contains(&t) => {
                ProgramHeaderEntryType::ProcessorSpecific(t)
            }
            _ => unreachable!(),
        }
    }

    pub fn offset(&self) -> u64 {
        match self {
            HeaderEntry::Elf32(elf32_header_entry) => elf32_header_entry.offset.get() as u64,
            HeaderEntry::Elf64(elf64_header_entry) => elf64_header_entry.offset.get(),
        }
    }

    pub fn virtual_address(&self) -> u64 {
        match self {
            HeaderEntry::Elf32(elf32_header_entry) => {
                elf32_header_entry.virtual_address.get() as u64
            }
            HeaderEntry::Elf64(elf64_header_entry) => elf64_header_entry.virtual_address.get(),
        }
    }

    pub fn physical_address(&self) -> u64 {
        match self {
            HeaderEntry::Elf32(elf32_header_entry) => {
                elf32_header_entry.physical_address.get() as u64
            }
            HeaderEntry::Elf64(elf64_header_entry) => elf64_header_entry.physical_address.get(),
        }
    }

    pub fn on_file_size(&self) -> u64 {
        match self {
            HeaderEntry::Elf32(elf32_header_entry) => {
                elf32_header_entry.segment_size_on_file.get() as u64
            }
            HeaderEntry::Elf64(elf64_header_entry) => elf64_header_entry.segment_size_on_file.get(),
        }
    }

    pub fn in_memory_size(&self) -> u64 {
        match self {
            HeaderEntry::Elf32(elf32_header_entry) => {
                elf32_header_entry.segment_size_in_memory.get() as u64
            }
            HeaderEntry::Elf64(elf64_header_entry) => {
                elf64_header_entry.segment_size_in_memory.get()
            }
        }
    }

    pub fn address_alignment(&self) -> u64 {
        match self {
            HeaderEntry::Elf32(elf32_header_entry) => elf32_header_entry.alignment.get() as u64,
            HeaderEntry::Elf64(elf64_header_entry) => elf64_header_entry.alignment.get(),
        }
    }

    pub fn write_to<W: core::fmt::Write>(&self, writer: &mut W) -> error::Result<()> {
        writeln!(writer, "Type: {}", self.r#type())?;
        writeln!(writer, "Offset: {:#x}", self.offset())?;
        writeln!(writer, "Virtual Address: {:#x}", self.virtual_address())?;
        writeln!(writer, "Physical Address: {:#x}", self.physical_address())?;
        writeln!(writer, "Size on file: {}", self.on_file_size())?;
        writeln!(writer, "Size in memory: {}", self.in_memory_size())?;
        writeln!(writer, "Address Alignment: {:#x}", self.address_alignment())?;
        Ok(())
    }
}

#[derive(Debug)]
#[repr(u32)]
pub enum ProgramHeaderEntryType {
    Null = 0,
    Load = 1,
    Dynamic = 2,
    Interpreter = 3,
    Note = 4,
    SharedLibrary = 5,
    ProgramHeader = 6,
    ThreadLocalStorage = 7,
    OsSpecific(u32),
    ProcessorSpecific(u32),
}

impl TryFrom<u32> for ProgramHeaderEntryType {
    type Error = Reason;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(ProgramHeaderEntryType::Null),
            1 => Ok(ProgramHeaderEntryType::Load),
            2 => Ok(ProgramHeaderEntryType::Dynamic),
            3 => Ok(ProgramHeaderEntryType::Interpreter),
            4 => Ok(ProgramHeaderEntryType::Note),
            5 => Ok(ProgramHeaderEntryType::SharedLibrary),
            6 => Ok(ProgramHeaderEntryType::ProgramHeader),
            7 => Ok(ProgramHeaderEntryType::ThreadLocalStorage),
            t if (8..=0x5FFFFFFF).contains(&t) => Ok(ProgramHeaderEntryType::OsSpecific(t)),
            t if (0x60000000..=0xFFFFFFFF).contains(&t) => {
                Ok(ProgramHeaderEntryType::ProcessorSpecific(t))
            }
            _ => Err(Reason::InvalidValue(value.into())),
        }
    }
}

impl core::fmt::Display for ProgramHeaderEntryType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProgramHeaderEntryType::Null => write!(f, "NULL"),
            ProgramHeaderEntryType::Load => write!(f, "LOAD"),
            ProgramHeaderEntryType::Dynamic => write!(f, "DYNAMIC"),
            ProgramHeaderEntryType::Interpreter => write!(f, "INTERP"),
            ProgramHeaderEntryType::Note => write!(f, "NOTE"),
            ProgramHeaderEntryType::SharedLibrary => write!(f, "SHLIB"),
            ProgramHeaderEntryType::ProgramHeader => write!(f, "PHDR"),
            ProgramHeaderEntryType::ThreadLocalStorage => write!(f, "TLS"),
            ProgramHeaderEntryType::OsSpecific(t) => write!(f, "OS-SPECIFIC({t:#x})"),
            ProgramHeaderEntryType::ProcessorSpecific(t) => write!(f, "PROCESSOR-SPECIFIC({t:#x})"),
        }
    }
}

pub(crate) struct ProgramHeaderEntries<'a> {
    bytes: &'a [u8],
    class: header::Class,
    bytes_read_so_far: usize,
}
use error::Kind::*;
use zerocopy::TryFromBytes as _;

impl<'a> ProgramHeaderEntries<'a> {
    fn error(kind: error::Kind) -> error::Error {
        error::Error::InternalError(error::InternalError::new(
            error::Facility::ProgramHeader,
            kind,
            error::Context::Parsing,
        ))
    }

    pub(crate) fn new(
        bytes: &'a [u8],
        class: header::Class,
        n_entries: Halfword,
    ) -> error::Result<Self> {
        let entry_size = match class {
            header::Class::Elf32 => ELF32_ENTRY_SIZE,
            header::Class::Elf64 => ELF64_ENTRY_SIZE,
        };
        if bytes.len() < (n_entries as u32 * entry_size as u32) as usize {
            return Err(Self::error(CantFit("program headers")));
        }

        Ok(Self {
            bytes,
            class,
            bytes_read_so_far: 0,
        })
    }
}

impl<'a> Iterator for ProgramHeaderEntries<'a> {
    type Item = error::Result<HeaderEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes_read_so_far >= self.bytes.len() {
            return None;
        }

        let entry_size = match self.class {
            header::Class::Elf32 => ELF32_ENTRY_SIZE,
            header::Class::Elf64 => ELF64_ENTRY_SIZE,
        };

        Some(
            HeaderEntry::try_from_bytes(
                self.bytes.get(self.bytes_read_so_far..)?,
                self.class,
                error::Facility::ProgramHeaderEntry(entry_size as Halfword),
            )
            .inspect(|_| {
                self.bytes_read_so_far += entry_size;
            }),
        )
    }
}
