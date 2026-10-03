use zerocopy::TryFromBytes as _;

use crate::{
    error::{self},
    mmio::VolatileValue,
    scsi::LogicalUnitNumber,
    usb::{
        bbb::{
            CommandBlock, CommandBlockLength, CommandBlockWrapper, CommandStatusWrapper,
            CommandWrapperTag, Direction,
        },
        ehci::{
            alloc::{self, QtdLink, QtdLinkSource, allocate_static_bundle},
            queue_head::{EndpointSpeed, QueueHeadIndex},
            transfer_descriptor::{
                BufferIndex, BufferPage, PacketId, QueueTransferDescriptorIndex,
            },
        },
        setup::Address,
    },
};

pub struct CommandBundle {
    pub bundle: alloc::StaticBundle,
    pub status_offset: usize,
    pub first_free_byte_in_first_buffer: usize,
}

pub struct CommandAllocationRequest {
    pub address: Address,
    pub endpoint_speed: EndpointSpeed,
    pub logical_unit_number: LogicalUnitNumber,
    pub tag: CommandWrapperTag,
    pub bulk_in: crate::usb::mass_storage::EndpointDescriptor,
    pub bulk_out: crate::usb::mass_storage::EndpointDescriptor,
    pub n_additional_queue_heads: usize,
    pub n_additional_qtds: usize,
    pub n_additional_buffers: usize,
    pub payload_length: CommandBlockLength,
    pub direction: Direction,
    pub data_transfer_length: u32,
}

impl CommandBundle {
    pub fn get_command_status_wrapper(&self) -> error::Result<CommandStatusWrapper> {
        let bytes = &self.bundle.buffers()[0][self.status_offset..];
        CommandStatusWrapper::try_read_from_prefix(bytes)
            .map(|(data, _)| data)
            .map_err(error::convert_try_read_error)
    }
}

impl core::ops::Deref for CommandBundle {
    type Target = alloc::StaticBundle;

    fn deref(&self) -> &Self::Target {
        &self.bundle
    }
}

impl core::ops::DerefMut for CommandBundle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.bundle
    }
}

pub fn allocate_bundle(
    allocation_request: &CommandAllocationRequest,
    payload: CommandBlock,
) -> error::ResultWithTrace<CommandBundle> {
    let &CommandAllocationRequest {
        address,
        endpoint_speed,
        logical_unit_number,
        tag,
        bulk_in,
        bulk_out,
        n_additional_queue_heads,
        n_additional_qtds,
        n_additional_buffers,
        payload_length,
        direction,
        data_transfer_length,
    } = allocation_request;

    let n_queue_heads = 2 + n_additional_queue_heads;
    let n_queue_transfer_descriptors = 2 + n_additional_qtds;
    let n_buffers = 1 + n_additional_buffers;

    let mut bundle = allocate_static_bundle(alloc::AllocationRequest {
        n_queue_heads,
        n_queue_transfer_descriptors,
        n_buffers,
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
        n_queue_heads - 1,
        address,
        bulk_in.number(),
        endpoint_speed,
        bulk_in.max_packet_length(),
        false,
    )?;

    bundle.logically_link_qtds(
        QtdLinkSource::QueueHead(QueueHeadIndex::from(0)),
        QtdLink::Next,
        Some(QueueTransferDescriptorIndex::from(0)),
    )?;

    bundle.logically_link_qtds(
        QtdLinkSource::QueueHead(QueueHeadIndex::from(n_queue_heads - 1)),
        QtdLink::Next,
        Some(QueueTransferDescriptorIndex::from(
            n_queue_transfer_descriptors - 1,
        )),
    )?;

    let cbw = CommandBlockWrapper::new(
        tag,
        data_transfer_length,
        direction,
        logical_unit_number,
        payload,
        payload_length,
    );

    bundle.initialize_high_speed_queue_transfer_descriptor(
        0,
        size_of::<CommandBlockWrapper>() as u16,
        PacketId::Out,
    )?;

    bundle.initialize_high_speed_queue_transfer_descriptor(
        n_queue_transfer_descriptors - 1,
        size_of::<CommandStatusWrapper>() as u16,
        PacketId::In,
    )?;

    bundle.queue_transfer_descriptors_mut()[0].buffer_pointers_mut()[0] =
        Some(BufferIndex::new(0, 0)?);

    let buffer = &mut bundle.buffers_mut()[0];

    let page: *mut BufferPage = core::ptr::from_mut(&mut **buffer);

    let request: *mut VolatileValue<CommandBlockWrapper> = page.cast();
    // SAFETY: page is mapped, 4096-aligned, zero-initialized; every bit
    // pattern is a valid CommandBlockWrapper (repr(C), integer fields); no other agent
    // accesses it concurrently
    unsafe { (&mut *request).set(cbw) };

    let status_offset =
        size_of::<CommandBlockWrapper>().next_multiple_of(align_of::<CommandStatusWrapper>());

    bundle.queue_transfer_descriptors_mut()[n_queue_transfer_descriptors - 1]
        .buffer_pointers_mut()[0] = Some(BufferIndex::new(0, status_offset)?);

    Ok(CommandBundle {
        bundle,
        status_offset,
        first_free_byte_in_first_buffer: status_offset + size_of::<CommandStatusWrapper>(),
    })
}
