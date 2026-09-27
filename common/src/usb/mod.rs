use core::fmt::Display;

use num_enum::TryFromPrimitive;

use crate::error;

pub mod bbb;
pub mod ehci;
pub mod mass_storage;
pub mod setup;

#[derive(TryFromPrimitive)]
#[repr(u8)]
pub enum DeviceClassType {
    UseInterfaceDescriptors,
    Communications = 0x02,
    Hub = 0x09,
    Billboard = 0x11,
    Diagnostic = 0xdc,
    Miscellaneous = 0xef,
    VendorSpecific = 0xff,
}

#[derive(Debug)]
pub enum Class {
    UseInterfaceDescriptors, // 0,
    Audio,
    Communications,
    HumanInterfaceDevice,
    Physical, // 5
    StillImaging,
    Printer,
    MassStorage(MassStorageSubclass, MassStorageProtocol),
    Hub,
    CDCDataDevice,
    SmartCard,
    ContentSecurity,
    Video,
    PersonalHealthcare,
    AudioVideo,
    Billboard,
    USBCBridge,
    USBBulckDisplayProtocol,
    MCTPOverUSBProtocolEndpoint,
    I3C,                 // 0x3c,
    Diagnostic,          // = 0xdc,
    WirelessController,  // = 0xe0,
    Miscellaneous,       // = 0xef,
    ApplicationSpecific, // = 0xfe,
    VendorSpecific,      // = 0xff,
}

#[derive(Debug, TryFromPrimitive)]
#[repr(u8)]
pub enum MassStorageSubclass {
    SCSICommandSetNotReported,
    Rbc,
    Mmc5,
    Qic157,
    Ufi,
    Sff8070i,
    SCSITransparentCommandSet,
    LsdFs,
    Ieee1667,
    Reserved,       // 0x09..=0xfe
    VendorSpecific, // 0xff
}

#[derive(Debug)]
#[repr(u8)]
pub enum MassStorageProtocol {
    CBIWithCommandCompletionInterrupt,
    CBIWithoutCommandCompletionInterrupt,
    Obsolete,
    Reserved03h4fh,
    Bbb = 0x50,
    Reserved51h61h,
    Uas = 0x62,
    Reserved63hfeh,
    VendorSpecific, // 0xff
}

impl Display for DeviceClassType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DeviceClassType::UseInterfaceDescriptors => {
                write!(f, "Use class code info from Interface Descriptors")
            }
            DeviceClassType::Communications => write!(f, "Communications and CDC Control"),
            DeviceClassType::Hub => write!(f, "Hub"),
            DeviceClassType::Billboard => write!(f, "Billboard"),
            DeviceClassType::Diagnostic => write!(f, "Diagnostic Device"),
            DeviceClassType::Miscellaneous => write!(f, "Miscellaneous"),
            DeviceClassType::VendorSpecific => write!(f, "Vendor Specific"),
        }
    }
}

impl TryFrom<u8> for MassStorageProtocol {
    type Error = error::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(Self::CBIWithCommandCompletionInterrupt),
            0x01 => Ok(Self::CBIWithCommandCompletionInterrupt),
            0x02 => Ok(Self::Obsolete),
            0x03..=0x4f => Ok(Self::Reserved03h4fh),
            0x50 => Ok(Self::Bbb),
            0x51..=0x61 => Ok(Self::Reserved51h61h),
            0x62 => Ok(Self::Uas),
            0x63..=0xfe => Ok(Self::Reserved63hfeh),
            0xff => Ok(Self::VendorSpecific),
        }
    }
}
