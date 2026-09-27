use crate::{
    array_vec::ArrayVec8,
    error::{self, Context, Error, Facility, Fault},
    usb::{
        self,
        setup::{EndpointNumber, InterfaceNumber, MaxPacketLength},
    },
};

#[derive(Clone, Copy)]
pub struct EndpointDescriptor {
    number: EndpointNumber,
    max_packet_length: MaxPacketLength,
}

impl EndpointDescriptor {
    pub fn max_packet_length(&self) -> MaxPacketLength {
        self.max_packet_length
    }

    pub fn number(&self) -> EndpointNumber {
        self.number
    }
}

// FIXME: move this to usb
pub struct Device {
    controller_address: error::PciDevice,
    device_address: usb::setup::Address,
    configuration_value: usb::setup::ConfigurationValue,
    class: usb::Class,
    interface_number: InterfaceNumber,
    bulk_in: EndpointDescriptor,
    bulk_out: EndpointDescriptor,
}

impl Device {
    pub fn new(
        controller_address: error::PciDevice,
        device_address: usb::setup::Address,
        configuration_value: usb::setup::ConfigurationValue,
        class: usb::Class,
        interface_descriptor: usb::setup::InterfaceDescriptor,
        endpoint_descriptors: ArrayVec8<usb::setup::EndpointDescriptor>,
    ) -> error::Result<Self> {
        let error = Error::blank()
            .with_facility(Facility::EhciFunction {
                pci_device: controller_address,
                address: device_address,
                configuration_value,
                interface_number: interface_descriptor.interface_number(),
            })
            .with_context(Context::BuildingUSBMassStorageDevice);
        let (bulk_in, bulk_out) = {
            let Some(in_descriptor) = endpoint_descriptors.iter().find(|endpoint_descriptor| {
                endpoint_descriptor
                    .address()
                    .is_set(usb::setup::EndpointAddressBit::In)
                    && matches!(
                        endpoint_descriptor.attributes().get_transfer_type(),
                        usb::setup::TransferType::Bulk
                    )
            }) else {
                return Err(error.with_fault(Fault::NoBulkInEndpoint));
            };
            let Some(out_descriptor) = endpoint_descriptors.iter().find(|endpoint_descriptor| {
                !endpoint_descriptor
                    .address()
                    .is_set(usb::setup::EndpointAddressBit::In)
                    && matches!(
                        endpoint_descriptor.attributes().get_transfer_type(),
                        usb::setup::TransferType::Bulk
                    )
            }) else {
                return Err(error.with_fault(Fault::NoBulkOutEndpoint));
            };
            (
                EndpointDescriptor {
                    number: in_descriptor.address().get_number(),
                    max_packet_length: MaxPacketLength::try_from(
                        in_descriptor.max_packet_size().get(),
                    )
                    .map_err(|err| error.with_fault(err.fault()))?,
                },
                EndpointDescriptor {
                    number: out_descriptor.address().get_number(),
                    max_packet_length: MaxPacketLength::try_from(
                        out_descriptor.max_packet_size().get(),
                    )
                    .map_err(|err| error.with_fault(err.fault()))?,
                },
            )
        };

        Ok(Self {
            controller_address,
            device_address,
            configuration_value,
            class,
            interface_number: interface_descriptor.interface_number(),
            bulk_in,
            bulk_out,
        })
    }

    pub fn find_corresponding_device_and_controller_mut<'a>(
        &self,
        usb_devices: &'a mut [usb::ehci::Device],
        controllers: &'a mut [usb::ehci::Controller],
    ) -> Option<(&'a mut usb::ehci::Device, &'a mut usb::ehci::Controller)> {
        usb_devices
            .iter_mut()
            .find(|usb_device| {
                usb_device.controller_address() == self.controller_address
                    && usb_device.address() == self.device_address
            })
            .and_then(|usb_device| {
                let corresponding_controller =
                    usb_device.find_corresponding_controller_mut(controllers)?;
                Some((usb_device, corresponding_controller))
            })
    }

    pub fn configuration_value(&self) -> usb::setup::ConfigurationValue {
        self.configuration_value
    }

    pub fn interface_number(&self) -> InterfaceNumber {
        self.interface_number
    }

    pub fn bulk_in(&self) -> &EndpointDescriptor {
        &self.bulk_in
    }

    pub fn bulk_out(&self) -> &EndpointDescriptor {
        &self.bulk_out
    }

    pub fn class(&self) -> &usb::Class {
        &self.class
    }
}
