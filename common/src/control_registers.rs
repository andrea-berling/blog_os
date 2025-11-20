use core::arch::asm;

// https://cdrdv2-public.intel.com/868137/325462-089-sdm-vol-1-2abcd-3abcd-4.pdf
use crate::{
    error::{Error, Fault, bounded_context},
    make_bitmap, paging,
};

#[allow(unused)]
#[repr(u32)]
pub enum ControlRegister0Bit {
    Paging = 1 << 31,
    CacheDisable = 1 << 30,
    NotWriteThrough = 1 << 29,
    AutomaticAlignmentChecking = 1 << 18,
    WriteProtect = 1 << 16,
    ReportFPUNumericError = 1 << 5,
    MathCoprocessor = 1 << 4,
    TaskSwitched = 1 << 3,
    MonitorCoprocessor = 1 << 1,
    ProtectedMode = 1 << 0,
}

make_bitmap!(new_type: ControlRegister0, underlying_flag_type: ControlRegister0Bit, repr: u32, nodisplay);

#[allow(unused)]
#[repr(u64)]
pub enum ControlRegister3Bit {
    LinearAddressMasking48 = 1 << 62,
    LinearAddressMasking57 = 1 << 61,
    PageLevelCacheDisable = 1 << 4,
    PageLevelWriteThrough = 1 << 3,
}

make_bitmap!(new_type: ControlRegister3, underlying_flag_type: ControlRegister3Bit, repr: u64, nodisplay);

impl ControlRegister3 {
    pub fn set_pml4(&mut self, pml4: &'static paging::PML4) -> Result<(), Fault> {
        let address = pml4 as *const _ as u64;
        if !address.is_multiple_of(0x1000) {
            return Err(Fault::InvalidAddressForType {
                address,
                dst_type_prefix: bounded_context(core::any::type_name::<paging::PML4>().as_bytes()),
                alignment: 0x1000,
            });
        }
        self.bits = address;
        Ok(())
    }
}

#[allow(unused)]
#[repr(u32)]
pub enum ControlRegister4Bit {
    Virtual8086ModeExtensions = 1 << 0,
    ProtectedModeVirtualInterrupts = 1 << 1,
    TimestampDisable = 1 << 2,
    DebuggingExtensions = 1 << 3,
    PhysicalSizeExtensions = 1 << 4,
    PhysicalAddressExtensions = 1 << 5,
    MachineCheckExceptions = 1 << 6,
    GlobalPage = 1 << 7,
    PerformanceMonitoringCounter = 1 << 8,
    OperatingSystemSupportForFXSAVEAndFXRSTOR = 1 << 9,
    OperatingSystemSupportForUnmaskedSIMDFloatingPointExceptions = 1 << 10,
    UserModeInstructionPrevention = 1 << 11,
    _5LevelPaging = 1 << 12,
    VirtualMachineExtensions = 1 << 13,
    SaferModeExtensions = 1 << 14,
    FSGSBASEEnable = 1 << 16,
    ProcessContextIdentifiers = 1 << 17,
    XSAVEAndProcessorExtendedStates = 1 << 18,
    KeyLockerEnableBit = 1 << 19,
    SupervisorModeExecutionPrevention = 1 << 20,
    SupervisorModeAccessPrevention = 1 << 21,
    ProtectionKeysForUserModePages = 1 << 22,
    ControlflowEnforcementTechnology = 1 << 23,
    ProtectionKeysForSupervisorModePages = 1 << 24,
    UserInterrupts = 1 << 25,
    LinearAddressSpaceSeparation = 1 << 27,
    SupervisorLinearAddressMasking = 1 << 28,
}

make_bitmap!(new_type: ControlRegister4, underlying_flag_type: ControlRegister4Bit, repr: u32, nodisplay);

#[repr(u32)]
pub enum Msr {
    Efer(ExtendedFeatureEnableRegister) = 0xC000_0080,
}

pub fn wrmsr(msr: &Msr) {
    // SAFETY: Msr has a primitive representation which allows pointer casting to retrieve the
    // discriminant
    let register_index = unsafe { *(msr as *const Msr as *const u32) };

    let (low, high) = match msr {
        Msr::Efer(extended_feature_enable_register) => {
            let bits = u64::from(*extended_feature_enable_register);
            (bits as u32, (bits >> 32) as u32)
        }
    };

    // SAFETY: The validity of the value for the given MSR is guaranteed by the type signature
    unsafe {
        asm!(
          "wrmsr",
          in("eax") low,
          in("edx") high,
          in("ecx") register_index,
        )
    }
}

#[allow(unused)]
#[repr(u64)]
pub enum ExtendedFeatureEnableRegisterBit {
    SyscallEnable = 1 << 0,
    IA32eEnabled = 1 << 8,
    IA32eActive = 1 << 10,
    ExecuteDisableBitEnabled = 1 << 11,
}

make_bitmap!(new_type: ExtendedFeatureEnableRegister, underlying_flag_type: ExtendedFeatureEnableRegisterBit, repr: u64, nodisplay);
