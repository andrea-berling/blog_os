use core::cmp::min;

// TODO: sort things in order

use thiserror::Error;
use zerocopy::{TryFromBytes, TryReadError};

pub const CONTEXT_LENGTH: usize = 16;

#[derive(Clone, Copy, Error, Debug)]
pub enum Context {
    #[error("none")]
    None,
    #[error("parsing")]
    Parsing,
    #[error("loading ELF segment into memory")]
    LoadingSegment,
    #[error("I/O")]
    Io,
    #[error("loading the kernel")]
    LoadingKernel,
    #[error("reading kernel bytes from disk")]
    ReadingKernelFromDisk,
    #[error("preparing to jump to the kernel")]
    PreparingForJumpToKernel,
    #[error("setting up control register {0}")]
    SettingUpControlRegister(&'static str),
    #[error("setting up page table")]
    SettingUpPageTable,
    #[error("setting up processor data structures")]
    SettingUpProcessor,
}

impl Error {
    pub fn new(fault: Fault, context: Context, facility: Facility) -> Self {
        Self {
            facility,
            fault,
            context,
        }
    }

    pub fn parsing_error(fault: Fault, facility: Facility) -> Self {
        Self {
            facility,
            fault,
            context: Context::Parsing,
        }
    }

    pub const fn blank() -> Self {
        Self {
            fault: Fault::None,
            context: Context::None,
            facility: Facility::None,
        }
    }
}

pub fn bounded_context<const N: usize>(context_bytes: &[u8]) -> [u8; N] {
    let mut context = [0u8; N];
    context[..min(N, context_bytes.len())]
        .copy_from_slice(&context_bytes[..min(N, context_bytes.len())]);
    context
}

pub fn try_read_error<U: TryFromBytes>(facility: Facility, err: TryReadError<&[u8], U>) -> Error {
    let dst_type_prefix = bounded_context(core::any::type_name::<U>().as_bytes());
    Error::parsing_error(
        match err {
            zerocopy::ConvertError::Alignment(_) => {
                unreachable!()
            }
            zerocopy::ConvertError::Size(size_error) => Fault::InvalidSizeForType {
                size: size_error.into_src().len(),
                dst_type_prefix,
            },
            zerocopy::ConvertError::Validity(validity_error) => Fault::InvalidValueForType {
                value_prefix: bounded_context(validity_error.into_src()),
                dst_type_prefix,
            },
        },
        facility,
    )
}

#[derive(Clone, Copy, Debug, Error)]
pub enum Fault {
    #[error("none")]
    None,
    #[error("invalid value for field '{0}'")]
    InvalidValueForField(&'static str),
    #[error("not supported endianness (Big Endian)")]
    UnsupportedEndianness,
    #[error("invalid value {value_prefix:#x?} for type {dst_type:?}", dst_type = core::str::from_utf8(dst_type_prefix))]
    InvalidValueForType {
        value_prefix: [u8; CONTEXT_LENGTH],
        dst_type_prefix: [u8; CONTEXT_LENGTH],
    },
    #[error("incorrect size {size} for destination type {dst_type:?}", dst_type = core::str::from_utf8(dst_type_prefix))]
    InvalidSizeForType {
        size: usize,
        dst_type_prefix: [u8; CONTEXT_LENGTH],
    },
    #[error("incorrect address {address:#x} for destination type {dst_type:?} with alignment {alignment}", dst_type = core::str::from_utf8(dst_type_prefix))]
    InvalidAddressForType {
        address: u64,
        dst_type_prefix: [u8; CONTEXT_LENGTH],
        alignment: usize,
    },
    #[error("not enough bytes for '{0}'")]
    NotEnoughBytesFor(&'static str),
    #[error("Invalid LBA address '{0}' (max allowed: {1})")]
    InvalidLBAAddress(u64, u64),
    #[error("Can't read into the given buffer: needed '{1}' bytes, only have {0}")]
    CantReadIntoBuffer(u64, u64),
    #[error("timeout ({0} ns)")]
    Timeout(u64),
    #[error("invalid segment parameters: virtual address: {virtual_address}, size: {size}")]
    InvalidSegmentParameters { virtual_address: u64, size: u64 },
    #[error("I/O error")]
    IOError,
    #[error("invalid elf")]
    InvalidElf,
    #[error("unsupported boot medium")]
    UnsupportedBootMedium,
    #[error("unsupported CPU feature: {0}")]
    UnsupportedFeature(Feature),
    #[error("too many sectors: {0}")]
    TooManySectors(u32),
    #[error("hanging ATA device")]
    HangingAtaDevice,
    #[error("ATA device not ready for commands")]
    AtaDeviceNotReady,
    #[error("kernel entrypoint above addressable memory for 32-bit")]
    KernelEntrypointAbove4G,
    #[error("kernel entrypoint too high for a 1MB stack")]
    KernelEntrypointTooHigh,
    #[error("kernel initialization fault")]
    KernelInitialization,
    #[error("invalid drive parameters pointer: {0:#p}")]
    InvalidDriveParametersPointer(*const u8),
    #[error("invalid stack start: {0:#x}")]
    InvalidStackStart(u32),
    #[error("couldn't identify boot device")]
    FailedBootDeviceIdentification,
}

#[derive(Debug, Error, Clone, Copy)]
pub enum Feature {
    #[error("1GB pages")]
    _1GBPages,
}

#[derive(Clone, Copy, Debug, Error)]
pub enum Facility {
    #[error("none")]
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
}

#[derive(Clone, Copy, Debug, Error)]
#[error("  (what)={fault}\n  (context)={context}\n  (where)={facility}")]
pub struct Error {
    fault: Fault,       // what happened?
    context: Context,   // what were you doing?
    facility: Facility, // where did it happen?
}

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
