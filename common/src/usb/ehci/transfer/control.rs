use zerocopy::TryFromBytes;

use crate::{
    error::{self, Context, convert_try_read_error},
    usb::{
        ehci::{
            alloc::{
                AllocationRequest, QtdLink, QtdLinkSource, StaticBundle, allocate_static_bundle,
            },
            queue_head::EndpointSpeed,
            transfer_descriptor::{
                BufferIndex, PacketId, QueueTransferDescriptorIndex, QueueTransferDescriptorToken,
            },
        },
        setup::{
            Address, ConfigurationDescriptor, ConfigurationValue, Descriptor, DescriptorType,
            DeviceDescriptor, EndpointAddress, Feature, InterfaceNumber, LanguageId,
            MaxPacketLength, SetupData,
        },
    },
};

#[derive(Clone, Copy)]
pub struct StandardParameters {
    pub address: Address,
    pub endpoint_speed: EndpointSpeed,
    pub max_packet_length: Option<MaxPacketLength>,
}

pub struct GetDescriptorStaticBundle {
    bundle: StaticBundle,
    descriptor_type: DescriptorType,
    descriptor_length: u16,
    descriptor_alignment: usize,
}

pub struct GetMaxLunStaticBundle(StaticBundle);

#[derive(Clone, Copy)]
pub struct GetDescriptorParameters {
    pub descriptor_type: DescriptorType,
    pub descriptor_length: u16,
    pub descriptor_alignment: usize,
    pub descriptor_index: u8,
    pub lang_id: Option<LanguageId>,
}

pub enum FeatureRecipient {
    Endpoint(EndpointAddress),
}

impl GetDescriptorStaticBundle {
    pub fn get_descriptor(&self) -> error::ResultWithTrace<Descriptor> {
        match self.descriptor_type {
            DescriptorType::Device => {
                DeviceDescriptor::try_read_from_bytes(self.get_descriptor_buffer())
                    .map(Descriptor::Device)
                    .map_err(|err| {
                        convert_try_read_error(err)
                            .with_context(Context::ReadingDescriptor)
                            .into()
                    })
            }
            DescriptorType::Configuration => {
                ConfigurationDescriptor::try_read_from_bytes(self.get_descriptor_buffer())
                    .map(Descriptor::Configuration)
                    .map_err(|err| {
                        convert_try_read_error(err)
                            .with_context(Context::ReadingDescriptor)
                            .into()
                    })
            }
            DescriptorType::String => todo!(),
            DescriptorType::Interface => todo!(),
            DescriptorType::Endpoint => todo!(),
            DescriptorType::DeviceQualifier => todo!(),
            DescriptorType::OtherSpeedConfiguration => todo!(),
            DescriptorType::InterfacePower => todo!(),
            DescriptorType::Other(_) => todo!(),
        }
    }

    pub fn get_descriptor_buffer(&self) -> &[u8] {
        &self.bundle.buffers()[0]
            [size_of::<SetupData>().next_multiple_of(self.descriptor_alignment)..]
            [..self.descriptor_length as usize]
    }

    pub fn initialize(
        &mut self,
        StandardParameters {
            address,
            endpoint_speed,
            max_packet_length,
        }: StandardParameters,
        GetDescriptorParameters {
            descriptor_type,
            descriptor_length,
            descriptor_alignment,
            descriptor_index,
            lang_id,
        }: GetDescriptorParameters,
    ) -> error::ResultWithTrace<()> {
        use crate::usb::ehci::transfer_descriptor::PacketId::*;
        use crate::usb::ehci::transfer_descriptor::QueueTransferDescriptorTokenBit::Active;
        use crate::usb::ehci::transfer_descriptor::QueueTransferDescriptorTokenBit::InterruptOnComplete;
        self.initialize_control_queue_head(address, endpoint_speed, max_packet_length)?;
        self.initialize_setup_queue_transfer_descriptor(SetupData::get_descriptor(
            descriptor_type,
            descriptor_index,
            lang_id,
            descriptor_length,
        ))?;

        self.logically_link_qtds(
            QtdLinkSource::QueueTransferDescriptor(QueueTransferDescriptorIndex::from(1)),
            QtdLink::Next,
            Some(QueueTransferDescriptorIndex::from(2)),
        )?;

        let td2 = &mut self.queue_transfer_descriptors_mut()[1];
        let mut token: QueueTransferDescriptorToken = Default::default();
        token.set_total_bytes_to_transfer(descriptor_length)?;
        token.clear_flag(InterruptOnComplete);
        token.set_packet_id(In);
        token.set_flag(Active);
        td2.token_mut().set(token);
        td2.buffer_pointers_mut()[0] = Some(BufferIndex::new(
            0,
            size_of::<SetupData>().next_multiple_of(descriptor_alignment),
        )?);

        self.handshake_last_queue_transfer_descriptor(PacketId::Out)?;
        self.link_things_up()?;

        self.descriptor_type = descriptor_type;
        self.descriptor_length = descriptor_length;
        self.descriptor_alignment = descriptor_alignment;
        Ok(())
    }
}

impl GetMaxLunStaticBundle {
    pub fn get_max_lun(&self) -> u8 {
        self.buffers()[0][size_of::<SetupData>().next_multiple_of(1)]
    }
}

impl core::ops::Deref for GetDescriptorStaticBundle {
    type Target = StaticBundle;

    fn deref(&self) -> &Self::Target {
        &self.bundle
    }
}

impl core::ops::DerefMut for GetDescriptorStaticBundle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.bundle
    }
}

impl core::ops::Deref for GetMaxLunStaticBundle {
    type Target = StaticBundle;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for GetMaxLunStaticBundle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

pub fn set_address_bundle(
    StandardParameters {
        address,
        endpoint_speed,
        ..
    }: StandardParameters,
) -> error::ResultWithTrace<StaticBundle> {
    let mut bundle = allocate_static_bundle(AllocationRequest {
        n_queue_heads: 1,
        n_queue_transfer_descriptors: 2,
        n_buffers: 1,
    })?;

    bundle.initialize_control_queue_head(Address::default(), endpoint_speed, None)?;
    bundle.initialize_setup_queue_transfer_descriptor(SetupData::set_address(address))?;
    bundle.handshake_last_queue_transfer_descriptor(PacketId::In)?;
    bundle.link_things_up()?;

    Ok(bundle)
}

/// Builds the EHCI data structure bundle needed to perform a GET_DESCRIPTOR control
/// transfer over the default control pipe
///
/// # Errors
/// Fails if the bundle can't be allocated or initialized, or if the buffer page offset
/// where the descriptor will land (the first offset after the setup data, aligned to
/// `descriptor_alignment`) doesn't fit in a buffer page
pub fn get_descriptor_bundle(
    standard_parameters: StandardParameters,
    get_descriptor_parameters: GetDescriptorParameters,
) -> error::ResultWithTrace<GetDescriptorStaticBundle> {
    let bundle = allocate_static_bundle(AllocationRequest {
        n_queue_heads: 1,
        n_queue_transfer_descriptors: 3,
        n_buffers: 1,
    })?;
    let mut result = GetDescriptorStaticBundle {
        bundle,
        descriptor_type: get_descriptor_parameters.descriptor_type,
        descriptor_length: get_descriptor_parameters.descriptor_length,
        descriptor_alignment: get_descriptor_parameters.descriptor_alignment,
    };
    result.initialize(standard_parameters, get_descriptor_parameters)?;
    Ok(result)
}

pub fn set_configuration_bundle(
    StandardParameters {
        address,
        endpoint_speed,
        max_packet_length,
    }: StandardParameters,
    configuration_value: ConfigurationValue,
) -> error::ResultWithTrace<StaticBundle> {
    let mut bundle = allocate_static_bundle(AllocationRequest {
        n_queue_heads: 1,
        n_queue_transfer_descriptors: 2,
        n_buffers: 1,
    })?;

    bundle.initialize_control_queue_head(address, endpoint_speed, max_packet_length)?;
    bundle.initialize_setup_queue_transfer_descriptor(SetupData::set_configuration(
        configuration_value,
    ))?;
    bundle.handshake_last_queue_transfer_descriptor(PacketId::In)?;
    bundle.link_things_up()?;

    Ok(bundle)
}

pub fn get_max_lun_bundle(
    StandardParameters {
        address,
        endpoint_speed,
        max_packet_length,
    }: StandardParameters,
    interface_number: InterfaceNumber,
) -> error::ResultWithTrace<GetMaxLunStaticBundle> {
    let mut bundle = allocate_static_bundle(AllocationRequest {
        n_queue_heads: 1,
        n_queue_transfer_descriptors: 3,
        n_buffers: 1,
    })?;

    use crate::usb::ehci::transfer_descriptor::PacketId::*;
    use crate::usb::ehci::transfer_descriptor::QueueTransferDescriptorTokenBit::Active;
    use crate::usb::ehci::transfer_descriptor::QueueTransferDescriptorTokenBit::InterruptOnComplete;
    bundle.initialize_control_queue_head(address, endpoint_speed, max_packet_length)?;
    bundle.initialize_setup_queue_transfer_descriptor(SetupData::get_max_lun(interface_number))?;

    bundle.logically_link_qtds(
        QtdLinkSource::QueueTransferDescriptor(QueueTransferDescriptorIndex::from(1)),
        QtdLink::Next,
        Some(QueueTransferDescriptorIndex::from(2)),
    )?;

    let td2 = &mut bundle.queue_transfer_descriptors_mut()[1];
    let mut token: QueueTransferDescriptorToken = Default::default();
    token.set_total_bytes_to_transfer(1)?;
    token.clear_flag(InterruptOnComplete);
    token.set_packet_id(In);
    token.set_flag(Active);
    td2.token_mut().set(token);
    td2.buffer_pointers_mut()[0] = Some(BufferIndex::new(
        0,
        size_of::<SetupData>().next_multiple_of(1),
    )?);

    bundle.handshake_last_queue_transfer_descriptor(PacketId::Out)?;
    bundle.link_things_up()?;

    Ok(GetMaxLunStaticBundle(bundle))
}

pub fn clear_feature(
    StandardParameters {
        address,
        endpoint_speed,
        max_packet_length,
    }: StandardParameters,
    feature: Feature,
    feature_recipient: FeatureRecipient,
) -> error::ResultWithTrace<StaticBundle> {
    let mut bundle = allocate_static_bundle(AllocationRequest {
        n_queue_heads: 1,
        n_queue_transfer_descriptors: 2,
        n_buffers: 1,
    })?;

    bundle.initialize_control_queue_head(address, endpoint_speed, max_packet_length)?;
    bundle.initialize_setup_queue_transfer_descriptor(match feature_recipient {
        FeatureRecipient::Endpoint(endpoint_address) => {
            SetupData::clear_endpoint_feature(endpoint_address, feature)
        }
    })?;

    bundle.handshake_last_queue_transfer_descriptor(PacketId::In)?;
    bundle.link_things_up()?;

    Ok(bundle)
}
