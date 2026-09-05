use core::{fmt::Display, time::Duration};

use num_enum::TryFromPrimitive;

use crate::{
    bits,
    error::{self, Context, Error, Facility, Fault},
    make_bitmap, mmio,
    pci::ConfigAddressRegister,
    timer::{self, LowPrecisionTimer},
};


pub mod alloc;
pub mod control_transfer;
pub mod queue_head;
pub mod transfer_descriptor;

// Source of truth: EHCI specification 1.0

#[derive(TryFromPrimitive, Clone, Copy)]
#[repr(u32)]
pub enum HostControllerStructuralParametersFlag {
    SupportsPortPowerSwitches = 1 << 4,
    PortRoutingInHCSPPortrouteArray = 1 << 7,
    SupportsPortIndicatorControl = 1 << 16,
}

impl Display for HostControllerStructuralParametersFlag {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            HostControllerStructuralParametersFlag::SupportsPortPowerSwitches => {
                write!(f, "Power Port Control")
            }
            HostControllerStructuralParametersFlag::PortRoutingInHCSPPortrouteArray => {
                write!(f, "Explicit Port Routing")
            }
            HostControllerStructuralParametersFlag::SupportsPortIndicatorControl => {
                write!(f, "Port Indicators")
            }
        }
    }
}

make_bitmap!(new_type: HostControllerStructuralParameters, underlying_flag_type: HostControllerStructuralParametersFlag, repr: u32, bit_skipper: |i| i != 4 && i != 7 && i != 16);

impl HostControllerStructuralParameters {
    pub fn n_ports(&self) -> u8 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 4, starts_at_bit: 0, return_ty: u8)
    }

    pub fn n_ports_per_companion_controller(&self) -> u8 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 4, starts_at_bit: 8, return_ty: u8)
    }

    pub fn n_companion_controllers(&self) -> u8 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 4, starts_at_bit: 12, return_ty: u8)
    }

    pub fn debug_port_number(&self) -> u8 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 4, starts_at_bit: 20, return_ty: u8)
    }
}

pub enum HCIVersion {
    _1_0,
}

#[derive(TryFromPrimitive, Clone, Copy)]
#[repr(u32)]
pub enum USBLegacySupportExtendedCapabilityFlag {
    OsOwned = 1 << 24,
    BiosOwned = 1 << 16,
}

impl Display for USBLegacySupportExtendedCapabilityFlag {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            USBLegacySupportExtendedCapabilityFlag::OsOwned => f.write_str("OS Owned"),
            USBLegacySupportExtendedCapabilityFlag::BiosOwned => f.write_str("BIOS Owned"),
        }
    }
}

make_bitmap!(new_type: USBLegacySupportExtendedCapability, underlying_flag_type: USBLegacySupportExtendedCapabilityFlag, repr: u32, bit_skipper: |i| i != 24 && i != 16);

// NOTE: the u32 in this slice should not be read directly! You should take addr_of(ports[i])
// and do a volatile read
#[derive(Clone, Copy)]
pub struct Ports<'a>(&'a [u32]);

pub struct Port {
    portsc: PortStatusAndControlRegister,
    index: usize,
}

impl Port {
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn portsc(&self) -> &PortStatusAndControlRegister {
        &self.portsc
    }
}

impl core::ops::DerefMut for Port {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.portsc
    }
}

impl core::ops::Deref for Port {
    type Target = PortStatusAndControlRegister;

    fn deref(&self) -> &Self::Target {
        &self.portsc
    }
}

pub struct PortsIterator<'a> {
    ports: Ports<'a>,
    current_index: usize,
}

impl<'a> Ports<'a> {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn get(&self, index: usize) -> Port {
        Port {
            portsc: PortStatusAndControlRegister {
                bits: mmio::Volatile::new(core::ptr::addr_of!(self.0[index]) as usize).readd(),
            },
            index,
        }
    }

    pub fn set(&self, port: Port) {
        mmio::Volatile::new(core::ptr::addr_of!(self.0[port.index]) as usize)
            .writed(port.portsc.bits);
    }
}

impl<'a> IntoIterator for &Ports<'a> {
    type Item = <PortsIterator<'a> as Iterator>::Item;

    type IntoIter = PortsIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        PortsIterator {
            ports: *self,
            current_index: 0,
        }
    }
}

impl Iterator for PortsIterator<'_> {
    type Item = Port;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index < self.ports.len() {
            self.current_index += 1;
            Some(self.ports.get(self.current_index - 1))
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Owner {
    Bios,
    Os,
}

impl Display for Owner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Owner::Bios => f.write_str("BIOS"),
            Owner::Os => f.write_str("OS"),
        }
    }
}

struct USBCommandRegisterMMIO(mmio::Volatile);

impl USBCommandRegisterMMIO {
    pub fn get(&self) -> USBCommandRegister {
        self.0.readd().into()
    }

    pub fn set(&mut self, usb_command: USBCommandRegister) {
        self.0.writed(usb_command.into())
    }
}

struct USBStatusRegisterMMIO(mmio::Volatile);

impl USBStatusRegisterMMIO {
    pub fn get(&self) -> USBStatusRegister {
        self.0.readd().into()
    }
}

pub struct Controller {
    base_address: u32,
    capability_register_length: u8,
    hcsp: HostControllerStructuralParameters,
    eecp_pci_offset: Option<u8>,
    pci_config_addr: ConfigAddressRegister,
    has_64_bit_addressing_capability: bool,
    usb_command_register: USBCommandRegisterMMIO,
    usb_status_register: USBStatusRegisterMMIO,
    owner: Option<Owner>,
    ports: Ports<'static>,
}

impl Controller {
    pub fn new(base_address: u32, pci_config_addr: ConfigAddressRegister) -> Self {
        // SAFETY: It is assumed that the given address points to the base of a EHCI device
        // Else it's UB
        let capability_register_length = mmio::Volatile::new(base_address as usize).readb();
        let hcsp = HostControllerStructuralParameters {
            bits: mmio::Volatile::new((base_address + 4) as usize).readd(),
        };
        let n_ports = hcsp.n_ports();
        let hccp = mmio::Volatile::new((base_address + 8) as usize).readd();
        let eecp_pci_offset = ((hccp >> 8) & 0xff) as u8;
        let has_64_bit_addressing_capability = (hccp & 0x1) != 0;
        let operational_base = u32::from(capability_register_length) + base_address;
        let usb_command_register = mmio::Volatile::new(operational_base as usize);
        let usb_status_register = mmio::Volatile::new((operational_base + 0x04) as usize);
        // TODO: check that eccp_pci_offset >= 0x40 and 32-bit aligned!
        let mut controller = Self {
            base_address,
            hcsp,
            // SAFETY: it is assumed that the value reported in the HCSP register is the correct
            // number of ports
            ports: Ports(unsafe {
                core::slice::from_raw_parts(
                    (base_address + capability_register_length as u32 + 0x44) as *const u32,
                    n_ports as usize,
                )
            }),
            eecp_pci_offset: if eecp_pci_offset != 0 {
                Some(eecp_pci_offset)
            } else {
                None
            },
            has_64_bit_addressing_capability,
            pci_config_addr,
            capability_register_length,
            usb_command_register: USBCommandRegisterMMIO(usb_command_register),
            usb_status_register: USBStatusRegisterMMIO(usb_status_register),
            owner: None,
        };
        Self {
            owner: controller.read_owner_from_usblegsup(),
            ..controller
        }
    }

    fn usb_legsup(&mut self) -> Option<USBLegacySupportExtendedCapability> {
        if let Some(eecp_pci_offset) = self.eecp_pci_offset {
            self.pci_config_addr.set_register_offset(eecp_pci_offset);
            Some(self.pci_config_addr.read_dword().into())
        } else {
            None
        }
    }

    fn read_owner_from_usblegsup(&mut self) -> Option<Owner> {
        if let Some(usb_legsup) = self.usb_legsup() {
            if usb_legsup.is_set(USBLegacySupportExtendedCapabilityFlag::OsOwned)
                && !usb_legsup.is_set(USBLegacySupportExtendedCapabilityFlag::BiosOwned)
            {
                Some(Owner::Os)
            } else if !usb_legsup.is_set(USBLegacySupportExtendedCapabilityFlag::OsOwned)
                && usb_legsup.is_set(USBLegacySupportExtendedCapabilityFlag::BiosOwned)
            {
                Some(Owner::Bios)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn owner(&self) -> Option<Owner> {
        self.owner
    }

    pub fn switch_ownership(&mut self, owner: Owner) -> error::Result<()> {
        let error: Error = Facility::EhciController(self.pci_config_addr.clone().into()).into();
        match owner {
            Owner::Bios => todo!(),
            Owner::Os => {
                let Some(mut usb_legsup) = self.usb_legsup() else {
                    return Err(error.with_fault(Fault::NoUSBLEGSUP));
                };
                let Some(eecp_pci_offset) = self.eecp_pci_offset else {
                    return Err(error.with_fault(Fault::NoEECP));
                };
                usb_legsup.set_flag(USBLegacySupportExtendedCapabilityFlag::OsOwned);
                usb_legsup.clear_flag(USBLegacySupportExtendedCapabilityFlag::BiosOwned);
                self.pci_config_addr.set_register_offset(eecp_pci_offset);
                self.pci_config_addr.write_dword(usb_legsup.into());
                timer::bounded_wait!(matches!(self.read_owner_from_usblegsup(), Some(Owner::Os)), wait_for_ms: 100)
                    .map_err(|err| {
                        error
                            .with_context(Context::WaitingHostControllerOwnershipSwitch)
                            .with_fault(err.fault())
                    })?;
                self.owner = Some(owner);
                Ok(())
            }
        }
    }

    pub fn has_port_power_control(&self) -> bool {
        self.hcsp
            .is_set(HostControllerStructuralParametersFlag::SupportsPortPowerSwitches)
    }

    /// # Panics
    /// Panics if the HCI version is not 1.0
    pub fn hci_version(&self) -> HCIVersion {
        match mmio::Volatile::new((self.base_address + 2) as usize).readw() {
            0x0100 => HCIVersion::_1_0,
            version => panic!("Unexpected HCIVersion: {version:#x}"),
        }
    }

    pub fn ports(&self) -> &Ports<'_> {
        &self.ports
    }

    pub fn halt(&mut self) -> error::Result<()> {
        let error = Error::blank()
            .with_facility(Facility::EhciController(
                self.pci_config_addr.clone().into(),
            ))
            .with_context(Context::HaltingEhciController);
        let usb_status: USBStatusRegister = self.usb_status_register.get();
        if !usb_status.is_set(USBStatusRegisterFlag::HostControllerHalted) {
            let mut usb_command_register: USBCommandRegister = self.usb_command_register.get();
            usb_command_register.clear_flag(USBCommandRegisterFlag::Run);
            self.usb_command_register.set(usb_command_register);

            timer::bounded_wait!(self.usb_status_register.get().is_set(USBStatusRegisterFlag::HostControllerHalted), wait_for_ms: 100)
                    .map_err(|err| {
                        error
                            .with_fault(err.fault())
                    })?;
        }
        Ok(())
    }

    pub fn reset(&mut self) -> error::Result<()> {
        let error = Error::blank()
            .with_facility(Facility::EhciController(
                self.pci_config_addr.clone().into(),
            ))
            .with_context(Context::ResettingEhciController);
        self.halt()?;

        let usb_command_register: USBCommandRegister =
            USBCommandRegisterFlag::HostControllerReset.into();
        self.usb_command_register.set(usb_command_register);

        timer::bounded_wait!(!self.usb_command_register.get().is_set(USBCommandRegisterFlag::HostControllerReset), wait_for_ms: 100)
        .map_err(|err| error.with_fault(err.fault()))?;

        Ok(())
    }

    pub fn initialize() -> error::Result<()> {
        Ok(())
    }

    pub fn reset_port(&self, mut port: Port) -> error::Result<()> {
        let index = port.index;
        port.set_flag(PortStatusAndControlRegisterFlag::Reset);
        port.clear_flag(PortStatusAndControlRegisterFlag::Enabled);
        self.ports().set(port);
        LowPrecisionTimer::wait_for_ms(50);
        port = self.ports().get(index);
        port.clear_flag(PortStatusAndControlRegisterFlag::Reset);
        self.ports().set(port);
        // NOTE: code below uses the LowPrecisionTimer directly due to the while let statement,
        // which has no case in the macro (and is not worth generalising the macro for)
        let clear_reset_timeout_ms = 2;
        let mut timer =
            LowPrecisionTimer::new(Duration::from_millis(clear_reset_timeout_ms).as_nanos() as u64);
        while let port = self.ports().get(index)
            && !port.is_set(PortStatusAndControlRegisterFlag::Enabled)
            && !timer.timeout()
        {
            timer.update();
        }

        if let port = self.ports().get(index)
            && !port.is_set(PortStatusAndControlRegisterFlag::Enabled)
            && timer.timeout()
        {
            return Err(Error::new(
                Fault::Timeout(clear_reset_timeout_ms),
                Context::WaitingUSBPortResetClear(index as u8),
                Facility::EhciController(self.pci_config_addr.clone().into()),
            ));
        }
        Ok(())
    }
}

impl Display for Controller {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "Base address: {:#p}", self.base_address as *const u8)?;
        writeln!(f, "CAPLENGTH: {}", self.capability_register_length)?;
        writeln!(f, "Host Controller Structural Parameters: {}", self.hcsp)?;
        for port in self.ports.into_iter() {
            writeln!(f, "Port number: {}", port.index)?;
            writeln!(f, "Port Status and Control: {}", port.portsc)?;
        }
        writeln!(
            f,
            "EHCI Extended Capabilities Pointer: {:#x?}",
            self.eecp_pci_offset
        )?;

        writeln!(
            f,
            "PCI address: {:02x}:{:02x}.{}",
            self.pci_config_addr.get_bus_number(),
            self.pci_config_addr.get_device_number(),
            self.pci_config_addr.get_function_number(),
        )?;

        writeln!(f, "Owner: {:?}", self.owner())?;
        writeln!(
            f,
            "64-bit Addressing Capability: {}",
            self.has_64_bit_addressing_capability
        )?;
        Ok(())
    }
}

#[repr(u32)]
#[derive(TryFromPrimitive, Clone, Copy)]
pub enum PortStatusAndControlRegisterFlag {
    DevicePresent = 1 << 0,
    CurrentConnectStatusChange = 1 << 1,
    Enabled = 1 << 2,
    PortEnabledStatusChange = 1 << 3,
    InOverCurrent = 1 << 4,
    OverCurrentStatusChange = 1 << 5,
    ResumeDetected = 1 << 6,
    // Always reads 1
    Suspend = 1 << 7,
    Reset = 1 << 8,
    DataPlus = 1 << 10,
    DataMinus = 1 << 11,
    PortPowerControlSwitchIsOn = 1 << 12,
    CompanionHostControllerOwned = 1 << 13,
    WakeOnConnect = 1 << 20,
    WakeOnDisconnect = 1 << 21,
    WakeOnOverCurrent = 1 << 22,
}

impl Display for PortStatusAndControlRegisterFlag {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let description = match self {
            PortStatusAndControlRegisterFlag::DevicePresent => "Device Present",
            PortStatusAndControlRegisterFlag::CurrentConnectStatusChange => {
                "Device was attached or detached"
            }
            PortStatusAndControlRegisterFlag::Enabled => "Port Enabled",
            PortStatusAndControlRegisterFlag::PortEnabledStatusChange => {
                "Port was enabled or disabled"
            }
            PortStatusAndControlRegisterFlag::DataPlus => "Data Plus is high",
            PortStatusAndControlRegisterFlag::DataMinus => "Data Minus in high",
            PortStatusAndControlRegisterFlag::ResumeDetected => "Resume",
            PortStatusAndControlRegisterFlag::Reset => "In Reset",
            PortStatusAndControlRegisterFlag::Suspend => "In Suspend",
            PortStatusAndControlRegisterFlag::InOverCurrent => "In over-current",
            PortStatusAndControlRegisterFlag::OverCurrentStatusChange => {
                "Over-current state changed"
            }
            PortStatusAndControlRegisterFlag::PortPowerControlSwitchIsOn => {
                "Port power control is on"
            }
            PortStatusAndControlRegisterFlag::CompanionHostControllerOwned => {
                "Companion Host Controller Onwed"
            }
            PortStatusAndControlRegisterFlag::WakeOnConnect => "Wake on Connect",
            PortStatusAndControlRegisterFlag::WakeOnDisconnect => "Wake on Disconnect",
            PortStatusAndControlRegisterFlag::WakeOnOverCurrent => "Wake on over-current",
        };
        write!(f, "{}", description)
    }
}

make_bitmap!(new_type: PortStatusAndControlRegister, underlying_flag_type: PortStatusAndControlRegisterFlag, repr: u32, nodisplay);

#[repr(u32)]
#[derive(TryFromPrimitive, Clone, Copy)]
pub enum USBInterruptEnableRegisterFlag {
    ThresholdInterrupts = 1 << 0,
    USBError = 1 << 1,
    PortChange = 1 << 2,
    FrameListRollover = 1 << 3,
    HostSystemError = 1 << 4,
    InterruptOnAsyncAdvance = 1 << 5,
}

make_bitmap!(new_type: USBInterruptEnableRegister, underlying_flag_type: USBInterruptEnableRegisterFlag, repr: u32, nodisplay);

#[repr(u32)]
#[derive(TryFromPrimitive, Clone, Copy)]
pub enum USBCommandRegisterFlag {
    Run = 1 << 0,
    HostControllerReset = 1 << 1,
    PeriodicScheduleEnable = 1 << 4,
    AsynchronousScheduleEnable = 1 << 5,
    InterruptOnAsyncAdvanceDoorbell = 1 << 6,
    LightHostControllerReset = 1 << 7,
    AsynchronousScheduleParkModeEnable = 1 << 11,
}

make_bitmap!(new_type: USBCommandRegister, underlying_flag_type: USBCommandRegisterFlag, repr: u32, nodisplay);

#[repr(u32)]
#[derive(TryFromPrimitive, Clone, Copy)]
pub enum USBStatusRegisterFlag {
    USBInterrupt = 1 << 0,
    USBErrorInterrupt = 1 << 1,
    PortChangeDetect = 1 << 2,
    FrameListRollover = 1 << 3,
    HostSystemError = 1 << 4,
    InterruptOnAsyncAdvance = 1 << 5,
    HostControllerHalted = 1 << 12,
    Reclamation = 1 << 13,
    PeriodicScheduleStatus = 1 << 14,
    AsynchronousScheduleStatus = 1 << 15,
}

make_bitmap!(new_type: USBStatusRegister, underlying_flag_type: USBStatusRegisterFlag, repr: u32, nodisplay);

#[derive(TryFromPrimitive, Debug)]
#[repr(u8)]
pub enum PortIndicatorStatus {
    Off,
    Amber,
    Green,
    Undefined,
}

#[derive(TryFromPrimitive, Debug)]
#[repr(u8)]
pub enum PortTestControl {
    Disabled,
    TestJState,
    TestKState,
    TestSE0NAK,
    TestPacket,
    TestForceEnable,
}

impl PortStatusAndControlRegister {
    /// # Panics
    /// Never
    pub fn port_indicator_status(&self) -> PortIndicatorStatus {
        bits::get_bits!(bits_expr: self.bits, n_bits: 2, starts_at_bit: 14, return_ty: u8)
            .try_into()
            .expect("unreachable")
    }

    /// # Panics
    /// If the test control bits have a value > 0b1001
    pub fn port_test_control(&self) -> PortTestControl {
        bits::get_bits!(bits_expr: self.bits, n_bits: 4, starts_at_bit: 16, return_ty: u8)
            .try_into()
            .expect("unexpected value for port test control")
    }

    // NOTE: only meaningful is PortPowerControlSwitchIsOn (bit 12) is 1
    pub fn needs_reset(&self) -> bool {
        let line_status =
            bits::get_bits!(bits_expr: self.bits, n_bits: 2, starts_at_bit: 10, return_ty: u32);
        !self.is_set(PortStatusAndControlRegisterFlag::Enabled)
            && self.is_set(PortStatusAndControlRegisterFlag::DevicePresent)
            && line_status != 0b10
    }
}

impl core::fmt::Display for PortStatusAndControlRegister {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut printed_once = false;

        write!(f, "Flags: ")?;
        for i in 0..u32::BITS {
            if (i > 8 && i < 12) || (i > 13 && i < 20) || i > 22 {
                continue;
            }
            let flag = <PortStatusAndControlRegisterFlag>::try_from(1 << i).expect("invalid flag");
            if self.is_set(flag) {
                if printed_once {
                    write!(f, "|")?;
                }
                write!(f, "{}", flag)?;
                printed_once = true;
            }
        }
        writeln!(f)?;
        let line_status =
            bits::get_bits!(bits_expr: self.bits, n_bits: 2, starts_at_bit: 10, return_ty: u32);
        writeln!(
            f,
            "Line status: {line_status:#b} ({interpretation})",
            interpretation = match line_status {
                0b00 | 0b10 | 0b11 => "Not Low-speed device, perform EHCI reset.",
                0b01 => "Low-speed device, release ownership of port",
                _ => unreachable!(),
            }
        )?;
        writeln!(
            f,
            "Port Indicator Status: {:?}",
            self.port_indicator_status()
        )?;
        writeln!(f, "Port Test Control: {:?}", self.port_test_control())
    }
}
