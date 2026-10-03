use core::{
    cmp::min,
    fmt::Display,
    ops::{Index as _, IndexMut as _},
};

// TODO: sort things in order

use thiserror::Error;
use zerocopy::{TryCastError, TryFromBytes, TryReadError};

use crate::{
    array_vec::ArrayVec,
    usb::setup::{Address, ConfigurationValue, InterfaceNumber},
};

#[derive(Clone, Copy)]
pub struct Prelude<const N: usize>([u8; N]);

#[derive(Clone, Copy, Error, Debug)]
pub enum Context {
    #[error("None")]
    None,
    #[error("Parsing")]
    Parsing,
    #[error("Loading ELF segment into memory")]
    LoadingSegment,
    #[error("I/O")]
    Io,
    #[error("Loading the kernel")]
    LoadingKernel,
    #[error("Reading kernel bytes from disk")]
    ReadingKernelFromDisk,
    #[error("Preparing to jump to the kernel")]
    PreparingForJumpToKernel,
    #[error("Setting up control register {0}")]
    SettingUpControlRegister(&'static str),
    #[error("Setting up page table")]
    SettingUpPageTable,
    #[error("Setting up processor data structures")]
    SettingUpProcessor,
    #[error("Waiting for Host Controller ownership to switch")]
    WaitingHostControllerOwnershipSwitch,
    #[error("Waiting for USB Port reset bit to clear")]
    WaitingUSBPortResetClear(u8),
    #[error("Halting EHCI controller")]
    HaltingEhciController,
    #[error("Resetting EHCI controller")]
    ResettingEhciController,
    #[error("Waiting for Async Schedule Enable")]
    WaitingForAsyncScheduleEnable,
    #[error("Setting EHCI device address")]
    SettingEHCIDeviceAddress,
    #[error("Getting device descriptor")]
    GettingDeviceDescriptor,
    #[error("Getting configuration descriptor {0}")]
    GettingConfigurationDescriptor(u8),
    #[error("Setting configuration value {0}")]
    SettingConfigurationValue(u8),
    #[error("Reading descriptor")]
    ReadingDescriptor,
    #[error("Parsing interface descriptor")]
    ParsingInterfaceDescriptor,
    #[error("Parsing endpoint descriptor")]
    ParsingEndpointDescriptor,
    #[error("Starting EHCI Schedule Execution")]
    StartingEHCIScheduleExecution,
    #[error("Stopping EHCI Schedule Execution")]
    StoppingEHCIScheduleExecution,
    #[error("Looking for USB Mass Storage devices")]
    LookingForUSBMassStorageDevices,
    #[error("Getting max LUN (interface {0})")]
    GettingMaxLUN(u8),
    #[error("Clearing feature")]
    ClearingFeature,
    #[error("SCSI inquiry on {0}")]
    ScsiInquiry(u8),
    #[error("Enumerating USB Devices")]
    EnumeratingUSBDevices,
    #[error("Building USB Mass Storage devices")]
    BuildingUSBMassStorageDevice,
    #[error("SCSI test unit ready on {0}")]
    ScsiTestUnitReady(u8),
}

#[derive(Clone, Copy, Debug, Error)]
pub enum Fault {
    #[error("None")]
    None,
    #[error("Invalid value for field '{0}'")]
    InvalidValueForField(&'static str),
    #[error("Not supported endianness (Big Endian)")]
    UnsupportedEndianness,
    #[error("Invalid value for type {dst_type:?}. First {VALUE_LENGTH_BYTES} bytes: {value:#x?}", dst_type = core::str::from_utf8(dst_type_name))]
    InvalidValueForType {
        value: Prelude<VALUE_LENGTH_BYTES>,
        dst_type_name: Prelude<TYPE_NAME_LENGTH_BYTES>,
    },
    #[error("Incorrect size for destination type {dst_type:?}: {size}", dst_type = core::str::from_utf8(dst_type_name))]
    InvalidSizeForType {
        size: usize,
        dst_type_name: Prelude<TYPE_NAME_LENGTH_BYTES>,
    },
    #[error("Incorrect address for destination type {dst_type:?}: {address:#x} with alignment {alignment}", dst_type = core::str::from_utf8(dst_type_name))]
    InvalidAddressForType {
        address: u64,
        dst_type_name: Prelude<TYPE_NAME_LENGTH_BYTES>,
        alignment: usize,
    },
    #[error("Not enough bytes for '{0}'")]
    NotEnoughBytesFor(&'static str),
    #[error("Invalid LBA address '{0}' (max allowed: {1})")]
    InvalidLBAAddress(u64, u64),
    #[error("Can't read into the given buffer: needed '{1}' bytes, only have {0}")]
    CantReadIntoBuffer(u64, u64),
    #[error("Wrong buffer size:  expected '{expected}' bytes, only have {actual}")]
    WrongBufferSize { expected: u64, actual: u64 },
    #[error("Timeout ({0} ns)")]
    Timeout(u64),
    #[error("Invalid segment parameters: virtual address: {virtual_address}, size: {size}")]
    InvalidSegmentParameters { virtual_address: u64, size: u64 },
    #[error("I/O error")]
    IOError,
    #[error("Invalid elf")]
    InvalidElf,
    #[error("Unsupported boot medium")]
    UnsupportedBootMedium,
    #[error("Unsupported CPU feature: {0}")]
    UnsupportedFeature(Feature),
    #[error("Too many sectors: {0}")]
    TooManySectors(u32),
    #[error("FDTB is not available")]
    NoFDTBAvailable,
    #[error("Device path information is not available")]
    NoDevicePathInformationAvailable,
    #[error("Not an ATA device")]
    NotAnATADevice,
    #[error("Hanging ATA device")]
    HangingAtaDevice,
    #[error("ATA device not ready for commands")]
    AtaDeviceNotReady,
    #[error("Kernel entrypoint above addressable memory for 32-bit")]
    KernelEntrypointAbove4G,
    #[error("Kernel entrypoint too high for a 1MB stack")]
    KernelEntrypointTooHigh,
    #[error("Kernel initialization fault")]
    KernelInitialization,
    #[error("Invalid drive parameters pointer: {0:#p}")]
    InvalidDriveParametersPointer(*const u8),
    #[error("Invalid stack start: {0:#x}")]
    InvalidStackStart(u32),
    #[error("Couldn't identify boot device")]
    FailedBootDeviceIdentification,
    #[error("Invalid PCI Configuration Space Header")]
    InvalidPCIConfigSpaceHeader,
    #[error("Invalid PCI Header Type")]
    InvalidPCIHeaderType(u8),
    #[error("Invalid PCI Class: {0:#x}")]
    InvalidPCIClass(u32),
    #[error("Invalid PCI Memory Addressing Type: {0:#x}")]
    InvalidPCIMemoryAddressingType(u8),
    #[error("USB Legacy Support Extended Capability not available")]
    NoUSBLEGSUP,
    #[error("EHCI Extended Capabilities Pointer not available")]
    NoEECP,
    #[error("Invalid USB Address: {0:#x}")]
    InvalidUSBAddress(u8),
    #[error("Invalid USB Max Packet length: {0}")]
    InvalidUSBMaxPacketLength(u16),
    #[error("Invalid USB Total Bytes to Transfer: {0}")]
    InvalidUSBTotalBytesToTransfer(u16),
    #[error("Invalid USB Current Page: {0}")]
    InvalidUSBCurrentPage(u8),
    #[error("Invalid USB Current Buffer Page Pointer Offset: {0}")]
    InvalidUSBCurrentBufferPagePointerOffset(u16),
    #[error("Unaligned EHCI Buffer Page Pointer: {0}")]
    UnalignedEHCIBufferPagePointer(u32),
    #[error("Out of EHCI data structures to allocate")]
    OutOfEHCIDataStructures,
    #[error("Too many EHCI data structures to allocate")]
    TooManyEHCIDataStructuresRequested,
    #[error("Invalid queue head bundle reference: {0}")]
    InvalidQueueHeadBundleReference(usize),
    #[error("Invalid transfer descriptor bundle reference: {0}")]
    InvalidTransferDescriptorBundleReference(usize),
    #[error("Invalid buffer page bundle reference: {0}")]
    InvalidBufferPageBundleReference(usize),
    #[error("Host Controller is not halted")]
    HostControllerNotHalted,
    #[error("EHCI queue transfer halted")]
    EHCITransferHalted,
    #[error("Unexpected descriptor type: 0x{0:02x}")]
    UnexpectedDescriptorType(u8),
    #[error("Invalid EHCI Extended Capabilities Pointer offset: {0:#x}")]
    InvalidEECPOffset(u8),
    #[error("ArrayVec is full (capacity: {0})")]
    FullArrayVec(usize),
    #[error(
        "Requested bitset size can not be provided: requested {desired_size}, have capacity for {capacity}"
    )]
    BitSetSizeTooBig {
        desired_size: usize,
        capacity: usize,
    },
    #[error("Out of bounds bit set index: {index} (max size: {max_size})")]
    OutOfBoundsBitSetIndex { index: usize, max_size: usize },
    #[error("Invalid EHCI buffer page offset: {offset} (max: {max})")]
    InvalidEHCIBufferOffset { offset: usize, max: usize },
    #[error("Not enough bytes for a USB descriptor")]
    NotEnoughBytesForAUSBDescriptor,
    #[error("Invalid USB descriptor header")]
    InvalidUSBDescriptorHeader,
    #[error("Fewer bytes available for a USB descriptor than the USB Device header requested")]
    FewerBytesThanUSBDeviceHeaderRequested,
    #[error("Corrupt USB interface descriptor")]
    CorruptUSBInterfaceDescriptor,
    #[error("Corrupt USB endpoint descriptor")]
    CorruptUSBEndpointDescriptor,
    #[error("Corrupt ELF header: stored object type 0x{0:04x} is invalid")]
    CorruptELFHeader(u16),
    #[error("Corrupt ELF section header entry: stored section type 0x{0:08x} is invalid")]
    CorruptELFSectionHeaderEntry(u32),
    #[error("Corrupt ELF program header entry: stored segment type 0x{0:08x} is invalid")]
    CorruptELFProgramHeaderEntry(u32),
    #[error("Invalid CSW status byte: {0:#x}")]
    InvalidCSWByte(u8),
    #[error("BBB command failed")]
    BBBCommandFailed,
    #[error("BBB phase error")]
    BBBPhaseError,
    #[error("Invalid CSW signature")]
    InvalidCSWSignature,
    #[error("CSW tag doesn't match")]
    CSWTagDoesntMatch,
    #[error("Non-zero CSW data residue: {0}")]
    CSWNonZeroDataResidue(u32),
    #[error("No Bulk In endpoint")]
    NoBulkInEndpoint,
    #[error("No Bulk Out endpoint")]
    NoBulkOutEndpoint,
    #[error("Invalid command block length: {0}")]
    InvalidCommandBlockLength(u8),
    #[error("Invalid logical unit number: {0}")]
    InvalidLogicalUnitNumber(u8),
}

#[derive(Debug, Error, Clone, Copy)]
pub enum Feature {
    #[error("1GB pages")]
    _1GBPages,
}

// TODO: get rid of this and use the type in pci
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PciDevice {
    bus_number: u8,
    device_number: u8,
    function_number: u8,
}

#[derive(Clone, Copy, Debug, Error)]
pub enum Facility {
    #[error("None")]
    None,

    // EDD
    #[error("EDD: drive parameters")]
    EDDDriveParameters,
    #[error("EDD: device path information")]
    EDDDevicePathInformation,
    #[error("EDD: fixed disk parameter table")]
    EDDFixedDiskParameterTable,

    // Elf
    #[error("ELF file")]
    ElfFile,
    #[error("ELF header")]
    ElfHeader,
    #[error("ELF section header")]
    ElfSectionHeader,
    #[error("ELF program header")]
    ElfProgramHeader,
    #[error("ELF section header entry {0}")]
    ElfSectionHeaderEntry(u16),
    #[error("ELF program header entry {0}")]
    ElfProgramHeaderEntry(u16),

    // Ata
    #[error("Ata Device (base io port: {0:#x})")]
    AtaDevice(u16),

    // Bootloader
    #[error("Bootloader")]
    Bootloader,

    // PCI
    #[error("PCI device: {0}")]
    PciDevice(PciDevice),

    // PCI
    #[error("EHCI controller: {0}")]
    EhciController(PciDevice),

    // USB
    #[error("EHCI device {1} (EHCI controller: {0})")]
    EhciDevice(PciDevice, Address),

    #[error(
        "EHCI device {address}, {configuration_value:?}, {interface_number:?} (EHCI controller: {pci_device})"
    )]
    EhciFunction {
        pci_device: PciDevice,
        address: Address,
        configuration_value: ConfigurationValue,
        interface_number: InterfaceNumber,
    },
}

#[derive(Clone, Copy, Debug, Error)]
#[error("  (what)={fault}\n  (context)={context}\n  (where)={facility}")]
pub struct Error {
    fault: Fault,       // what happened?
    context: Context,   // what were you doing?
    facility: Facility, // where did it happen?
}

pub type Result<T> = core::result::Result<T, Error>;

pub struct ErrorWithTrace {
    primary: Error,
    theres_more: bool,
}

pub const VALUE_LENGTH_BYTES: usize = 20;
pub const TYPE_NAME_LENGTH_BYTES: usize = 40;

pub type ResultWithTrace<T> = core::result::Result<T, ErrorWithTrace>;

impl PciDevice {
    pub fn new(bus_number: u8, device_number: u8, function_number: u8) -> Self {
        Self {
            bus_number,
            device_number,
            function_number,
        }
    }
}

impl Error {
    pub fn new(fault: Fault, context: Context, facility: Facility) -> Self {
        Self {
            facility,
            fault,
            context,
        }
    }

    pub fn with_context(self, context: Context) -> Self {
        Self { context, ..self }
    }

    pub fn with_facility(self, facility: Facility) -> Self {
        Self { facility, ..self }
    }

    pub fn with_fault(self, fault: Fault) -> Self {
        Self { fault, ..self }
    }

    pub const fn blank() -> Self {
        Self {
            fault: Fault::None,
            context: Context::None,
            facility: Facility::None,
        }
    }

    pub fn fault(&self) -> Fault {
        self.fault
    }

    pub fn context(&self) -> Context {
        self.context
    }

    pub fn facility(&self) -> Facility {
        self.facility
    }
}

impl<const N: usize> From<&[u8]> for Prelude<N> {
    fn from(value: &[u8]) -> Self {
        let mut inner_value = [0; N];
        let range = ..min(N, value.len());
        inner_value
            .index_mut(range)
            .copy_from_slice(value.index(range));
        Self(inner_value)
    }
}

impl ErrorWithTrace {
    pub fn push_lossy(&mut self, error: Error) {
        debug_assert!(
            !matches!(error.fault(), Fault::None),
            "an error frame must carry a fault"
        );
        let trace = &raw mut GLOBAL_ERROR_TRACE;
        // SAFETY: no threads means no concurrent access
        let trace = unsafe { &mut *trace };
        if trace.try_push(error).is_err() {
            self.theres_more = true
        }
    }

    pub fn replace_primary(&mut self, new_primary: Error) {
        debug_assert!(
            !matches!(new_primary.fault(), Fault::None),
            "an error frame must carry a fault"
        );
        let previous_primary = self.primary;
        self.push_lossy(previous_primary);
        self.primary = new_primary;
    }

    pub fn with_context(self, context: Context) -> Self {
        Self {
            primary: self.primary.with_context(context),
            ..self
        }
    }

    pub fn with_facility(self, facility: Facility) -> Self {
        Self {
            primary: self.primary.with_facility(facility),
            ..self
        }
    }

    pub fn with_fault(self, fault: Fault) -> Self {
        Self {
            primary: self.primary.with_fault(fault),
            ..self
        }
    }

    pub fn fault(&self) -> Fault {
        self.primary.fault()
    }
}

impl From<Fault> for Error {
    fn from(fault: Fault) -> Self {
        Error::blank().with_fault(fault)
    }
}

impl From<Facility> for Error {
    fn from(facility: Facility) -> Self {
        Error::blank().with_facility(facility)
    }
}

impl From<Context> for Error {
    fn from(context: Context) -> Self {
        Error::blank().with_context(context)
    }
}

impl From<Error> for ErrorWithTrace {
    fn from(value: Error) -> Self {
        // SAFETY: no threads means no concurrent access
        #[allow(static_mut_refs)]
        unsafe {
            GLOBAL_ERROR_TRACE.truncate(0);
        };
        Self {
            primary: value,
            theres_more: false,
        }
    }
}

impl From<Fault> for ErrorWithTrace {
    fn from(fault: Fault) -> Self {
        Self::from(Error::from(fault))
    }
}

impl From<Facility> for ErrorWithTrace {
    fn from(facility: Facility) -> Self {
        Self::from(Error::from(facility))
    }
}

impl From<Context> for ErrorWithTrace {
    fn from(context: Context) -> Self {
        Self::from(Error::from(context))
    }
}

impl<const N: usize> core::fmt::Debug for Prelude<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

impl Display for PciDevice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{:02x}:{:02x}.{}",
            self.bus_number, self.device_number, self.function_number
        )
    }
}

impl core::fmt::Display for ErrorWithTrace {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let trace = &raw mut GLOBAL_ERROR_TRACE;
        // SAFETY: no threads means no concurrent access
        let trace = unsafe { &mut *trace };

        writeln!(f, "Error:")?;
        if f.alternate() {
            for cause in trace.iter() {
                writeln!(f, "{cause}")?;
                writeln!(f, "Causing:")?;
            }
            writeln!(f, "{}", self.primary)?;
        } else {
            writeln!(f, "{}", self.primary)?;
            for cause in trace.iter().rev() {
                writeln!(f, "Caused by:")?;
                writeln!(f, "{cause}")?;
            }
        }

        if self.theres_more {
            writeln!(
                f,
                "Error chain length was truncated at {}, there's more",
                trace.len()
            )?;
        }

        Ok(())
    }
}

impl<const N: usize> core::ops::Deref for Prelude<N> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[macro_export]
macro_rules! with {
    (Facility::$($facility:tt)*) => {
        |err| err.with_facility(Facility::$($facility)*)
    };
    (Fault::$($fault:tt)*) => {
        |err| err.with_fault(Fault::$($fault)*)
    };
    (Context::$($context:tt)*) => {
        |err| err.with_context(Context::$($context)*)
    };
}

#[macro_export]
macro_rules! try_with_trace {
    ($expr:expr, $(context: $context:expr,)? $(facility: $facility:expr)?) => {
        match $expr {
            Ok(value) => value,
            Err(err) => {
                let err: $crate::error::ErrorWithTrace = err.into();
                return Err(err$(.with_context($context))?$(.with_facility($facility))?);
            }
        }
    };
    ($expr:expr, fail_with: $parent_error:expr) => {
        match $expr {
            Ok(value) => value,
            Err(err) => {
                let mut err: $crate::error::ErrorWithTrace = err.into();
                err.replace_primary($parent_error);
                return Err(err);
            }
        }
    };
}

pub use try_with_trace;
pub use with;

pub fn bounded_context<const N: usize>(context_bytes: &[u8]) -> [u8; N] {
    let mut context = [0u8; N];
    context[..min(N, context_bytes.len())]
        .copy_from_slice(&context_bytes[..min(N, context_bytes.len())]);
    context
}

pub fn convert_try_read_error<U: TryFromBytes>(err: TryReadError<&[u8], U>) -> Error {
    let dst_type = core::any::type_name::<U>().as_bytes();
    match err {
        zerocopy::ConvertError::Alignment(_) => {
            unreachable!()
        }
        zerocopy::ConvertError::Size(size_error) => Fault::InvalidSizeForType {
            size: size_error.into_src().len(),
            dst_type_name: dst_type.into(),
        },
        zerocopy::ConvertError::Validity(validity_error) => Fault::InvalidValueForType {
            value: validity_error.into_src().into(),
            dst_type_name: dst_type.into(),
        },
    }
    .into()
}

pub fn convert_try_cast_error<U: TryFromBytes>(err: TryCastError<&[u8], U>) -> Error {
    let dst_type = core::any::type_name::<U>().as_bytes();
    match err {
        zerocopy::ConvertError::Alignment(_) => {
            unreachable!()
        }
        zerocopy::ConvertError::Size(size_error) => Fault::InvalidSizeForType {
            size: size_error.into_src().len(),
            dst_type_name: dst_type.into(),
        },
        zerocopy::ConvertError::Validity(validity_error) => Fault::InvalidValueForType {
            value: validity_error.into_src().into(),
            dst_type_name: dst_type.into(),
        },
    }
    .into()
}

static MAX_TRACE_LENGTH: usize = 5;
static mut GLOBAL_ERROR_TRACE: ArrayVec<Error, MAX_TRACE_LENGTH> = const { ArrayVec::new() };
