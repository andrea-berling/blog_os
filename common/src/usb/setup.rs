use core::fmt::Display;

use zerocopy::{LE, TryFromBytes, U16};

use crate::{
    bits,
    error::{self, Fault},
    make_bitmap,
    usb::ClassType,
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
#[repr(u8)]
#[derive(TryFromBytes)]
pub enum DescriptorType {
    Device = 1,
    Configuration,
    String,
    Interface,
    Endpoint,
    DeviceQualifier,
    OtherSpeedConfiguration,
    InterfacePower,
}

#[derive(TryFromBytes)]
pub struct VendorId([u8; 2]);

#[derive(TryFromBytes)]
pub struct ProductId([u8; 2]);

#[derive(TryFromBytes)]
#[repr(C)]
pub struct DeviceDescriptor {
    length: u8,
    r#type: DescriptorType,
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

pub enum Descriptor {
    Device(DeviceDescriptor),
}

#[repr(C)]
pub struct SetupData {
    request_type: BmRequestType,
    request: Request,
    value: u16,
    index: u16,
    length: u16,
}

pub struct LanguageId;

#[derive(Clone, Copy, Default)]
pub struct Address(u8);

#[derive(Clone, Copy)]
pub struct MaxPacketLength(u16);

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
    pub fn get_class_type(&self) -> Option<ClassType> {
        ClassType::try_from(self.class).ok()
    }

    pub fn max_packet_size_endpoint_0_offset() -> usize {
        core::mem::offset_of!(DeviceDescriptor, max_packet_size_endpoint_0)
    }
}

impl Descriptor {
    pub fn descriptor_type(&self) -> DescriptorType {
        match self {
            Descriptor::Device(_) => DescriptorType::Device,
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
        descriptor_index: usize,
        lang_id: Option<LanguageId>,
        descriptor_length: u16,
    ) -> error::Result<SetupData> {
        let mut value = 0u16;
        bits::set_bits!(bits_expr: value, value: descriptor_type, n_bits: 8, starts_at_bit: 8, bits_expr_ty: u16);
        bits::set_bits!(bits_expr: value, value: descriptor_index, n_bits: 8, starts_at_bit: 0, bits_expr_ty: u16);
        Ok(Self {
            request_type: BmRequestType::get_descriptor(),
            request: Request::GetDescriptor,
            value,
            index: lang_id.map_or(0, |_| todo!()),
            length: descriptor_length,
        })
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
            length,
            r#type,
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
        writeln!(f, "Descriptor Length: {length}")?;
        writeln!(f, "Descriptor type: {}", r#type)?;
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
