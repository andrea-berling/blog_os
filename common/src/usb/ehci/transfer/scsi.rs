use zerocopy::TryFromBytes as _;

use crate::{
    error::{self, convert_try_read_error},
    mmio::VolatileValue,
    scsi::{self, LogicalUnitNumber},
    usb::{
        self,
        bbb::CommandWrapperTag,
        ehci::{
            alloc::{
                AllocationRequest, QtdLink, QtdLinkSource, StaticBundle, allocate_static_bundle,
            },
            queue_head::{EndpointSpeed, QueueHeadIndex},
            transfer_descriptor::{
                BufferIndex, BufferPage, PacketId, QueueTransferDescriptorIndex,
            },
        },
        setup::Address,
    },
};

pub struct InquiryStaticBundle {
    bundle: StaticBundle,
    inquiry_data_offset: usize,
    status_offset: usize,
}

impl InquiryStaticBundle {
    pub fn get_inquiry_data(&self) -> error::Result<scsi::cdb::data::Inquiry> {
        let bytes = &self.buffers()[0][self.inquiry_data_offset..];
        scsi::cdb::data::Inquiry::try_read_from_prefix(bytes)
            .map(|(data, _)| data)
            .map_err(convert_try_read_error)
    }

    pub fn get_command_status_wrapper(&self) -> error::Result<usb::bbb::CommandStatusWrapper> {
        let bytes = &self.buffers()[0][self.status_offset..];
        usb::bbb::CommandStatusWrapper::try_read_from_prefix(bytes)
            .map(|(data, _)| data)
            .map_err(convert_try_read_error)
    }
}

impl core::ops::Deref for InquiryStaticBundle {
    type Target = StaticBundle;

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
    let mut bundle = allocate_static_bundle(AllocationRequest {
        n_queue_heads: 2,
        n_queue_transfer_descriptors: 3,
        n_buffers: 1,
    })?;

    bundle.initialize_high_speed_queue_head(
        0,
        address,
        bulk_out.number(),
        endpoint_speed,
        bulk_out.max_packet_length(),
        true,
    )?;

    bundle.initialize_high_speed_queue_head(
        1,
        address,
        bulk_in.number(),
        endpoint_speed,
        bulk_in.max_packet_length(),
        false,
    )?;

    bundle.logically_link_queue_heads(QueueHeadIndex::from(0), Some(QueueHeadIndex::from(1)))?;
    bundle.logically_link_queue_heads(QueueHeadIndex::from(1), Some(QueueHeadIndex::from(0)))?;

    bundle.logically_link_qtds(
        QtdLinkSource::QueueHead(QueueHeadIndex::from(0)),
        QtdLink::Next,
        Some(QueueTransferDescriptorIndex::from(0)),
    )?;

    bundle.logically_link_qtds(
        QtdLinkSource::QueueHead(QueueHeadIndex::from(1)),
        QtdLink::Next,
        Some(QueueTransferDescriptorIndex::from(1)),
    )?;

    let inquiry_command = scsi::cdb::command::Inquiry::new(logical_unit_number);
    let cbw = usb::bbb::CommandBlockWrapper::new(
        tag,
        size_of::<scsi::cdb::data::Inquiry>() as u32,
        usb::bbb::Direction::DeviceToHost,
        logical_unit_number,
        inquiry_command.try_into()?,
        (size_of::<scsi::cdb::command::Inquiry>() as u8).try_into()?,
    );

    bundle.initialize_high_speed_queue_transfer_descriptor(
        0,
        size_of::<usb::bbb::CommandBlockWrapper>() as u16,
        PacketId::Out,
    )?;

    bundle.queue_transfer_descriptors_mut()[0].buffer_pointers_mut()[0] =
        Some(BufferIndex::new(0, 0)?);

    let buffer = &mut bundle.buffers_mut()[0];

    let page: *mut BufferPage = core::ptr::from_mut(&mut **buffer);

    let request = page.cast::<VolatileValue<usb::bbb::CommandBlockWrapper>>();
    // SAFETY: page is mapped, 4096-aligned, zero-initialized; every bit
    // pattern is a valid CommandBlockWrapper (repr(C), integer fields); no other agent
    // accesses it concurrently
    unsafe { (&mut *request).set(cbw) };

    let inquiry_data_offset = size_of::<usb::bbb::CommandBlockWrapper>()
        .next_multiple_of(align_of::<scsi::cdb::data::Inquiry>());

    bundle.initialize_high_speed_queue_transfer_descriptor(
        1,
        size_of::<scsi::cdb::data::Inquiry>() as u16,
        PacketId::In,
    )?;
    bundle.queue_transfer_descriptors_mut()[1].buffer_pointers_mut()[0] =
        Some(BufferIndex::new(0, inquiry_data_offset)?);

    bundle.logically_link_qtds(
        QtdLinkSource::QueueTransferDescriptor(QueueTransferDescriptorIndex::from(1)),
        QtdLink::Next,
        Some(QueueTransferDescriptorIndex::from(2)),
    )?;

    bundle.initialize_high_speed_queue_transfer_descriptor(
        2,
        size_of::<usb::bbb::CommandStatusWrapper>() as u16,
        PacketId::In,
    )?;

    let status_offset = (inquiry_data_offset + size_of::<scsi::cdb::data::Inquiry>())
        .next_multiple_of(align_of::<usb::bbb::CommandStatusWrapper>());

    bundle.queue_transfer_descriptors_mut()[2].buffer_pointers_mut()[0] =
        Some(BufferIndex::new(0, status_offset)?);

    bundle.link_things_up()?;

    Ok(InquiryStaticBundle {
        bundle,
        inquiry_data_offset,
        status_offset,
    })
}
