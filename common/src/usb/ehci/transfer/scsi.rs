use core::marker::PhantomData;

use crate::{
    error::{self, convert_try_read_error},
    scsi::{self, BlockLengthBytes, LogicalBlockAddress, LogicalUnitNumber},
    usb::{
        self,
        bbb::CommandWrapperTag,
        ehci::{
            alloc::{QtdLink, QtdLinkSource},
            bbb::CommandBundle,
            queue_head::{EndpointSpeed, QueueHeadIndex},
            transfer_descriptor::{
                BufferIndex, BufferPage, PacketId, QueueTransferDescriptorIndex,
            },
        },
        setup::Address,
    },
};

pub const SECTOR_SIZE: usize = 512;

pub struct StaticBundleWithData<T: zerocopy::TryFromBytes> {
    bundle: usb::ehci::bbb::CommandBundle,
    data_offset: usize,
    data: PhantomData<T>,
}

pub struct ReadBundle {
    bundle: usb::ehci::bbb::CommandBundle,
    bytes_to_read: usize,
}

pub struct ChunksIterator<'a> {
    bundle: &'a usb::ehci::bbb::CommandBundle,
    current_sector: usize,
    bytes_left_to_read: usize,
}

pub struct Chunk<'a>(&'a [u8]);

pub trait BundleWithData {
    type Data;

    fn get_data(&self) -> error::Result<Self::Data>;
}

impl<T: zerocopy::TryFromBytes> StaticBundleWithData<T> {
    pub fn new(bundle: usb::ehci::bbb::CommandBundle, data_offset: usize) -> Self {
        Self {
            bundle,
            data_offset,
            data: PhantomData,
        }
    }
}

impl<T: zerocopy::TryFromBytes> BundleWithData for StaticBundleWithData<T> {
    type Data = T;

    fn get_data(&self) -> error::Result<Self::Data> {
        let bytes = &self.buffers()[0][self.data_offset..];
        T::try_read_from_prefix(bytes)
            .map(|(data, _)| data)
            .map_err(convert_try_read_error)
    }
}

impl<T: zerocopy::TryFromBytes> core::ops::Deref for StaticBundleWithData<T> {
    type Target = usb::ehci::bbb::CommandBundle;

    fn deref(&self) -> &Self::Target {
        &self.bundle
    }
}

impl<T: zerocopy::TryFromBytes> core::ops::DerefMut for StaticBundleWithData<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.bundle
    }
}

impl ReadBundle {
    pub fn chunks(&self) -> ChunksIterator<'_> {
        ChunksIterator {
            bundle: &self.bundle,
            current_sector: 0,
            bytes_left_to_read: self.bytes_to_read,
        }
    }
}

impl core::ops::Deref for ReadBundle {
    type Target = CommandBundle;

    fn deref(&self) -> &Self::Target {
        &self.bundle
    }
}

impl<'a> Iterator for ChunksIterator<'a> {
    type Item = Chunk<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes_left_to_read == 0 {
            return None;
        }
        let starting_byte_position = self.current_sector * SECTOR_SIZE;
        let buffer_index = 1 + starting_byte_position / size_of::<BufferPage>();
        let offset = starting_byte_position % size_of::<BufferPage>();
        let n_bytes = core::cmp::min(SECTOR_SIZE, self.bytes_left_to_read);

        self.current_sector += 1;
        self.bytes_left_to_read -= n_bytes;

        Some(Chunk(
            &self.bundle.buffers()[buffer_index][offset..][..n_bytes],
        ))
    }
}

impl<'a> core::ops::Deref for Chunk<'a> {
    type Target = &'a [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub fn inquiry_bundle(
    address: Address,
    endpoint_speed: EndpointSpeed,
    logical_unit_number: LogicalUnitNumber,
    tag: CommandWrapperTag,
    bulk_in: crate::usb::mass_storage::EndpointDescriptor,
    bulk_out: crate::usb::mass_storage::EndpointDescriptor,
) -> error::ResultWithTrace<StaticBundleWithData<scsi::cdb::data::Inquiry>> {
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

    Ok(StaticBundleWithData::new(bundle, inquiry_data_offset))
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

pub fn read_capacity_bundle(
    address: Address,
    endpoint_speed: EndpointSpeed,
    logical_unit_number: LogicalUnitNumber,
    tag: CommandWrapperTag,
    bulk_in: crate::usb::mass_storage::EndpointDescriptor,
    bulk_out: crate::usb::mass_storage::EndpointDescriptor,
) -> error::ResultWithTrace<StaticBundleWithData<scsi::cdb::data::ReadCapacity>> {
    type CommandType = scsi::cdb::command::ReadCapacity;
    type DataType = scsi::cdb::data::ReadCapacity;
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
            n_additional_qtds: 1,
            n_additional_buffers: 0,
            // The bootability specification pads every CDB to 12 bytes and requires devices
            // to accept a bCBWCBLength of 12 (sections 1.3.3 and 2).
            payload_length: (size_of::<CommandType>() as u8).try_into()?,
            direction: usb::bbb::Direction::DeviceToHost,
            data_transfer_length: size_of::<DataType>() as u32,
        },
        command.try_into()?,
    )?;

    bundle.logically_chain_queue_heads()?;

    bundle.initialize_high_speed_queue_transfer_descriptor(
        1,
        size_of::<DataType>() as u16,
        PacketId::In,
    )?;

    let data_offset = bundle
        .first_free_byte_in_first_buffer
        .next_multiple_of(align_of::<DataType>());

    bundle.queue_transfer_descriptors_mut()[1].buffer_pointers_mut()[0] =
        Some(BufferIndex::new(0, data_offset)?);

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

    Ok(StaticBundleWithData::new(bundle, data_offset))
}

// NOTE: why don't we link a TD to get the CSW? This bug: https://gitlab.com/qemu-project/qemu/-/work_items/4658
pub fn read_bundle(
    address: Address,
    endpoint_speed: EndpointSpeed,
    logical_unit_number: LogicalUnitNumber,
    tag: CommandWrapperTag,
    bulk_in: crate::usb::mass_storage::EndpointDescriptor,
    bulk_out: crate::usb::mass_storage::EndpointDescriptor,
    lba: LogicalBlockAddress,
    transfer_length: u16,
    block_length: BlockLengthBytes,
) -> error::ResultWithTrace<ReadBundle> {
    type CommandType = scsi::cdb::command::Read;
    let command = CommandType::new(logical_unit_number, lba, transfer_length);

    let bytes_to_read = transfer_length as u32 * u32::from(block_length);
    let mut bytes_left_to_read = bytes_to_read;

    let n_data_buffers = (bytes_left_to_read as usize).div_ceil(size_of::<BufferPage>());
    let needed_extra_qtds = n_data_buffers.div_ceil(5);

    let mut bundle = usb::ehci::bbb::allocate_bundle(
        &usb::ehci::bbb::CommandAllocationRequest {
            address,
            endpoint_speed,
            logical_unit_number,
            tag,
            bulk_in,
            bulk_out,
            n_additional_queue_heads: 0,
            n_additional_qtds: needed_extra_qtds,
            n_additional_buffers: n_data_buffers,
            // The bootability specification pads every CDB to 12 bytes and requires devices
            // to accept a bCBWCBLength of 12 (sections 1.3.3 and 2).
            payload_length: (size_of::<CommandType>() as u8).try_into()?,
            direction: usb::bbb::Direction::DeviceToHost,
            data_transfer_length: bytes_left_to_read,
        },
        command.try_into()?,
    )?;

    bundle.logically_chain_queue_heads()?;

    let mut buffer_cursor = 1;

    for qtd_index in 1..=needed_extra_qtds {
        let n_needed_buffers = core::cmp::min(
            bytes_left_to_read.div_ceil(size_of::<BufferPage>() as u32),
            5,
        );
        let bytes_to_transfer =
            core::cmp::min(bytes_left_to_read, 5 * size_of::<BufferPage>() as u32);
        bundle.initialize_high_speed_queue_transfer_descriptor(
            qtd_index,
            bytes_to_transfer as u16,
            PacketId::In,
        )?;

        for buffer_idx in 0..n_needed_buffers {
            bundle.queue_transfer_descriptors_mut()[qtd_index].buffer_pointers_mut()
                [buffer_idx as usize] =
                Some(BufferIndex::new((buffer_cursor + buffer_idx) as usize, 0)?);
        }

        if qtd_index < needed_extra_qtds {
            bundle.logically_link_qtds(
                QtdLinkSource::QueueTransferDescriptor(QueueTransferDescriptorIndex::from(
                    qtd_index,
                )),
                QtdLink::Next,
                Some(QueueTransferDescriptorIndex::from(qtd_index + 1)),
            )?;
        }

        buffer_cursor += n_needed_buffers;
        bytes_left_to_read -= bytes_to_transfer as u32;
    }

    bundle.logically_link_qtds(
        QtdLinkSource::QueueHead(QueueHeadIndex::from(1)),
        QtdLink::Next,
        Some(QueueTransferDescriptorIndex::from(1)),
    )?;

    bundle.link_things_up()?;

    Ok(ReadBundle {
        bundle,
        bytes_to_read: bytes_to_read as usize,
    })
}

pub fn csw_bundle(
    address: Address,
    endpoint_speed: EndpointSpeed,
    bulk_in: crate::usb::mass_storage::EndpointDescriptor,
    data_toggle: bool,
) -> error::ResultWithTrace<CommandBundle> {
    let mut bundle =
        usb::ehci::alloc::allocate_static_bundle(usb::ehci::alloc::AllocationRequest {
            n_queue_heads: 1,
            n_queue_transfer_descriptors: 1,
            n_buffers: 1,
        })?;

    bundle.initialize_high_speed_queue_head(
        0,
        address,
        bulk_in.number(),
        endpoint_speed,
        bulk_in.max_packet_length(),
        true,
    )?;

    bundle.queue_heads_mut()[0]
        .endpoint_characteristics_mut()
        .update(|endpoint_characteristics| {
            endpoint_characteristics
                .set_flag(usb::ehci::queue_head::EndpointCharacteristicsBit::DataToggleControl);
        });

    bundle.logically_chain_queue_heads()?;

    bundle.initialize_high_speed_queue_transfer_descriptor(
        0,
        size_of::<usb::bbb::CommandStatusWrapper>() as u16,
        PacketId::In,
    )?;

    bundle.queue_transfer_descriptors_mut()[0]
        .token_mut()
        .update(|token| {
            if data_toggle {
                token.set_flag(
                    usb::ehci::transfer_descriptor::QueueTransferDescriptorTokenBit::DataToggle,
                );
            } else {
                token.clear_flag(
                    usb::ehci::transfer_descriptor::QueueTransferDescriptorTokenBit::DataToggle,
                );
            }
        });

    bundle.queue_transfer_descriptors_mut()[0].buffer_pointers_mut()[0] =
        Some(BufferIndex::new(0, 0)?);

    bundle.logically_link_qtds(
        QtdLinkSource::QueueHead(QueueHeadIndex::from(0)),
        QtdLink::Next,
        Some(QueueTransferDescriptorIndex::from(0)),
    )?;

    bundle.link_things_up()?;

    Ok(CommandBundle {
        bundle,
        status_offset: 0,
        first_free_byte_in_first_buffer: size_of::<usb::bbb::CommandStatusWrapper>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_map_to_the_page_the_dma_filled() {
        // Two data pages worth of sectors: 16 sectors of 512 bytes.
        let Ok(mut bundle) =
            usb::ehci::alloc::allocate_static_bundle(usb::ehci::alloc::AllocationRequest {
                n_queue_heads: 1,
                n_queue_transfer_descriptors: 1,
                n_buffers: 3,
            })
        else {
            panic!("the pool still has a bundle this small")
        };

        // Every page carries its bundle index in each byte, so a chunk's contents
        // tell which page it came from.
        for (index, page) in bundle.buffers_mut().iter_mut().enumerate() {
            page.copy_from_slice(&[index as u8; size_of::<BufferPage>()]);
        }

        let read_bundle = ReadBundle {
            bundle: CommandBundle {
                bundle,
                status_offset: 0,
                first_free_byte_in_first_buffer: 0,
            },
            bytes_to_read: 2 * size_of::<BufferPage>(),
        };

        for (sector_index, chunk) in read_bundle.chunks().enumerate() {
            // The DMA fills the pages contiguously from page 1: sectors 0-7 land
            // in page 1, sectors 8-15 in page 2.
            let expected_page = 1 + sector_index / (size_of::<BufferPage>() / SECTOR_SIZE);
            assert_eq!(chunk.len(), SECTOR_SIZE);
            assert!(
                chunk.iter().all(|byte| *byte == expected_page as u8),
                "sector {sector_index} came from page {expected_page}"
            );
        }
    }
}
