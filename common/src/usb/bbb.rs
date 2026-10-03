use core::cmp::min;

use num_enum::TryFromPrimitive;
use zerocopy::TryFromBytes;

use crate::{
    error::{self, Fault},
    make_bitmap,
    scsi::LogicalUnitNumber,
};

pub struct CommandBlockWrapperSignature(u32);
#[derive(TryFromBytes)]
pub struct CommandStatusWrapperSignature(u32);
#[derive(TryFromBytes, PartialEq, Eq, Clone, Copy)]
pub struct CommandWrapperTag(u32);
#[derive(Clone, Copy)]
pub struct CommandBlockLength(u8);
pub struct CommandBlock([u8; 16]);

#[derive(Clone, Copy)]
pub enum Direction {
    DeviceToHost,
    HostToDevice,
}

pub enum CommandBlockWrapperFlag {
    DataIn = 1 << 7,
}

make_bitmap!(new_type: CommandBlockWrapperFlags, underlying_flag_type: CommandBlockWrapperFlag, repr: u8, nodisplay);

#[repr(C, packed)]
pub struct CommandBlockWrapper {
    signature: CommandBlockWrapperSignature,
    tag: CommandWrapperTag,
    data_transfer_length: u32,
    flags: CommandBlockWrapperFlags,
    logical_unit_number: LogicalUnitNumber,
    command_block_length: CommandBlockLength,
    command_block: CommandBlock,
}

#[repr(C, packed)]
// TODO: use u32<LE>, and fix up tag and signature too
#[derive(TryFromBytes)]
pub struct CommandStatusWrapper {
    signature: CommandStatusWrapperSignature,
    tag: CommandWrapperTag,
    data_residue: u32,
    status: u8,
}

#[derive(TryFromPrimitive, Debug)]
#[repr(u8)]
pub enum Status {
    CommandPassed,
    CommandFailed,
    PhaseError,
}

impl CommandBlockWrapperSignature {
    const MAGIC: Self = Self(0x43425355u32.to_le());

    pub fn new() -> Self {
        Self::MAGIC
    }
}

impl CommandStatusWrapperSignature {
    const MAGIC: Self = Self(0x53425355u32.to_le());

    pub fn new() -> Self {
        Self::MAGIC
    }

    pub fn is_valid(&self) -> bool {
        self.0 == Self::MAGIC.0
    }
}

impl CommandWrapperTag {
    pub fn increment(&mut self) {
        self.0 = self.0.wrapping_add(1)
    }
}

impl CommandBlockWrapper {
    pub fn new(
        tag: CommandWrapperTag,
        data_transfer_length: u32,
        direction: Direction,
        logical_unit_number: LogicalUnitNumber,
        command_block: CommandBlock,
        command_block_length: CommandBlockLength,
    ) -> Self {
        Self {
            signature: CommandBlockWrapperSignature::new(),
            tag,
            data_transfer_length,
            flags: match direction {
                Direction::DeviceToHost => CommandBlockWrapperFlag::DataIn.into(),
                Direction::HostToDevice => 0.into(),
            },
            logical_unit_number,
            command_block_length,
            command_block,
        }
    }

    pub fn data_transfer_length(&self) -> u32 {
        self.data_transfer_length
    }

    pub fn set_lun(&mut self, logical_unit_number: LogicalUnitNumber) -> error::Result<()> {
        if u8::from(logical_unit_number) > 15 {
            return Err(Fault::InvalidLogicalUnitNumber(logical_unit_number.into()).into());
        }
        self.logical_unit_number = logical_unit_number;
        Ok(())
    }
}

impl CommandStatusWrapper {
    pub fn get_status(&self) -> Option<Status> {
        self.status.try_into().ok()
    }

    pub fn status_raw(&self) -> u8 {
        self.status
    }

    pub fn tag(&self) -> CommandWrapperTag {
        // SAFETY: same as Self::signature()
        unsafe { (&raw const self.tag).read_unaligned() }
    }

    pub fn data_residue(&self) -> u32 {
        self.data_residue
    }

    pub fn signature(&self) -> CommandStatusWrapperSignature {
        // SAFETY: reading a u32 from a reference to a struct owned by self is sound
        unsafe { (&raw const self.signature).read_unaligned() }
    }

    pub fn validate(&self, tag: CommandWrapperTag) -> error::Result<()> {
        crate::serial::log::debug_no_sync!("Status:\n{self}");

        if !self.signature().is_valid() {
            return Err(Fault::InvalidCSWSignature.into());
        }

        if self.tag() != tag {
            return Err(Fault::CSWTagDoesntMatch.into());
        }

        if self.data_residue() != 0 {
            return Err(Fault::CSWNonZeroDataResidue(self.data_residue()).into());
        }

        match self.get_status() {
            Some(status) => match status {
                Status::CommandPassed => {}
                Status::CommandFailed => {
                    return Err(Fault::BBBCommandFailed.into());
                }
                Status::PhaseError => {
                    return Err(Fault::BBBPhaseError.into());
                }
            },
            None => {
                return Err(Fault::InvalidCSWByte(self.status_raw()).into());
            }
        }

        Ok(())
    }
}

impl Default for CommandBlockWrapperSignature {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for CommandStatusWrapperSignature {
    fn default() -> Self {
        Self::new()
    }
}

impl From<u32> for CommandWrapperTag {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

impl From<CommandWrapperTag> for u32 {
    fn from(value: CommandWrapperTag) -> Self {
        value.0
    }
}

impl From<CommandBlockLength> for u8 {
    fn from(value: CommandBlockLength) -> Self {
        value.0
    }
}

impl TryFrom<u8> for CommandBlockLength {
    type Error = error::Error;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if (1..=16).contains(&value) {
            Ok(Self(value))
        } else {
            Err(Fault::InvalidCommandBlockLength(value).into())
        }
    }
}

impl TryFrom<&[u8]> for CommandBlock {
    type Error = error::Error;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        let _ = CommandBlockLength::try_from(value.len() as u8)?;
        let len = min(value.len(), 16);
        let mut result = Self([0; _]);
        result.0[..len].copy_from_slice(&value[..len]);
        Ok(result)
    }
}

impl core::fmt::Display for CommandStatusWrapper {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "Signature: {:#x}", self.signature().0)?;
        writeln!(f, "Tag: {:#x}", self.tag().0)?;
        writeln!(f, "Data residue: {}", self.data_residue())?;
        writeln!(f, "Status: {:?}", self.get_status())?;
        Ok(())
    }
}

impl core::fmt::Display for Status {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Status::CommandPassed => f.write_str("Command passed"),
            Status::CommandFailed => f.write_str("Command failed"),
            Status::PhaseError => f.write_str("Phase error"),
        }
    }
}
