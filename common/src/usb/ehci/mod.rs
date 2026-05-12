use core::{fmt::Display, time::Duration};

use num_enum::TryFromPrimitive;

use crate::{make_bitmap, mmio, timer::LowPrecisionTimer};

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
        self.bits as u8 & 0xf
    }

    pub fn n_ports_per_companion_controller(&self) -> u8 {
        (self.bits >> 8) as u8 & 0xf
    }

    pub fn n_companion_controllers(&self) -> u8 {
        (self.bits >> 12) as u8 & 0xf
    }

    pub fn debug_port_number(&self) -> u8 {
        (self.bits >> 20) as u8 & 0xf
    }
}

pub enum HCIVersion {
    _1_0,
}

// NOTE: the u32 in this slice should not be read directly! You should take addr_of(ports[i])
// and do a volatile read
#[derive(Clone, Copy)]
pub struct Ports<'a>(&'a [u32]);

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

    pub fn get(&self, index: usize) -> PortStatusAndControlRegister {
        PortStatusAndControlRegister {
            bits: mmio::Volatile::new(core::ptr::addr_of!(self.0[index]) as u32).readd(),
        }
    }

    pub fn set(&self, index: usize, pscr: PortStatusAndControlRegister) {
        mmio::Volatile::new(core::ptr::addr_of!(self.0[index]) as u32).writed(pscr.bits);
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
    type Item = PortStatusAndControlRegister;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index < self.ports.len() {
            self.current_index += 1;
            Some(self.ports.get(self.current_index - 1))
        } else {
            None
        }
    }
}

pub struct Controller {
    base_address: u32,
    capability_register_length: u8,
    hcsp: HostControllerStructuralParameters,
    ports: Ports<'static>,
}

impl Controller {
    pub fn new(base_address: u32) -> Self {
        // SAFETY: It is assumed that the given address points to the base of a EHCI device
        // Else it's UB
        let capability_register_length = mmio::Volatile::new(base_address).readb();
        let hcsp = HostControllerStructuralParameters {
            bits: mmio::Volatile::new(base_address + 4).readd(),
        };
        let n_ports = hcsp.n_ports();
        Self {
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
            capability_register_length,
        }
    }

    pub fn has_port_power_control(&self) -> bool {
        self.hcsp
            .is_set(HostControllerStructuralParametersFlag::SupportsPortPowerSwitches)
    }

    /// # Panics
    /// Panics if the HCI version is not 1.0
    pub fn hci_version(&self) -> HCIVersion {
        match mmio::Volatile::new(self.base_address + 2).readw() {
            0x0100 => HCIVersion::_1_0,
            version => panic!("Unexpected HCIVersion: {version:#x}"),
        }
    }

    pub fn ports(&self) -> &Ports<'_> {
        &self.ports
    }

    /// # Panics
    /// If EHCI reset fails
    pub fn reset_port(&self, index: usize) {
        let mut port = self.ports().get(index);
        port.set_flag(PortStatusAndControlRegisterFlag::Reset);
        port.clear_flag(PortStatusAndControlRegisterFlag::Enabled);
        self.ports().set(index, port);
        let mut timer = LowPrecisionTimer::new(Duration::from_millis(50).as_nanos() as u64);
        while !timer.timeout() {
            timer.update();
        }
        port.clear_flag(PortStatusAndControlRegisterFlag::Reset);
        self.ports().set(index, port);
        let mut timer = LowPrecisionTimer::new(Duration::from_millis(2).as_nanos() as u64);
        while let port = self.ports().get(index)
            && !port.is_set(PortStatusAndControlRegisterFlag::Enabled)
            && !timer.timeout()
        {
            timer.update();
        }

        if timer.timeout() {
            // TODO: return Result instead of panicking
            panic!("Couldn't initialise USB port");
        }
    }
}

impl Display for Controller {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "Base address: {:#p}", self.base_address as *const u8)?;
        writeln!(f, "CAPLENGTH: {}", self.capability_register_length)?;
        writeln!(f, "Host Controller Structural Parameters: {}", self.hcsp)?;
        for (i, port) in self.ports.into_iter().enumerate() {
            writeln!(f, "Port number: {i}")?;
            writeln!(f, "{port}")?;
        }
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
        ((self.bits >> 14) as u8 & 0b11)
            .try_into()
            .expect("unreachable")
    }

    /// # Panics
    /// If the test control bits have a value > 0b1001
    pub fn port_test_control(&self) -> PortTestControl {
        ((self.bits >> 16) as u8 & 0xf)
            .try_into()
            .expect("unexpected value for port test control")
    }

    // NOTE: only meaningful is PortPowerControlSwitchIsOn (bit 12) is 1
    pub fn needs_reset(&self) -> bool {
        let line_status = (self.bits >> 10) & 0b11;
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
        let line_status = (self.bits >> 10) & 0b11;
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
