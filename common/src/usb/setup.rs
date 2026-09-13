use core::fmt::{Debug, Display};

use num_enum::TryFromPrimitive;
use zerocopy::{Immutable, KnownLayout, LE, TryFromBytes, U16};

use crate::{
    bits,
    error::{self, Fault, convert_try_cast_error, convert_try_read_error},
    make_bitmap,
    usb::{DeviceClassType, MassStorageProtocol, MassStorageSubclass},
};

pub const SMALLEST_LEGAL_MAX_PACKET_SIZE: u16 = 8;
pub const LARGEST_LEGAL_MAX_PACKET_SIZE: u16 = 1024;

#[repr(u8)]
pub enum RequestType {
    Standard,
    Class,
    Vendor,
}

#[repr(u8)]
pub enum Recipient {
    Device,
    Interface,
    Endpoint,
    Other,
}

#[repr(u8)]
pub enum BmRequestTypeBit {
    DeviceToHost = 1 << 7,
}

make_bitmap!(new_type: BmRequestType, underlying_flag_type: BmRequestTypeBit, repr: u8, nodisplay);

#[repr(u8)]
pub enum Request {
    GetStatus,
    ClearFeature,
    SetFeature = 3,
    SetAddress = 5,
    GetDescriptor,
    SetDescriptor,
    GetConfiguration,
    SetConfiguration,
    GetInterface,
    SetInterface,
    SynchFrame,
}

#[derive(Clone, Copy, Debug)]
pub enum DescriptorType {
    Device,
    Configuration,
    String,
    Interface,
    Endpoint,
    DeviceQualifier,
    OtherSpeedConfiguration,
    InterfacePower,
    Other(u8),
}

#[derive(TryFromBytes)]
pub struct VendorId([u8; 2]);

#[derive(TryFromBytes)]
pub struct ProductId([u8; 2]);

#[derive(KnownLayout, Immutable, TryFromBytes)]
#[repr(C)]
pub struct DescriptorHeader {
    length: u8,
    descriptor_type: u8,
}

#[derive(TryFromBytes)]
#[repr(C)]
pub struct DeviceDescriptor {
    header: DescriptorHeader,
    bcd_usb_release_number: U16<LE>,
    class: u8,
    subclass: u8,
    protocol: u8,
    max_packet_size_endpoint_0: u8,
    vendor_id: VendorId,
    product_id: ProductId,
    bcd_device_release_number: U16<LE>,
    manufacturer_string_index: u8,
    product_string_index: u8,
    serial_number_string_index: u8,
    n_configurations: u8,
}

pub enum ConfigurationAttribute {
    SelfPowered = 1 << 6,
    RemoteWakeup = 1 << 5,
}

make_bitmap!(new_type: ConfigurationAttributes, underlying_flag_type: ConfigurationAttribute, repr: u8, nodisplay);

#[derive(TryFromBytes)]
#[repr(C)]
pub struct ConfigurationDescriptor {
    header: DescriptorHeader,
    total_length: U16<LE>,
    n_interfaces: u8,
    configuration_value: u8,
    string_index: u8,
    attributes: ConfigurationAttributes,
    max_power: u8,
}

#[derive(TryFromPrimitive)]
#[repr(u8)]
pub enum InterfaceClassType {
    Audio = 0x01,
    Communications,
    HumanInterfaceDevice,
    Physical = 5,
    StillImaging,
    Printer,
    MassStorage,
    CDCDataDevice = 0x0a,
    SmartCard,
    ContentSecurity = 0x0d,
    Video,
    PersonalHealthcare,
    AudioVideo,
    USBCBridge = 0x12,
    USBBulckDisplayProtocol,
    MCTPOverUSBProtocolEndpoint,
    I3C = 0x3c,
    Diagnostic = 0xdc,
    WirelessController = 0xe0,
    Miscellaneous = 0xef,
    ApplicationSpecific = 0xfe,
    VendorSpecific = 0xff,
}

#[derive(TryFromBytes)]
#[repr(C)]
pub struct InterfaceDescriptor {
    header: DescriptorHeader,
    interface_number: u8,
    alternate_setting: u8,
    n_endpoints: u8,
    class: u8,
    subclass: u8,
    protocol: u8,
    string_index: u8,
}

pub enum InterfaceSubclassType {
    MassStorage(MassStorageSubclass),
}

pub enum InterfaceProtocolType {
    MassStorage(MassStorageProtocol),
}

pub enum EndpointAddressBit {
    In = 1 << 7,
}

make_bitmap!(new_type: EndpointAddress, underlying_flag_type: EndpointAddressBit, repr: u8, nodisplay);

#[derive(TryFromBytes)]
pub struct InterfaceAttributes {
    bits: u8,
}

#[derive(TryFromBytes)]
#[repr(C)]
pub struct EndpointDescriptor {
    header: DescriptorHeader,
    address: EndpointAddress,
    attributes: InterfaceAttributes,
    max_packet_size: U16<LE>,
    polling_interval: u8,
}

pub enum Descriptor {
    Device(DeviceDescriptor),
    Configuration(ConfigurationDescriptor),
    Interface(InterfaceDescriptor),
    Endpoint(EndpointDescriptor),
}

#[repr(C)]
pub struct SetupData {
    request_type: BmRequestType,
    request: Request,
    value: u16,
    index: u16,
    length: u16,
}

#[derive(Debug)]
pub struct EndpointNumber(u8);

#[derive(TryFromPrimitive, Debug)]
#[repr(u8)]
pub enum TransferType {
    Control,
    Isochronous,
    Bulk,
    Interrupt,
}

#[derive(TryFromPrimitive, Debug)]
#[repr(u8)]
pub enum SynchronizationType {
    None,
    Asynchronous,
    Adaptive,
    Synchronous,
}

#[derive(TryFromPrimitive, Debug)]
#[repr(u8)]
pub enum UsageType {
    Data,
    Feedback,
    ImplicitFeedbackData,
    Reserved,
}

#[derive(Clone, Copy)]
pub struct LanguageId;

#[derive(Clone, Copy, Debug, Default)]
pub struct Address(u8);

#[derive(Clone, Copy)]
pub struct MaxPacketLength(u16);

/// A single-step walker over a byte buffer of USB descriptors.
///
/// Each call to [`Iterator::next`] parses at most one descriptor at the cursor and
/// advances past it. The cursor policy on error is the contract callers rely on:
///
/// - On a parse failure the cursor is advanced past the offending descriptor
///   (its declared `bLength`), so the caller may simply call `next` again to skip
///   it.
/// - The two exceptions where the cursor does *not* advance and the caller must
///   stop the walk are [`Fault::InvalidUSBDescriptorHeader`] (`bLength < 2`, the
///   cursor cannot make progress past a header it cannot even read) and
///   [`Fault::NotEnoughBytesForAUSBDescriptor`] (fewer than 2 bytes remain, so
///   advancing is impossible). Continuing on either of these loops forever.
/// - `None` means the buffer is exhausted. A truncated stream surfaces as
///   [`Fault::FewerBytesThanUSBDeviceHeaderRequested`], after which one more
///   `next` call returns `None`.
pub struct DescriptorIterator<'a> {
    bytes: &'a [u8],
}

impl BmRequestType {
    /// Returns a BmRequestType fit for a SET_ADDRESS request
    pub fn set_address() -> Self {
        let mut result = Self::default();
        result.set_type(RequestType::Standard);
        result.set_recipient(Recipient::Device);
        result.clear_flag(BmRequestTypeBit::DeviceToHost);
        result
    }

    pub fn get_descriptor() -> Self {
        let mut result = Self::default();
        result.set_type(RequestType::Standard);
        result.set_recipient(Recipient::Device);
        result.set_flag(BmRequestTypeBit::DeviceToHost);
        result
    }

    fn set_type(&mut self, r#type: RequestType) {
        bits::set_bits!(bits_expr: self.bits, value: r#type, n_bits: 2, starts_at_bit: 5, bits_expr_ty: u8);
    }

    fn set_recipient(&mut self, recipient: Recipient) {
        bits::set_bits!(bits_expr: self.bits, value: recipient, n_bits: 5, starts_at_bit: 0, bits_expr_ty: u8);
    }
}

impl DeviceDescriptor {
    pub fn get_class_type(&self) -> Option<DeviceClassType> {
        DeviceClassType::try_from(self.class).ok()
    }

    pub fn max_packet_size_endpoint_0_offset() -> usize {
        core::mem::offset_of!(Self, max_packet_size_endpoint_0)
    }

    pub fn descriptor_type(&self) -> DescriptorType {
        self.header.descriptor_type.into()
    }

    pub fn n_configurations(&self) -> u8 {
        self.n_configurations
    }
}

impl Descriptor {
    pub fn descriptor_type(&self) -> DescriptorType {
        match self {
            Descriptor::Device(_) => DescriptorType::Device,
            Descriptor::Configuration(_) => DescriptorType::Configuration,
            Descriptor::Interface(_) => DescriptorType::Interface,
            Descriptor::Endpoint(_) => DescriptorType::Endpoint,
        }
    }

    pub fn traverse(bytes: &[u8]) -> DescriptorIterator<'_> {
        DescriptorIterator { bytes }
    }

    pub fn parse(bytes: &[u8]) -> error::ResultWithTrace<(Descriptor, &[u8])> {
        let (header, _) =
            DescriptorHeader::try_ref_from_prefix(bytes).map_err(convert_try_cast_error)?;
        if header.length < 2 {
            return Err(Fault::InvalidUSBDescriptorHeader.into());
        }
        if header.length as usize > bytes.len() {
            return Err(Fault::FewerBytesThanUSBDeviceHeaderRequested.into());
        }
        match header.descriptor_type() {
            DescriptorType::Device => {
                if header.length as usize != size_of::<DeviceDescriptor>() {
                    return Err(Fault::InvalidUSBDescriptorHeader.into());
                }
                DeviceDescriptor::try_read_from_prefix(bytes)
                    .map(|(descriptor, bytes)| (Descriptor::Device(descriptor), bytes))
                    .map_err(|err| convert_try_read_error(err).into())
            }
            DescriptorType::Configuration => {
                if header.length as usize != size_of::<ConfigurationDescriptor>() {
                    return Err(Fault::InvalidUSBDescriptorHeader.into());
                }
                ConfigurationDescriptor::try_read_from_prefix(bytes)
                    .map(|(descriptor, bytes)| (Descriptor::Configuration(descriptor), bytes))
                    .map_err(|err| convert_try_read_error(err).into())
            }
            DescriptorType::String => todo!(),
            DescriptorType::Interface => {
                if header.length as usize != size_of::<InterfaceDescriptor>() {
                    return Err(Fault::InvalidUSBDescriptorHeader.into());
                }
                InterfaceDescriptor::try_read_from_prefix(bytes)
                    .map(|(descriptor, bytes)| (Descriptor::Interface(descriptor), bytes))
                    .map_err(|err| convert_try_read_error(err).into())
            }
            DescriptorType::Endpoint => {
                if header.length as usize != size_of::<EndpointDescriptor>() {
                    return Err(Fault::InvalidUSBDescriptorHeader.into());
                }
                EndpointDescriptor::try_read_from_prefix(bytes)
                    .map(|(descriptor, bytes)| (Descriptor::Endpoint(descriptor), bytes))
                    .map_err(|err| convert_try_read_error(err).into())
            }
            DescriptorType::DeviceQualifier => todo!(),
            DescriptorType::OtherSpeedConfiguration => todo!(),
            DescriptorType::InterfacePower => todo!(),
            DescriptorType::Other(..) => todo!(),
        }
    }
}

impl SetupData {
    pub fn set_address(address: Address) -> SetupData {
        Self {
            request_type: BmRequestType::set_address(),
            request: Request::SetAddress,
            value: u8::from(address).into(),
            index: 0,
            length: 0,
        }
    }

    pub fn get_descriptor(
        descriptor_type: DescriptorType,
        descriptor_index: u8,
        lang_id: Option<LanguageId>,
        descriptor_length: u16,
    ) -> SetupData {
        let mut value = 0u16;
        bits::set_bits!(bits_expr: value, value: u8::from(descriptor_type), n_bits: 8, starts_at_bit: 8, bits_expr_ty: u16);
        bits::set_bits!(bits_expr: value, value: descriptor_index, n_bits: 8, starts_at_bit: 0, bits_expr_ty: u16);
        Self {
            request_type: BmRequestType::get_descriptor(),
            request: Request::GetDescriptor,
            value,
            index: lang_id.map_or(0, |_| todo!()),
            length: descriptor_length,
        }
    }
}

impl MaxPacketLength {
    /// Initial MaxPacketSize for the default control pipe (endpoint 0), before the
    /// device's real `bMaxPacketSize0` is known. 64 is the maximum legal value for a
    /// high-speed control endpoint, so it is guaranteed to accommodate the fixed 8-byte
    /// SETUP packet and every legal response.
    pub const DEFAULT_CONTROL_PIPE_MAX_PACKET_LENGTH: Self = Self(64);
}

impl TryFrom<u8> for Address {
    type Error = error::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value > 127 {
            return Err(Fault::InvalidUSBAddress(value).into());
        }
        Ok(Self(value))
    }
}

impl From<Address> for u8 {
    fn from(value: Address) -> Self {
        value.0
    }
}

impl TryFrom<u16> for MaxPacketLength {
    type Error = error::Error;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        // 8 is the smallest legal wMaxPacketSize (low-speed control); 1024 is the
        // largest (high-speed bulk/interrupt/isochronous). 0 in particular is invalid
        // even though it fits the 11-bit field
        if !(SMALLEST_LEGAL_MAX_PACKET_SIZE..=LARGEST_LEGAL_MAX_PACKET_SIZE).contains(&value) {
            return Err(Fault::InvalidUSBMaxPacketLength(value).into());
        }
        Ok(Self(value))
    }
}

impl From<MaxPacketLength> for u16 {
    fn from(value: MaxPacketLength) -> Self {
        value.0
    }
}

impl From<u8> for DescriptorType {
    fn from(value: u8) -> Self {
        match value {
            0x1 => Self::Device,
            0x2 => Self::Configuration,
            0x3 => Self::String,
            0x4 => Self::Interface,
            0x5 => Self::Endpoint,
            0x6 => Self::DeviceQualifier,
            0x7 => Self::OtherSpeedConfiguration,
            0x8 => Self::InterfacePower,
            _ => Self::Other(value),
        }
    }
}

impl From<DescriptorType> for u8 {
    fn from(value: DescriptorType) -> Self {
        match value {
            DescriptorType::Device => 0x1,
            DescriptorType::Configuration => 0x2,
            DescriptorType::String => 0x3,
            DescriptorType::Interface => 0x4,
            DescriptorType::Endpoint => 0x5,
            DescriptorType::DeviceQualifier => 0x6,
            DescriptorType::OtherSpeedConfiguration => 0x7,
            DescriptorType::InterfacePower => 0x8,
            DescriptorType::Other(other) => other,
        }
    }
}

impl Display for DescriptorType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DescriptorType::Device => write!(f, "Device"),
            DescriptorType::Configuration => write!(f, "Configuration"),
            DescriptorType::String => write!(f, "String"),
            DescriptorType::Interface => write!(f, "Interface"),
            DescriptorType::Endpoint => write!(f, "Endpoint"),
            DescriptorType::DeviceQualifier => write!(f, "Device Qualifier"),
            DescriptorType::OtherSpeedConfiguration => write!(f, "Other Speed Configuration"),
            DescriptorType::InterfacePower => write!(f, "Interface Power"),
            DescriptorType::Other(descriptor) => write!(f, "Unknown (0x{descriptor:02x})"),
        }
    }
}

impl Display for VendorId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:04x}", u16::from_le_bytes(self.0))
    }
}

impl Display for ProductId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:04x}", u16::from_le_bytes(self.0))
    }
}

impl Display for DeviceDescriptor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Self {
            header,
            bcd_usb_release_number,
            class,
            subclass,
            protocol,
            max_packet_size_endpoint_0: max_packet_size,
            vendor_id,
            product_id,
            bcd_device_release_number,
            manufacturer_string_index,
            product_string_index,
            serial_number_string_index,
            n_configurations,
        } = self;
        let DescriptorHeader {
            length,
            descriptor_type,
        } = header;
        writeln!(f, "Descriptor Length: {length}")?;
        writeln!(f, "Descriptor type: {}", descriptor_type)?;
        writeln!(f, "BCD USB Release number: 0x{bcd_usb_release_number:04x}")?;
        write!(f, "Descriptor Class: {class:#x} (")?;
        if let Some(class_type) = self.get_class_type() {
            write!(f, "{class_type}")?;
        } else {
            write!(f, "UNKNOWN")?;
        }
        writeln!(f, ")")?;
        writeln!(f, "Descriptor Subclass: {subclass:#x}")?;
        writeln!(f, "Descriptor Protocol: {protocol:#x}")?;
        writeln!(f, "Max packet size : {max_packet_size}")?;
        writeln!(f, "Vendor ID and Product ID: {vendor_id}:{product_id}")?;
        writeln!(
            f,
            "BCD Device Release number: 0x{bcd_device_release_number:04x}"
        )?;
        writeln!(f, "Manufacturer string index: {manufacturer_string_index}")?;
        writeln!(f, "Product string index: {product_string_index}")?;
        writeln!(
            f,
            "Serial number string index: {serial_number_string_index}"
        )?;
        writeln!(f, "Number of configurations: {n_configurations}")?;
        Ok(())
    }
}

impl core::fmt::Display for Address {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl EndpointAddress {
    pub fn get_number(&self) -> EndpointNumber {
        EndpointNumber(
            bits::get_bits!(bits_expr: self.bits, n_bits: 4, starts_at_bit: 0, return_ty: u8),
        )
    }
}

impl InterfaceAttributes {
    /// Returns the transfer type of this [`InterfaceAttributes`].
    ///
    /// # Panics
    ///
    /// Never
    pub fn get_transfer_type(&self) -> TransferType {
        TransferType::try_from(
            bits::get_bits!(bits_expr: self.bits, n_bits: 2, starts_at_bit: 0, return_ty: u8),
        )
        .expect("this can't heappen: 2 bits, four enum cases, always successful conversion")
    }

    /// Returns synchronization type of this [`InterfaceAttributes`].
    ///
    /// # Panics
    ///
    /// Never
    pub fn get_synchronization_type(&self) -> SynchronizationType {
        SynchronizationType::try_from(
            bits::get_bits!(bits_expr: self.bits, n_bits: 2, starts_at_bit: 2, return_ty: u8),
        )
        .expect("this can't heappen: 2 bits, four enum cases, always successful conversion")
    }

    /// Returns usage type of this [`InterfaceAttributes`].
    ///
    /// # Panics
    ///
    /// Never
    pub fn get_usage_type(&self) -> UsageType {
        UsageType::try_from(
            bits::get_bits!(bits_expr: self.bits, n_bits: 2, starts_at_bit: 4, return_ty: u8),
        )
        .expect("this can't heappen: 2 bits, four enum cases, always successful conversion")
    }
}

impl Display for InterfaceClassType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InterfaceClassType::Audio => write!(f, "Audio"),
            InterfaceClassType::Communications => write!(f, "Communications and CDC Control"),
            InterfaceClassType::HumanInterfaceDevice => write!(f, "Human Interface Device"),
            InterfaceClassType::Physical => write!(f, "Physical"),
            InterfaceClassType::StillImaging => write!(f, "Still Imaging"),
            InterfaceClassType::Printer => write!(f, "Printer"),
            InterfaceClassType::MassStorage => write!(f, "Mass Storage"),
            InterfaceClassType::CDCDataDevice => write!(f, "CDC-Data"),
            InterfaceClassType::SmartCard => write!(f, "Smart Card"),
            InterfaceClassType::ContentSecurity => write!(f, "Content Security"),
            InterfaceClassType::Video => write!(f, "Video"),
            InterfaceClassType::PersonalHealthcare => write!(f, "Personal Healthcare"),
            InterfaceClassType::AudioVideo => write!(f, "Audio/Video Devices"),
            InterfaceClassType::USBCBridge => write!(f, "USB Type-C Bridge"),
            InterfaceClassType::USBBulckDisplayProtocol => write!(f, "USB Bulk Display Protocol"),
            InterfaceClassType::MCTPOverUSBProtocolEndpoint => {
                write!(f, "MCTP over USB Protocol Endpoint")
            }
            InterfaceClassType::I3C => write!(f, "I3C Device"),
            InterfaceClassType::Diagnostic => write!(f, "Diagnostic Device"),
            InterfaceClassType::WirelessController => write!(f, "Wireless Controller"),
            InterfaceClassType::Miscellaneous => write!(f, "Miscellaneous"),
            InterfaceClassType::ApplicationSpecific => write!(f, "Application Specific"),
            InterfaceClassType::VendorSpecific => write!(f, "Vendor Specific"),
        }
    }
}

impl InterfaceDescriptor {
    pub fn get_class_type(&self) -> Option<InterfaceClassType> {
        InterfaceClassType::try_from(self.class).ok()
    }

    pub fn get_subclass_type(&self) -> Option<InterfaceSubclassType> {
        match self.get_class_type()? {
            InterfaceClassType::Audio => todo!(),
            InterfaceClassType::Communications => todo!(),
            InterfaceClassType::HumanInterfaceDevice => todo!(),
            InterfaceClassType::Physical => todo!(),
            InterfaceClassType::StillImaging => todo!(),
            InterfaceClassType::Printer => todo!(),
            InterfaceClassType::MassStorage => MassStorageSubclass::try_from(self.subclass)
                .ok()
                .map(InterfaceSubclassType::MassStorage),
            InterfaceClassType::CDCDataDevice => todo!(),
            InterfaceClassType::SmartCard => todo!(),
            InterfaceClassType::ContentSecurity => todo!(),
            InterfaceClassType::Video => todo!(),
            InterfaceClassType::PersonalHealthcare => todo!(),
            InterfaceClassType::AudioVideo => todo!(),
            InterfaceClassType::USBCBridge => todo!(),
            InterfaceClassType::USBBulckDisplayProtocol => todo!(),
            InterfaceClassType::MCTPOverUSBProtocolEndpoint => todo!(),
            InterfaceClassType::I3C => todo!(),
            InterfaceClassType::Diagnostic => todo!(),
            InterfaceClassType::WirelessController => todo!(),
            InterfaceClassType::Miscellaneous => todo!(),
            InterfaceClassType::ApplicationSpecific => todo!(),
            InterfaceClassType::VendorSpecific => todo!(),
        }
    }

    pub fn get_protocol_type(&self) -> Option<InterfaceProtocolType> {
        match self.get_class_type()? {
            InterfaceClassType::Audio => todo!(),
            InterfaceClassType::Communications => todo!(),
            InterfaceClassType::HumanInterfaceDevice => todo!(),
            InterfaceClassType::Physical => todo!(),
            InterfaceClassType::StillImaging => todo!(),
            InterfaceClassType::Printer => todo!(),
            InterfaceClassType::MassStorage => MassStorageProtocol::try_from(self.protocol)
                .ok()
                .map(InterfaceProtocolType::MassStorage),
            InterfaceClassType::CDCDataDevice => todo!(),
            InterfaceClassType::SmartCard => todo!(),
            InterfaceClassType::ContentSecurity => todo!(),
            InterfaceClassType::Video => todo!(),
            InterfaceClassType::PersonalHealthcare => todo!(),
            InterfaceClassType::AudioVideo => todo!(),
            InterfaceClassType::USBCBridge => todo!(),
            InterfaceClassType::USBBulckDisplayProtocol => todo!(),
            InterfaceClassType::MCTPOverUSBProtocolEndpoint => todo!(),
            InterfaceClassType::I3C => todo!(),
            InterfaceClassType::Diagnostic => todo!(),
            InterfaceClassType::WirelessController => todo!(),
            InterfaceClassType::Miscellaneous => todo!(),
            InterfaceClassType::ApplicationSpecific => todo!(),
            InterfaceClassType::VendorSpecific => todo!(),
        }
    }

    pub fn get_class(&self) -> Option<super::Class> {
        match self.get_class_type()? {
            InterfaceClassType::Audio => todo!(),
            InterfaceClassType::Communications => todo!(),
            InterfaceClassType::HumanInterfaceDevice => todo!(),
            InterfaceClassType::Physical => todo!(),
            InterfaceClassType::StillImaging => todo!(),
            InterfaceClassType::Printer => todo!(),
            InterfaceClassType::MassStorage => {
                let InterfaceSubclassType::MassStorage(subclass) = self.get_subclass_type()?;
                let InterfaceProtocolType::MassStorage(protocol) = self.get_protocol_type()?;

                Some(super::Class::MassStorage(subclass, protocol))
            }
            InterfaceClassType::CDCDataDevice => todo!(),
            InterfaceClassType::SmartCard => todo!(),
            InterfaceClassType::ContentSecurity => todo!(),
            InterfaceClassType::Video => todo!(),
            InterfaceClassType::PersonalHealthcare => todo!(),
            InterfaceClassType::AudioVideo => todo!(),
            InterfaceClassType::USBCBridge => todo!(),
            InterfaceClassType::USBBulckDisplayProtocol => todo!(),
            InterfaceClassType::MCTPOverUSBProtocolEndpoint => todo!(),
            InterfaceClassType::I3C => todo!(),
            InterfaceClassType::Diagnostic => todo!(),
            InterfaceClassType::WirelessController => todo!(),
            InterfaceClassType::Miscellaneous => todo!(),
            InterfaceClassType::ApplicationSpecific => todo!(),
            InterfaceClassType::VendorSpecific => todo!(),
        }
    }

    pub fn n_endpoints(&self) -> u8 {
        self.n_endpoints
    }
}

impl Display for InterfaceDescriptor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Self {
            header,
            interface_number,
            alternate_setting,
            n_endpoints,
            class,
            subclass,
            protocol,
            string_index,
        } = self;
        let DescriptorHeader {
            length,
            descriptor_type,
        } = header;
        writeln!(f, "Descriptor Length: {length}")?;
        writeln!(f, "Descriptor type: {}", descriptor_type)?;
        writeln!(f, "Interface number: {interface_number}")?;
        writeln!(f, "Alternate setting: {alternate_setting}")?;
        writeln!(f, "Number of endpoints: {n_endpoints}")?;
        write!(f, "Descriptor Class: ")?;
        if let Some(class) = self.get_class() {
            writeln!(f, "{class:?}")?;
        } else {
            write!(f, "{class:#x} (")?;
            if let Some(class_type) = self.get_class_type() {
                write!(f, "{class_type}")?;
            } else {
                write!(f, "UNKNOWN")?;
            }
            writeln!(f, ")")?;
            write!(f, "Descriptor Subclass: ")?;
            write!(f, "{subclass:#x} (")?;
            if let Some(subclass_type) = self.get_subclass_type() {
                write!(f, "{subclass_type}")?;
            } else {
                write!(f, "UNKNOWN")?;
            }
            writeln!(f, ")")?;
            write!(f, "Descriptor Protocol: ")?;
            write!(f, "{protocol:#x} (")?;
            if let Some(protocol_type) = self.get_protocol_type() {
                write!(f, "{protocol_type}")?;
            } else {
                write!(f, "UNKNOWN")?;
            }
            writeln!(f, ")")?;
        }
        writeln!(f, "String index: {string_index}")?;
        Ok(())
    }
}

impl Display for EndpointDescriptor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Self {
            header,
            address,
            attributes,
            max_packet_size,
            polling_interval,
        } = self;
        let DescriptorHeader {
            length,
            descriptor_type,
        } = header;
        writeln!(f, "Descriptor Length: {length}")?;
        writeln!(f, "Descriptor type: {}", descriptor_type)?;
        write!(
            f,
            "Endpoint address: {:?} ({}",
            address.get_number(),
            if address.is_set(EndpointAddressBit::In) {
                "IN"
            } else {
                "OUT"
            }
        )?;
        writeln!(f, ")")?;
        writeln!(f, "Transfer type: {:?}", attributes.get_transfer_type())?;
        writeln!(
            f,
            "Synchronization type: {:?}",
            attributes.get_synchronization_type()
        )?;
        writeln!(f, "Usage type: {:?}", attributes.get_usage_type())?;
        writeln!(f, "Max packet size: {max_packet_size}")?;
        writeln!(f, "Polling interval: {polling_interval}")?;
        Ok(())
    }
}

impl ConfigurationDescriptor {
    pub fn total_length_offset() -> usize {
        core::mem::offset_of!(Self, total_length)
    }

    pub fn total_length(&self) -> U16<zerocopy::LittleEndian> {
        self.total_length
    }

    pub fn n_interfaces(&self) -> u8 {
        self.n_interfaces
    }
}

impl Display for InterfaceSubclassType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InterfaceSubclassType::MassStorage(mass_storage_subclass) => {
                mass_storage_subclass.fmt(f)
            }
            _ => todo!(),
        }
    }
}

impl Display for InterfaceProtocolType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InterfaceProtocolType::MassStorage(mass_storage_protocol) => {
                mass_storage_protocol.fmt(f)
            }
            _ => todo!(),
        }
    }
}

impl<'a> Iterator for DescriptorIterator<'a> {
    type Item = error::ResultWithTrace<Descriptor>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.is_empty() {
            return None;
        }
        if self.bytes.len() < size_of::<DescriptorHeader>() {
            return Some(Err(Fault::NotEnoughBytesForAUSBDescriptor.into()));
        }
        let Ok((header, _)) = DescriptorHeader::try_ref_from_prefix(self.bytes) else {
            unreachable!()
        };
        Some(
            Descriptor::parse(self.bytes)
                .map(|(descriptor, bytes)| {
                    self.bytes = bytes;
                    descriptor
                })
                .inspect_err(|err| {
                    if let Fault::InvalidUSBDescriptorHeader = err.fault() {
                        return;
                    }
                    self.bytes = &self.bytes[header.length as usize..]
                }),
        )
    }
}

impl DescriptorHeader {
    pub fn descriptor_type(&self) -> DescriptorType {
        self.descriptor_type.into()
    }
}

impl<'a> DescriptorIterator<'a> {
    /// Applies the skip policy on top of [`DescriptorIterator::next`].
    ///
    /// Walks the buffer, silently skipping descriptors that do not parse (the
    /// cursor already advanced past them, so a retry simply resumes after the
    /// offending entry) and yielding only what parsed successfully. This is
    /// what makes class-specific and otherwise unrecognized descriptors
    /// transparent to the walker.
    ///
    /// The two exceptions where the walk stops are structural corruption, the
    /// faults on which [`DescriptorIterator::next`] cannot make progress:
    /// [`Fault::InvalidUSBDescriptorHeader`] and
    /// [`Fault::NotEnoughBytesForAUSBDescriptor`]. These are returned as
    /// `Some(Err(..))` for the caller to propagate; every other fault is
    /// skipped and the walk continues.
    ///
    /// Returns `None` once the buffer is exhausted. Note that a truncated
    /// stream ends the walk here as `None` rather than as an error: the
    /// remaining length checks (interface and endpoint counts) are what detect
    /// the shortfall.
    pub fn next_parsed(&mut self) -> Option<error::ResultWithTrace<Descriptor>> {
        loop {
            match self.next()? {
                Ok(descriptor) => return Some(Ok(descriptor)),
                // structural corruption: the iterator cannot make progress, so stop
                Err(err)
                    if matches!(
                        err.fault(),
                        Fault::InvalidUSBDescriptorHeader | Fault::NotEnoughBytesForAUSBDescriptor
                    ) =>
                {
                    return Some(Err(err));
                }
                // everything else: iterator already skipped past it, retry
                Err(_) => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DescriptorType;

    #[test]
    fn descriptor_type_round_trips_through_u8() {
        for raw in 0..=u8::MAX {
            assert_eq!(u8::from(DescriptorType::from(raw)), raw);
        }
    }
}
