use core::{fmt::Display, str::Utf8Error};

use crate::elf::error::Facility;
use crate::elf::{self, Halfword, Word, header};

use common::error::Context;
use common::error::InternalError;
use common::error::Kind;
use common::error::Kind::*;
use common::error::Reason;
use common::error::Result;
use common::error::try_read_error;
use elf::Error;

mod inner {
    use zerocopy::{LE, TryFromBytes, U32, U64};

    #[cfg_attr(test, derive(Default, PartialEq, Eq))]
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

    #[cfg_attr(test, derive(Default, PartialEq, Eq))]
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

    #[derive(Debug)]
    pub(super) enum HeaderEntry {
        Elf32(Elf32HeaderEntry),
        Elf64(Elf64HeaderEntry),
    }
}

pub const ELF32_ENTRY_SIZE: usize = size_of::<inner::Elf32HeaderEntry>();
pub const ELF64_ENTRY_SIZE: usize = size_of::<inner::Elf64HeaderEntry>();

#[derive(Debug)]
pub struct HeaderEntry(inner::HeaderEntry);

impl HeaderEntry {
    fn error(kind: Kind, facility: Facility) -> Error {
        Error::InternalError(InternalError::new(facility, kind, Context::Parsing))
    }

    pub fn try_from_bytes(
        bytes: &[u8],
        class: header::Class,
        facility: elf::error::Facility,
    ) -> Result<Self, Facility> {
        match class {
            header::Class::Elf32 => inner::Elf32HeaderEntry::try_read_from_prefix(bytes)
                .map_err(|err| try_read_error(facility, err))
                .and_then(|(header_entry, _rest)| {
                    let type_halfword = header_entry.r#type.get();

                    match SectionEntryType::try_from(type_halfword) {
                        Ok(_) => Ok(header_entry),
                        Err(err) => Err(Self::error(CantReadField("type", err), facility)),
                    }
                })
                .map(inner::HeaderEntry::Elf32)
                .map(HeaderEntry),
            header::Class::Elf64 => inner::Elf64HeaderEntry::try_read_from_prefix(bytes)
                .map_err(|err| try_read_error(facility, err))
                .and_then(|(header_entry, _rest)| {
                    let type_halfword = header_entry.r#type.get();

                    match SectionEntryType::try_from(type_halfword) {
                        Ok(_) => Ok(header_entry),
                        Err(err) => Err(Self::error(CantReadField("type", err), facility)),
                    }
                })
                .map(inner::HeaderEntry::Elf64)
                .map(HeaderEntry),
        }
    }

    pub fn name_index(&self) -> Word {
        match &self.0 {
            inner::HeaderEntry::Elf32(entry) => entry.name_index.get(),
            inner::HeaderEntry::Elf64(entry) => entry.name_index.get(),
        }
    }

    pub fn r#type(&self) -> SectionEntryType {
        // PANIC: shouldn't panic, we check the type as soon as the entry is created
        match &self.0 {
            inner::HeaderEntry::Elf32(entry) => entry.r#type.get().try_into().unwrap(),
            inner::HeaderEntry::Elf64(entry) => entry.r#type.get().try_into().unwrap(),
        }
    }

    pub fn address(&self) -> u64 {
        match &self.0 {
            inner::HeaderEntry::Elf32(entry) => entry.address.get() as u64,
            inner::HeaderEntry::Elf64(entry) => entry.address.get(),
        }
    }

    pub fn offset(&self) -> u64 {
        match &self.0 {
            inner::HeaderEntry::Elf32(entry) => entry.offset.get() as u64,
            inner::HeaderEntry::Elf64(entry) => entry.offset.get(),
        }
    }

    pub fn address_alignment(&self) -> u64 {
        match &self.0 {
            inner::HeaderEntry::Elf32(entry) => entry.address_alignment.get() as u64,
            inner::HeaderEntry::Elf64(entry) => entry.address_alignment.get(),
        }
    }

    pub fn size(&self) -> u64 {
        match &self.0 {
            inner::HeaderEntry::Elf32(entry) => entry.size.get() as u64,
            inner::HeaderEntry::Elf64(entry) => entry.size.get(),
        }
    }

    pub fn link(&self) -> u32 {
        match &self.0 {
            inner::HeaderEntry::Elf32(entry) => entry.link.get(),
            inner::HeaderEntry::Elf64(entry) => entry.link.get(),
        }
    }

    pub fn info(&self) -> u32 {
        match &self.0 {
            inner::HeaderEntry::Elf32(entry) => entry.info.get(),
            inner::HeaderEntry::Elf64(entry) => entry.info.get(),
        }
    }

    pub fn entry_size(&self) -> u64 {
        match &self.0 {
            inner::HeaderEntry::Elf32(entry) => entry.entry_size.get() as u64,
            inner::HeaderEntry::Elf64(entry) => entry.entry_size.get(),
        }
    }

    pub fn try_to_entry<'a, 'b>(&'a self, bytes: &'b [u8]) -> Result<Section<'b>, Facility>
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
        Flags(match &self.0 {
            inner::HeaderEntry::Elf32(elf32_header_entry) => elf32_header_entry.flags.get().into(),
            inner::HeaderEntry::Elf64(elf64_header_entry) => elf64_header_entry.flags.get(),
        })
    }

    /// Print out the header using the given writer
    /// String formatting is considered infallible,
    pub fn write_to<W: core::fmt::Write>(&self, writer: &mut W) -> Result<(), Facility> {
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

#[cfg_attr(test, derive(PartialEq, Eq))]
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
    type Error = Reason;

    fn try_from(value: Word) -> core::result::Result<Self, Self::Error> {
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
            _ => Err(Reason::InvalidValue(value as u64)),
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

make_flags!(new_type: Flags, underlying_flag_type: FlagType, repr: u64, bit_skipper: |i| i == 3 || i > 6);

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
use num_enum::TryFromPrimitive;
use zerocopy::TryFromBytes;

impl<'a> Iterator for SectionHeaderEntries<'a> {
    type Item = Result<HeaderEntry, Facility>;

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
    fn error(kind: Kind) -> Error {
        Error::InternalError(InternalError::new(
            elf::error::Facility::SectionHeader,
            kind,
            Context::Parsing,
        ))
    }

    pub(crate) fn new(
        bytes: &'a [u8],
        class: header::Class,
        n_entries: Halfword,
    ) -> Result<Self, Facility> {
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
    pub fn downcast_to_string_table(&self) -> Result<StringTable<'a>, Facility> {
        match self {
            Section::StringTable(items) => Ok(StringTable(items)),
        }
    }
}

pub struct StringTable<'a>(&'a [u8]);

impl<'a> StringTable<'a> {
    pub fn get_string(&self, index: usize) -> Option<core::result::Result<&str, Utf8Error>> {
        if index >= self.0.len() {
            return None;
        }

        let endpoint = self.0[index..].iter().position(|&c| c == 0x0)?;

        Some(str::from_utf8(&self.0[index..][..endpoint]))
    }
}

#[cfg(test)]
mod tests {
    use crate::elf::{
        error,
        section::{
            FlagType, Flags, HeaderEntry, SectionEntryType,
            inner::{Elf32HeaderEntry, Elf64HeaderEntry},
        },
    };

    const NULL_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    const PROGBITS_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0xe0, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xe0, 0x02, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    const NOTE_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x09, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0xfc, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xfc, 0x02, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    const DYNSYM_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x2a, 0x00, 0x00, 0x00, 0x0b, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x40, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x03, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x48, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01,
        0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    const OS_SPECIFIC_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x32, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0x6f, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x88, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x88, 0x09, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x86, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    const STRING_TABLE_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x58, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x4c, 0x0b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x4c, 0x0b, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xfb, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    const RELA_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x60, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x48, 0x0f, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x48, 0x0f, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xa0, 0x6b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    const RELA_PLT_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x6a, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0xe8, 0x7a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xe8, 0x7a, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x60, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x1b,
        0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    const RODATA_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x87, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x32, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x70, 0x7b, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x70, 0x7b, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xa0, 0x78, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    const TEXT_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0xb9, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x2c, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x02, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xc0, 0xec, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    const GOT_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x0b, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x50, 0x6b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x50, 0x4b, 0x08, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xc8, 0x09, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    const BSS_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x3e, 0x01, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x08, 0x8f, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x08, 0x5f, 0x08, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xc8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    const SYMBOL_TABLE_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0xca, 0x01, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xd0, 0x3d, 0x4e, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xc0, 0xdb, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2c, 0x00, 0x00, 0x00, 0xbd,
        0x07, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x18, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn test_headers_64bit() {
        let mut header = HeaderEntry::try_from_bytes(
            &NULL_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(0, header.name_index());
        assert_eq!(SectionEntryType::Null, header.r#type());
        assert_eq!(Flags::empty(), header.flags());
        assert_eq!(0x0, header.address());
        assert_eq!(0x0, header.offset());
        assert_eq!(0, header.size());
        assert_eq!(0, header.link());
        assert_eq!(0, header.info());
        assert_eq!(0, header.address_alignment());
        assert_eq!(0, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &PROGBITS_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(1, header.name_index());
        assert_eq!(SectionEntryType::Progbits, header.r#type());
        assert_eq!(Flags::from(FlagType::Allocated), header.flags());
        assert_eq!(0x2e0, header.address());
        assert_eq!(0x2e0, header.offset());
        assert_eq!(28, header.size());
        assert_eq!(0, header.link());
        assert_eq!(0, header.info());
        assert_eq!(0x1, header.address_alignment());
        assert_eq!(0, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &NOTE_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(9, header.name_index());
        assert_eq!(SectionEntryType::Note, header.r#type());
        assert_eq!(Flags::from(FlagType::Allocated), header.flags());
        assert_eq!(0x2fc, header.address());
        assert_eq!(0x2fc, header.offset());
        assert_eq!(32, header.size());
        assert_eq!(0, header.link());
        assert_eq!(0, header.info());
        assert_eq!(0x4, header.address_alignment());
        assert_eq!(0, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &DYNSYM_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(42, header.name_index());
        assert_eq!(SectionEntryType::DynSym, header.r#type());
        assert_eq!(Flags::from(FlagType::Allocated), header.flags());
        assert_eq!(0x340, header.address());
        assert_eq!(0x340, header.offset());
        assert_eq!(1608, header.size());
        assert_eq!(8, header.link());
        assert_eq!(1, header.info());
        assert_eq!(0x8, header.address_alignment());
        assert_eq!(24, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &OS_SPECIFIC_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(50, header.name_index());
        assert_eq!(SectionEntryType::OsSpecific(0x6fffffff), header.r#type());
        assert_eq!(Flags::from(FlagType::Allocated), header.flags());
        assert_eq!(0x988, header.address());
        assert_eq!(0x988, header.offset());
        assert_eq!(134, header.size());
        assert_eq!(4, header.link());
        assert_eq!(0, header.info());
        assert_eq!(0x2, header.address_alignment());
        assert_eq!(2, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &STRING_TABLE_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(88, header.name_index());
        assert_eq!(SectionEntryType::Strtab, header.r#type());
        assert_eq!(Flags::from(FlagType::Allocated), header.flags());
        assert_eq!(0xb4c, header.address());
        assert_eq!(0xb4c, header.offset());
        assert_eq!(1019, header.size());
        assert_eq!(0, header.link());
        assert_eq!(0, header.info());
        assert_eq!(0x1, header.address_alignment());
        assert_eq!(0, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &RELA_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(96, header.name_index());
        assert_eq!(SectionEntryType::Rela, header.r#type());
        assert_eq!(Flags::from(FlagType::Allocated), header.flags());
        assert_eq!(0xf48, header.address());
        assert_eq!(0xf48, header.offset());
        assert_eq!(27552, header.size());
        assert_eq!(4, header.link());
        assert_eq!(0, header.info());
        assert_eq!(0x8, header.address_alignment());
        assert_eq!(24, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &RELA_PLT_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(106, header.name_index());
        assert_eq!(SectionEntryType::Rela, header.r#type());
        assert_eq!(FlagType::Allocated | FlagType::InfoLink, header.flags());
        assert_eq!(0x7ae8, header.address());
        assert_eq!(0x7ae8, header.offset());
        assert_eq!(96, header.size());
        assert_eq!(4, header.link());
        assert_eq!(27, header.info());
        assert_eq!(0x8, header.address_alignment());
        assert_eq!(24, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &RODATA_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(135, header.name_index());
        assert_eq!(SectionEntryType::Progbits, header.r#type());
        assert_eq!(
            FlagType::Allocated | FlagType::Merge | FlagType::Strings,
            header.flags()
        );
        assert_eq!(0x7b70, header.address());
        assert_eq!(0x7b70, header.offset());
        assert_eq!(30880, header.size());
        assert_eq!(0, header.link());
        assert_eq!(0, header.info());
        assert_eq!(0x10, header.address_alignment());
        assert_eq!(0, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &TEXT_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(185, header.name_index());
        assert_eq!(SectionEntryType::Progbits, header.r#type());
        assert_eq!(
            FlagType::Allocated | FlagType::ExecutableInstructions,
            header.flags()
        );
        assert_eq!(0x22c00, header.address());
        assert_eq!(0x21c00, header.offset());
        assert_eq!(388288, header.size());
        assert_eq!(0, header.link());
        assert_eq!(0, header.info());
        assert_eq!(0x10, header.address_alignment());
        assert_eq!(0, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &GOT_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(267, header.name_index());
        assert_eq!(SectionEntryType::Progbits, header.r#type());
        assert_eq!(FlagType::Writeable | FlagType::Allocated, header.flags());
        assert_eq!(0x86b50, header.address());
        assert_eq!(0x84b50, header.offset());
        assert_eq!(2504, header.size());
        assert_eq!(0, header.link());
        assert_eq!(0, header.info());
        assert_eq!(0x8, header.address_alignment());
        assert_eq!(0, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &BSS_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(318, header.name_index());
        assert_eq!(SectionEntryType::NoBits, header.r#type());
        assert_eq!(FlagType::Writeable | FlagType::Allocated, header.flags());
        assert_eq!(0x88f08, header.address());
        assert_eq!(0x85f08, header.offset());
        assert_eq!(200, header.size());
        assert_eq!(0, header.link());
        assert_eq!(0, header.info());
        assert_eq!(0x8, header.address_alignment());
        assert_eq!(0, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &SYMBOL_TABLE_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(458, header.name_index());
        assert_eq!(SectionEntryType::Symtab, header.r#type());
        assert_eq!(Flags::empty(), header.flags());
        assert_eq!(0, header.address());
        assert_eq!(0x4e3dd0, header.offset());
        assert_eq!(56256, header.size());
        assert_eq!(44, header.link());
        assert_eq!(1981, header.info());
        assert_eq!(0x8, header.address_alignment());
        assert_eq!(24, header.entry_size());
    }

    const NULL_HEADER_32_BIT: [u8; size_of::<Elf32HeaderEntry>()] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const TEXT_HEADER_32_BIT: [u8; size_of::<Elf32HeaderEntry>()] = [
        0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
        0x00, 0x00, 0x10, 0x00, 0x00, 0x5d, 0x7d, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const RODATA_HEADER_32_BIT: [u8; size_of::<Elf32HeaderEntry>()] = [
        0x07, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x32, 0x00, 0x00, 0x00, 0x60, 0x7d, 0x01,
        0x00, 0x60, 0x8d, 0x00, 0x00, 0xcc, 0x19, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const BSS_HEADER_32_BIT: [u8; size_of::<Elf32HeaderEntry>()] = [
        0x0f, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x30, 0x97, 0x01,
        0x00, 0x30, 0xa7, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const SYMBOL_TABLE_HEADER_32_BIT: [u8; size_of::<Elf32HeaderEntry>()] = [
        0x14, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x30, 0xa7, 0x00, 0x00, 0xc0, 0x0b, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x79, 0x00,
        0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00,
    ];

    const STRING_TABLE_HEADER_32_BIT: [u8; size_of::<Elf32HeaderEntry>()] = [
        0x1c, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0xf0, 0xb2, 0x00, 0x00, 0x2e, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn test_headers_32bit() {
        let mut header = HeaderEntry::try_from_bytes(
            &NULL_HEADER_32_BIT[..],
            crate::elf::header::Class::Elf32,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(0, header.name_index());
        assert_eq!(SectionEntryType::Null, header.r#type());
        assert_eq!(Flags::empty(), header.flags());
        assert_eq!(0x0, header.address());
        assert_eq!(0x0, header.offset());
        assert_eq!(0, header.size());
        assert_eq!(0, header.link());
        assert_eq!(0, header.info());
        assert_eq!(0, header.address_alignment());
        assert_eq!(0, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &TEXT_HEADER_32_BIT[..],
            crate::elf::header::Class::Elf32,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(1, header.name_index());
        assert_eq!(SectionEntryType::Progbits, header.r#type());
        assert_eq!(
            FlagType::Allocated | FlagType::ExecutableInstructions,
            header.flags()
        );
        assert_eq!(0x10000, header.address());
        assert_eq!(0x1000, header.offset());
        assert_eq!(32093, header.size());
        assert_eq!(0, header.link());
        assert_eq!(0, header.info());
        assert_eq!(0x10, header.address_alignment());
        assert_eq!(0, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &RODATA_HEADER_32_BIT[..],
            crate::elf::header::Class::Elf32,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(7, header.name_index());
        assert_eq!(SectionEntryType::Progbits, header.r#type());
        assert_eq!(
            FlagType::Allocated | FlagType::Merge | FlagType::Strings,
            header.flags()
        );
        assert_eq!(0x17d60, header.address());
        assert_eq!(0x8d60, header.offset());
        assert_eq!(6604, header.size());
        assert_eq!(0, header.link());
        assert_eq!(0, header.info());
        assert_eq!(0x10, header.address_alignment());
        assert_eq!(0, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &BSS_HEADER_32_BIT[..],
            crate::elf::header::Class::Elf32,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(15, header.name_index());
        assert_eq!(SectionEntryType::NoBits, header.r#type());
        assert_eq!(FlagType::Allocated | FlagType::Writeable, header.flags());
        assert_eq!(0x19730, header.address());
        assert_eq!(0xa730, header.offset());
        assert_eq!(1, header.size());
        assert_eq!(0, header.link());
        assert_eq!(0, header.info());
        assert_eq!(0x10, header.address_alignment());
        assert_eq!(0, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &SYMBOL_TABLE_HEADER_32_BIT[..],
            crate::elf::header::Class::Elf32,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(20, header.name_index());
        assert_eq!(SectionEntryType::Symtab, header.r#type());
        assert_eq!(Flags::empty(), header.flags());
        assert_eq!(0x0, header.address());
        assert_eq!(0xa730, header.offset());
        assert_eq!(3008, header.size());
        assert_eq!(6, header.link());
        assert_eq!(121, header.info());
        assert_eq!(0x4, header.address_alignment());
        assert_eq!(16, header.entry_size());

        header = HeaderEntry::try_from_bytes(
            &STRING_TABLE_HEADER_32_BIT[..],
            crate::elf::header::Class::Elf32,
            error::Facility::SectionHeader,
        )
        .unwrap();
        assert_eq!(28, header.name_index());
        assert_eq!(SectionEntryType::Strtab, header.r#type());
        assert_eq!(Flags::empty(), header.flags());
        assert_eq!(0x0, header.address());
        assert_eq!(0xb2f0, header.offset());
        assert_eq!(46, header.size());
        assert_eq!(0, header.link());
        assert_eq!(0, header.info());
        assert_eq!(0x1, header.address_alignment());
        assert_eq!(0, header.entry_size());
    }
}
