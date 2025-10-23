// The sacred scriptures:
// https://wiki.sensi.org/download/doc/ata_edd_11.pdf
// http://www.o3one.org/hwdocs/bios_doc/bios_specs_edd30.pdf

pub mod error;
use core::fmt::Display;

use num_enum::TryFromPrimitive;
use zerocopy::{LE, TryFromBytes, TryReadError, U16, U32, U64};

use crate::{
    error::{InternalError, Kind, try_read_error},
    make_flags,
};

pub const DRIVE_PARAMETERS_BUFFER_SIZE: usize = size_of::<DriveParametersRaw>();

#[derive(TryFromBytes)]
#[repr(C)]
struct DriveParametersRaw {
    buffer_size: U16<LE>,
    information_flags: U16<LE>,
    cylinders: U32<LE>,
    heads: U32<LE>,
    sectors_per_track: U32<LE>,
    sectors: U64<LE>,
    bytes_per_sector: U16<LE>,
    configuration_parameters: U32<LE>,
}

#[derive(TryFromPrimitive, Clone, Copy)]
#[repr(u16)]
pub enum InfoFlagType {
    DmaBoundaryErrorsHandledTransparently = 0x1,
    SuppliedGeometryValid = 0x2,
    Removable = 0x4,
    SupportsWriteWithVerify = 0x8,
    SupportsLineChange = 0x10,
    Lockable = 0x20,
    NoMediaPresent = 0x40,
}

impl Display for InfoFlagType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InfoFlagType::DmaBoundaryErrorsHandledTransparently => {
                write!(f, "DMA_BOUNDARY_ERRORS_HANDLED_TRANSPARENTLY")
            }
            InfoFlagType::SuppliedGeometryValid => write!(f, "SUPPLIED_GEOMETRY_VALID"),
            InfoFlagType::Removable => write!(f, "REMOVABLE"),
            InfoFlagType::SupportsWriteWithVerify => write!(f, "SUPPORTS_WRITE_WITH_VERIFY"),
            InfoFlagType::SupportsLineChange => write!(f, "SUPPORTS_LINE_CHANGE"),
            InfoFlagType::Lockable => write!(f, "LOCKABLE"),
            InfoFlagType::NoMediaPresent => write!(f, "NO_MEDIA_PRESENT"),
        }
    }
}

make_flags!(new_type: InfoFlags, underlying_flag_type: InfoFlagType, repr: u16, bit_skipper: |i| i > 6);

#[derive(Debug)]
pub struct DriveParameters {
    buffer_size: u16,
    information_flags: InfoFlags,
    cylinders: u32,
    heads: u32,
    sectors_per_track: u32,
    sectors: u64,
    bytes_per_sector: u16,
    configuration_parameters: Option<FixedDiskParameterTable>,
}

impl Display for DriveParameters {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "Drive Parameters:")?;
        writeln!(f, "  Buffer Size: {}", self.buffer_size)?;
        writeln!(f, "  Information Flags: {}", self.information_flags)?;
        writeln!(f, "  Cylinders: {}", self.cylinders)?;
        writeln!(f, "  Heads: {}", self.heads)?;
        writeln!(f, "  Sectors per Track: {}", self.sectors_per_track)?;
        writeln!(f, "  Total Sectors: {}", self.sectors)?;
        writeln!(f, "  Bytes per Sector: {}", self.bytes_per_sector)?;
        match &self.configuration_parameters {
            Some(configuration_parameters) => {
                writeln!(f, "{configuration_parameters}")
            }
            None => writeln!(f, "  Configuration Parameters: Not Present"),
        }
    }
}

impl TryFrom<&DriveParametersRaw> for DriveParameters {
    type Error = crate::error::Error;

    fn try_from(value: &DriveParametersRaw) -> Result<Self, Self::Error> {
        use crate::error::Kind::*;
        use crate::error::Reason::*;
        if value.buffer_size.get() != 26 && value.buffer_size.get() != 30 {
            return Err(Self::error(CantReadField(
                "buffer size",
                InvalidValue(value.buffer_size.get().into()),
            )));
        }

        let information_flags: InfoFlags = InfoFlags(value.information_flags.get());
        if information_flags.is_set(InfoFlagType::SuppliedGeometryValid) {
            if value.cylinders.get() == 0 {
                return Err(Self::error(CantReadField(
                    "cylinders",
                    InvalidValue(value.cylinders.get().into()),
                )));
            }
            if value.heads.get() == 0 {
                return Err(Self::error(CantReadField(
                    "heads",
                    InvalidValue(value.heads.get().into()),
                )));
            }
            if value.sectors_per_track.get() == 0 {
                return Err(Self::error(CantReadField(
                    "sectors_per_track",
                    InvalidValue(value.sectors_per_track.get().into()),
                )));
            }
        }

        if value.bytes_per_sector.get() == 0 {
            return Err(Self::error(CantReadField(
                "bytes_per_sector",
                InvalidValue(value.bytes_per_sector.get().into()),
            )));
        }

        if information_flags.is_set(InfoFlagType::Removable) {
            if !information_flags.is_set(InfoFlagType::SupportsLineChange) {
                return Err(Self::error(CantReadField(
                    "information_flags",
                    InvalidValue(value.information_flags.get().into()),
                )));
            }
            if !information_flags.is_set(InfoFlagType::Lockable) {
                return Err(Self::error(CantReadField(
                    "information_flags",
                    InvalidValue(value.information_flags.get().into()),
                )));
            }
        }

        if information_flags.is_set(InfoFlagType::NoMediaPresent)
            && !information_flags.is_set(InfoFlagType::Removable)
        {
            return Err(Self::error(CantReadField(
                "information_flags",
                InvalidValue(value.information_flags.get().into()),
            )));
        }

        Ok(Self {
            buffer_size: value.buffer_size.get(),
            information_flags,
            cylinders: value.cylinders.get(),
            heads: value.heads.get(),
            sectors_per_track: value.sectors_per_track.get(),
            sectors: value.sectors.get(),
            bytes_per_sector: value.bytes_per_sector.get(),
            configuration_parameters: None,
        })
    }
}

impl DriveParameters {
    fn try_read_error<U: TryFromBytes>(err: TryReadError<&[u8], U>) -> crate::error::Error {
        use crate::error::Facility::*;
        use error::Facility::*;
        try_read_error(Edd(DriveParameters), err)
    }

    pub fn from_bytes(bytes: &[u8], resolve_fdpt: bool) -> crate::error::Result<Self> {
        let (drive_parameters_raw, _rest) =
            DriveParametersRaw::try_read_from_prefix(bytes).map_err(Self::try_read_error)?;

        let mut result = Self::try_from(&drive_parameters_raw)?;
        if resolve_fdpt && drive_parameters_raw.configuration_parameters.get() != u32::MAX {
            result.resolve_fdbt(drive_parameters_raw.configuration_parameters.get())?;
        }
        Ok(result)
    }

    pub fn resolve_fdbt(&mut self, mut fdbt_address: u32) -> crate::error::Result<()> {
        use crate::error::Kind::*;

        if fdbt_address == u32::MAX {
            // Nothing to do, the fdbt address is invalid
            return Ok(());
        }

        if self.buffer_size != 30 {
            return Err(Self::error(CantFit("fixed disk parameter table")));
        }
        // Address is in seg:offset format, with offset coming first
        fdbt_address = ((fdbt_address >> 16) * 16) + (fdbt_address & 0xffff);

        self.configuration_parameters = Some(FixedDiskParameterTable::try_from(
            //SAFETY: If we got to this point, the fdbt address is valid and points to a
            //FixedDiskParameterTableRaw sized byte array
            unsafe {
                core::slice::from_raw_parts(
                    fdbt_address as *const u8,
                    size_of::<FixedDiskParameterTableRaw>(),
                )
            },
        )?);
        Ok(())
    }

    fn error(kind: Kind) -> crate::error::Error {
        crate::error::Error::InternalError(InternalError::new(
            crate::error::Facility::Edd(error::Facility::DriveParameters),
            kind,
            crate::error::Context::Parsing,
        ))
    }
}

#[derive(TryFromPrimitive, Clone, Copy)]
#[repr(u8)]
pub enum HeadRegisterFlagType {
    Slave = 0x10,
    LBAEnabled = 0x40,
}

impl Display for HeadRegisterFlagType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            HeadRegisterFlagType::Slave => write!(f, "SLAVE"),
            HeadRegisterFlagType::LBAEnabled => write!(f, "LBA_ENABLED"),
        }
    }
}

make_flags!(new_type: HeadRegisterUpperNibble, underlying_flag_type: HeadRegisterFlagType, repr: u8, bit_skipper: |i| i != 4 && i != 6);

#[derive(Debug)]
pub struct FixedDiskParameterTable {
    io_port_base: u16,
    control_port_base: u16,
    head_prefix: HeadRegisterUpperNibble,
    irq: u8,
    sector_count: u8,
    dma_channel: u8,
    dma_type: u8,
    pio_type: u8,
    hardware_specific_option_flags: HWSpecificOptionFlags,
    extension_revision: u8,
    checksum: u8,
}

impl FixedDiskParameterTable {
    fn try_read_error<U: TryFromBytes>(err: TryReadError<&[u8], U>) -> crate::error::Error {
        use crate::error::Facility::*;
        use error::Facility::*;
        try_read_error(Edd(FixedDiskParameterTable), err)
    }
}

impl TryFrom<&[u8]> for FixedDiskParameterTable {
    type Error = crate::error::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        use crate::error::Kind::*;
        use crate::error::Reason::*;
        let (fixed_disk_parameter_table_raw, _rest) =
            FixedDiskParameterTableRaw::try_read_from_prefix(value)
                .map_err(Self::try_read_error)?;

        let checksum: u8 = value[..size_of::<FixedDiskParameterTableRaw>() - 1]
            .iter()
            .fold(0, |checksum, &byte| checksum.wrapping_add(byte));

        if checksum.wrapping_add(fixed_disk_parameter_table_raw.checksum) != 0 {
            return Err(crate::error::Error::InternalError(InternalError::new(
                crate::error::Facility::Edd(error::Facility::FixedDiskParameterTable),
                CantReadField(
                    "checksum",
                    InvalidValue(fixed_disk_parameter_table_raw.checksum.into()),
                ),
                crate::error::Context::Parsing,
            )));
        }

        Self::try_from(&fixed_disk_parameter_table_raw)
    }
}

impl TryFrom<&FixedDiskParameterTableRaw> for FixedDiskParameterTable {
    type Error = crate::error::Error;

    fn try_from(value: &FixedDiskParameterTableRaw) -> Result<Self, Self::Error> {
        use crate::error::Kind::*;
        use crate::error::Reason::*;

        if value.extension_revision != 0x11 {
            return Err(crate::error::Error::InternalError(InternalError::new(
                crate::error::Facility::Edd(error::Facility::FixedDiskParameterTable),
                CantReadField(
                    "extension_revision",
                    InvalidValue(value.extension_revision.into()),
                ),
                crate::error::Context::Parsing,
            )));
        }

        if value.head_prefix & 0b10001111 != 0b10000000 {
            return Err(crate::error::Error::InternalError(InternalError::new(
                crate::error::Facility::Edd(error::Facility::FixedDiskParameterTable),
                CantReadField("head_prefix", InvalidValue(value.head_prefix.into())),
                crate::error::Context::Parsing,
            )));
        }

        if value.irq & 0xf0 != 0 {
            return Err(crate::error::Error::InternalError(InternalError::new(
                crate::error::Facility::Edd(error::Facility::FixedDiskParameterTable),
                CantReadField("irq", InvalidValue(value.irq.into())),
                crate::error::Context::Parsing,
            )));
        }

        if value.pio_type & 0xf0 != 0 {
            return Err(crate::error::Error::InternalError(InternalError::new(
                crate::error::Facility::Edd(error::Facility::FixedDiskParameterTable),
                CantReadField("pio_type", InvalidValue(value.pio_type.into())),
                crate::error::Context::Parsing,
            )));
        }

        let hw_flags = HWSpecificOptionFlags(value.hardware_specific_option_flags.get());

        if hw_flags.is_set(HWSpecificOptionFlagType::Atapi)
            && !hw_flags.is_set(HWSpecificOptionFlagType::AtapiUsesInterruptDRQ)
        {
            return Err(crate::error::Error::InternalError(InternalError::new(
                crate::error::Facility::Edd(error::Facility::FixedDiskParameterTable),
                CantReadField(
                    "hardware_specific_option_flags",
                    InvalidValue(value.hardware_specific_option_flags.get().into()),
                ),
                crate::error::Context::Parsing,
            )));
        }

        if !hw_flags.is_set(HWSpecificOptionFlagType::CHSTranslation)
            && (hw_flags.is_set(HWSpecificOptionFlagType::TranslationTypeFirstBit)
                || hw_flags.is_set(HWSpecificOptionFlagType::TranslationTypeSecondBit))
        {
            return Err(crate::error::Error::InternalError(InternalError::new(
                crate::error::Facility::Edd(error::Facility::FixedDiskParameterTable),
                CantReadField(
                    "hardware_specific_option_flags",
                    InvalidValue(value.hardware_specific_option_flags.get().into()),
                ),
                crate::error::Context::Parsing,
            )));
        }

        Ok(Self {
            io_port_base: value.io_port_base.get(),
            control_port_base: value.control_port_base.get(),
            head_prefix: HeadRegisterUpperNibble(value.head_prefix),
            irq: value.irq,
            sector_count: value.sector_count,
            dma_channel: value.dma_channel_type & 0xf,
            dma_type: value.dma_channel_type >> 4,
            pio_type: value.pio_type,
            hardware_specific_option_flags: HWSpecificOptionFlags(
                value.hardware_specific_option_flags.get(),
            ),
            extension_revision: value.extension_revision,
            checksum: value.checksum,
        })
    }
}

impl Display for FixedDiskParameterTable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "Fixed Disk Parameter Table:")?;
        writeln!(f, "  I/O Port Base: {:#X}", self.io_port_base)?;
        writeln!(f, "  Control Port Base: {:#X}", self.control_port_base)?;
        writeln!(f, "  Head Prefix: {}", self.head_prefix)?;
        writeln!(f, "  IRQ: {}", self.irq)?;
        writeln!(f, "  Sector Count: {}", self.sector_count)?;
        writeln!(f, "  DMA Channel: {}", self.dma_channel)?;
        writeln!(f, "  DMA Type: {}", self.dma_type)?;
        writeln!(f, "  PIO Type: {}", self.pio_type)?;
        writeln!(
            f,
            "  Hardware Specific Option Flags: {}",
            self.hardware_specific_option_flags
        )?;
        writeln!(f, "  Extension Revision: {}", self.extension_revision)?;
        writeln!(f, "  Checksum: {:#X}", self.checksum)
    }
}

#[derive(TryFromBytes)]
#[repr(C)]
pub struct FixedDiskParameterTableRaw {
    io_port_base: U16<LE>,
    control_port_base: U16<LE>,
    head_prefix: u8,
    internal: u8,
    irq: u8,
    sector_count: u8,
    dma_channel_type: u8,
    pio_type: u8,
    hardware_specific_option_flags: U16<LE>,
    unused: U16<LE>,
    extension_revision: u8,
    checksum: u8,
}

#[derive(TryFromPrimitive, Clone, Copy)]
#[repr(u16)]
pub enum HWSpecificOptionFlagType {
    FastPIO = 0x1,
    FastDMA = 0x2,
    BlockPIO = 0x4,
    CHSTranslation = 0x8,
    LBATranslation = 0x10,
    RemovableMedia = 0x20,
    Atapi = 0x40,
    _32BitTransferMode = 0x80,
    AtapiUsesInterruptDRQ = 0x100,
    TranslationTypeFirstBit = 0x200,
    TranslationTypeSecondBit = 0x400,
}

impl Display for HWSpecificOptionFlagType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            HWSpecificOptionFlagType::FastPIO => write!(f, "FAST_PIO"),
            HWSpecificOptionFlagType::FastDMA => write!(f, "FAST_DMA"),
            HWSpecificOptionFlagType::BlockPIO => write!(f, "BLOCK_PIO"),
            HWSpecificOptionFlagType::CHSTranslation => write!(f, "CHS_TRANSLATION"),
            HWSpecificOptionFlagType::LBATranslation => write!(f, "LBA_TRANSLATION"),
            HWSpecificOptionFlagType::RemovableMedia => write!(f, "REMOVABLE_MEDIA"),
            HWSpecificOptionFlagType::Atapi => write!(f, "ATAPI"),
            HWSpecificOptionFlagType::_32BitTransferMode => write!(f, "32_BIT_TRANSFER_MODE"),
            HWSpecificOptionFlagType::AtapiUsesInterruptDRQ => {
                write!(f, "ATAPI_USES_INTERRUPT_DRQ")
            }
            HWSpecificOptionFlagType::TranslationTypeFirstBit => {
                write!(f, "TRANSLATION_TYPE_FIRST_BIT")
            }
            HWSpecificOptionFlagType::TranslationTypeSecondBit => {
                write!(f, "TRANSLATION_TYPE_SECOND_BIT")
            }
        }
    }
}

make_flags!(new_type: HWSpecificOptionFlags, underlying_flag_type: HWSpecificOptionFlagType, repr: u16, bit_skipper: |i| i > 10);
