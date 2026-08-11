use num_enum::TryFromPrimitive;
use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes};

// Source of truth: PCI Local Bus Specification revision 2.2
use crate::{
    bits,
    error::{
        self, Error,
        Fault::{self, InvalidPCIMemoryAddressingType},
        PciDevice, convert_try_read_error,
    },
    ioport::Port,
    make_bitmap,
    usb::{self, ehci},
};

pub const MAX_BUS_NUMBER: usize = 255;
pub const MAX_DEVICE_NUMBER: usize = 31;
pub const MAX_FUNCTION_NUMBER: usize = 7;
const CONFIG_ADDRESS: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

#[repr(u32)]
pub enum ConfigAddressRegisterFlag {
    Enable = 1 << 31,
}

make_bitmap!(new_type: ConfigAddressRegister, underlying_flag_type: ConfigAddressRegisterFlag, repr: u32, nodisplay);

impl ConfigAddressRegister {
    pub fn set_register_offset(&mut self, register_number: u8) {
        self.bits &= !(0x3f << 2);
        self.bits |= register_number as u32 & (0x3f << 2);
    }

    pub fn set_function_number(&mut self, function_number: u8) {
        bits::set_bits!(bits_expr: self.bits, value: function_number, n_bits: 3, starts_at_bit: 8, bits_expr_ty: u32);
    }

    pub fn set_device_number(&mut self, device_number: u8) {
        bits::set_bits!(bits_expr: self.bits, value: device_number, n_bits: 5, starts_at_bit: 11, bits_expr_ty: u32);
    }

    pub fn set_bus_number(&mut self, bus_number: u8) {
        bits::set_bits!(bits_expr: self.bits, value: bus_number, n_bits: 8, starts_at_bit: 16, bits_expr_ty: u32);
    }

    pub fn get_function_number(&self) -> u8 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 3, starts_at_bit: 8, return_ty: u8)
    }

    pub fn get_device_number(&self) -> u8 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 5, starts_at_bit: 11, return_ty: u8)
    }

    pub fn get_bus_number(&self) -> u8 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 8, starts_at_bit: 16, return_ty: u8)
    }

    pub fn read_dword(&mut self) -> u32 {
        let config_address_port = Port::new(CONFIG_ADDRESS);
        let config_data_port = Port::new(CONFIG_DATA);
        self.set_flag(ConfigAddressRegisterFlag::Enable);

        config_address_port.writed(self.bits);
        config_data_port.readd()
    }

    pub fn write_dword(&mut self, dword: u32) {
        let config_address_port = Port::new(CONFIG_ADDRESS);
        let config_data_port = Port::new(CONFIG_DATA);
        self.set_flag(ConfigAddressRegisterFlag::Enable);

        config_address_port.writed(self.bits);
        config_data_port.writed(dword);
    }

    pub fn dump_configuration_space_header(
        &mut self,
    ) -> Option<error::Result<ConfigurationSpaceHeader>> {
        let mut bytes = [0u8; size_of::<ConfigurationSpaceHeader>()];
        let mut offset = 0usize;
        self.set_register_offset(offset as u8);
        self.read_dword()
            .to_le_bytes()
            .write_to_prefix(&mut bytes[offset..])
            .ok()?;
        offset += 4;
        if u16::from_le_bytes([bytes[0], bytes[1]]) == 0xff_ff {
            return None;
        }
        while offset < bytes.len() {
            self.set_register_offset(offset as u8);
            self.read_dword()
                .to_le_bytes()
                .write_to_prefix(&mut bytes[offset..])
                .ok()?;
            offset += 4;
        }
        Some(ConfigurationSpaceHeader::try_from(bytes.as_slice()))
    }
}

impl From<ConfigAddressRegister> for PciDevice {
    fn from(value: ConfigAddressRegister) -> Self {
        PciDevice::new(
            value.get_bus_number(),
            value.get_device_number(),
            value.get_function_number(),
        )
    }
}

#[repr(u32)]
pub enum ConfigDataRegisterFlag {
    ConfigurationDataWindow = 1 << 31,
}

make_bitmap!(new_type: ConfigDataRegister, underlying_flag_type: ConfigDataRegisterFlag, repr: u32, nodisplay);

impl ConfigDataRegister {
    pub fn set_value(&mut self, value: u32) {
        self.bits &= 0x7f_ff_ff_ff;
        self.bits |= value & 0x7f_ff_ff_ff;
    }
}

#[repr(u8)]
pub enum ConfigurationSpaceHeaderVersionFlag {
    MultiFunctionDevice = 1 << 7,
}

make_bitmap!(new_type: ConfigurationSpaceHeaderVersion, underlying_flag_type: ConfigurationSpaceHeaderVersionFlag, repr: u8, nodisplay);

#[derive(TryFromPrimitive)]
#[repr(u8)]
pub enum HeaderType {
    Type0,
    Type1,
    CardbusBridge,
}

impl core::fmt::Display for HeaderType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            HeaderType::Type0 => write!(f, "Type 0 (Endpoints)"),
            HeaderType::Type1 => write!(f, "Type 1 (PCI-to-PCI Bridge)"),
            HeaderType::CardbusBridge => write!(f, "CardBus Bridge"),
        }
    }
}

impl ConfigurationSpaceHeaderVersion {
    pub fn type0() -> Self {
        Self { bits: 0 }
    }

    pub fn type1() -> Self {
        Self { bits: 1 }
    }

    pub fn cardbus_bridge() -> Self {
        Self { bits: 2 }
    }
}

#[derive(TryFromPrimitive, Clone, Copy)]
#[repr(u32)]
pub enum Class {
    NonVGACompatibleDevice = 0,
    VGACompatibleDevice = 0x00_01_00,

    SCSIBusController = 0x01_00_00,
    IDEController = 0x01_01_00,
    FloppyDiskController = 0x01_02_00,
    IPIBusController = 0x01_03_00,
    RAIDController = 0x01_04_00,
    ATASingleDmaController = 0x01_05_20,
    ATAChainedDmaController = 0x01_05_30,
    VendorSpecificSATAController = 0x01_06_00,
    AHCISATAController = 0x01_06_01,
    SerialStorageBusSATAController = 0x01_06_02,
    SASSerialAttachedSCSIController = 0x01_07_00,
    SerialStorageBusSerialAttachedSCSIController = 0x01_07_01,
    NVMHCIController = 0x01_08_01,
    NVMEController = 0x01_08_02,
    MassStorageController = 0x01_80_00,

    EthernetController = 0x02_00_00,
    TokenRingController = 0x02_01_00,
    FDDIController = 0x02_02_00,
    ATMController = 0x02_03_00,
    ISDNController = 0x02_04_00,
    OtherNetworkController = 0x02_80_00,

    VGACompatibleController = 0x03_00_00,
    _8514CompatibleController = 0x03_00_01,
    XGAController = 0x03_01_00,
    _3DController = 0x03_02_00,
    OtherDisplayController = 0x03_80_00,

    MultimediaVideoController = 0x04_00_00,
    MultimediaAudioController = 0x04_01_00,
    ComputerTelephonyDevice = 0x04_02_00,
    AudioDevice = 0x04_03_00,
    OtherMultimediaDevice = 0x04_80_00,
    Ram = 0x05_00_00,
    Flash = 0x05_01_00,
    OtherMemoryController = 0x05_80_00,

    HostBridge = 0x06_00_00,
    ISABridge = 0x06_01_00,
    EISABridge = 0x06_02_00,
    MCABridge = 0x06_03_00,
    PCItoPCIBridge = 0x06_04_00,
    SubtractiveDecodePCItoPCIBridge = 0x06_04_01,
    PCMCIABridge = 0x06_05_00,
    NuBusBridge = 0x06_06_00,
    CardBusBridge = 0x06_07_00,
    RACEwayBridge = 0x06_08_00,
    OtherBridgeDevice = 0x06_80_00,

    XTCompatibleSerialController = 0x07_00_00,
    _16450CompatibleSerialController = 0x07_00_01,
    _16550CompatibleSerialController = 0x07_00_02,
    _16650CompatibleSerialController = 0x07_00_03,
    _16750CompatibleSerialController = 0x07_00_04,
    _16850CompatibleSerialController = 0x07_00_05,
    _16950CompatibleSerialController = 0x07_00_06,
    ParallelPort = 0x07_01_00,
    BidirectionalParallelPort = 0x07_01_01,
    ECP1XCompliantPort = 0x07_01_02,
    IEEE1284Controller = 0x07_01_03,
    IEEE1284TargetDevice = 0x07_01_fe,
    MultiportSerialController = 0x07_02_00,
    GenericModem = 0x07_03_00,
    HayesCompatibleModem16450 = 0x07_03_01,
    HayesCompatibleModem16550 = 0x07_03_02,
    HayesCompatibleModem16650 = 0x07_03_03,
    HayesCompatibleModem16750 = 0x07_03_04,
    OtherCommunicationsDevice = 0x07_80_00,

    Generic8259PIC = 0x08_00_00,
    IsaPIC = 0x08_00_01,
    EisaPIC = 0x08_00_02,
    IOAPICInterruptController = 0x08_10_00,
    IOXAPICInterruptController = 0x08_20_00,
    Generic8237DMAController = 0x08_01_00,
    ISADMAController = 0x08_01_01,
    EISADMAController = 0x08_01_02,
    Generic8254SystemTimer = 0x08_02_00,
    ISASystemTimer = 0x08_02_01,
    EISASystemTimers = 0x08_02_02,
    GenericRTCController = 0x08_03_00,
    IsaRTCController = 0x08_03_01,
    GenericPCIHotPlugController = 0x08_04_00,
    SDHostController = 0x08_05_00,
    Iommu = 0x08_06_00,
    OtherSystemPeripheral = 0x08_80_00,

    KeyboardController = 0x09_00_00,
    PenDigitizer = 0x09_01_00,
    MouseController = 0x09_02_00,
    ScannerController = 0x09_03_00,
    GameportController = 0x09_04_00,
    LegacyGameportController = 0x09_04_01,
    OtherInputController = 0x09_80_00,

    GenericDockingStation = 0x0a_00_00,
    OtherTypeDockingStation = 0x0a_80_00,

    _386Processor = 0x0b_00_00,
    _486Processor = 0x0b_01_00,
    PentiumProcessor = 0x0b_02_00,
    AlphaProcessor = 0x0b_10_00,
    PowerPCProcessor = 0x0b_20_00,
    MIPSProcessor = 0x0b_30_00,
    CoProcessor = 0x0b_40_00,

    FirewireIEEE394 = 0x0c_00_00,
    OpenHCIIEEE394 = 0x0c_00_10,
    ACCESSBus = 0x0c_01_00,
    Ssa = 0x0c_02_00,
    UHCIUsb = 0x0c_03_00,
    OHCIUsb = 0x0c_03_10,
    EHCIUsb = 0x0c_03_20,
    XHCIUsb = 0x0c_03_30,
    GenericUsb = 0x0c_03_80,
    UsbDevice = 0x0c_03_fe,
    FibreChannel = 0x0c_04_00,
    SystemManagementBus = 0x0c_05_00,

    IRDACompatibleController = 0x0d_00_00,
    ConsumerIRController = 0x0d_01_00,
    RFController = 0x0d_10_00,
    OtherWirelessController = 0x0d_80_00,

    I2OMessageFifoOffset040h = 0x0e_00_00,
    IntelligentIOController = 0x0e_00_01,

    Tv = 0x0f_01_00,
    Audio = 0x0f_02_00,
    Voice = 0x0f_03_00,
    Data = 0x0f_04_00,

    NetworkAndComputingEnDecryption = 0x10_00_00,
    EntairtainementEnDecryption = 0x10_10_00,
    OtherEnDecryption = 0x10_80_00,

    DPIOModules = 0x11_00_00,
    OtherDataAcquisitionSignalProcessingController = 0x11_80_00,
}

impl core::fmt::Display for Class {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let name: &'static str = match self {
            Class::NonVGACompatibleDevice => "Non-VGA-compatible device",
            Class::VGACompatibleDevice => "VGA-compatible device",

            Class::SCSIBusController => "SCSI bus controller",
            Class::IDEController => "IDE controller",
            Class::FloppyDiskController => "Floppy disk controller",
            Class::IPIBusController => "IPI bus controller",
            Class::RAIDController => "RAID controller",
            Class::ATASingleDmaController => "ATA controller (single DMA)",
            Class::ATAChainedDmaController => "ATA controller (chained DMA)",
            Class::VendorSpecificSATAController => "Vendor-specific SATA controller",
            Class::AHCISATAController => "AHCI SATA controller",
            Class::SerialStorageBusSATAController => "Serial Storage Bus SATA controller",
            Class::SASSerialAttachedSCSIController => "SAS (Serial Attached SCSI) controller",
            Class::SerialStorageBusSerialAttachedSCSIController => {
                "Serial Storage Bus Serial Attached SCSI controller"
            }
            Class::NVMHCIController => "NVMHCI controller",
            Class::NVMEController => "NVMe controller",
            Class::MassStorageController => "Mass storage controller",

            Class::EthernetController => "Ethernet controller",
            Class::TokenRingController => "Token Ring controller",
            Class::FDDIController => "FDDI controller",
            Class::ATMController => "ATM controller",
            Class::ISDNController => "ISDN controller",
            Class::OtherNetworkController => "Other network controller",

            Class::VGACompatibleController => "VGA-compatible controller",
            Class::_8514CompatibleController => "8514-compatible controller",
            Class::XGAController => "XGA controller",
            Class::_3DController => "3D controller",
            Class::OtherDisplayController => "Other display controller",

            Class::MultimediaVideoController => "Multimedia Video controller",
            Class::MultimediaAudioController => "Multimedia Audio controller",
            Class::AudioDevice => "Audio device",
            Class::ComputerTelephonyDevice => "Computer telephony device",
            Class::OtherMultimediaDevice => "Other multimedia device",
            Class::Ram => "RAM controller",
            Class::Flash => "Flash memory controller",
            Class::OtherMemoryController => "Other memory controller",

            Class::HostBridge => "Host bridge",
            Class::ISABridge => "ISA bridge",
            Class::EISABridge => "EISA bridge",
            Class::MCABridge => "MCA bridge",
            Class::PCItoPCIBridge => "PCI-to-PCI bridge",
            Class::SubtractiveDecodePCItoPCIBridge => "Subtractive-decode PCI-to-PCI bridge",
            Class::PCMCIABridge => "PCMCIA bridge",
            Class::NuBusBridge => "NuBus bridge",
            Class::CardBusBridge => "CardBus bridge",
            Class::RACEwayBridge => "RACEway bridge",
            Class::OtherBridgeDevice => "Other bridge device",

            Class::XTCompatibleSerialController => "XT-compatible serial controller",
            Class::_16450CompatibleSerialController => "16450-compatible serial controller",
            Class::_16550CompatibleSerialController => "16550-compatible serial controller",
            Class::_16650CompatibleSerialController => "16650-compatible serial controller",
            Class::_16750CompatibleSerialController => "16750-compatible serial controller",
            Class::_16850CompatibleSerialController => "16850-compatible serial controller",
            Class::_16950CompatibleSerialController => "16950-compatible serial controller",
            Class::ParallelPort => "Parallel port",
            Class::BidirectionalParallelPort => "Bidirectional parallel port",
            Class::ECP1XCompliantPort => "ECP 1.x-compliant port",
            Class::IEEE1284Controller => "IEEE 1284 controller",
            Class::IEEE1284TargetDevice => "IEEE 1284 target device",
            Class::MultiportSerialController => "Multiport serial controller",
            Class::GenericModem => "Generic modem",
            Class::HayesCompatibleModem16450 => "Hayes-compatible modem (16450)",
            Class::HayesCompatibleModem16550 => "Hayes-compatible modem (16550)",
            Class::HayesCompatibleModem16650 => "Hayes-compatible modem (16650)",
            Class::HayesCompatibleModem16750 => "Hayes-compatible modem (16750)",
            Class::OtherCommunicationsDevice => "Other communications device",

            Class::Generic8259PIC => "Generic 8259 PIC",
            Class::IsaPIC => "ISA PIC",
            Class::EisaPIC => "EISA PIC",
            Class::IOAPICInterruptController => "I/O APIC interrupt controller",
            Class::IOXAPICInterruptController => "I/Ox APIC interrupt controller",
            Class::Generic8237DMAController => "Generic 8237 DMA controller",
            Class::ISADMAController => "ISA DMA controller",
            Class::EISADMAController => "EISA DMA controller",
            Class::Generic8254SystemTimer => "Generic 8254 system timer",
            Class::ISASystemTimer => "ISA system timer",
            Class::EISASystemTimers => "EISA system timers",
            Class::GenericRTCController => "Generic RTC controller",
            Class::IsaRTCController => "ISA RTC controller",
            Class::GenericPCIHotPlugController => "Generic PCI hot-plug controller",
            Class::SDHostController => "SD host controller",
            Class::Iommu => "IOMMU",
            Class::OtherSystemPeripheral => "Other system peripheral",

            Class::KeyboardController => "Keyboard controller",
            Class::PenDigitizer => "Pen digitizer",
            Class::MouseController => "Mouse controller",
            Class::ScannerController => "Scanner controller",
            Class::GameportController => "Gameport controller",
            Class::LegacyGameportController => "Legacy gameport controller",
            Class::OtherInputController => "Other input controller",

            Class::GenericDockingStation => "Generic docking station",
            Class::OtherTypeDockingStation => "Other type of docking station",

            Class::_386Processor => "80386 processor",
            Class::_486Processor => "80486 processor",
            Class::PentiumProcessor => "Pentium processor",
            Class::AlphaProcessor => "Alpha processor",
            Class::PowerPCProcessor => "PowerPC processor",
            Class::MIPSProcessor => "MIPS processor",
            Class::CoProcessor => "Coprocessor",

            Class::FirewireIEEE394 => "IEEE 1394 (FireWire) controller",
            Class::OpenHCIIEEE394 => "IEEE 1394 (FireWire) controller (OpenHCI)",
            Class::ACCESSBus => "ACCESS.bus controller",
            Class::Ssa => "SSA controller",
            Class::UHCIUsb => "USB controller (UHCI)",
            Class::EHCIUsb => "USB controller (EHCI)",
            Class::XHCIUsb => "USB controller (XHCI)",
            Class::OHCIUsb => "USB controller (OHCI)",
            Class::GenericUsb => "USB controller (generic)",
            Class::UsbDevice => "USB device",
            Class::FibreChannel => "Fibre Channel controller",
            Class::SystemManagementBus => "System Management Bus controller",

            Class::IRDACompatibleController => "IrDA-compatible controller",
            Class::ConsumerIRController => "Consumer IR controller",
            Class::RFController => "RF controller",
            Class::OtherWirelessController => "Other wireless controller",

            Class::I2OMessageFifoOffset040h => "I2O message FIFO (offset 0x40)",
            Class::IntelligentIOController => "Intelligent I/O controller",

            Class::Tv => "TV controller",
            Class::Audio => "Audio controller",
            Class::Voice => "Voice controller",
            Class::Data => "Data controller",

            Class::NetworkAndComputingEnDecryption => "Network and computing encryption/decryption",
            Class::EntairtainementEnDecryption => "Entertainment encryption/decryption",
            Class::OtherEnDecryption => "Other encryption/decryption",

            Class::DPIOModules => "Data acquisition / DPIO modules",
            Class::OtherDataAcquisitionSignalProcessingController => {
                "Other data acquisition / signal processing controller"
            }
        };

        write!(f, "{} (0x{:06x})", name, *self as u32)
    }
}

#[derive(TryFromBytes, Immutable, Debug, Clone, Copy)]
pub struct Reserved<const N: usize>([u8; N]);

#[derive(TryFromBytes, KnownLayout, Immutable, Debug)]
#[repr(C, packed)]
pub struct ConfigurationSpaceHeader {
    // REQUIRED
    vendor_id: u16,
    // REQUIRED
    device_id: u16,
    // REQUIRED
    command: u16,
    // REQUIRED
    status: u16,
    // REQUIRED
    revision_id: u8,
    // REQUIRED
    programming_interface: u8,
    // REQUIRED
    subclass: u8,
    // REQUIRED
    base_class: u8,
    cache_line_size: u8,
    latency_timer: u8,
    // REQUIRED
    header_type: u8,
    builtin_self_test: u8,
    base_address_register_1: u32,
    base_address_register_2: u32,
    base_address_register_3: u32,
    base_address_register_4: u32,
    base_address_register_5: u32,
    base_address_register_6: u32,
    cardbus_cis_pointer: u32,
    subsystem_vendor_id: u16,
    subsystem_id: u16,
    expansion_rom_base_address: u32,
    capabilities_pointer: u8,
    reserved_1: Reserved<3>,
    reserved_2: Reserved<4>,
    interrupt_line: u8,
    interrupt_pin: u8,
    min_gnt: u8,
    max_lat: u8,
}

impl TryFrom<&[u8]> for ConfigurationSpaceHeader {
    type Error = Error;

    fn try_from(bytes: &[u8]) -> error::Result<Self> {
        let configuration_space_header_raw = ConfigurationSpaceHeader::try_read_from_prefix(bytes)
            .map(|(result, _rest)| result)
            .map_err(convert_try_read_error)?;
        let _ = HeaderType::try_from(configuration_space_header_raw.header_type & 0x7f)
            .map_err(|err| -> error::Error { Fault::InvalidPCIHeaderType(err.number).into() })?;

        let _ = configuration_space_header_raw.try_get_class()?;
        Ok(configuration_space_header_raw)
    }
}

impl ConfigurationSpaceHeader {
    /// # Panics
    /// Will panic if proper validation wasn't made before creating an instance of
    /// ConfigurationSpaceHeader
    pub fn get_header_type(&self) -> HeaderType {
        HeaderType::try_from(self.header_type & 0x7f)
            .expect("header_type field did not contain a valid header type in its low bits")
    }

    fn try_get_class(&self) -> error::Result<Class> {
        if self.base_class == 0x01 && self.subclass == 0x01 {
            return Ok(Class::IDEController);
        }
        if self.base_class == 0x06 && self.subclass == 0x08 {
            return Ok(Class::RACEwayBridge);
        }

        if self.base_class == 0x0e && self.subclass == 0x08 {
            return Ok(Class::IntelligentIOController);
        }

        let class = (self.base_class as u32) << 16
            | ((self.subclass as u32) << 8)
            | (self.programming_interface as u32);
        let invalid_class_error: error::Error = Fault::InvalidPCIClass(class).into();

        if self.subclass == 0x80 {
            match self.base_class {
                0x01 => return Ok(Class::MassStorageController),
                0x02 => return Ok(Class::OtherNetworkController),
                0x03 => return Ok(Class::OtherDisplayController),
                0x04 => return Ok(Class::OtherMultimediaDevice),
                0x05 => return Ok(Class::OtherMemoryController),
                0x06 => return Ok(Class::OtherBridgeDevice),
                0x07 => return Ok(Class::OtherCommunicationsDevice),
                0x08 => return Ok(Class::OtherSystemPeripheral),
                0x09 => return Ok(Class::OtherInputController),
                0x0a => return Ok(Class::OtherTypeDockingStation),
                0x10 => return Ok(Class::OtherEnDecryption),
                0x11 => return Ok(Class::OtherDataAcquisitionSignalProcessingController),
                _ => {
                    return Err(invalid_class_error);
                }
            }
        }

        class.try_into().map_err(|_| invalid_class_error)
    }

    /// # Panics
    /// will panic if proper validation wasn't made before creating an instance of
    /// ConfigurationSpaceHeader
    pub fn get_class(&self) -> Class {
        self.try_get_class()
            .expect("an invalid class value was stored in self.class")
    }

    pub fn get_command(&self) -> CommandRegister {
        CommandRegister::from(self.command)
    }

    pub fn get_status(&self) -> DeviceStatus {
        DeviceStatus::from(self.status)
    }

    pub fn is_multi_function_device(&self) -> bool {
        (ConfigurationSpaceHeaderVersion {
            bits: self.header_type,
        })
        .is_set(ConfigurationSpaceHeaderVersionFlag::MultiFunctionDevice)
    }

    pub fn is_usb(&self) -> bool {
        matches!(
            self.get_class(),
            Class::UHCIUsb
                | Class::OHCIUsb
                | Class::GenericUsb
                | Class::UsbDevice
                | Class::EHCIUsb
                | Class::XHCIUsb
        )
    }

    pub fn base_address_register_5(&self) -> u32 {
        self.base_address_register_5
    }

    pub fn base_address_register_1(&self) -> u32 {
        self.base_address_register_1
    }
}

impl core::fmt::Display for ConfigurationSpaceHeader {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "Vendor ID: {:x}", { self.vendor_id })?;
        writeln!(f, "Device ID: {:x}", { self.device_id })?;
        writeln!(f, "Revision ID: {:x}", self.revision_id)?;
        writeln!(f, "Subsystem Vendor ID: {:x}", { self.subsystem_vendor_id })?;
        writeln!(f, "Subsystem ID: {:x}", { self.subsystem_id })?;
        writeln!(f, "Header Type: {}", self.get_header_type())?;
        writeln!(
            f,
            "Is multi-function device: {}",
            self.is_multi_function_device()
        )?;
        writeln!(f, "Class: {}", { self.get_class() })?;
        writeln!(f, "Command: {}", { self.get_command() })?;
        writeln!(f, "Status: {}", { self.get_status() })?;
        writeln!(f, "Interrupt Line: {}", self.interrupt_line)?;
        writeln!(f, "Interrupt Pin: {}", self.interrupt_pin)?;
        writeln!(f, "Base address register 1: {:#0x}", {
            self.base_address_register_1
        })?;
        writeln!(f, "Base address register 2: {:#0x}", {
            self.base_address_register_2
        })?;
        writeln!(f, "Base address register 3: {:#0x}", {
            self.base_address_register_3
        })?;
        writeln!(f, "Base address register 4: {:#0x}", {
            self.base_address_register_4
        })?;
        writeln!(f, "Base address register 5: {:#0x}", {
            self.base_address_register_5
        })?;
        writeln!(f, "Base address register 6: {:#0x}", {
            self.base_address_register_6
        })
    }
}

#[derive(TryFromPrimitive, Clone, Copy)]
#[repr(u16)]
pub enum CommandRegisterFlag {
    EnableIOSpaceAccesses = 1 << 0,
    EnableMemorySpaceAccesses = 1 << 1,
    AllowBehaveAsMaster = 1 << 2,
    AllowSpecialCyclesOperationsMonitoring = 1 << 3,
    EnableMemoryWriteAndInvalidate = 1 << 4,
    EnableVGAPaletteSnooping = 1 << 5,
    NormalResponseToParityErrors = 1 << 6,
    AddressDataStepping = 1 << 7,
    EnableSERRDriver = 1 << 8,
    AllowFastBackToBackTransactionsToDifferentAgents = 1 << 9,
}

impl core::fmt::Display for CommandRegisterFlag {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CommandRegisterFlag::EnableIOSpaceAccesses => {
                write!(f, "Enable I/O Space Accesses")
            }
            CommandRegisterFlag::EnableMemorySpaceAccesses => {
                write!(f, "Enable Memory Space Accesses")
            }
            CommandRegisterFlag::AllowBehaveAsMaster => write!(f, "Allow Behave As Master"),
            CommandRegisterFlag::AllowSpecialCyclesOperationsMonitoring => {
                write!(f, "Allow Special Cycles Operations Monitoring")
            }
            CommandRegisterFlag::EnableMemoryWriteAndInvalidate => {
                write!(f, "Enable Memory Write and Invalidate")
            }
            CommandRegisterFlag::EnableVGAPaletteSnooping => {
                write!(f, "Enable VGA Palette Snooping")
            }
            CommandRegisterFlag::NormalResponseToParityErrors => {
                write!(f, "Normal Response to Parity Errors")
            }
            CommandRegisterFlag::AddressDataStepping => write!(f, "Address/Data Stepping"),
            CommandRegisterFlag::EnableSERRDriver => write!(f, "Enable SERR# Driver"),
            CommandRegisterFlag::AllowFastBackToBackTransactionsToDifferentAgents => {
                write!(
                    f,
                    "Allow Fast Back-to-Back Transactions to Different Agents"
                )
            }
        }
    }
}

make_bitmap!(new_type: CommandRegister, underlying_flag_type: CommandRegisterFlag, repr: u16, bit_skipper: |i| i > 9);

#[repr(u8)]
pub enum DevSelTiming {
    Fast,
    Medium,
    Slow,
}

#[repr(u16)]
#[derive(TryFromPrimitive, Clone, Copy)]
pub enum DeviceStatusFlag {
    HasNewCapabilitiesList = 1 << 4,
    _66MHzCapable = 1 << 5,
    CanAcceptFastBackToBackTransactionsToDifferentAgents = 1 << 7,
    MasterDataParityError = 1 << 8,
    TargetAbort = 1 << 11,
    MasterTargetAbort = 1 << 12,
    MasterAbort = 1 << 13,
    SignaledSystemError = 1 << 14,
    DetectedParityError = 1 << 15,
}

impl core::fmt::Display for DeviceStatusFlag {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DeviceStatusFlag::HasNewCapabilitiesList => {
                write!(f, "Has New Capabilities List")
            }
            DeviceStatusFlag::_66MHzCapable => write!(f, "66 MHz Capable"),
            DeviceStatusFlag::CanAcceptFastBackToBackTransactionsToDifferentAgents => {
                write!(
                    f,
                    "Can Accept Fast Back-to-Back Transactions to Different Agents"
                )
            }
            DeviceStatusFlag::MasterDataParityError => write!(f, "Master Data Parity Error"),
            DeviceStatusFlag::TargetAbort => write!(f, "Target Abort"),
            DeviceStatusFlag::MasterTargetAbort => write!(f, "Master Target Abort"),
            DeviceStatusFlag::MasterAbort => write!(f, "Master Abort"),
            DeviceStatusFlag::SignaledSystemError => write!(f, "Signaled System Error"),
            DeviceStatusFlag::DetectedParityError => write!(f, "Detected Parity Error"),
        }
    }
}

make_bitmap!(new_type: DeviceStatus, underlying_flag_type: DeviceStatusFlag, repr: u16, bit_skipper: |i| {i < 4 || i == 6 || i == 9 || i == 10});

impl DeviceStatus {
    pub fn set_devsel_timing(&mut self, devsel_timing: DevSelTiming) {
        bits::set_bits!(bits_expr: self.bits, value: devsel_timing, n_bits: 2, starts_at_bit: 9, bits_expr_ty: u16);
    }
}

#[repr(u8)]
#[derive(TryFromPrimitive)]
pub enum MemoryBaseAddressRegisterType {
    Anywhere32Bit = 0b00,
    Anywhere64Bit = 0b10,
}

// 28 bits of base address (power of 2), prefetchable bit, 2-bit type, 0 bit
pub struct MemoryBaseAddressRegister {
    bits: u32,
}

impl MemoryBaseAddressRegister {
    const PREFETCHABLE_MASK: u32 = 0b1;
    const PREFETCHABLE_SHIFT: u32 = 0b11;
    const TYPE_SHIFT: u32 = 0x3;
    const TYPE_MASK: u32 = 1;

    pub fn is_prefetchable(&self) -> bool {
        (self.bits >> Self::PREFETCHABLE_SHIFT) & Self::PREFETCHABLE_MASK != 0
    }

    pub fn memory_addressing_type(&mut self) -> error::Result<MemoryBaseAddressRegisterType> {
        (((self.bits >> Self::TYPE_SHIFT) & Self::TYPE_MASK) as u8)
            .try_into()
            .map_err(
                |err: num_enum::TryFromPrimitiveError<MemoryBaseAddressRegisterType>| -> error::Error {
                    InvalidPCIMemoryAddressingType(err.number).into()
                },
            )
    }
}

// 30 bits of base address (power of 2), 0 reserved bit, 1 bit
pub struct IOBaseAddressRegister {
    bits: u32,
}

pub enum BaseAddressRegister {
    Memory(MemoryBaseAddressRegister),
    Io(IOBaseAddressRegister),
}

impl From<u32> for BaseAddressRegister {
    fn from(value: u32) -> Self {
        match value & 0x1 {
            0 => Self::Memory(MemoryBaseAddressRegister { bits: value }),
            1 => Self::Io(IOBaseAddressRegister { bits: value }),
            _ => unreachable!(),
        }
    }
}

#[derive(Default)]
pub struct EHCIControllers {
    config_addr: ConfigAddressRegister,
}

impl EHCIControllers {
    pub fn new() -> Self {
        Default::default()
    }

    fn try_create_ehci_controller(
        &mut self,
        config_header: &ConfigurationSpaceHeader,
        config_addr: ConfigAddressRegister,
    ) -> Option<ehci::Controller> {
        match config_header.get_class() {
            Class::EHCIUsb => Some(usb::ehci::Controller::new(
                config_header.base_address_register_1() & !0xf,
                config_addr,
            )),
            Class::UHCIUsb
            | Class::OHCIUsb
            | Class::GenericUsb
            | Class::UsbDevice
            | Class::XHCIUsb => None,
            _ => unreachable!(),
        }
    }
}

impl Iterator for EHCIControllers {
    type Item = usb::ehci::Controller;

    fn next(&mut self) -> Option<Self::Item> {
        // Brute-force enumeration
        for bus_number in self.config_addr.get_bus_number()..=MAX_BUS_NUMBER as u8 {
            self.config_addr.set_bus_number(bus_number);
            self.config_addr.set_flag(ConfigAddressRegisterFlag::Enable);
            for device_number in self.config_addr.get_device_number()..=MAX_DEVICE_NUMBER as u8 {
                self.config_addr.set_device_number(device_number);
                if let Some(Ok(ref config_header)) =
                    self.config_addr.dump_configuration_space_header()
                {
                    if config_header.is_usb() {
                        let controller_config_addr = self.config_addr.clone();
                        // Make sure that on next iteration, we go to the next device
                        self.config_addr.set_device_number(device_number + 1);
                        if let Some(value) =
                            self.try_create_ehci_controller(config_header, controller_config_addr)
                        {
                            return Some(value);
                        }
                    }

                    if config_header.is_multi_function_device() {
                        for function in
                            self.config_addr.get_function_number()..=MAX_FUNCTION_NUMBER as u8
                        {
                            self.config_addr.set_function_number(function);
                            if let Some(Ok(ref config_header)) =
                                self.config_addr.dump_configuration_space_header()
                                && config_header.is_usb()
                            {
                                let controller_config_addr = self.config_addr.clone();
                                // Make sure that on next iteration, we go to the next function
                                self.config_addr.set_device_number(function + 1);
                                if let Some(value) = self.try_create_ehci_controller(
                                    config_header,
                                    controller_config_addr,
                                ) {
                                    return Some(value);
                                }
                            }
                        }
                        self.config_addr.set_function_number(0);
                    }
                }
                self.config_addr.set_device_number(0);
            }
        }
        None
    }
}
