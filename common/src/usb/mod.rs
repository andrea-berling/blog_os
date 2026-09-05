use core::fmt::Display;

use num_enum::TryFromPrimitive;

pub mod ehci;
pub mod setup;

#[derive(TryFromPrimitive)]
#[repr(u8)]
pub enum ClassType {
    UseInterfaceDescriptors,
    Audio,
    Communications,
    HumanInterfaceDevice,
    Physical = 5,
    StillImaging,
    Printer,
    MassStorage,
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
    I3C = 0x3c,
    Diagnostic = 0xdc,
    WirelessController = 0xe0,
    Miscellaneous = 0xef,
    ApplicationSpecific = 0xfe,
    VendorSpecific = 0xff,
}

impl Display for ClassType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ClassType::UseInterfaceDescriptors => {
                write!(f, "Use class code info from Interface Descriptors")
            }
            ClassType::Audio => write!(f, "Audio"),
            ClassType::Communications => write!(f, "Communications and CDC Control"),
            ClassType::HumanInterfaceDevice => write!(f, "Human Interface Device"),
            ClassType::Physical => write!(f, "Physical"),
            ClassType::StillImaging => write!(f, "Still Imaging"),
            ClassType::Printer => write!(f, "Printer"),
            ClassType::MassStorage => write!(f, "Mass Storage"),
            ClassType::Hub => write!(f, "Hub"),
            ClassType::CDCDataDevice => write!(f, "CDC-Data"),
            ClassType::SmartCard => write!(f, "Smart Card"),
            ClassType::ContentSecurity => write!(f, "Content Security"),
            ClassType::Video => write!(f, "Video"),
            ClassType::PersonalHealthcare => write!(f, "Personal Healthcare"),
            ClassType::AudioVideo => write!(f, "Audio/Video Devices"),
            ClassType::Billboard => write!(f, "Billboard"),
            ClassType::USBCBridge => write!(f, "USB Type-C Bridge"),
            ClassType::USBBulckDisplayProtocol => write!(f, "USB Bulk Display Protocol"),
            ClassType::MCTPOverUSBProtocolEndpoint => write!(f, "MCTP over USB Protocol Endpoint"),
            ClassType::I3C => write!(f, "I3C Device"),
            ClassType::Diagnostic => write!(f, "Diagnostic Device"),
            ClassType::WirelessController => write!(f, "Wireless Controller"),
            ClassType::Miscellaneous => write!(f, "Miscellaneous"),
            ClassType::ApplicationSpecific => write!(f, "Application Specific"),
            ClassType::VendorSpecific => write!(f, "Vendor Specific"),
        }
    }
}

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

pub enum MassStorageProtocol {
    CBIWithCommandCompletionInterrupt,
    CBIWithoutCommandCompletionInterrupt,
    Reserved03h4fh,
    Bbb, // 0x50
    Reserved51h61h,
    Uas, // 0x62
    Reserved63hfeh,
    VendorSpecific, // 0xff
}
