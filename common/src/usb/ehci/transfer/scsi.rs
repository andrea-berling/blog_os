use zerocopy::TryFromBytes as _;

use crate::{
    error::{self, convert_try_read_error},
    scsi::{self, LogicalUnitNumber},
    usb::{
        self,
        bbb::CommandWrapperTag,
        ehci::{
            alloc::{QtdLink, QtdLinkSource},
            queue_head::{EndpointSpeed, QueueHeadIndex},
            transfer_descriptor::{BufferIndex, PacketId, QueueTransferDescriptorIndex},
        },
        setup::Address,
    },
};

pub struct InquiryStaticBundle {
    bundle: usb::ehci::bbb::CommandBundle,
    inquiry_data_offset: usize,
}

impl InquiryStaticBundle {
    pub fn get_inquiry_data(&self) -> error::Result<scsi::cdb::data::Inquiry> {
        let bytes = &self.buffers()[0][self.inquiry_data_offset..];
        scsi::cdb::data::Inquiry::try_read_from_prefix(bytes)
            .map(|(data, _)| data)
            .map_err(convert_try_read_error)
    }
}

impl core::ops::Deref for InquiryStaticBundle {
    type Target = usb::ehci::bbb::CommandBundle;

    fn deref(&self) -> &Self::Target {
        &self.bundle
    }
}

impl core::ops::DerefMut for InquiryStaticBundle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.bundle
    }
}

pub fn inquiry_bundle(
    address: Address,
    endpoint_speed: EndpointSpeed,
    logical_unit_number: LogicalUnitNumber,
    tag: CommandWrapperTag,
    bulk_in: crate::usb::mass_storage::EndpointDescriptor,
    bulk_out: crate::usb::mass_storage::EndpointDescriptor,
) -> error::ResultWithTrace<InquiryStaticBundle> {
    let inquiry_command = scsi::cdb::command::Inquiry::new(logical_unit_number);

    let mut bundle = usb::ehci::bbb::allocate_bundle(
        &usb::ehci::bbb::CommandAllocationRequest {
            address,
            endpoint_speed,
            logical_unit_number,
            tag,
            bulk_in,
            bulk_out,
            n_additional_queue_heads: 0,
            n_additional_qtds: 1,
            n_additional_buffers: 0,
            payload_length: (size_of::<scsi::cdb::command::Inquiry>() as u8).try_into()?,
            direction: usb::bbb::Direction::DeviceToHost,
            data_transfer_length: size_of::<scsi::cdb::data::Inquiry>() as u32,
        },
        inquiry_command.try_into()?,
    )?;

    bundle.logically_chain_queue_heads()?;

    bundle.initialize_high_speed_queue_transfer_descriptor(
        1,
        size_of::<scsi::cdb::data::Inquiry>() as u16,
        PacketId::In,
    )?;

    let inquiry_data_offset = bundle
        .first_free_byte_in_first_buffer
        .next_multiple_of(align_of::<scsi::cdb::data::Inquiry>());

    bundle.queue_transfer_descriptors_mut()[1].buffer_pointers_mut()[0] =
        Some(BufferIndex::new(0, inquiry_data_offset)?);

    bundle.logically_link_qtds(
        QtdLinkSource::QueueHead(QueueHeadIndex::from(1)),
        QtdLink::Next,
        Some(QueueTransferDescriptorIndex::from(1)),
    )?;
    bundle.logically_link_qtds(
        QtdLinkSource::QueueTransferDescriptor(QueueTransferDescriptorIndex::from(1)),
        QtdLink::Next,
        Some(QueueTransferDescriptorIndex::from(2)),
    )?;

    bundle.link_things_up()?;

    Ok(InquiryStaticBundle {
        bundle,
        inquiry_data_offset,
    })
}

pub fn test_unit_ready_bundle(
    address: Address,
    endpoint_speed: EndpointSpeed,
    logical_unit_number: LogicalUnitNumber,
    tag: CommandWrapperTag,
    bulk_in: crate::usb::mass_storage::EndpointDescriptor,
    bulk_out: crate::usb::mass_storage::EndpointDescriptor,
) -> error::ResultWithTrace<usb::ehci::bbb::CommandBundle> {
    type CommandType = scsi::cdb::command::TestUnitReady;
    let command = CommandType::new(logical_unit_number);

    let mut bundle = usb::ehci::bbb::allocate_bundle(
        &usb::ehci::bbb::CommandAllocationRequest {
            address,
            endpoint_speed,
            logical_unit_number,
            tag,
            bulk_in,
            bulk_out,
            n_additional_queue_heads: 0,
            n_additional_qtds: 0,
            n_additional_buffers: 0,
            payload_length: (size_of::<CommandType>() as u8).try_into()?,
            direction: usb::bbb::Direction::HostToDevice,
            data_transfer_length: 0,
        },
        command.try_into()?,
    )?;

    bundle.logically_chain_queue_heads()?;
    bundle.link_things_up()?;

    Ok(bundle)
}
