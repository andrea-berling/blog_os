use crate::elf::{
    Halfword,
    error::{self, Facility},
    header,
};

use crate::error::{Kind, Reason, try_read_error};

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

#[cfg_attr(test, derive(PartialEq, Eq))]
#[derive(TryFromPrimitive, Clone, Copy)]
#[repr(u8)]
pub enum PermissionFlag {
    Executable = 0x1,
    Writable = 0x2,
    Readable = 0x4,
}

impl core::fmt::Display for PermissionFlag {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PermissionFlag::Executable => write!(f, "EXECUTABLE"),
            PermissionFlag::Writable => write!(f, "WRITABLE"),
            PermissionFlag::Readable => write!(f, "READABLE"),
        }
    }
}

make_flags!(new_type: Permissions, underlying_flag_type: PermissionFlag, repr: u8, bit_skipper: |i| i > 2);

pub const ELF32_ENTRY_SIZE: usize = size_of::<inner::Elf32HeaderEntry>();
pub const ELF64_ENTRY_SIZE: usize = size_of::<inner::Elf64HeaderEntry>();

#[derive(Debug)]
pub enum HeaderEntry {
    Elf32(inner::Elf32HeaderEntry),
    Elf64(inner::Elf64HeaderEntry),
}

impl HeaderEntry {
    fn error(kind: Kind, facility: Facility) -> crate::error::Error {
        crate::error::Error::InternalError(crate::error::InternalError::new(
            crate::error::Facility::Elf(facility),
            kind,
            crate::error::Context::Parsing,
        ))
    }

    pub fn try_from_bytes(
        bytes: &[u8],
        class: header::Class,
        facility: Facility,
    ) -> crate::error::Result<Self> {
        match class {
            header::Class::Elf32 => inner::Elf32HeaderEntry::try_read_from_prefix(bytes)
                .map_err(|err| try_read_error(crate::error::Facility::Elf(facility), err))
                .and_then(|(header_entry, _rest)| {
                    let type_halfword = header_entry.r#type.get();

                    if let Err(err) = ProgramHeaderEntryType::try_from(type_halfword) {
                        return Err(Self::error(CantReadField("type", err), facility));
                    }

                    let flags_word = header_entry.flags.get();

                    if flags_word > 7 {
                        return Err(Self::error(
                            CantReadField("type", Reason::InvalidValue(flags_word.into())),
                            facility,
                        ));
                    }

                    Ok(header_entry)
                })
                .map(HeaderEntry::Elf32),

            header::Class::Elf64 => inner::Elf64HeaderEntry::try_read_from_prefix(bytes)
                .map_err(|err| try_read_error(crate::error::Facility::Elf(facility), err))
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

    pub fn segment_size_on_file(&self) -> u64 {
        match self {
            HeaderEntry::Elf32(elf32_header_entry) => {
                elf32_header_entry.segment_size_on_file.get() as u64
            }
            HeaderEntry::Elf64(elf64_header_entry) => elf64_header_entry.segment_size_on_file.get(),
        }
    }

    pub fn segment_size_in_memory(&self) -> u64 {
        match self {
            HeaderEntry::Elf32(elf32_header_entry) => {
                elf32_header_entry.segment_size_in_memory.get() as u64
            }
            HeaderEntry::Elf64(elf64_header_entry) => {
                elf64_header_entry.segment_size_in_memory.get()
            }
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

    pub fn permissions(&self) -> Permissions {
        Permissions(match self {
            HeaderEntry::Elf32(elf32_header_entry) => elf32_header_entry.flags.get() as u8,
            HeaderEntry::Elf64(elf64_header_entry) => elf64_header_entry.flags.get() as u8,
        })
    }

    pub fn write_to<W: core::fmt::Write>(&self, writer: &mut W) -> crate::error::Result<()> {
        writeln!(writer, "Type: {}", self.r#type())?;
        writeln!(writer, "Offset: {:#x}", self.offset())?;
        writeln!(writer, "Virtual Address: {:#x}", self.virtual_address())?;
        writeln!(writer, "Physical Address: {:#x}", self.physical_address())?;
        writeln!(writer, "Size on file: {}", self.on_file_size())?;
        writeln!(writer, "Size in memory: {}", self.in_memory_size())?;
        writeln!(writer, "Address Alignment: {:#x}", self.address_alignment())?;
        writeln!(writer, "Permissions: {}", self.permissions())?;
        Ok(())
    }
}

#[cfg_attr(test, derive(PartialEq, Eq))]
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
use crate::error::Kind::*;
use common::make_flags;
use num_enum::TryFromPrimitive;
use zerocopy::TryFromBytes as _;

impl<'a> ProgramHeaderEntries<'a> {
    fn error(kind: crate::error::Kind) -> crate::error::Error {
        crate::error::Error::InternalError(crate::error::InternalError::new(
            crate::error::Facility::Elf(error::Facility::ProgramHeader),
            kind,
            crate::error::Context::Parsing,
        ))
    }

    pub(crate) fn new(
        bytes: &'a [u8],
        class: header::Class,
        n_entries: Halfword,
    ) -> crate::error::Result<Self> {
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
    type Item = crate::error::Result<HeaderEntry>;

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

#[cfg(test)]
mod tests {
    use crate::elf::{
        self,
        program_header::{
            HeaderEntry, PermissionFlag, Permissions, ProgramHeaderEntryType,
            inner::{Elf32HeaderEntry, Elf64HeaderEntry},
        },
    };

    const PHDR_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x06, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xa0, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xa0, 0x02, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const INTERPRETER_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x03, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xe0, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0xe0, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xe0, 0x02, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x1c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const PT_LOAD_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x01, 0x00, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x1c, 0x02, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x2c, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2c, 0x02, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x40, 0xed, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0xed, 0x05, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const TLS_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x07, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x40, 0x09, 0x08, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x40, 0x29, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x29, 0x08, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x50, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const DYNAMIC_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x02, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x80, 0x49, 0x08, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x80, 0x69, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x69, 0x08, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xd0, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xd0, 0x01, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const PROCESSOR_SPECIFIC_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x52, 0xe5, 0x74, 0x64, 0x04, 0x00, 0x00, 0x00, 0x40, 0x09, 0x08, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x40, 0x29, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x29, 0x08, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x10, 0x4c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0, 0x56, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    const NOTE_HEADER_64_BIT: [u8; size_of::<Elf64HeaderEntry>()] = [
        0x04, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xfc, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0xfc, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xfc, 0x02, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x44, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x44, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    #[test]
    fn test_headers_64bit() {
        let mut header = HeaderEntry::try_from_bytes(
            &PHDR_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            elf::error::Facility::ProgramHeader,
        )
        .unwrap();
        assert_eq!(ProgramHeaderEntryType::ProgramHeader, header.r#type());
        assert_eq!(0x40, header.offset());
        assert_eq!(0x40, header.virtual_address());
        assert_eq!(0x40, header.physical_address());
        assert_eq!(672, header.segment_size_on_file());
        assert_eq!(672, header.segment_size_in_memory());
        assert_eq!(0x8, header.address_alignment());
        assert_eq!(
            Permissions::from(PermissionFlag::Readable),
            header.permissions()
        );

        header = HeaderEntry::try_from_bytes(
            &INTERPRETER_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            elf::error::Facility::ProgramHeader,
        )
        .unwrap();
        assert_eq!(ProgramHeaderEntryType::Interpreter, header.r#type());
        assert_eq!(0x2e0, header.offset());
        assert_eq!(0x2e0, header.virtual_address());
        assert_eq!(0x2e0, header.physical_address());
        assert_eq!(28, header.segment_size_on_file());
        assert_eq!(28, header.segment_size_in_memory());
        assert_eq!(0x1, header.address_alignment());
        assert_eq!(
            Permissions::from(PermissionFlag::Readable),
            header.permissions()
        );

        header = HeaderEntry::try_from_bytes(
            &PT_LOAD_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            elf::error::Facility::ProgramHeader,
        )
        .unwrap();
        assert_eq!(ProgramHeaderEntryType::Load, header.r#type());
        assert_eq!(0x21c00, header.offset());
        assert_eq!(0x22c00, header.virtual_address());
        assert_eq!(0x22c00, header.physical_address());
        assert_eq!(388416, header.segment_size_on_file());
        assert_eq!(388416, header.segment_size_in_memory());
        assert_eq!(0x1000, header.address_alignment());
        assert_eq!(
            PermissionFlag::Readable | PermissionFlag::Executable,
            header.permissions()
        );

        header = HeaderEntry::try_from_bytes(
            &TLS_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            elf::error::Facility::ProgramHeader,
        )
        .unwrap();
        assert_eq!(ProgramHeaderEntryType::ThreadLocalStorage, header.r#type());
        assert_eq!(0x80940, header.offset());
        assert_eq!(0x82940, header.virtual_address());
        assert_eq!(0x82940, header.physical_address());
        assert_eq!(32, header.segment_size_on_file());
        assert_eq!(80, header.segment_size_in_memory());
        assert_eq!(0x8, header.address_alignment());
        assert_eq!(
            Permissions::from(PermissionFlag::Readable),
            header.permissions()
        );

        header = HeaderEntry::try_from_bytes(
            &DYNAMIC_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            elf::error::Facility::ProgramHeader,
        )
        .unwrap();
        assert_eq!(ProgramHeaderEntryType::Dynamic, header.r#type());
        assert_eq!(0x84980, header.offset());
        assert_eq!(0x86980, header.virtual_address());
        assert_eq!(0x86980, header.physical_address());
        assert_eq!(464, header.segment_size_on_file());
        assert_eq!(464, header.segment_size_in_memory());
        assert_eq!(0x8, header.address_alignment());
        assert_eq!(
            PermissionFlag::Writable | PermissionFlag::Readable,
            header.permissions()
        );

        header = HeaderEntry::try_from_bytes(
            &PROCESSOR_SPECIFIC_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            elf::error::Facility::ProgramHeader,
        )
        .unwrap();
        assert_eq!(
            ProgramHeaderEntryType::ProcessorSpecific(0x6474e552),
            header.r#type()
        );
        assert_eq!(0x80940, header.offset());
        assert_eq!(0x82940, header.virtual_address());
        assert_eq!(0x82940, header.physical_address());
        assert_eq!(19472, header.segment_size_on_file());
        assert_eq!(22208, header.segment_size_in_memory());
        assert_eq!(0x1, header.address_alignment());
        assert_eq!(
            Permissions::from(PermissionFlag::Readable),
            header.permissions()
        );

        header = HeaderEntry::try_from_bytes(
            &NOTE_HEADER_64_BIT[..],
            crate::elf::header::Class::Elf64,
            elf::error::Facility::ProgramHeader,
        )
        .unwrap();
        assert_eq!(ProgramHeaderEntryType::Note, header.r#type());
        assert_eq!(0x2fc, header.offset());
        assert_eq!(0x2fc, header.virtual_address());
        assert_eq!(0x2fc, header.physical_address());
        assert_eq!(68, header.segment_size_on_file());
        assert_eq!(68, header.segment_size_in_memory());
        assert_eq!(0x4, header.address_alignment());
        assert_eq!(
            Permissions::from(PermissionFlag::Readable),
            header.permissions()
        )
    }

    const PT_LOAD_HEADER_32_BIT: [u8; size_of::<Elf32HeaderEntry>()] = [
        0x01, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
        0x00, 0x5d, 0x7d, 0x00, 0x00, 0x5d, 0x7d, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x10,
        0x00, 0x00,
    ];

    const PROCESSOR_SPECIFIC_HEADER_32_BIT: [u8; size_of::<Elf32HeaderEntry>()] = [
        0x51, 0xe5, 0x74, 0x64, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00,
    ];

    #[test]
    fn test_headers_32bit() {
        let mut header = HeaderEntry::try_from_bytes(
            &PT_LOAD_HEADER_32_BIT[..],
            crate::elf::header::Class::Elf32,
            elf::error::Facility::ProgramHeader,
        )
        .unwrap();
        assert_eq!(ProgramHeaderEntryType::Load, header.r#type());
        assert_eq!(0x1000, header.offset());
        assert_eq!(0x10000, header.virtual_address());
        assert_eq!(0x10000, header.physical_address());
        assert_eq!(32093, header.segment_size_on_file());
        assert_eq!(32093, header.segment_size_in_memory());
        assert_eq!(0x1000, header.address_alignment());
        assert_eq!(
            PermissionFlag::Readable | PermissionFlag::Executable,
            header.permissions()
        );

        header = HeaderEntry::try_from_bytes(
            &PROCESSOR_SPECIFIC_HEADER_32_BIT[..],
            crate::elf::header::Class::Elf32,
            elf::error::Facility::ProgramHeader,
        )
        .unwrap();
        assert_eq!(
            ProgramHeaderEntryType::ProcessorSpecific(0x6474e551),
            header.r#type()
        );
        assert_eq!(0x0, header.offset());
        assert_eq!(0x0, header.virtual_address());
        assert_eq!(0x0, header.physical_address());
        assert_eq!(0, header.segment_size_on_file());
        assert_eq!(0, header.segment_size_in_memory());
        assert_eq!(0x0, header.address_alignment());
        assert_eq!(
            PermissionFlag::Writable | PermissionFlag::Readable,
            header.permissions()
        );
    }
}
