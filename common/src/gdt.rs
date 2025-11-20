use core::mem::size_of;

use num_enum::TryFromPrimitive;

use crate::{make_bitmap, protection::PrivilegeLevel, tss};

macro_rules! impl_descriptor_ops {
    ($descriptor_type:ty) => {
        impl $descriptor_type {
            pub fn set_limit_hi(&mut self, limit_hi: u8) {
                self.bits &= !0x0f_00;
                self.bits |= (limit_hi as u16 & 0x0f) << 8;
            }

            pub fn set_privilege_level(&mut self, privilege_level: PrivilegeLevel) {
                self.bits &= !0x60_00;
                self.bits |= (privilege_level as u16) << 12;
            }
        }
    };
}

macro_rules! update_flags {
    ($segment: ident, $update_code: expr) => {{
        // SAFETY: Reading unaligned data is UB, but the field is a `SegmentDescriptorFlags` which
        // has an alignment of 1, so this is safe.
        let mut __flags = unsafe { (&raw const $segment.flags).read_unaligned() };
        $update_code(&mut __flags);
        // SAFETY: Writing unaligned data is UB, but the field is a `SegmentDescriptorFlags` which
        // has an alignment of 1, so this is safe.
        unsafe {
            (&raw mut $segment.flags).write_unaligned(__flags);
        }
    }};
}

pub type GDT<const N: usize> = [SegmentDescriptor; N];

#[repr(C, packed)]
pub struct GDTDescriptor {
    size: u16,
    address: u32,
}

impl<const N: usize> From<&'static GDT<N>> for GDTDescriptor {
    fn from(value: &'static GDT<N>) -> Self {
        Self {
            address: value as *const _ as u32,
            size: size_of::<GDT<N>>() as u16 - 1,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SegmentDescriptorFlags(u16);

impl SegmentDescriptorFlags {
    pub fn set_present(&mut self) {
        let mut flags = SegmentFlags::from(self.0);
        flags.set_present();
        *self = flags.into();
    }

    pub fn set_4k_granularity(&mut self) {
        let mut flags = SegmentFlags::from(self.0);
        flags.set_4k_granularity();
        *self = flags.into();
    }

    pub fn set_long(&mut self) {
        let mut flags = SegmentFlags::from(self.0);
        flags.set_long();
        *self = flags.into();
    }

    pub fn set_limit_hi(&mut self, limit_hi: u8) {
        let mut flags = SegmentFlags::from(self.0);
        flags.set_limit_hi(limit_hi);
        *self = flags.into();
    }
}

impl From<SegmentFlags> for SegmentDescriptorFlags {
    fn from(value: SegmentFlags) -> Self {
        match value {
            SegmentFlags::Code(code_segment_descriptor_flags) => {
                Self(u16::from(code_segment_descriptor_flags))
            }
            SegmentFlags::Data(data_segment_descriptor_flags) => {
                Self(u16::from(data_segment_descriptor_flags))
            }
            SegmentFlags::Task(task_segment_descriptor_flags) => {
                Self(u16::from(task_segment_descriptor_flags))
            }
        }
    }
}

#[allow(unused)]
#[repr(u16)]
#[derive(TryFromPrimitive, Clone, Copy)]
pub enum DataSegmentDescriptorBit {
    Accessed = 1 << 0,
    Writable = 1 << 1,
    ExpandsDown = 1 << 2,
    Present = 1 << 7,
    Available = 1 << 12,
    LongMode = 1 << 13,
    Big = 1 << 14,
    SegmentLimitHas4KGranularity = 1 << 15,
}

make_bitmap!(new_type: DataSegmentDescriptorFlags, underlying_flag_type: DataSegmentDescriptorBit, repr: u16, nodisplay);
impl_descriptor_ops!(DataSegmentDescriptorFlags);

#[allow(unused)]
#[repr(u16)]
#[derive(TryFromPrimitive, Clone, Copy)]
pub enum CodeSegmentDescriptorBit {
    Accessed = 1 << 0,
    Readable = 1 << 1,
    Conforming = 1 << 2,
    Present = 1 << 7,
    Available = 1 << 12,
    LongMode = 1 << 13,
    DefaultOperandLengthIs32Bit = 1 << 14,
    SegmentLimitHas4KGranularity = 1 << 15,
}

make_bitmap!(new_type: CodeSegmentDescriptorFlags, underlying_flag_type: CodeSegmentDescriptorBit, repr: u16, nodisplay);
impl_descriptor_ops!(CodeSegmentDescriptorFlags);

#[allow(unused)]
#[repr(u16)]
#[derive(TryFromPrimitive, Clone, Copy)]
pub enum TaskSegmentDescriptorBit {
    Present = 1 << 7,
    LongMode = 1 << 13,
    SegmentLimitHas4KGranularity = 1 << 15,
}

make_bitmap!(new_type: TaskSegmentDescriptorFlags, underlying_flag_type: TaskSegmentDescriptorBit, repr: u16, nodisplay);
impl_descriptor_ops!(TaskSegmentDescriptorFlags);

#[repr(u8)]
enum SegmentType {
    Data = 0b10,
    Code = 0b11,
}

pub enum SegmentKind {
    Code,
    Data,
}

enum SegmentFlags {
    Code(CodeSegmentDescriptorFlags),
    Data(DataSegmentDescriptorFlags),
    Task(TaskSegmentDescriptorFlags),
}

impl From<u16> for SegmentFlags {
    fn from(value: u16) -> Self {
        match (value >> 3) & 0x3 {
            b if b == (SegmentType::Data as u16) => {
                Self::Data(DataSegmentDescriptorFlags { bits: value })
            }
            b if b == (SegmentType::Code as u16) => {
                Self::Code(CodeSegmentDescriptorFlags { bits: value })
            }
            _ => Self::Task(TaskSegmentDescriptorFlags { bits: value }),
        }
    }
}

impl SegmentFlags {
    pub fn set_limit_hi(&mut self, limit_hi: u8) {
        match self {
            SegmentFlags::Code(code_segment_descriptor_flags) => {
                code_segment_descriptor_flags.set_limit_hi(limit_hi)
            }
            SegmentFlags::Data(data_segment_descriptor_flags) => {
                data_segment_descriptor_flags.set_limit_hi(limit_hi)
            }
            SegmentFlags::Task(task_segment_descriptor_flags) => {
                task_segment_descriptor_flags.set_limit_hi(limit_hi)
            }
        }
    }

    pub fn set_present(&mut self) {
        match self {
            SegmentFlags::Code(code_segment_descriptor_flags) => {
                code_segment_descriptor_flags.set_flag(CodeSegmentDescriptorBit::Present);
            }
            SegmentFlags::Data(data_segment_descriptor_flags) => {
                data_segment_descriptor_flags.set_flag(DataSegmentDescriptorBit::Present);
            }
            SegmentFlags::Task(task_segment_descriptor_flags) => {
                task_segment_descriptor_flags.set_flag(TaskSegmentDescriptorBit::Present);
            }
        }
    }

    pub fn is_present(&self) -> bool {
        match self {
            SegmentFlags::Code(code_segment_descriptor_flags) => {
                code_segment_descriptor_flags.is_set(CodeSegmentDescriptorBit::Present)
            }
            SegmentFlags::Data(data_segment_descriptor_flags) => {
                data_segment_descriptor_flags.is_set(DataSegmentDescriptorBit::Present)
            }
            SegmentFlags::Task(task_segment_descriptor_flags) => {
                task_segment_descriptor_flags.is_set(TaskSegmentDescriptorBit::Present)
            }
        }
    }

    pub fn set_4k_granularity(&mut self) {
        match self {
            SegmentFlags::Code(code_segment_descriptor_flags) => {
                code_segment_descriptor_flags
                    .set_flag(CodeSegmentDescriptorBit::SegmentLimitHas4KGranularity);
            }
            SegmentFlags::Data(data_segment_descriptor_flags) => {
                data_segment_descriptor_flags
                    .set_flag(DataSegmentDescriptorBit::SegmentLimitHas4KGranularity);
            }
            SegmentFlags::Task(task_segment_descriptor_flags) => {
                task_segment_descriptor_flags
                    .set_flag(TaskSegmentDescriptorBit::SegmentLimitHas4KGranularity);
            }
        }
    }

    pub fn has_4k_granularity(&self) -> bool {
        match self {
            SegmentFlags::Code(code_segment_descriptor_flags) => code_segment_descriptor_flags
                .is_set(CodeSegmentDescriptorBit::SegmentLimitHas4KGranularity),
            SegmentFlags::Data(data_segment_descriptor_flags) => data_segment_descriptor_flags
                .is_set(DataSegmentDescriptorBit::SegmentLimitHas4KGranularity),
            SegmentFlags::Task(task_segment_descriptor_flags) => task_segment_descriptor_flags
                .is_set(TaskSegmentDescriptorBit::SegmentLimitHas4KGranularity),
        }
    }

    pub fn set_long(&mut self) {
        match self {
            SegmentFlags::Code(code_segment_descriptor_flags) => {
                code_segment_descriptor_flags.set_flag(CodeSegmentDescriptorBit::LongMode);
                code_segment_descriptor_flags
                    .clear_flag(CodeSegmentDescriptorBit::DefaultOperandLengthIs32Bit);
            }
            SegmentFlags::Data(data_segment_descriptor_flags) => {
                data_segment_descriptor_flags.set_flag(DataSegmentDescriptorBit::LongMode);
                data_segment_descriptor_flags.clear_flag(DataSegmentDescriptorBit::Big);
            }
            SegmentFlags::Task(task_segment_descriptor_flags) => {
                task_segment_descriptor_flags.set_flag(TaskSegmentDescriptorBit::LongMode);
            }
        }
    }
}

#[repr(C, packed)]
#[derive(Clone, Copy, Debug)]
pub struct SegmentDescriptor {
    segment_limit_lo: u16,
    base_low: u16,
    base_mid: u8,
    flags: SegmentDescriptorFlags,
    base_hi: u8,
}

impl SegmentDescriptor {
    pub const fn blank() -> Self {
        Self {
            base_hi: 0,
            flags: SegmentDescriptorFlags(0),
            base_mid: 0,
            base_low: 0,
            segment_limit_lo: 0,
        }
    }

    pub fn new_flat(kind: SegmentKind, long: bool) -> Self {
        let mut new_segment = Self::blank();
        new_segment.set_base(0);
        new_segment.set_limit(u32::MAX);
        update_flags!(new_segment, |flags: &mut SegmentDescriptorFlags| {
            flags.set_present();
            flags.set_4k_granularity();
        });
        match kind {
            SegmentKind::Code => {
                new_segment.set_code();
                let SegmentFlags::Code(mut code_segment_descriptor_flags) =
                    SegmentFlags::from(new_segment.flags.0)
                else {
                    unreachable!()
                };
                code_segment_descriptor_flags.set_flag(CodeSegmentDescriptorBit::Readable);
                code_segment_descriptor_flags
                    .set_flag(CodeSegmentDescriptorBit::DefaultOperandLengthIs32Bit);
                new_segment.flags = SegmentFlags::Code(code_segment_descriptor_flags).into();
            }
            SegmentKind::Data => {
                new_segment.set_data();
                let SegmentFlags::Data(mut data_segment_descriptor_flags) =
                    SegmentFlags::from(new_segment.flags.0)
                else {
                    unreachable!()
                };
                data_segment_descriptor_flags.set_flag(DataSegmentDescriptorBit::Writable);
                data_segment_descriptor_flags.set_flag(DataSegmentDescriptorBit::Big);
                new_segment.flags = SegmentFlags::Data(data_segment_descriptor_flags).into();
            }
        }
        if long && matches!(kind, SegmentKind::Code) {
            update_flags!(new_segment, |flags: &mut SegmentDescriptorFlags| {
                flags.set_long();
            });
        }
        new_segment
    }

    pub fn new_tss(tss: &tss::TaskStateSegment) -> Self {
        let mut new_segment = Self::blank();
        new_segment.set_tss();
        new_segment.set_base(tss as *const _ as u32);
        // Skipping the io permissions bitmap
        new_segment.set_limit(size_of::<tss::TaskStateSegment>() as u32 - 4 - 1);
        update_flags!(new_segment, |flags: &mut SegmentDescriptorFlags| {
            flags.set_present();
        });
        new_segment
    }

    fn set_base(&mut self, base_addr: u32) {
        self.base_hi = (base_addr >> 24) as u8;
        self.base_mid = (base_addr >> 16) as u8;
        self.base_low = base_addr as u16;
    }

    pub fn get_base(&self) -> u32 {
        ((self.base_hi as u32) << 24) | ((self.base_mid as u32) << 16) | (self.base_low as u32)
    }

    fn set_tss(&mut self) {
        self.flags.0 &= !0x1f;
        self.flags.0 |= 0x09;
    }

    fn set_code(&mut self) {
        self.flags.0 &= !0b11000;
        self.flags.0 |= (SegmentType::Code as u16) << 3;
    }

    fn set_data(&mut self) {
        self.flags.0 &= !0b11000;
        self.flags.0 |= (SegmentType::Data as u16) << 3;
    }

    fn set_limit(&mut self, limit: u32) {
        self.segment_limit_lo = limit as u16;
        update_flags!(self, |flags: &mut SegmentDescriptorFlags| {
            flags.set_limit_hi((limit >> 16) as u8);
        });
    }

    pub fn get_limit(&self) -> u32 {
        let limit_hi = ((self.flags.0 >> 8) & 0x0f) as u32;
        let limit_lo = self.segment_limit_lo as u32;
        (limit_hi << 16) | limit_lo
    }

    pub fn is_present(&self) -> bool {
        SegmentFlags::from(self.flags.0).is_present()
    }

    pub fn is_tss(&self) -> bool {
        matches!(SegmentFlags::from(self.flags.0), SegmentFlags::Task(_))
    }

    pub fn has_4k_granularity(&self) -> bool {
        SegmentFlags::from(self.flags.0).has_4k_granularity()
    }
}

#[cfg(test)]
mod tests {
    use crate::gdt::{self, SegmentDescriptor};

    #[test]
    fn flat_32bit() {
        let code_segment = SegmentDescriptor::new_flat(gdt::SegmentKind::Code, false);
        assert_eq!([0xff, 0xff, 0, 0, 0, 0x9a, 0xcf, 0], unsafe {
            core::mem::transmute::<SegmentDescriptor, [u8; 8]>(code_segment)
        });
        let data_segment = SegmentDescriptor::new_flat(gdt::SegmentKind::Data, false);
        assert_eq!([0xff, 0xff, 0, 0, 0, 0x92, 0xcf, 0], unsafe {
            core::mem::transmute::<SegmentDescriptor, [u8; 8]>(data_segment)
        });
    }

    #[test]
    fn flat_64bit() {
        let code_segment = SegmentDescriptor::new_flat(gdt::SegmentKind::Code, true);
        assert_eq!([0xff, 0xff, 0, 0, 0, 0x9a, 0xaf, 0], unsafe {
            core::mem::transmute::<SegmentDescriptor, [u8; 8]>(code_segment)
        });
        let data_segment = SegmentDescriptor::new_flat(gdt::SegmentKind::Data, true);
        assert_eq!([0xff, 0xff, 0, 0, 0, 0x92, 0xcf, 0], unsafe {
            core::mem::transmute::<SegmentDescriptor, [u8; 8]>(data_segment)
        });
    }
}