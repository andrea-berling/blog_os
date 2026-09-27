use core::fmt::Display;

use num_enum::TryFromPrimitive;

use crate::{
    array_vec::ArrayVec8,
    bits::{self},
    error::{self, Context, Error, Facility, Fault, PciDevice},
    make_bitmap,
    mmio::{self, Maskable},
    pci::ConfigAddressRegister,
    scsi::{self, LogicalUnitNumber},
    timer::{self, LowPrecisionTimer},
    try_with_trace,
    usb::{
        self,
        bbb::CommandWrapperTag,
        ehci::{
            alloc::StaticBundle,
            queue_head::{EndpointSpeed, RawQueueHead},
            transfer::control::{
                FeatureRecipient, get_descriptor_bundle, get_max_lun_bundle, set_address_bundle,
                set_configuration_bundle,
            },
        },
        setup::{
            Address, ConfigurationDescriptor, ConfigurationValue, Descriptor, DeviceDescriptor,
            EndpointDescriptor, Feature, InterfaceDescriptor, InterfaceNumber, MaxPacketLength,
        },
    },
};

pub mod alloc;
pub mod queue_head;
pub mod transfer;
pub mod transfer_descriptor;

// Source of truth: EHCI specification 1.0

#[derive(TryFromPrimitive, Clone, Copy)]
#[repr(u32)]
pub enum HostControllerStructuralParametersFlag {
    SupportsPortPowerSwitches = 1 << 4,
    PortRoutingInHCSPPortrouteArray = 1 << 7,
    SupportsPortIndicatorControl = 1 << 16,
}

make_bitmap!(new_type: HostControllerStructuralParameters, underlying_flag_type: HostControllerStructuralParametersFlag, repr: u32, bit_skipper: |i| i != 4 && i != 7 && i != 16);

pub enum HCIVersion {
    _1_0,
}

#[derive(TryFromPrimitive, Clone, Copy)]
#[repr(u32)]
pub enum USBLegacySupportExtendedCapabilityFlag {
    OsOwned = 1 << 24,
    BiosOwned = 1 << 16,
}

make_bitmap!(new_type: USBLegacySupportExtendedCapability, underlying_flag_type: USBLegacySupportExtendedCapabilityFlag, repr: u32, bit_skipper: |i| i != 24 && i != 16);

// NOTE: the u32 in this slice should not be read directly! You should take addr_of(ports[i])
// and do a volatile read
#[derive(Clone, Copy)]
pub struct Ports {
    base: *const (),
    n_ports: usize,
}

pub struct Port {
    portsc: mmio::VolatilePtr<PortStatusAndControlRegister>,
    index: usize,
}

pub struct PortsIterator {
    ports: Ports,
    current_index: usize,
}

#[derive(Clone, Copy, Debug)]
pub enum Owner {
    Bios,
    Os,
}

pub struct AsyncListAddr(u32);

pub struct Controller {
    base_address: u32,
    capability_register_length: u8,
    hcsp: mmio::VolatilePtr<HostControllerStructuralParameters>,
    eecp_pci_offset: Option<u8>,
    pci_config_addr: ConfigAddressRegister,
    has_64_bit_addressing_capability: bool,
    usb_command_register: mmio::VolatilePtr<USBCommandRegister>,
    usb_status_register: mmio::VolatilePtr<USBStatusRegister>,
    async_list_addr: mmio::VolatilePtr<AsyncListAddr>,
    owner: Option<Owner>,
    ports: Ports,
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

pub struct Device {
    controller_address: PciDevice,
    address: Address,
    default_endpoint_speed: EndpointSpeed,
    descriptor: DeviceDescriptor,
    configuration_value: Option<ConfigurationValue>,
    max_packet_size_endpoint_0: MaxPacketLength,
}

impl Device {
    pub fn controller_address(&self) -> PciDevice {
        self.controller_address
    }

    pub fn descriptor(&self) -> &DeviceDescriptor {
        &self.descriptor
    }

    pub fn get_configuration_descriptor_full(
        &self,
        index: u8,
        ehci_controller: &mut Controller,
    ) -> error::ResultWithTrace<ArrayVec8<(InterfaceDescriptor, ArrayVec8<EndpointDescriptor>)>>
    {
        let get_descriptor_error = Error::blank()
            .with_facility(Facility::EhciDevice(
                self.controller_address(),
                self.address,
            ))
            .with_context(Context::GettingConfigurationDescriptor(index));

        let standard_parameters = transfer::control::StandardParameters {
            address: self.address,
            endpoint_speed: self.default_endpoint_speed,
            max_packet_length: Some(self.max_packet_size_endpoint_0),
        };

        let get_descriptor_parameters = transfer::control::GetDescriptorParameters {
            descriptor_type: super::setup::DescriptorType::Configuration,
            descriptor_length: size_of::<ConfigurationDescriptor>() as u16,
            descriptor_alignment: align_of::<ConfigurationDescriptor>(),
            descriptor_index: index,
            lang_id: None,
        };

        let mut get_configuration_descriptor_bundle =
            get_descriptor_bundle(standard_parameters, get_descriptor_parameters)?;

        error::try_with_trace!(
            ehci_controller.run_transfer_sync(&get_configuration_descriptor_bundle),
            context: Context::GettingConfigurationDescriptor(index),
            facility: Facility::EhciDevice(self.controller_address(), self.address)
        );

        let configuration_descriptor = match error::try_with_trace!(
            get_configuration_descriptor_bundle.get_descriptor(),
            context: Context::GettingConfigurationDescriptor(index),
            facility: Facility::EhciDevice(self.controller_address(), self.address)
        ) {
            Descriptor::Configuration(descriptor) => descriptor,
            unexpected => {
                return Err(get_descriptor_error
                    .with_fault(Fault::UnexpectedDescriptorType(
                        unexpected.descriptor_type().into(),
                    ))
                    .into());
            }
        };

        let total_length = configuration_descriptor.total_length().get();

        get_configuration_descriptor_bundle.initialize(
            standard_parameters,
            transfer::control::GetDescriptorParameters {
                descriptor_length: total_length,
                ..get_descriptor_parameters
            },
        )?;

        error::try_with_trace!(
            ehci_controller.run_transfer_sync(&get_configuration_descriptor_bundle),
            context: Context::GettingConfigurationDescriptor(index),
            facility: Facility::EhciDevice(self.controller_address(), self.address)
        );

        let bytes = &get_configuration_descriptor_bundle.get_descriptor_buffer()
            [size_of::<ConfigurationDescriptor>()..]
            [..total_length as usize - size_of::<ConfigurationDescriptor>()];

        let mut interface_descriptors = ArrayVec8::new();
        let mut descriptors_iterator = Descriptor::traverse(bytes);
        while interface_descriptors.len() < configuration_descriptor.n_interfaces() as usize
            && let Some(descriptor) = descriptors_iterator.next_parsed()
        {
            if let Descriptor::Interface(interface_descriptor) = error::try_with_trace!(
                descriptor,
                context: Context::ParsingInterfaceDescriptor,
                facility: Facility::EhciDevice(self.controller_address(), self.address)
            ) {
                let mut endpoint_descriptors = ArrayVec8::<EndpointDescriptor>::new();
                while endpoint_descriptors.len() < interface_descriptor.n_endpoints() as usize
                    && let Some(subdescriptor) = descriptors_iterator.next_parsed()
                {
                    if let Descriptor::Endpoint(endpoint_descriptor) = error::try_with_trace!(
                        subdescriptor,
                        context: Context::ParsingEndpointDescriptor,
                        facility: Facility::EhciDevice(self.controller_address(), self.address)
                    ) {
                        endpoint_descriptors.try_push(endpoint_descriptor)?;
                    }
                }
                if endpoint_descriptors.len() != interface_descriptor.n_endpoints() as usize {
                    return Err(get_descriptor_error
                        .with_fault(Fault::CorruptUSBEndpointDescriptor)
                        .into());
                }
                interface_descriptors.try_push((interface_descriptor, endpoint_descriptors))?;
            }
        }

        if interface_descriptors.len() != configuration_descriptor.n_interfaces() as usize {
            return Err(get_descriptor_error
                .with_fault(Fault::CorruptUSBInterfaceDescriptor)
                .into());
        }

        Ok(interface_descriptors)
    }

    pub fn set_configuration(
        &mut self,
        configuration_value: ConfigurationValue,
        ehci_controller: &mut Controller,
    ) -> error::ResultWithTrace<()> {
        let standard_parameters = transfer::control::StandardParameters {
            address: self.address,
            endpoint_speed: self.default_endpoint_speed,
            max_packet_length: Some(self.max_packet_size_endpoint_0),
        };

        let set_configuration_bundle =
            set_configuration_bundle(standard_parameters, configuration_value)?;

        error::try_with_trace!(
            ehci_controller.run_transfer_sync(&set_configuration_bundle),
            context: Context::SettingConfigurationValue(configuration_value.into()),
            facility: Facility::EhciDevice(self.controller_address(), self.address)
        );

        self.configuration_value = Some(configuration_value);

        Ok(())
    }

    pub fn get_max_lun(
        &mut self,
        interface_number: InterfaceNumber,
        ehci_controller: &mut Controller,
    ) -> error::ResultWithTrace<LogicalUnitNumber> {
        let standard_parameters = transfer::control::StandardParameters {
            address: self.address,
            endpoint_speed: self.default_endpoint_speed,
            max_packet_length: Some(self.max_packet_size_endpoint_0),
        };

        let get_max_lun_bundle = get_max_lun_bundle(standard_parameters, interface_number)?;

        error::try_with_trace!(
            ehci_controller.run_transfer_sync(&get_max_lun_bundle),
            context: Context::GettingMaxLUN(interface_number.into()),
            facility: Facility::EhciDevice(self.controller_address(), self.address)
        );

        Ok(get_max_lun_bundle.get_max_lun().into())
    }

    pub fn clear_feature(
        &mut self,
        feature: Feature,
        feature_recipient: FeatureRecipient,
        ehci_controller: &mut Controller,
    ) -> error::ResultWithTrace<()> {
        let standard_parameters = transfer::control::StandardParameters {
            address: self.address,
            endpoint_speed: self.default_endpoint_speed,
            max_packet_length: Some(self.max_packet_size_endpoint_0),
        };

        let get_max_lun_bundle =
            transfer::control::clear_feature(standard_parameters, feature, feature_recipient)?;

        error::try_with_trace!(
            ehci_controller.run_transfer_sync(&get_max_lun_bundle),
            context: Context::ClearingFeature,
            facility: Facility::EhciDevice(self.controller_address(), self.address)
        );

        Ok(())
    }

    pub fn scsi_inquiry(
        &self,
        logical_unit_number: LogicalUnitNumber,
        tag: CommandWrapperTag,
        bulk_in: crate::usb::mass_storage::EndpointDescriptor,
        bulk_out: crate::usb::mass_storage::EndpointDescriptor,
        ehci_controller: &mut Controller,
    ) -> error::ResultWithTrace<scsi::cdb::data::Inquiry> {
        let scsi_inquiry_bundle = usb::ehci::transfer::scsi::inquiry_bundle(
            self.address,
            self.default_endpoint_speed,
            logical_unit_number,
            tag,
            bulk_in,
            bulk_out,
        )?;

        error::try_with_trace!(
            ehci_controller.run_transfer_sync(&scsi_inquiry_bundle),
            context: Context::ScsiInquiry(logical_unit_number.into()),
            facility: Facility::EhciDevice(self.controller_address(), self.address)
        );

        let csw = scsi_inquiry_bundle.get_command_status_wrapper()?;

        let error = Error::blank()
            .with_context(Context::ScsiInquiry(logical_unit_number.into()))
            .with_facility(Facility::EhciDevice(
                self.controller_address(),
                self.address,
            ));

        try_with_trace!(csw.validate(tag), context: error.context(), facility: error.facility());

        scsi_inquiry_bundle.get_inquiry_data().map_err(|err| {
            err.with_context(error.context())
                .with_facility(error.facility())
                .into()
        })
    }

    pub fn address(&self) -> Address {
        self.address
    }

    pub fn find_corresponding_controller_mut<'a>(
        &self,
        controllers: &'a mut [Controller],
    ) -> Option<&'a mut Controller> {
        controllers.iter_mut().find(|controller| {
            error::PciDevice::from(controller.pci_config_addr().clone())
                == self.controller_address()
        })
    }
}

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

impl Port {
    pub fn index(&self) -> usize {
        self.index
    }

    pub fn portsc(&self) -> &mmio::VolatilePtr<PortStatusAndControlRegister> {
        &self.portsc
    }
}

impl Ports {
    pub fn is_empty(&self) -> bool {
        self.n_ports == 0
    }

    pub fn len(&self) -> usize {
        self.n_ports
    }

    pub fn get(&self, index: usize) -> Port {
        Port {
            // SAFETY: the PORTSC register addresses are derived from the controller's
            // MMIO operational base; the construction guarantees are the ones documented
            // at the top of `Controller::new`
            portsc: unsafe {
                mmio::VolatilePtr::from_raw(
                    ((self.base as usize) + index * size_of::<u32>())
                        as *mut PortStatusAndControlRegister,
                )
            },
            index,
        }
    }

    pub fn new(base: *const (), n_ports: usize) -> Self {
        Self { base, n_ports }
    }
}

impl AsyncListAddr {
    pub fn set_address(&mut self, address: *const RawQueueHead) {
        // FIXME: today addresses are always physical, one day they'll be virtual. This will break
        // that day
        bits::set_bits!(bits_expr: self.0, value: (address as u32) >> 5, n_bits: 27, starts_at_bit: 5, bits_expr_ty: u32);
    }
}

impl Controller {
    const SET_ADDRESS_RECOVERY_DELAY_MS: u64 = 2;

    pub fn new(
        base_address: u32,
        pci_config_addr: ConfigAddressRegister,
    ) -> error::ResultWithTrace<Self> {
        // SAFETY: all the `VolatilePtr`s constructed below point into the EHCI register
        // space derived from `base_address`, the MMIO base taken from the PCI BAR: it is
        // mapped and uncached, and only accessed by this code (single thread). The
        // register types are integer-backed (plain integers or bitmap structs over
        // integers), so every bit pattern they can hold is a valid value. The pointers
        // keep these guarantees for their whole lifetime, which is as long as the
        // controller (forever, in the bootloader)
        let capability_register_length =
            unsafe { mmio::VolatilePtr::from_raw(base_address as *mut u8) }.read();
        // SAFETY: see the comment at the top of `new`
        let hcsp: mmio::VolatilePtr<_> = unsafe {
            mmio::VolatilePtr::from_raw(
                (base_address + 4) as *mut HostControllerStructuralParameters,
            )
        };
        let n_ports = hcsp.read_with(|x| x.n_ports());
        // SAFETY: see the comment at the top of `new`
        let hccp = unsafe { mmio::VolatilePtr::from_raw((base_address + 8) as *mut u32) }.read();
        let eecp_pci_offset = ((hccp >> 8) & 0xff) as u8;
        let has_64_bit_addressing_capability = (hccp & 0x1) != 0;
        let operational_base = u32::from(capability_register_length) + base_address;
        // SAFETY: see the comment at the top of `new`
        let usb_command_register =
            unsafe { mmio::VolatilePtr::from_raw(operational_base as *mut USBCommandRegister) };
        // SAFETY: see the comment at the top of `new`
        let usb_status_register = unsafe {
            mmio::VolatilePtr::from_raw((operational_base + 0x04) as *mut USBStatusRegister)
        };
        // SAFETY: see the comment at the top of `new`
        let async_list_addr =
            unsafe { mmio::VolatilePtr::from_raw((operational_base + 0x18) as *mut AsyncListAddr) };
        let eecp_pci_offset = if eecp_pci_offset != 0 {
            // The EECP must point past the standard PCI configuration space header
            // (0x40 bytes) and be 32-bit aligned
            if eecp_pci_offset < 0x40 || !eecp_pci_offset.is_multiple_of(4) {
                return Err(Error::blank()
                    .with_facility(Facility::EhciController(pci_config_addr.clone().into()))
                    .with_fault(Fault::InvalidEECPOffset(eecp_pci_offset))
                    .into());
            }
            Some(eecp_pci_offset)
        } else {
            None
        };
        let mut controller = Self {
            base_address,
            hcsp,
            // SAFETY: it is assumed that the value reported in the HCSP register is the correct
            // number of ports
            ports: Ports::new((operational_base + 0x44) as *const (), n_ports as usize),
            eecp_pci_offset,
            has_64_bit_addressing_capability,
            pci_config_addr,
            capability_register_length,
            usb_command_register,
            usb_status_register,
            owner: None,
            async_list_addr,
        };
        Ok(Self {
            owner: controller.read_owner_from_usblegsup(),
            ..controller
        })
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

    pub fn switch_ownership(&mut self, owner: Owner) -> error::ResultWithTrace<()> {
        let error: Error = Facility::EhciController(self.pci_config_addr.clone().into()).into();
        match owner {
            Owner::Bios => todo!(),
            Owner::Os => {
                let Some(mut usb_legsup) = self.usb_legsup() else {
                    return Err(error.with_fault(Fault::NoUSBLEGSUP).into());
                };
                let Some(eecp_pci_offset) = self.eecp_pci_offset else {
                    return Err(error.with_fault(Fault::NoEECP).into());
                };
                usb_legsup.set_flag(USBLegacySupportExtendedCapabilityFlag::OsOwned);
                usb_legsup.clear_flag(USBLegacySupportExtendedCapabilityFlag::BiosOwned);
                self.pci_config_addr.set_register_offset(eecp_pci_offset);
                self.pci_config_addr.write_dword(usb_legsup.into());
                error::try_with_trace!(
                    timer::bounded_wait!(matches!(self.read_owner_from_usblegsup(), Some(Owner::Os)), wait_for_ms: 100),
                    context: Context::WaitingHostControllerOwnershipSwitch,
                    facility: Facility::EhciController(self.pci_config_addr.clone().into())
                );
                self.owner = Some(owner);
                Ok(())
            }
        }
    }

    pub fn has_port_power_control(&self) -> bool {
        self.hcsp.read_with(|x| {
            x.is_set(HostControllerStructuralParametersFlag::SupportsPortPowerSwitches)
        })
    }

    /// # Panics
    /// Panics if the HCI version is not 1.0
    pub fn hci_version(&self) -> HCIVersion {
        // SAFETY: HCI version register: same construction guarantees as `Controller::new`
        match unsafe { mmio::VolatilePtr::from_raw((self.base_address + 2) as *mut u16) }.read() {
            0x0100 => HCIVersion::_1_0,
            version => panic!("Unexpected HCIVersion: {version:#x}"),
        }
    }

    pub fn ports(&self) -> &Ports {
        &self.ports
    }

    pub fn halt(&mut self) -> error::ResultWithTrace<()> {
        let usb_status: USBStatusRegister = self.usb_status_register.clone_read();
        if !usb_status.is_set(USBStatusRegisterFlag::HostControllerHalted) {
            self.usb_command_register.update(|usb_command_register| {
                usb_command_register.clear_flag(USBCommandRegisterFlag::Run);
            });

            error::try_with_trace!(
                timer::bounded_wait!(self.usb_status_register.read_with(|x| x.is_set(USBStatusRegisterFlag::HostControllerHalted)), wait_for_ms: 100),
                context: Context::HaltingEhciController,
                facility: Facility::EhciController(self.pci_config_addr.clone().into())
            );
        }
        Ok(())
    }

    pub fn reset(&mut self) -> error::ResultWithTrace<()> {
        error::try_with_trace!(
            self.halt(),
            context: Context::ResettingEhciController,
            facility: Facility::EhciController(self.pci_config_addr.clone().into())
        );

        let usb_command_register: USBCommandRegister =
            USBCommandRegisterFlag::HostControllerReset.into();
        self.usb_command_register.set(usb_command_register);

        error::try_with_trace!(
            timer::bounded_wait!(!self.usb_command_register.read_with(|x| x.is_set(USBCommandRegisterFlag::HostControllerReset)), wait_for_ms: 100),
            context: Context::ResettingEhciController,
            facility: Facility::EhciController(self.pci_config_addr.clone().into())
        );

        Ok(())
    }

    pub fn initialize() -> error::ResultWithTrace<()> {
        Ok(())
    }

    pub fn reset_port(&self, port: &mut Port) -> error::ResultWithTrace<()> {
        // NOTE: PORTSC has RW1C change bits, so the write-backs below clear any change
        // bit that was set at read time; that is acceptable here (we are resetting the
        // port anyway, stale change indications are discarded on purpose)
        port.update_masked(
            PortStatusAndControlRegisterFlag::Reset | PortStatusAndControlRegisterFlag::Enabled,
            |port| {
                port.set_flag(PortStatusAndControlRegisterFlag::Reset);
                port.clear_flag(PortStatusAndControlRegisterFlag::Enabled);
            },
        );
        LowPrecisionTimer::wait_for_ms(50);
        port.update_masked(PortStatusAndControlRegisterFlag::Reset.into(), |port| {
            port.clear_flag(PortStatusAndControlRegisterFlag::Reset);
        });

        error::try_with_trace!(
            timer::bounded_wait!(port.read_with(|x| x.is_set(PortStatusAndControlRegisterFlag::Enabled)), wait_for_ms: 2),
            context: Context::WaitingUSBPortResetClear(port.index() as u8),
            facility: Facility::EhciController(self.pci_config_addr.clone().into())
        );

        Ok(())
    }

    pub fn set_async_addr_list(&mut self, address: *const RawQueueHead) {
        self.async_list_addr
            .update(|async_list_addr| async_list_addr.set_address(address));
    }

    pub fn enable_async_schedule(&mut self) -> error::ResultWithTrace<()> {
        if self
            .usb_status_register
            .read_with(|usbsts| usbsts.is_set(USBStatusRegisterFlag::AsynchronousScheduleStatus))
        {
            return Ok(());
        }
        self.usb_command_register
            .update(|usbcmd| usbcmd.set_flag(USBCommandRegisterFlag::AsynchronousScheduleEnable));
        Ok(())
    }

    pub fn run(&mut self) -> error::ResultWithTrace<()> {
        let error: Error = Error::blank()
            .with_facility(Facility::EhciController(
                self.pci_config_addr.clone().into(),
            ))
            .with_context(Context::StartingEHCIScheduleExecution);
        if !self
            .usb_status_register
            .read_with(|usbsts| usbsts.is_set(USBStatusRegisterFlag::HostControllerHalted))
        {
            return Err(error.with_fault(Fault::HostControllerNotHalted).into());
        }
        self.usb_command_register
            .update(|usbcmd| usbcmd.set_flag(USBCommandRegisterFlag::Run));
        error::try_with_trace!(
            timer::bounded_wait!(!self
                .usb_status_register
                .read_with(|usbsts| usbsts.is_set(USBStatusRegisterFlag::HostControllerHalted)), wait_for_ms: 4),
            context: Context::StartingEHCIScheduleExecution,
            facility: Facility::EhciController(self.pci_config_addr.clone().into())
        );
        Ok(())
    }

    pub fn stop(&mut self) -> error::ResultWithTrace<()> {
        if self
            .usb_status_register
            .read_with(|usbsts| usbsts.is_set(USBStatusRegisterFlag::HostControllerHalted))
        {
            return Ok(());
        }
        self.usb_command_register
            .update(|usbcmd| usbcmd.clear_flag(USBCommandRegisterFlag::Run));
        error::try_with_trace!(
            timer::bounded_wait!(self
                .usb_status_register
                .read_with(|usbsts| usbsts.is_set(USBStatusRegisterFlag::HostControllerHalted)), wait_for_ms: 4),
            context: Context::StoppingEHCIScheduleExecution,
            facility: Facility::EhciController(self.pci_config_addr.clone().into())
        );
        Ok(())
    }

    pub fn initialize_device(
        &mut self,
        new_address: Address,
        endpoint_speed: EndpointSpeed,
    ) -> error::ResultWithTrace<Device> {
        let facility = Facility::EhciController(self.pci_config_addr.clone().into());
        let get_descriptor_error = Error::blank()
            .with_facility(facility)
            .with_context(Context::GettingDeviceDescriptor);
        let set_address_bundle = set_address_bundle(transfer::control::StandardParameters {
            address: new_address,
            endpoint_speed,
            max_packet_length: None,
        })?;

        error::try_with_trace!(
            self.run_transfer_sync(&set_address_bundle),
            context: Context::SettingEHCIDeviceAddress,
            facility: facility
        );

        // The USB 2.0 spec requires at least 2 ms of recovery between a SET_ADDRESS
        // completing and the next request over the default pipe.
        // FIXME: this is a busy-wait for now; switch to a timer-interrupt based sleep once
        // one exists
        timer::LowPrecisionTimer::wait_for_ms(Self::SET_ADDRESS_RECOVERY_DELAY_MS);
        let device_descriptor_prefix_length = DeviceDescriptor::max_packet_size_endpoint_0_offset()
            .clamp(
                super::setup::SMALLEST_LEGAL_MAX_PACKET_SIZE as usize,
                super::setup::LARGEST_LEGAL_MAX_PACKET_SIZE as usize,
            ) as u16;
        let mut get_device_descriptor_bundle = get_descriptor_bundle(
            transfer::control::StandardParameters {
                address: new_address,
                endpoint_speed,
                max_packet_length: Some(device_descriptor_prefix_length.try_into()?),
            },
            transfer::control::GetDescriptorParameters {
                descriptor_type: super::setup::DescriptorType::Device,
                descriptor_length: device_descriptor_prefix_length,
                descriptor_alignment: align_of::<DeviceDescriptor>(),
                descriptor_index: 0,
                lang_id: None,
            },
        )?;
        error::try_with_trace!(
            self.run_transfer_sync(&get_device_descriptor_bundle),
            context: Context::GettingDeviceDescriptor,
            facility: facility
        );

        let max_packet_size_endpoint_0: MaxPacketLength = error::try_with_trace!(
           (get_device_descriptor_bundle.get_descriptor_buffer()
           [DeviceDescriptor::max_packet_size_endpoint_0_offset()] as u16).try_into(),
            context: Context::GettingDeviceDescriptor,
            facility: facility
        );

        get_device_descriptor_bundle.initialize(
            transfer::control::StandardParameters {
                address: new_address,
                endpoint_speed,
                max_packet_length: Some(max_packet_size_endpoint_0),
            },
            transfer::control::GetDescriptorParameters {
                descriptor_type: super::setup::DescriptorType::Device,
                descriptor_length: size_of::<DeviceDescriptor>() as u16,
                descriptor_alignment: align_of::<DeviceDescriptor>(),
                descriptor_index: 0,
                lang_id: None,
            },
        )?;

        error::try_with_trace!(
            self.run_transfer_sync(&get_device_descriptor_bundle),
            context: Context::GettingDeviceDescriptor,
            facility: facility
        );

        let descriptor = match error::try_with_trace!(
            get_device_descriptor_bundle.get_descriptor(),
            context: Context::GettingDeviceDescriptor,
            facility: facility
        ) {
            Descriptor::Device(descriptor) => descriptor,
            unexpected => {
                return Err(get_descriptor_error
                    .with_fault(Fault::UnexpectedDescriptorType(
                        unexpected.descriptor_type().into(),
                    ))
                    .into());
            }
        };

        Ok(Device {
            address: new_address,
            controller_address: self.pci_config_addr.clone().into(),
            default_endpoint_speed: endpoint_speed,
            descriptor,
            max_packet_size_endpoint_0,
            configuration_value: None,
        })
    }

    pub fn pci_config_addr(&self) -> &ConfigAddressRegister {
        &self.pci_config_addr
    }

    pub fn run_transfer_sync(&mut self, bundle: &StaticBundle) -> error::ResultWithTrace<()> {
        self.enable_async_schedule()?;
        self.set_async_addr_list(bundle.first_queue_head_raw());
        self.run()?;

        for index in 0..bundle.queue_heads().len() {
            if let Err(err) = timer::bounded_wait!(bundle.qh_was_fetched(index) &&
                {
                  let status = bundle.get_status(index);
                  !status.is_set(transfer_descriptor::QueueTransferDescriptorTokenBit::Active) ||
                  status.is_set(transfer_descriptor::QueueTransferDescriptorTokenBit::Halted)
                }, wait_for_ms: 2)
            {
                let _ = self.stop();
                return Err(err.into());
            }

            if bundle
                .get_status(index)
                .is_set(transfer_descriptor::QueueTransferDescriptorTokenBit::Halted)
            {
                self.stop()?;
                return Err(Fault::EHCITransferHalted.into());
            }
        }

        self.stop()?;

        Ok(())
    }
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

impl Display for USBLegacySupportExtendedCapabilityFlag {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            USBLegacySupportExtendedCapabilityFlag::OsOwned => f.write_str("OS Owned"),
            USBLegacySupportExtendedCapabilityFlag::BiosOwned => f.write_str("BIOS Owned"),
        }
    }
}

impl Display for Owner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Owner::Bios => f.write_str("BIOS"),
            Owner::Os => f.write_str("OS"),
        }
    }
}

impl Display for Controller {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "Base address: {:#p}", self.base_address as *const u8)?;
        writeln!(f, "CAPLENGTH: {}", self.capability_register_length)?;
        writeln!(
            f,
            "Host Controller Structural Parameters: {}",
            self.hcsp.clone_read()
        )?;
        for port in self.ports.into_iter() {
            writeln!(f, "Port number: {}", port.index)?;
            writeln!(f, "Port Status and Control: {}", port.portsc.clone_read())?;
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

impl Display for Device {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "USB Hub's PCI address: {}", self.controller_address)?;
        writeln!(f, "Device address: {}", self.address)?;
        writeln!(f, "Default Endpoint Speed: {}", self.default_endpoint_speed)?;
        writeln!(f, "Device Descriptor:\n{}\n======", self.descriptor)?;
        Ok(())
    }
}

impl core::ops::Deref for Port {
    type Target = mmio::VolatilePtr<PortStatusAndControlRegister>;

    fn deref(&self) -> &Self::Target {
        &self.portsc
    }
}

impl core::ops::DerefMut for Port {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.portsc
    }
}

impl Maskable for PortStatusAndControlRegister {
    type Mask = Self;

    fn mask(self, mask: Self::Mask) -> Self {
        (self.bits & mask.bits).into()
    }

    fn blend(self, other: Self, mask: Self::Mask) -> Self {
        ((self.bits & !mask.bits) | (other.bits & mask.bits)).into()
    }
}

impl IntoIterator for &Ports {
    type IntoIter = PortsIterator;
    type Item = <PortsIterator as Iterator>::Item;

    fn into_iter(self) -> Self::IntoIter {
        PortsIterator {
            ports: *self,
            current_index: 0,
        }
    }
}

impl Iterator for PortsIterator {
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
