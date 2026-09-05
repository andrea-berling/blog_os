use core::{
    cmp::min,
    fmt::Display,
    ops::{Index as _, IndexMut as _},
};

// TODO: sort things in order

use thiserror::Error;
use zerocopy::{TryFromBytes, TryReadError};

#[derive(Clone, Copy)]
pub struct Prelude<const N: usize>([u8; N]);

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

impl<const N: usize> core::fmt::Debug for Prelude<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.0.fmt(f)
    }
}

impl<const N: usize> core::ops::Deref for Prelude<N> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

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

pub const VALUE_LENGTH_BYTES: usize = 20;
pub const TYPE_NAME_LENGTH_BYTES: usize = 40;

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
    #[error("EHCI Controller is not halted")]
    EhciControllerNotHalted,
    #[error("Invalid USB Address: {0:#x}")]
    InvalidUSBAddress(u8),
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
    #[error("Invalid Max Packet length: {0}")]
    InvalidUSBMaxPacketLength(u16),
}

#[derive(Debug, Error, Clone, Copy)]
pub enum Feature {
    #[error("1GB pages")]
    _1GBPages,
}

#[derive(Clone, Copy, Debug)]
pub struct PciDevice {
    bus_number: u8,
    device_number: u8,
    function_number: u8,
}

impl PciDevice {
    pub fn new(bus_number: u8, device_number: u8, function_number: u8) -> Self {
        Self {
            bus_number,
            device_number,
            function_number,
        }
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
}

#[derive(Clone, Copy, Debug, Error)]
#[error("  (what)={fault}\n  (context)={context}\n  (where)={facility}")]
pub struct Error {
    fault: Fault,       // what happened?
    context: Context,   // what were you doing?
    facility: Facility, // where did it happen?
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

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug)]
pub struct ErrorChain<const N: usize> {
    errors: [Error; N],
    length: usize,
    theres_more: bool,
}

impl<const N: usize> ErrorChain<N> {
    fn push(&mut self, error: Error) {
        if self.length == N {
            self.theres_more = true;
            return;
        }
        self.errors[self.length] = error;
        self.length += 1;
    }

    fn clear(&mut self) {
        self.length = 0;
        self.theres_more = false;
    }
}

impl<const N: usize> core::fmt::Display for ErrorChain<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        enum Iter<'a> {
            LeafToRoot(core::slice::Iter<'a, Error>),
            RootToLeaf(core::iter::Rev<core::slice::Iter<'a, Error>>),
        }
        let iterator = self.errors[0..self.length].iter();
        let iterator = if f.alternate() && !self.theres_more {
            Iter::RootToLeaf(iterator.rev())
        } else {
            Iter::LeafToRoot(iterator)
        };

        impl<'a> Iterator for Iter<'a> {
            type Item = &'a Error;

            fn next(&mut self) -> Option<Self::Item> {
                match self {
                    Iter::LeafToRoot(iter) => iter.next(),
                    Iter::RootToLeaf(rev) => rev.next(),
                }
            }
        }

        writeln!(f, "Error:")?;
        for (i, error) in iterator.enumerate() {
            writeln!(f, "{error}")?;
            if i != self.length - 1 {
                writeln!(f, "{}", if f.alternate() { "Due to:" } else { "Causing:" })?;
            }
        }

        if self.theres_more {
            writeln!(f, "Error chaing length was truncated to {N}, there's more")?;
        }

        Ok(())
    }
}

static MAX_ERROR_CHAIN_LENGTH: usize = 5;
static mut GLOBAL_ERROR_CHAIN: ErrorChain<MAX_ERROR_CHAIN_LENGTH> = ErrorChain {
    errors: [Error::blank(); MAX_ERROR_CHAIN_LENGTH],
    length: 0,
    theres_more: false,
};

pub fn get_global_error_chain_no_sync() -> &'static ErrorChain<MAX_ERROR_CHAIN_LENGTH> {
    let error_chain_ptr = &raw const GLOBAL_ERROR_CHAIN;
    // SAFETY: no threads means no concurrent access
    unsafe { &*error_chain_ptr }
}

pub fn push_to_global_error_chain_no_sync(error: Error) {
    let error_chain_ptr = &raw mut GLOBAL_ERROR_CHAIN;
    // SAFETY: no threads means no concurrent access
    let error_chain = unsafe { &mut *error_chain_ptr };

    error_chain.push(error);
}

pub fn clear_global_error_chain_no_sync() {
    let error_chain_ptr = &raw mut GLOBAL_ERROR_CHAIN;
    // SAFETY: no threads means no concurrent access
    let error_chain = unsafe { &mut *error_chain_ptr };

    error_chain.clear();
}
