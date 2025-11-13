use core::mem::transmute;

use crate::{make_bitmap, protection::PrivilegeLevel};

#[repr(C, packed)]
#[derive(Debug)]
pub struct IDTDescriptor {
    size: u16,
    address: u32,
}

impl IDTDescriptor {
    pub fn new(size: u16, address: u32) -> Self {
        Self { size, address }
    }
}

#[derive(Clone, Copy)]
pub struct GateDescriptor(u64);

impl GateDescriptor {
    pub const fn blank() -> Self {
        Self(0)
    }
}

pub const STANDARD_VECTOR_TABLE_SIZE: usize = 256;

pub type IDT<const N: usize> = [GateDescriptor; N];

impl<const N: usize> From<&IDT<N>> for IDTDescriptor {
    fn from(value: &IDT<N>) -> Self {
        Self {
            address: value.as_ptr() as u32,
            size: size_of::<IDT<N>>() as u16 - 1,
        }
    }
}

#[repr(u8)]
pub enum Interrupt {
    DivideError,
    DebugException,
    NonMaskableInterrupt,
    Breakpoint,
    Overflow,
    BoundRangeExceeded,
    UndefinedOpcode,
    NoMathCoprocessor,
    DoubleFault,
    CoprocessorSegmentOverrun,
    InvalidTaskStateSegmentSelector,
    SegmentNotPresent,
    StackSegmentFault,
    GeneralProtectionFault,
    PageFault,
    IntelReserved,
    X87FPUError,
    AlignmentCheck,
    MachineCheck,
    SIMDFloatingPointException,
    VirtualizationException,
    ControlProtectionException,
    UserDefinedFirst = 32,
    UserDefinedLast = 255,
}

#[allow(unused)]
#[repr(u16)]
pub enum GateDescriptorBit {
    _32BitGate = 1 << 11,
    Present = 1 << 15,
}

make_bitmap!(new_type: GateDescriptorFlags, underlying_flag_type: GateDescriptorBit, repr: u16, nodisplay);

impl GateDescriptorFlags {
    pub fn set_privilege_level(&mut self, privilege_level: PrivilegeLevel) {
        self.0 &= !0x60_00;
        self.0 |= (privilege_level as u16) << 12;
    }
}

#[repr(C, packed)]
#[derive(Debug)]
pub struct InterruptGateDescriptor {
    offset_low: u16,
    segment_selector: u16,
    flags: GateDescriptorFlags,
    offset_hi: u16,
}

impl Default for InterruptGateDescriptor {
    /// Present, Descriptor Privilege Level = 0, Gate size = 32
    fn default() -> Self {
        let mut flags = GateDescriptorFlags::empty();
        flags.set_flag(GateDescriptorBit::Present);
        flags.set_privilege_level(PrivilegeLevel::Ring0);
        flags.0 |= 0b00110 << 8;
        flags.set_flag(GateDescriptorBit::_32BitGate);
        Self {
            offset_hi: Default::default(),
            flags,
            segment_selector: Default::default(),
            offset_low: Default::default(),
        }
    }
}

impl From<InterruptGateDescriptor> for GateDescriptor {
    fn from(value: InterruptGateDescriptor) -> Self {
        GateDescriptor(
            // SAFETY: `InterruptGateDescriptor` is a `#[repr(C, packed)]` struct with the same
            // size as a `u64`, so this is safe.
            unsafe { transmute::<InterruptGateDescriptor, u64>(value) },
        )
    }
}

impl InterruptGateDescriptor {
    pub fn with_address_and_segment_selector(address: u32, segment_selector: u16) -> Self {
        Self {
            offset_hi: (address >> 16) as u16,
            segment_selector,
            offset_low: address as u16,
            ..Default::default()
        }
    }
}
