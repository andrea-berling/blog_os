use core::fmt::Display;
use core::fmt::Write;

use crate::elf;
use crate::elf::error::Facility;
use crate::elf::program_header;
use crate::elf::section;
use common::error::Context;
use common::error::InternalError;
use common::error::Kind;
use common::error::Kind::*;
use common::error::Reason;
use common::error::Reason::*;
use common::error::try_read_error;
use elf::error::Error;
use num_enum::TryFromPrimitive;
use num_traits::AsPrimitive;
use num_traits::PrimInt;
use zerocopy::TryFromBytes;
use zerocopy::TryReadError;

use super::Halfword;

mod inner {
    use zerocopy::{LE, TryFromBytes, U16, U32, U64};

    use crate::elf::header::ElfIdentifier;

    pub(super) const HEADER_SIZE: [usize; 3] =
        [0, size_of::<Elf32Header>(), size_of::<Elf64Header>()];

    #[cfg_attr(test, derive(Default, PartialEq, Eq))]
    #[derive(Debug, TryFromBytes)]
    pub(super) struct Elf32Header {
        pub(super) identifier: ElfIdentifier,
        pub(super) r#type: U16<LE>,
        pub(super) machine: U16<LE>,
        pub(super) version: U32<LE>,
        pub(super) entrypoint: U32<LE>,
        pub(super) program_header_offset: U32<LE>,
        pub(super) section_header_offset: U32<LE>,
        pub(super) flags: U32<LE>,
        pub(super) size: U16<LE>,
        pub(super) program_header_entry_size: U16<LE>,
        pub(super) program_header_entries: U16<LE>,
        pub(super) section_header_entry_size: U16<LE>,
        pub(super) section_header_entries: U16<LE>,
        pub(super) string_table_index: U16<LE>,
    }

    #[cfg_attr(test, derive(Default, PartialEq, Eq))]
    #[derive(Debug, TryFromBytes)]
    pub(super) struct Elf64Header {
        pub(super) identifier: ElfIdentifier,
        pub(super) r#type: U16<LE>,
        pub(super) machine: U16<LE>,
        pub(super) version: U32<LE>,
        pub(super) entrypoint: U64<LE>,
        pub(super) program_header_offset: U64<LE>,
        pub(super) section_header_offset: U64<LE>,
        pub(super) flags: U32<LE>,
        pub(super) size: U16<LE>,
        pub(super) program_header_entry_size: U16<LE>,
        pub(super) program_header_entries: U16<LE>,
        pub(super) section_header_entry_size: U16<LE>,
        pub(super) section_header_entries: U16<LE>,
        pub(super) string_table_index: U16<LE>,
    }
}

#[cfg_attr(test, derive(Default))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromBytes)]
#[repr(u8)]
#[allow(unused)]
pub(crate) enum Encoding {
    #[cfg_attr(test, default)]
    LittleEndian = 1,
    BigEndian = 2,
}

impl Display for Encoding {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Encoding::LittleEndian => write!(f, "Little Endian"),
            Encoding::BigEndian => write!(f, "Big Endian"),
        }
    }
}

#[cfg_attr(test, derive(Default, PartialEq, Eq))]
#[derive(Debug, Clone, Copy, TryFromBytes, TryFromPrimitive)]
#[repr(u8)]
pub(crate) enum Class {
    #[cfg_attr(test, default)]
    Elf32 = 1,
    Elf64 = 2,
}

impl Display for Class {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Class::Elf32 => write!(f, "ELF32"),
            Class::Elf64 => write!(f, "ELF64"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
enum Version {
    Invalid,
    Current,
}

impl<T: AsPrimitive<u32> + PrimInt> From<T> for Version {
    fn from(value: T) -> Self {
        match value.as_() {
            1 => Self::Current,
            _ => Self::Invalid,
        }
    }
}

impl Display for Version {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Version::Invalid => write!(f, "Invalid"),
            Version::Current => write!(f, "Current"),
        }
    }
}

#[derive(Debug)]
#[allow(unused)]
pub enum ObjectType {
    None,
    Relocatable,
    Executable,
    Dynamic,
    Core,
    LoOS = 0xfe00,
    HiOS = 0xfeff,
    LoProc = 0xff00,
    HiProc = 0xffff,
}

impl Display for ObjectType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ObjectType::None => write!(f, "NONE (No file type)"),
            ObjectType::Relocatable => write!(f, "REL (Relocatable file)"),
            ObjectType::Executable => write!(f, "EXEC (Executable file)"),
            ObjectType::Dynamic => write!(f, "DYN (Shared object file)"),
            ObjectType::Core => write!(f, "CORE (Core file)"),
            ObjectType::LoOS => write!(f, "LOOS (Operating system-specific)"),
            ObjectType::HiOS => write!(f, "HIOS (Operating system-specific)"),
            ObjectType::LoProc => write!(f, "LOPROC (Processor-specific)"),
            ObjectType::HiProc => write!(f, "HIPROC (Processor-specific)"),
        }
    }
}

impl TryFrom<Halfword> for ObjectType {
    type Error = Reason;

    fn try_from(value: Halfword) -> core::result::Result<Self, Self::Error> {
        match value {
            0 => Ok(ObjectType::None),
            1 => Ok(ObjectType::Relocatable),
            2 => Ok(ObjectType::Executable),
            3 => Ok(ObjectType::Dynamic),
            4 => Ok(ObjectType::Core),
            0xfe00..=0xfeff => Ok(ObjectType::LoOS),
            0xff00..=0xffff => Ok(ObjectType::LoProc),
            _ => Err(Reason::InvalidValue(value.into())),
        }
    }
}

#[cfg_attr(test, derive(Default, PartialEq, Eq))]
#[derive(Debug, TryFromBytes)]
#[repr(C)]
struct ElfIdentifier {
    magic: [u8; 4],
    class: Class,
    encoding: Encoding,
    version: u8,
    os_abi: u8,
    os_abiversion: u8,
    os_pad: [u8; 6],
    nident: u8,
}

#[derive(Debug)]
#[repr(u16)]
#[allow(unused)]
enum Machine {
    None = 0,
    M32 = 1,
    Sparc = 2,
    I386 = 3,
    M68K = 4,
    M88K = 5,
    I860 = 7,
    Mips = 8,
    S370 = 9,
    MipsRs3Le = 10,
    Parisc = 15,
    VPP500 = 17,
    SPARC32PLUS = 18,
    I960 = 19,
    Ppc = 20,
    PPC64 = 21,
    S390 = 22,
    V800 = 36,
    FR20 = 37,
    RH32 = 38,
    Rce = 39,
    Arm = 40,
    Alpha = 41,
    SH = 42,
    SPARCV9 = 43,
    Tricore = 44,
    Arc = 45,
    H8_300 = 46,
    H8_300H = 47,
    H8S = 48,
    H8_500 = 49,
    Ia64 = 50,
    MipsX = 51,
    Coldfire = 52,
    M68HC12 = 53,
    Mma = 54,
    Pcp = 55,
    Ncpu = 56,
    NDR1 = 57,
    Starcore = 58,
    ME16 = 59,
    ST100 = 60,
    Tinyj = 61,
    X86_64 = 62,
    Pdsp = 63,
    PDP10 = 64,
    PDP11 = 65,
    FX66 = 66,
    ST9PLUS = 67,
    ST7 = 68,
    M68HC16 = 69,
    M68HC11 = 70,
    M68HC08 = 71,
    M68HC05 = 72,
    Svx = 73,
    ST19 = 74,
    Vax = 75,
    Cris = 76,
    Javelin = 77,
    Firepath = 78,
    Zsp = 79,
    Mmix = 80,
    Huany = 81,
    Prism = 82,
    Avr = 83,
    FR30 = 84,
    D10V = 85,
    D30V = 86,
    V850 = 87,
    M32R = 88,
    MN10300 = 89,
    MN10200 = 90,
    PJ = 91,
    Openrisc = 92,
    ArcA5 = 93,
    Xtensa = 94,
    Videocore = 95,
    TmmGpp = 96,
    NS32K = 97,
    Tpc = 98,
    SNP1K = 99,
    ST200 = 100,
}

#[cfg_attr(test, derive(PartialEq, Eq, Debug))]
pub enum Header {
    Elf32(inner::Elf32Header),
    Elf64(inner::Elf64Header),
}

impl TryFrom<&[u8]> for Header {
    type Error = Error;

    fn try_from(bytes: &[u8]) -> core::result::Result<Header, Self::Error> {
        let (elf_identifier, _rest) =
            ElfIdentifier::try_read_from_prefix(bytes).map_err(Self::try_read_error)?;

        if elf_identifier.magic != *b"\x7fELF" {
            return Err(Self::error(CantReadField(
                "magic",
                InvalidValue(u32::from_be_bytes(elf_identifier.magic) as u64),
            )));
        }

        if elf_identifier.encoding != Encoding::LittleEndian {
            return Err(Self::error(Generic(UnsupportedEndianness)));
        }

        let elf_header = match elf_identifier.class {
            Class::Elf32 => Header::Elf32(
                inner::Elf32Header::try_read_from_prefix(bytes)
                    .map_err(Self::try_read_error)?
                    .0,
            ),
            Class::Elf64 => Header::Elf64(
                inner::Elf64Header::try_read_from_prefix(bytes)
                    .map_err(Self::try_read_error)?
                    .0,
            ),
        };

        let type_halfword = match &elf_header {
            Header::Elf32(elf32_header) => elf32_header.r#type.get(),
            Header::Elf64(elf64_header) => elf64_header.r#type.get(),
        };

        let _ = ObjectType::try_from(type_halfword)
            .map_err(|err| Self::error(CantReadField("type", err)))?;

        if elf_identifier.encoding != Encoding::LittleEndian {
            return Err(Self::error(Generic(UnsupportedEndianness)));
        }

        if elf_header.version() != Version::Current {
            return Err(Self::error(CantReadField(
                "version",
                Reason::InvalidValue(elf_header.version() as u64),
            )));
        }

        if elf_header.size() != inner::HEADER_SIZE[elf_header.class() as usize] as Halfword {
            return Err(Self::error(CantReadField(
                "size",
                Reason::InvalidValue(elf_header.size().into()),
            )));
        }

        if elf_header.program_header_entry_size() as usize
            != (match elf_identifier.class {
                Class::Elf32 => program_header::ELF32_ENTRY_SIZE,
                Class::Elf64 => program_header::ELF64_ENTRY_SIZE,
            })
        {
            return Err(Self::error(CantReadField(
                "phentsize",
                InvalidValue(elf_header.section_header_entry_size().into()),
            )));
        }

        if elf_header.section_header_entry_size() as usize
            != (match elf_identifier.class {
                Class::Elf32 => section::ELF32_ENTRY_SIZE,
                Class::Elf64 => section::ELF64_ENTRY_SIZE,
            })
        {
            return Err(Self::error(CantReadField(
                "shentsize",
                InvalidValue(elf_header.section_header_entry_size().into()),
            )));
        }

        Ok(elf_header)
    }
}

impl Header {
    fn try_read_error<U: TryFromBytes>(err: TryReadError<&[u8], U>) -> Error {
        try_read_error(Facility::Header, err)
    }

    fn error(kind: Kind) -> Error {
        Error::InternalError(InternalError::new(Facility::Header, kind, Context::Parsing))
    }

    fn size(&self) -> Halfword {
        match &self {
            Header::Elf32(elf32_header) => elf32_header.size.get(),
            Header::Elf64(elf64_header) => elf64_header.size.get(),
        }
    }

    pub fn class(&self) -> Class {
        match &self {
            Header::Elf32(elf32_header) => elf32_header.identifier.class,
            Header::Elf64(elf64_header) => elf64_header.identifier.class,
        }
    }

    pub fn program_header_offset(&self) -> u64 {
        match &self {
            Header::Elf32(elf32_header) => elf32_header.program_header_offset.get().into(),
            Header::Elf64(elf64_header) => elf64_header.program_header_offset.get(),
        }
    }

    pub fn program_header_entry_size(&self) -> Halfword {
        match &self {
            Header::Elf32(elf32_header) => elf32_header.program_header_entry_size.get(),
            Header::Elf64(elf64_header) => elf64_header.program_header_entry_size.get(),
        }
    }

    pub fn program_header_entries(&self) -> Halfword {
        match &self {
            Header::Elf32(elf32_header) => elf32_header.program_header_entries.get(),
            Header::Elf64(elf64_header) => elf64_header.program_header_entries.get(),
        }
    }

    pub fn section_header_offset(&self) -> u64 {
        match &self {
            Header::Elf32(elf32_header) => elf32_header.section_header_offset.get().into(),
            Header::Elf64(elf64_header) => elf64_header.section_header_offset.get(),
        }
    }

    pub fn section_header_entry_size(&self) -> Halfword {
        match &self {
            Header::Elf32(elf32_header) => elf32_header.section_header_entry_size.get(),
            Header::Elf64(elf64_header) => elf64_header.section_header_entry_size.get(),
        }
    }

    pub fn section_header_entries(&self) -> Halfword {
        match &self {
            Header::Elf32(elf32_header) => elf32_header.section_header_entries.get(),
            Header::Elf64(elf64_header) => elf64_header.section_header_entries.get(),
        }
    }

    fn magic(&self) -> [u8; 4] {
        match &self {
            Header::Elf32(elf32_header) => elf32_header.identifier.magic,
            Header::Elf64(elf64_header) => elf64_header.identifier.magic,
        }
    }

    fn encoding(&self) -> Encoding {
        match &self {
            Header::Elf32(elf32_header) => elf32_header.identifier.encoding,
            Header::Elf64(elf64_header) => elf64_header.identifier.encoding,
        }
    }

    fn version(&self) -> Version {
        match &self {
            Header::Elf32(elf32_header) => elf32_header.version.get().into(),
            Header::Elf64(elf64_header) => elf64_header.version.get().into(),
        }
    }

    fn entrypoint(&self) -> u64 {
        match &self {
            Header::Elf32(elf32_header) => elf32_header.entrypoint.get() as u64,
            Header::Elf64(elf64_header) => elf64_header.entrypoint.get(),
        }
    }

    pub fn string_table_index(&self) -> Halfword {
        match &self {
            Header::Elf32(elf32_header) => elf32_header.string_table_index.get(),
            Header::Elf64(elf64_header) => elf64_header.string_table_index.get(),
        }
    }

    pub fn r#type(&self) -> ObjectType {
        // PANIC: shouldn't panic for we checked the type in the new method
        match &self {
            Header::Elf32(elf32_header) => elf32_header.r#type.get().try_into().unwrap(),
            Header::Elf64(elf64_header) => elf64_header.r#type.get().try_into().unwrap(),
        }
    }

    /// Print out the header using the given writer
    /// String formatting is considered infallible,
    pub fn write_to<W: Write>(&self, writer: &mut W) -> common::error::Result<(), Facility> {
        let magic = self.magic();

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
        writeln!(writer, "Class: {}", self.class())?;
        writeln!(writer, "Data Encoding: {}", self.encoding())?;
        writeln!(writer, "File Version: {}", self.version())?;
        writeln!(writer, "File type: {}", self.r#type())?;
        writeln!(writer, "Entrypoint: {:#x}", self.entrypoint())?;
        writeln!(writer, "Header size: {}", self.size())?;

        writeln!(
            writer,
            "Program header offset: {}",
            self.program_header_offset()
        )?;
        writeln!(
            writer,
            "Program header entries: {}",
            self.program_header_entries()
        )?;
        writeln!(
            writer,
            "Program header entry size: {}",
            self.program_header_entry_size()
        )?;

        writeln!(
            writer,
            "Section header offset: {}",
            self.section_header_offset()
        )?;
        writeln!(
            writer,
            "Section header entries: {}",
            self.section_header_entries()
        )?;
        writeln!(
            writer,
            "Section header entry size: {}",
            self.section_header_entry_size()
        )?;

        writeln!(writer, "String table index: {}", self.string_table_index())?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use zerocopy::{U16, U32, U64};

    use crate::elf::header::{
        ElfIdentifier, Header, Machine, ObjectType, Version,
        inner::{Elf32Header, Elf64Header},
    };

    const _32_BIT_BOOTLOADER_HEADER: [u8; size_of::<Elf32Header>()] = [
        0x7f, 0x45, 0x4c, 0x46, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x02, 0x00, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x34, 0x00,
        0x00, 0x00, 0x08, 0xe4, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x34, 0x00, 0x20, 0x00, 0x04,
        0x00, 0x28, 0x00, 0x07, 0x00, 0x05, 0x00,
    ];

    const _64_BIT_HEADER: [u8; size_of::<Elf64Header>()] = [
        0x7f, 0x45, 0x4c, 0x46, 0x02, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x03, 0x00, 0x3e, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x2c, 0x02, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0, 0xfd, 0x51, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x38, 0x00, 0x0c, 0x00, 0x40, 0x00,
        0x2d, 0x00, 0x2b, 0x00,
    ];

    #[test]
    fn test_header() {
        let header = Header::try_from(&_32_BIT_BOOTLOADER_HEADER[..]).unwrap();
        assert_eq!(
            Header::Elf32(Elf32Header {
                identifier: ElfIdentifier {
                    magic: *b"\x7fELF",
                    class: crate::elf::header::Class::Elf32,
                    encoding: crate::elf::header::Encoding::LittleEndian,
                    version: 1,
                    os_abi: 0,
                    os_abiversion: 0,
                    os_pad: [0, 0, 0, 0, 0, 0],
                    nident: 0
                },
                r#type: U16::new(ObjectType::Executable as u16),
                machine: U16::new(Machine::I386 as u16),
                version: U32::new(Version::Current as u32),
                entrypoint: U32::new(0x10000),
                program_header_offset: U32::new(52),
                section_header_offset: U32::new(58376),
                flags: U32::new(0),
                size: U16::new(52),
                program_header_entry_size: U16::new(32),
                program_header_entries: U16::new(4),
                section_header_entry_size: U16::new(40),
                section_header_entries: U16::new(7),
                string_table_index: U16::new(5)
            }),
            header
        );

        let header = Header::try_from(&_64_BIT_HEADER[..]).unwrap();
        assert_eq!(
            Header::Elf64(Elf64Header {
                identifier: ElfIdentifier {
                    magic: *b"\x7fELF",
                    class: crate::elf::header::Class::Elf64,
                    encoding: crate::elf::header::Encoding::LittleEndian,
                    version: 1,
                    os_abi: 0,
                    os_abiversion: 0,
                    os_pad: [0, 0, 0, 0, 0, 0],
                    nident: 0
                },
                r#type: U16::new(ObjectType::Dynamic as u16),
                machine: U16::new(Machine::X86_64 as u16),
                version: U32::new(Version::Current as u32),
                entrypoint: U64::new(142336),
                program_header_offset: U64::new(64),
                section_header_offset: U64::new(5373376),
                flags: U32::new(0),
                size: U16::new(64),
                program_header_entry_size: U16::new(56),
                program_header_entries: U16::new(12),
                section_header_entry_size: U16::new(64),
                section_header_entries: U16::new(45),
                string_table_index: U16::new(43)
            }),
            header
        );
    }
}
