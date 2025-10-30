use core::cmp::min;

use thiserror::Error;
use zerocopy::{TryFromBytes, TryReadError};

pub const CONTEXT_LENGTH: usize = 16;

#[derive(Error, Debug)]
pub enum Reason {
    #[error("invalid value {0:#x}")]
    InvalidValue(u64),
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
    #[error("invalid values for reserved bits")]
    InvalidValuesForReservedBits,
}

#[derive(Error, Debug)]
pub enum Kind {
    #[error("can't read '{0}' field: {1}")]
    CantReadField(&'static str, Reason),
    #[error("can't fit '{0}': not enough bytes")]
    CantFit(&'static str),
    #[error(transparent)]
    Generic(#[from] Reason),
}

#[derive(Error, Debug)]
pub enum Context {
    #[error("parsing")]
    Parsing,
}

#[derive(Error, Debug)]
#[error("(where)={facility} (context)={context} (kind)={kind}")]
pub struct InternalError<Facility: core::error::Error> {
    facility: Facility,
    kind: Kind,
    context: Context,
}

impl<Facility: core::error::Error> InternalError<Facility> {
    pub fn new(facility: Facility, kind: Kind, context: Context) -> Self {
        Self {
            facility,
            kind,
            context,
        }
    }
}

#[derive(Error, Debug)]
pub enum Error<Facility: core::error::Error> {
    #[error("internal error: {0}")]
    InternalError(#[from] InternalError<Facility>),
    #[error("couldn't format to string: {0}")]
    FormattingError(#[from] core::fmt::Error),
}

pub type Result<T, Facility> = core::result::Result<T, Error<Facility>>;

fn bounded_context<const N: usize>(context_bytes: &[u8]) -> [u8; N] {
    let mut context = [0u8; N];
    context[..min(N, context_bytes.len())]
        .copy_from_slice(&context_bytes[..min(N, context_bytes.len())]);
    context
}

pub fn try_read_error<U: TryFromBytes, Facility: core::error::Error>(
    facility: Facility,
    err: TryReadError<&[u8], U>,
) -> Error<Facility> {
    use Kind::*;
    use Reason::*;
    let dst_type_prefix = bounded_context(core::any::type_name::<U>().as_bytes());
    Error::InternalError(InternalError::new(
        facility,
        match err {
            zerocopy::ConvertError::Alignment(_) => {
                unreachable!()
            }
            zerocopy::ConvertError::Size(size_error) => Generic(InvalidSizeForType {
                size: size_error.into_src().len(),
                dst_type_prefix,
            }),
            zerocopy::ConvertError::Validity(validity_error) => Generic(InvalidValueForType {
                value_prefix: bounded_context(validity_error.into_src()),
                dst_type_prefix,
            }),
        },
        Context::Parsing,
    ))
}
