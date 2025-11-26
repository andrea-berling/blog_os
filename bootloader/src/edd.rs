// The sacred scriptures:
// https://wiki.sensi.org/download/doc/ata_edd_11.pdf
// http://www.o3one.org/hwdocs/bios_doc/bios_specs_edd30.pdf
use core::fmt::Display;

use common::error::{Error, Facility, Fault};
use common::make_bitmap;

use common::error::try_read_error;
use num_enum::TryFromPrimitive;
use zerocopy::{LE, TryFromBytes, TryReadError, U16, U32, U64};

pub const DRIVE_PARAMETERS_BUFFER_SIZE: usize =
    size_of::<DriveParametersRaw>() + size_of::<DevicePathInformationRaw>();

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

#[derive(TryFromBytes)]
#[repr(C)]
struct DevicePathInformationRaw {
    bedd: U16<LE>,
    length: u8,
    reserved_1: u8,
    reserved_2: U16<LE>,
    host_bus_type: [u8; 4],
    interface_type: [u8; 8],
    interface_path: U64<LE>,
    device_path: U64<LE>,
    reserved_3: u8,
    checksum: u8,
}

#[cfg_attr(test, derive(PartialEq, Eq))]
#[derive(Debug)]
pub enum HostBus {
    Pci { bus: u8, slot: u8, function: u8 },
    Isa { base_address: u16 },
}

impl Display for HostBus {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match &self {
            HostBus::Pci {
                bus,
                slot,
                function,
            } => writeln!(
                f,
                "  Host Bus: PCI (Bus: {}, Slot: {}, Function: {})",
                bus, slot, function
            ),
            HostBus::Isa { base_address } => {
                writeln!(f, "  Host Bus: ISA (Base Address: {:#X})", base_address)
            }
        }
    }
}

#[cfg_attr(test, derive(PartialEq, Eq))]
#[derive(Debug)]
pub enum Interface {
    Ata {
        is_slave: bool,
    },
    Atapi {
        is_slave: bool,
        logical_unit_number: u8,
    },
    Scsi {
        logical_unit_number: u8,
    },
    Usb {
        tbd: u8,
    },
    _1394 {
        guid: u64,
    },
    Fibre {
        wwn: u8,
    },
}

impl Display for Interface {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match &self {
            Interface::Ata { is_slave } => {
                writeln!(f, "  Interface: ATA (Is Slave: {})", is_slave)
            }
            Interface::Atapi {
                is_slave,
                logical_unit_number,
            } => writeln!(
                f,
                "  Interface: ATAPI (Is Slave: {}, LUN: {})",
                is_slave, logical_unit_number
            ),
            Interface::Scsi {
                logical_unit_number,
            } => writeln!(f, "  Interface: SCSI (LUN: {})", logical_unit_number),
            Interface::Usb { tbd } => writeln!(f, "  Interface: USB (TBD: {})", tbd),
            Interface::_1394 { guid } => writeln!(f, "  Interface: 1394 (GUID: {:#X})", guid),
            Interface::Fibre { wwn } => writeln!(f, "  Interface: FIBRE (WWN: {:#X})", wwn),
        }
    }
}

#[cfg_attr(test, derive(PartialEq, Eq))]
#[derive(Debug)]
pub struct DevicePathInformation {
    host_bus: HostBus,
    interface: Interface,
}

impl Display for DevicePathInformation {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "Device Path Information:")?;
        write!(f, "{}", self.host_bus)?;
        write!(f, "{}", self.interface)
    }
}

impl TryFrom<&[u8]> for DevicePathInformation {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let (device_path_information_raw, _rest) =
            DevicePathInformationRaw::try_read_from_prefix(value)
                .map_err(|err| try_read_error(Facility::EDDDevicePathInformation, err))?;

        if device_path_information_raw.bedd.get() != 0xbedd {
            return Err(Error::parsing_error(
                Fault::InvalidValueForField("bedd"),
                Facility::EDDDevicePathInformation,
            ));
        }

        if device_path_information_raw.reserved_1 != 0
            || device_path_information_raw.reserved_2.get() != 0
            || device_path_information_raw.reserved_3 != 0
        {
            return Err(Error::parsing_error(
                Fault::InvalidValueForField("reserved"),
                Facility::EDDDevicePathInformation,
            ));
        }

        if device_path_information_raw.length as usize != size_of::<DevicePathInformationRaw>() {
            return Err(Error::parsing_error(
                Fault::InvalidValueForField("length"),
                Facility::EDDDevicePathInformation,
            ));
        }

        let checksum: u8 = value[..size_of::<DevicePathInformationRaw>() - 1]
            .iter()
            .fold(0, |checksum, &byte| checksum.wrapping_add(byte));

        if checksum.wrapping_add(device_path_information_raw.checksum) != 0 {
            return Err(Error::parsing_error(
                Fault::InvalidValueForField("checksum"),
                Facility::EDDDevicePathInformation,
            ));
        }

        Self::try_from(&device_path_information_raw)
    }
}

impl TryFrom<&DevicePathInformationRaw> for DevicePathInformation {
    type Error = Error;

    fn try_from(value: &DevicePathInformationRaw) -> Result<Self, Self::Error> {
        let interface_path = value.interface_path.get().to_le_bytes();
        let host_bus = match value.host_bus_type {
            bytes if bytes.starts_with(b"PCI") => {
                let bus = interface_path[0];
                let slot = interface_path[1];
                let function = interface_path[2];
                if !interface_path[3..].iter().all(|&b| b == 0) {
                    return Err(Error::parsing_error(
                        Fault::InvalidValueForField("PCI interface path reserved bytes"),
                        Facility::EDDDevicePathInformation,
                    ));
                }
                HostBus::Pci {
                    bus,
                    slot,
                    function,
                }
            }
            bytes if bytes.starts_with(b"ISA") => {
                let base_address = value.interface_path.get() as u16;
                if !interface_path[2..].iter().all(|&b| b == 0) {
                    return Err(Error::parsing_error(
                        Fault::InvalidValueForField("ISA interface path reserved bytes"),
                        Facility::EDDDevicePathInformation,
                    ));
                }
                HostBus::Isa { base_address }
            }
            _ => {
                return Err(Error::parsing_error(
                    Fault::InvalidValueForField("host bus type"),
                    Facility::EDDDevicePathInformation,
                ));
            }
        };

        let device_path = value.device_path.get().to_le_bytes();
        let interface = match value.interface_type {
            bytes if bytes.starts_with(b"ATA") => {
                let is_slave = device_path[0] == 1;
                if !device_path[1..].iter().all(|&b| b == 0) {
                    return Err(Error::parsing_error(
                        Fault::InvalidValueForField("ATA device path reserved bytes"),
                        Facility::EDDDevicePathInformation,
                    ));
                }
                Interface::Ata { is_slave }
            }
            bytes if bytes.starts_with(b"ATAPI") => {
                let is_slave = device_path[0] == 1;
                let logical_unit_number = device_path[1];
                if !device_path[2..].iter().all(|&b| b == 0) {
                    return Err(Error::parsing_error(
                        Fault::InvalidValueForField("ATAPI device path reserved bytes"),
                        Facility::EDDDevicePathInformation,
                    ));
                }
                Interface::Atapi {
                    is_slave,
                    logical_unit_number,
                }
            }
            bytes if bytes.starts_with(b"SCSI") => {
                let logical_unit_number = device_path[0];
                if !device_path[1..].iter().all(|&b| b == 0) {
                    return Err(Error::parsing_error(
                        Fault::InvalidValueForField("SCSI device path reserved bytes"),
                        Facility::EDDDevicePathInformation,
                    ));
                }
                Interface::Scsi {
                    logical_unit_number,
                }
            }
            bytes if bytes.starts_with(b"USB") => {
                let tbd = device_path[0];
                if !device_path[1..].iter().all(|&b| b == 0) {
                    return Err(Error::parsing_error(
                        Fault::InvalidValueForField("USB device path reserved bytes"),
                        Facility::EDDDevicePathInformation,
                    ));
                }
                Interface::Usb { tbd }
            }
            bytes if bytes.starts_with(b"1394") => Interface::_1394 {
                guid: value.device_path.get(),
            },
            bytes if bytes.starts_with(b"FIBRE") => Interface::Fibre {
                wwn: device_path[0],
            },
            _ => {
                return Err(Error::parsing_error(
                    Fault::InvalidValueForField("interface type"),
                    Facility::EDDDevicePathInformation,
                ));
            }
        };
        Ok(Self {
            host_bus,
            interface,
        })
    }
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

make_bitmap!(new_type: InfoFlags, underlying_flag_type: InfoFlagType, repr: u16, bit_skipper: |i| i > 6);

#[cfg_attr(test, derive(PartialEq, Eq))]
#[derive(Debug, Default)]
pub struct DriveParameters {
    buffer_size: u16,
    information_flags: InfoFlags,
    cylinders: u32,
    heads: u32,
    sectors_per_track: u32,
    sectors: u64,
    bytes_per_sector: u16,
    fixed_disk_parameter_table: Option<FixedDiskParameterTable>,
    device_path_information: Option<DevicePathInformation>,
}

impl DriveParameters {
    fn try_read_error<U: TryFromBytes>(err: TryReadError<&[u8], U>) -> Error {
        try_read_error(Facility::EDDDriveParameters, err)
    }

    pub fn resolve_fdbt(&mut self, mut fdbt_address: u32) -> Result<(), Error> {
        if fdbt_address == u32::MAX {
            // Nothing to do, the fdbt address is invalid
            return Ok(());
        }

        if self.buffer_size as usize != size_of::<DriveParametersRaw>() {
            return Err(Error::parsing_error(
                Fault::NotEnoughBytesFor("fixed disk parameter table"),
                Facility::EDDFixedDiskParameterTable,
            ));
        }
        // Address is in seg:offset format, with offset coming first
        fdbt_address = ((fdbt_address >> 16) * 16) + (fdbt_address & 0xffff);

        self.fixed_disk_parameter_table = Some(FixedDiskParameterTable::try_from(
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
        match &self.fixed_disk_parameter_table {
            Some(configuration_parameters) => {
                write!(f, "{configuration_parameters}")?;
            }
            None => {
                writeln!(f, "  Configuration Parameters: Not Present")?;
            }
        }
        match &self.device_path_information {
            Some(device_path_information) => {
                write!(f, "{device_path_information}")
            }
            None => writeln!(f, "  Device Path Information: Not Present"),
        }
    }
}

impl TryFrom<&DriveParametersRaw> for DriveParameters {
    type Error = Error;

    fn try_from(value: &DriveParametersRaw) -> Result<Self, Self::Error> {
        if value.buffer_size.get() != 26 && value.buffer_size.get() != 30 {
            return Err(Error::parsing_error(
                Fault::InvalidValueForField("buffer size"),
                Facility::EDDDevicePathInformation,
            ));
        }

        let information_flags: InfoFlags = InfoFlags {
            bits: value.information_flags.get(),
        };
        if information_flags.is_set(InfoFlagType::SuppliedGeometryValid) {
            if value.cylinders.get() == 0 {
                return Err(Error::parsing_error(
                    Fault::InvalidValueForField("cylinders"),
                    Facility::EDDDevicePathInformation,
                ));
            }
            if value.heads.get() == 0 {
                return Err(Error::parsing_error(
                    Fault::InvalidValueForField("heads"),
                    Facility::EDDDevicePathInformation,
                ));
            }
            if value.sectors_per_track.get() == 0 {
                return Err(Error::parsing_error(
                    Fault::InvalidValueForField("sectors_per_track"),
                    Facility::EDDDevicePathInformation,
                ));
            }
        }

        if value.bytes_per_sector.get() == 0 {
            return Err(Error::parsing_error(
                Fault::InvalidValueForField("bytes_per_sector"),
                Facility::EDDDevicePathInformation,
            ));
        }

        if information_flags.is_set(InfoFlagType::Removable) {
            if !information_flags.is_set(InfoFlagType::SupportsLineChange) {
                return Err(Error::parsing_error(
                    Fault::InvalidValueForField("information_flags"),
                    Facility::EDDDevicePathInformation,
                ));
            }
            if !information_flags.is_set(InfoFlagType::Lockable) {
                return Err(Error::parsing_error(
                    Fault::InvalidValueForField("information_flags"),
                    Facility::EDDDevicePathInformation,
                ));
            }
        }

        if information_flags.is_set(InfoFlagType::NoMediaPresent)
            && !information_flags.is_set(InfoFlagType::Removable)
        {
            return Err(Error::parsing_error(
                Fault::InvalidValueForField("information_flags"),
                Facility::EDDDevicePathInformation,
            ));
        }

        Ok(Self {
            buffer_size: value.buffer_size.get(),
            information_flags,
            cylinders: value.cylinders.get(),
            heads: value.heads.get(),
            sectors_per_track: value.sectors_per_track.get(),
            sectors: value.sectors.get(),
            bytes_per_sector: value.bytes_per_sector.get(),
            fixed_disk_parameter_table: None,
            device_path_information: None,
        })
    }
}

impl TryFrom<DriveParameters> for common::ata::Device {
    type Error = DriveParameters;

    fn try_from(value: DriveParameters) -> Result<Self, Self::Error> {
        //io_port_base_address: u16, control_port_base_address: u16, is_slave: bool, sectors: u64, sector_size_bytes: u16
        let Some(fdpt) = &value.fixed_disk_parameter_table else {
            return Err(value);
        };
        let io_port_base_address = fdpt.io_port_base;
        let control_port_base_address = fdpt.control_port_base;
        let Some(device_path_information) = &value.device_path_information else {
            return Err(value);
        };
        let is_slave = match device_path_information.interface {
            Interface::Ata { is_slave } | Interface::Atapi { is_slave, .. } => is_slave,
            _ => {
                return Err(value);
            }
        };
        let sectors = value.sectors;
        let sector_size_bytes = value.bytes_per_sector;
        Ok(common::ata::Device::new(
            io_port_base_address,
            control_port_base_address,
            is_slave,
            sectors,
            sector_size_bytes,
        ))
    }
}

impl TryFrom<&[u8]> for DriveParameters {
    type Error = Error;

    fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
        let (drive_parameters_raw, _rest) =
            DriveParametersRaw::try_read_from_prefix(bytes).map_err(Self::try_read_error)?;

        let mut result = Self::try_from(&drive_parameters_raw)?;
        if drive_parameters_raw.configuration_parameters.get() != u32::MAX
            && drive_parameters_raw.buffer_size.get() as usize == size_of::<DriveParametersRaw>()
        {
            result.resolve_fdbt(drive_parameters_raw.configuration_parameters.get())?;
        }

        if u16::from_le_bytes([bytes[30], bytes[31]]) == 0xbedd {
            result.device_path_information = Some(DevicePathInformation::try_from(&bytes[30..])?)
        }

        Ok(result)
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

make_bitmap!(new_type: HeadRegisterUpperNibble, underlying_flag_type: HeadRegisterFlagType, repr: u8, bit_skipper: |i| i != 4 && i != 6);

#[cfg_attr(test, derive(PartialEq, Eq))]
#[derive(Debug, Default)]
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

impl TryFrom<&[u8]> for FixedDiskParameterTable {
    type Error = Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let (fixed_disk_parameter_table_raw, _rest) =
            FixedDiskParameterTableRaw::try_read_from_prefix(value)
                .map_err(|err| try_read_error(Facility::EDDFixedDiskParameterTable, err))?;

        let checksum: u8 = value[..size_of::<FixedDiskParameterTableRaw>() - 1]
            .iter()
            .fold(0, |checksum, &byte| checksum.wrapping_add(byte));

        if checksum.wrapping_add(fixed_disk_parameter_table_raw.checksum) != 0 {
            return Err(Error::parsing_error(
                Fault::InvalidValueForField("checksum"),
                Facility::EDDFixedDiskParameterTable,
            ));
        }

        Self::try_from(&fixed_disk_parameter_table_raw)
    }
}

impl TryFrom<&FixedDiskParameterTableRaw> for FixedDiskParameterTable {
    type Error = Error;

    fn try_from(value: &FixedDiskParameterTableRaw) -> Result<Self, Self::Error> {
        if value.extension_revision != 0x11 {
            return Err(Error::parsing_error(
                Fault::InvalidValueForField("extension revision"),
                Facility::EDDFixedDiskParameterTable,
            ));
        }

        if value.head_prefix & 0b10001111 != 0b10000000 {
            return Err(Error::parsing_error(
                Fault::InvalidValueForField("head_prefix"),
                Facility::EDDFixedDiskParameterTable,
            ));
        }

        if value.irq & 0xf0 != 0 {
            return Err(Error::parsing_error(
                Fault::InvalidValueForField("irq"),
                Facility::EDDFixedDiskParameterTable,
            ));
        }

        if value.pio_type & 0xf0 != 0 {
            return Err(Error::parsing_error(
                Fault::InvalidValueForField("pio_type"),
                Facility::EDDFixedDiskParameterTable,
            ));
        }

        let hw_flags = HWSpecificOptionFlags {
            bits: value.hardware_specific_option_flags.get(),
        };

        if hw_flags.is_set(HWSpecificOptionFlagType::Atapi)
            && !hw_flags.is_set(HWSpecificOptionFlagType::AtapiUsesInterruptDRQ)
        {
            return Err(Error::parsing_error(
                Fault::InvalidValueForField("hardware_specific_option_flag"),
                Facility::EDDFixedDiskParameterTable,
            ));
        }

        if !hw_flags.is_set(HWSpecificOptionFlagType::CHSTranslation)
            && (hw_flags.is_set(HWSpecificOptionFlagType::TranslationTypeFirstBit)
                || hw_flags.is_set(HWSpecificOptionFlagType::TranslationTypeSecondBit))
        {
            return Err(Error::parsing_error(
                Fault::InvalidValueForField("hardware_specific_option_flags"),
                Facility::EDDFixedDiskParameterTable,
            ));
        }

        Ok(Self {
            io_port_base: value.io_port_base.get(),
            control_port_base: value.control_port_base.get(),
            head_prefix: HeadRegisterUpperNibble {
                bits: value.head_prefix,
            },
            irq: value.irq,
            sector_count: value.sector_count,
            dma_channel: value.dma_channel_type & 0xf,
            dma_type: value.dma_channel_type >> 4,
            pio_type: value.pio_type,
            hardware_specific_option_flags: HWSpecificOptionFlags {
                bits: value.hardware_specific_option_flags.get(),
            },
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

make_bitmap!(new_type: HWSpecificOptionFlags, underlying_flag_type: HWSpecificOptionFlagType, repr: u16, bit_skipper: |i| i > 10);

#[cfg(test)]
mod tests {
    use crate::edd::{self, DevicePathInformation, FixedDiskParameterTable};

    const QEMU_DRIVE_PARAMETERS_BYTES: [u8; 66] = [
        0x1e, 0x0, 0x2, 0x0, 0x2, 0x0, 0x0, 0x0, 0x10, 0x0, 0x0, 0x0, 0x3f, 0x0, 0x0, 0x0, 0x91,
        0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, 0xff, 0xff, 0xff, 0xff, 0xdd, 0xbe, 0x24, 0x0,
        0x0, 0x0, 0x50, 0x43, 0x49, 0x20, 0x41, 0x54, 0x41, 0x20, 0x20, 0x20, 0x20, 0x20, 0x0, 0x1,
        0x1, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0xcd,
    ];

    const BOCHS_DRIVE_PARAMETERS_BYTES: [u8; 66] = [
        0x1e, 0x0, 0x2, 0x0, 0x1, 0x0, 0x0, 0x0, 0x1, 0x0, 0x0, 0x0, 0x12, 0x0, 0x0, 0x0, 0x91,
        0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x2, 0xff, 0xff, 0xff, 0xff, 0xdd, 0xbe, 0x24, 0x0,
        0x0, 0x0, 0x49, 0x53, 0x41, 0x20, 0x41, 0x54, 0x41, 0x20, 0x20, 0x20, 0x20, 0x20, 0xf0,
        0x1, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0xdd,
    ];

    const QEMU_FDPT_BYTES: [u8; 16] = [
        0xf0, 0x1, 0xf6, 0x3, 0xe0, 0xcb, 0xe, 0x1, 0x0, 0x0, 0x10, 0x0, 0x0, 0x0, 0x11, 0x3b,
    ];

    const BOCHS_FDPT_BYTES: [u8; 16] = [
        0xf0, 0x1, 0xf6, 0x3, 0xe0, 0xcb, 0xe, 0x1, 0x0, 0x0, 0x90, 0x0, 0x0, 0x0, 0x11, 0xbb,
    ];

    #[test]
    fn test_parse_drive_parameters() {
        let qemu_drive_parameters =
            edd::DriveParameters::try_from(&QEMU_DRIVE_PARAMETERS_BYTES[..]).unwrap();
        assert_eq!(
            edd::DriveParameters {
                buffer_size: 30,
                information_flags: edd::InfoFlags { bits: 2 },
                cylinders: 2,
                heads: 16,
                sectors_per_track: 63,
                sectors: 145,
                bytes_per_sector: 512,
                fixed_disk_parameter_table: None,
                device_path_information: Some(DevicePathInformation {
                    host_bus: edd::HostBus::Pci {
                        bus: 0,
                        slot: 1,
                        function: 1
                    },
                    interface: edd::Interface::Ata { is_slave: false }
                })
            },
            qemu_drive_parameters
        );

        let bochs_drive_parameters =
            edd::DriveParameters::try_from(&BOCHS_DRIVE_PARAMETERS_BYTES[..]).unwrap();
        assert_eq!(
            edd::DriveParameters {
                buffer_size: 30,
                information_flags: edd::InfoFlags { bits: 2 },
                cylinders: 1,
                heads: 1,
                sectors_per_track: 18,
                sectors: 145,
                bytes_per_sector: 512,
                fixed_disk_parameter_table: None,
                device_path_information: Some(DevicePathInformation {
                    host_bus: edd::HostBus::Isa {
                        base_address: 0x1f0
                    },
                    interface: edd::Interface::Ata { is_slave: false }
                })
            },
            bochs_drive_parameters
        );
    }

    #[test]
    fn test_parse_fdpt() {
        let qemu_fdpt = edd::FixedDiskParameterTable::try_from(&QEMU_FDPT_BYTES[..]).unwrap();
        use edd::HWSpecificOptionFlagType::*;
        use edd::HeadRegisterFlagType::*;
        assert_eq!(
            FixedDiskParameterTable {
                io_port_base: 0x1f0,
                control_port_base: 0x3f6,
                head_prefix: edd::HeadRegisterUpperNibble {
                    bits: 0xa0 | LBAEnabled as u8
                },
                irq: 14,
                sector_count: 1,
                dma_channel: 0,
                dma_type: 0,
                pio_type: 0,
                hardware_specific_option_flags: LBATranslation.into(),
                extension_revision: 17,
                checksum: 0x3b
            },
            qemu_fdpt
        );

        let bochs_fdpt = edd::FixedDiskParameterTable::try_from(&BOCHS_FDPT_BYTES[..]).unwrap();
        assert_eq!(
            FixedDiskParameterTable {
                io_port_base: 0x1f0,
                control_port_base: 0x3f6,
                head_prefix: edd::HeadRegisterUpperNibble {
                    bits: 0xa0 | LBAEnabled as u8
                },
                irq: 14,
                sector_count: 1,
                dma_channel: 0,
                dma_type: 0,
                pio_type: 0,
                hardware_specific_option_flags: LBATranslation | _32BitTransferMode,
                extension_revision: 17,
                checksum: 0xbb
            },
            bochs_fdpt
        );
    }
}
