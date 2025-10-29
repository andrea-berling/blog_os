use core::{fmt::Display, str::Utf8Error};

use crate::elf::{self, Halfword, Word, header};

use crate::error::{self, Facility, InternalError, Kind, try_read_error};

mod inner {
    use zerocopy::{LE, TryFromBytes, U32, U64};

    #[derive(Debug, TryFromBytes)]
    #[repr(C)]
    pub(super) struct Elf32HeaderEntry {
        pub(super) name_index: U32<LE>,
        pub(super) r#type: U32<LE>,
        pub(super) flags: U32<LE>,
        pub(super) address: U32<LE>,
        pub(super) offset: U32<LE>,
        pub(super) size: U32<LE>,
        pub(super) link: U32<LE>,
        pub(super) info: U32<LE>,
        pub(super) address_alignment: U32<LE>,
        pub(super) entry_size: U32<LE>,
    }

    #[derive(Debug, TryFromBytes)]
    #[repr(C)]
    pub(super) struct Elf64HeaderEntry {
        pub(super) name_index: U32<LE>,
        pub(super) r#type: U32<LE>,
        pub(super) flags: U64<LE>,
        pub(super) address: U64<LE>,
        pub(super) offset: U64<LE>,
        pub(super) size: U64<LE>,
        pub(super) link: U32<LE>,
        pub(super) info: U32<LE>,
        pub(super) address_alignment: U64<LE>,
        pub(super) entry_size: U64<LE>,
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
    fn error(kind: Kind, facility: elf::error::Facility) -> error::Error {
        error::Error::InternalError(InternalError::new(
            crate::error::Facility::Elf(facility),
            kind,
            error::Context::Parsing,
        ))
    }

    pub fn try_from_bytes(
        bytes: &[u8],
        class: header::Class,
        facility: elf::error::Facility,
    ) -> error::Result<Self> {
        match class {
            header::Class::Elf32 => inner::Elf32HeaderEntry::try_read_from_prefix(bytes)
                .map_err(|err| try_read_error(crate::error::Facility::Elf(facility), err))
                .and_then(|(header_entry, _rest)| {
                    let type_halfword = header_entry.r#type.get();

                    match SectionEntryType::try_from(type_halfword) {
                        Ok(_) => Ok(header_entry),
                        Err(err) => Err(Self::error(CantReadField("type", err), facility)),
                    }
                })
                .map(HeaderEntry::Elf32),
            header::Class::Elf64 => inner::Elf64HeaderEntry::try_read_from_prefix(bytes)
                .map_err(|err| try_read_error(crate::error::Facility::Elf(facility), err))
                .and_then(|(header_entry, _rest)| {
                    let type_halfword = header_entry.r#type.get();

                    match SectionEntryType::try_from(type_halfword) {
                        Ok(_) => Ok(header_entry),
                        Err(err) => Err(Self::error(CantReadField("type", err), facility)),
                    }
                })
                .map(HeaderEntry::Elf64),
        }
    }

    pub fn name_index(&self) -> Word {
        match self {
            HeaderEntry::Elf32(entry) => entry.name_index.get(),
            HeaderEntry::Elf64(entry) => entry.name_index.get(),
        }
    }

    pub fn r#type(&self) -> SectionEntryType {
        // PANIC: shouldn't panic, we check the type as soon as the entry is created
        match self {
            HeaderEntry::Elf32(entry) => entry.r#type.get().try_into().unwrap(),
            HeaderEntry::Elf64(entry) => entry.r#type.get().try_into().unwrap(),
        }
    }

    pub fn address(&self) -> u64 {
        match self {
            HeaderEntry::Elf32(entry) => entry.address.get() as u64,
            HeaderEntry::Elf64(entry) => entry.address.get(),
        }
    }

    pub fn offset(&self) -> u64 {
        match self {
            HeaderEntry::Elf32(entry) => entry.offset.get() as u64,
            HeaderEntry::Elf64(entry) => entry.offset.get(),
        }
    }

    pub fn address_alignment(&self) -> u64 {
        match self {
            HeaderEntry::Elf32(entry) => entry.address_alignment.get() as u64,
            HeaderEntry::Elf64(entry) => entry.address_alignment.get(),
        }
    }

    pub fn size(&self) -> u64 {
        match self {
            HeaderEntry::Elf32(entry) => entry.size.get() as u64,
            HeaderEntry::Elf64(entry) => entry.size.get(),
        }
    }

    pub fn try_to_entry<'a, 'b>(&'a self, bytes: &'b [u8]) -> error::Result<Section<'b>>
    where
        'b: 'a,
    {
        match self.r#type() {
            SectionEntryType::Null => todo!(),
            SectionEntryType::Progbits => todo!(),
            SectionEntryType::Symtab => todo!(),
            SectionEntryType::Strtab => Ok(Section::StringTable(bytes)),
            SectionEntryType::Rela => todo!(),
            SectionEntryType::Hash => todo!(),
            SectionEntryType::Dynamic => todo!(),
            SectionEntryType::Note => todo!(),
            SectionEntryType::NoBits => todo!(),
            SectionEntryType::Rel => todo!(),
            SectionEntryType::Shlib => todo!(),
            SectionEntryType::DynSym => todo!(),
            SectionEntryType::InitArray => todo!(),
            SectionEntryType::FiniArray => todo!(),
            SectionEntryType::PreinitArray => todo!(),
            SectionEntryType::Group => todo!(),
            SectionEntryType::SymtabIndex => todo!(),
            SectionEntryType::OsSpecific(_) => todo!(),
            SectionEntryType::ProcessorSpecific(_) => todo!(),
            SectionEntryType::UserSpecific(_) => todo!(),
        }
    }

    pub fn flags(&self) -> Flags {
        Flags(match self {
            HeaderEntry::Elf32(elf32_header_entry) => elf32_header_entry.flags.get().into(),
            HeaderEntry::Elf64(elf64_header_entry) => elf64_header_entry.flags.get(),
        })
    }

    /// Print out the header using the given writer
    /// String formatting is considered infallible,
    pub fn write_to<W: core::fmt::Write>(&self, writer: &mut W) -> error::Result<()> {
        writeln!(writer, "Name index: {}", self.name_index())?;
        writeln!(writer, "Type: {}", self.r#type())?;
        writeln!(writer, "Address: {:#x}", self.address())?;
        writeln!(writer, "Offset: {:#x}", self.offset())?;
        writeln!(writer, "Address Alignment: {:#x}", self.address_alignment())?;
        writeln!(writer, "Size: {}", self.size())?;
        writeln!(writer, "Flags: {}", self.flags())?;
        Ok(())
    }
}

#[derive(Debug)]
#[repr(u32)]
pub(crate) enum SectionEntryType {
    Null = 0,
    Progbits = 1,
    Symtab = 2,
    Strtab = 3,
    Rela = 4,
    Hash = 5,
    Dynamic = 6,
    Note = 7,
    NoBits = 8,
    Rel = 9,
    Shlib = 10,
    DynSym = 11,
    InitArray = 14,
    FiniArray = 15,
    PreinitArray = 16,
    Group = 17,
    SymtabIndex = 18,
    OsSpecific(u32),
    ProcessorSpecific(u32),
    UserSpecific(u32),
}

impl TryFrom<Word> for SectionEntryType {
    type Error = error::Reason;

    fn try_from(value: Word) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SectionEntryType::Null),
            1 => Ok(SectionEntryType::Progbits),
            2 => Ok(SectionEntryType::Symtab),
            3 => Ok(SectionEntryType::Strtab),
            4 => Ok(SectionEntryType::Rela),
            5 => Ok(SectionEntryType::Hash),
            6 => Ok(SectionEntryType::Dynamic),
            7 => Ok(SectionEntryType::Note),
            8 => Ok(SectionEntryType::NoBits),
            9 => Ok(SectionEntryType::Rel),
            10 => Ok(SectionEntryType::Shlib),
            11 => Ok(SectionEntryType::DynSym),
            14 => Ok(SectionEntryType::InitArray),
            15 => Ok(SectionEntryType::FiniArray),
            16 => Ok(SectionEntryType::PreinitArray),
            17 => Ok(SectionEntryType::Group),
            18 => Ok(SectionEntryType::SymtabIndex),
            v @ 0x60000000..=0x6fffffff => Ok(SectionEntryType::OsSpecific(v)),
            v @ 0x70000000..=0x7fffffff => Ok(SectionEntryType::ProcessorSpecific(v)),
            v @ 0x80000000..=0xffffffff => Ok(SectionEntryType::UserSpecific(v)),
            _ => Err(error::Reason::InvalidValue(value as u64)),
        }
    }
}

impl Display for SectionEntryType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SectionEntryType::Null => write!(f, "NULL"),
            SectionEntryType::Progbits => write!(f, "PROGBITS"),
            SectionEntryType::Symtab => write!(f, "SYMTAB"),
            SectionEntryType::Strtab => write!(f, "STRTAB"),
            SectionEntryType::Rela => write!(f, "RELA"),
            SectionEntryType::Hash => write!(f, "HASH"),
            SectionEntryType::Dynamic => write!(f, "DYNAMIC"),
            SectionEntryType::Note => write!(f, "NOTE"),
            SectionEntryType::NoBits => write!(f, "NOBITS"),
            SectionEntryType::Rel => write!(f, "REL"),
            SectionEntryType::Shlib => write!(f, "SHLIB"),
            SectionEntryType::DynSym => write!(f, "DYNSYM"),
            SectionEntryType::InitArray => write!(f, "INIT_ARRAY"),
            SectionEntryType::FiniArray => write!(f, "FINI_ARRAY"),
            SectionEntryType::PreinitArray => write!(f, "PREINIT_ARRAY"),
            SectionEntryType::Group => write!(f, "GROUP"),
            SectionEntryType::SymtabIndex => write!(f, "SYMTAB_INDEX"),
            SectionEntryType::OsSpecific(value) => {
                write!(f, "OS_SPECIFIC({value:#x})")
            }
            SectionEntryType::ProcessorSpecific(value) => {
                write!(f, "PROCESSOR_SPECIFIC({value:#x})")
            }
            SectionEntryType::UserSpecific(value) => {
                write!(f, "USER_SPECIFIC({value:#x})")
            }
        }
    }
}

make_flags!(new_type: Flags, underlying_flag_type: FlagType, repr: u64, bit_skipper: |i| i == 3);

#[derive(TryFromPrimitive, Clone, Copy)]
#[repr(u32)]
pub enum FlagType {
    Writeable = 0x1,
    Allocated = 0x2,
    ExecutableInstructions = 0x4,
    Merge = 0x10,
    Strings = 0x20,
    InfoLink = 0x40,
    LinkOrder = 0x80,
    OsNonconforming = 0x100,
    InGroup = 0x200,
    Tls = 0x400,
}

impl Display for FlagType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            FlagType::Writeable => write!(f, "WRITEABLE"),
            FlagType::Allocated => write!(f, "ALLOCATED"),
            FlagType::ExecutableInstructions => write!(f, "EXECUTABLE_INSTRUCTIONS"),
            FlagType::Merge => write!(f, "MERGE"),
            FlagType::Strings => write!(f, "STRINGS"),
            FlagType::InfoLink => write!(f, "INFO_LINK"),
            FlagType::LinkOrder => write!(f, "LINK_ORDER"),
            FlagType::OsNonconforming => write!(f, "OS_NONCONFORMING"),
            FlagType::InGroup => write!(f, "IN_GROUP"),
            FlagType::Tls => write!(f, "TLS"),
        }
    }
}

pub(crate) struct SectionHeaderEntries<'a> {
    bytes: &'a [u8],
    class: header::Class,
    bytes_read_so_far: usize,
}
use common::make_flags;
use error::Kind::*;
use num_enum::TryFromPrimitive;
use zerocopy::TryFromBytes;

impl<'a> Iterator for SectionHeaderEntries<'a> {
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
                elf::error::Facility::SectionHeaderEntry(entry_size as Halfword),
            )
            .inspect(|_| {
                self.bytes_read_so_far += entry_size;
            }),
        )
    }
}

impl<'a> SectionHeaderEntries<'a> {
    fn error(kind: error::Kind) -> error::Error {
        error::Error::InternalError(error::InternalError::new(
            error::Facility::Elf(elf::error::Facility::SectionHeader),
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
            return Err(Self::error(CantFit("sections")));
        }

        Ok(Self {
            bytes,
            class,
            bytes_read_so_far: 0,
        })
    }
}

#[derive(Debug)]
pub enum Section<'a> {
    StringTable(&'a [u8]),
}

impl<'a> Section<'a> {
    pub fn downcast_to_string_table(&self) -> Result<StringTable<'a>, Self> {
        match self {
            Section::StringTable(items) => Ok(StringTable(items)),
        }
    }
}

pub struct StringTable<'a>(&'a [u8]);

impl<'a> StringTable<'a> {
    pub fn get_string(&self, index: usize) -> Option<Result<&str, Utf8Error>> {
        if index >= self.0.len() {
            return None;
        }

        let endpoint = self.0[index..].iter().position(|&c| c == 0x0)?;

        Some(str::from_utf8(&self.0[index..][..endpoint]))
    }
}
