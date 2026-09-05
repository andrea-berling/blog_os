use core::mem::transmute;

use crate::{
    array_vec::ArrayVec,
    bits::SmallBitSet,
    error::{self, Fault},
    mmio::VolatileValue,
    usb::{
        ehci::{
            queue_head::{
                self, BlankQueueHead, LogicalQueueHeadPointer, N_BLANK_QUEUE_HEADS, QueueHead,
                QueueHeadIndex, QueueHeadPointer, QueueHeadPointerBit, RawQueueHead,
                RawQueueHeadPointer,
            },
            transfer_descriptor::{
                self, BlankQueueTransferDescriptor, BufferIndex, BufferIndexNoOffset, BufferPage,
                BufferPointer, N_BLANK_BUFFER_PAGES, N_BLANK_QUEUE_TRANSFER_DESCRIPTORS, PacketId,
                QueueTransferDescriptor, QueueTransferDescriptorIndex,
                QueueTransferDescriptorPointer, QueueTransferDescriptorPointerBit,
            },
        },
        setup::{Address, MaxPacketLength, SetupData},
    },
};

const MAX_QUEUE_HEADS: usize = 1;
const MAX_QUEUE_TRANSFER_DESCRIPTORS: usize = 5;
const MAX_BUFFER_PAGES: usize = 5;

#[derive(Clone, Copy)]
pub struct AllocationRequest {
    pub n_queue_heads: usize,
    pub n_queue_transfer_descriptors: usize,
    pub n_buffers: usize,
}

type StaticQueueHeadsVec = ArrayVec<&'static mut queue_head::QueueHead, MAX_QUEUE_HEADS>;
type StaticQueueTDsVec = ArrayVec<
    &'static mut transfer_descriptor::QueueTransferDescriptor,
    MAX_QUEUE_TRANSFER_DESCRIPTORS,
>;
type StaticBuffersVec = ArrayVec<&'static mut transfer_descriptor::BufferPage, MAX_BUFFER_PAGES>;

/// Which structure a logical qTD link starts from
pub enum QtdLinkSource {
    QueueHead(QueueHeadIndex),
    QueueTransferDescriptor(QueueTransferDescriptorIndex),
}

/// Which of the two qTD links (next or alternate next) to set
pub enum QtdLink {
    Next,
    AlternateNext,
}

pub struct StaticBundle {
    queue_heads: StaticQueueHeadsVec,
    queue_transfer_descriptors: StaticQueueTDsVec,
    buffers: StaticBuffersVec,
}

pub struct StaticBundleAllocator {
    free_qh_bitmap: SmallBitSet<QueueHeadIndex>,
    queue_heads: &'static mut [BlankQueueHead],
    free_qtd_bitmap: SmallBitSet<QueueTransferDescriptorIndex>,
    queue_transfer_descriptors: &'static mut [BlankQueueTransferDescriptor],
    free_buffers_bitmap: SmallBitSet<BufferIndexNoOffset>,
    buffers: &'static mut [BufferPage],
}

impl StaticBundle {
    /// Logically links the horizontal link of the queue head at `from` to the queue head
    /// at `to`. Passing `None` terminates the link. The physical pointer is resolved by
    /// [`Self::link_things_up`]
    ///
    /// # Errors
    /// Fails with [`Fault::InvalidQueueHeadBundleReference`] if `from` is out of bounds
    pub fn logically_link_queue_heads(
        &mut self,
        from: QueueHeadIndex,
        to: Option<QueueHeadIndex>,
    ) -> error::Result<()> {
        let Some(qh) = self.queue_heads_mut().get_mut(usize::from(from)) else {
            return Err(Fault::InvalidQueueHeadBundleReference(usize::from(from)).into());
        };
        *qh.queue_head_horizontal_link_pointer_mut() = to.map(LogicalQueueHeadPointer::QueueHead);
        Ok(())
    }

    /// Logically links the next or alternate-next qTD link of the queue head or transfer
    /// descriptor at `from` to the transfer descriptor at `to`. Passing `None` terminates
    /// the link. The physical pointer is resolved by [`Self::link_things_up`].
    ///
    /// Only sane link combinations are expressible: queue heads and transfer descriptors
    /// can only link to transfer descriptors (the types of `from` and `to` enforce this)
    ///
    /// # Errors
    /// Fails with [`Fault::InvalidQueueHeadBundleReference`] or
    /// [`Fault::InvalidTransferDescriptorBundleReference`] if `from` is out of bounds
    pub fn logically_link_qtds(
        &mut self,
        from: QtdLinkSource,
        link: QtdLink,
        to: Option<QueueTransferDescriptorIndex>,
    ) -> error::Result<()> {
        match from {
            QtdLinkSource::QueueHead(qh_index) => {
                let Some(qh) = self.queue_heads_mut().get_mut(usize::from(qh_index)) else {
                    return Err(
                        Fault::InvalidQueueHeadBundleReference(usize::from(qh_index)).into(),
                    );
                };
                match link {
                    QtdLink::Next => qh.set_next_queue_transfer_descriptor_index(to),
                    QtdLink::AlternateNext => {
                        qh.set_alternate_next_queue_transfer_descriptor_index(to)
                    }
                }
            }
            QtdLinkSource::QueueTransferDescriptor(qtd_index) => {
                let Some(qtd) = self
                    .queue_transfer_descriptors_mut()
                    .get_mut(usize::from(qtd_index))
                else {
                    return Err(Fault::InvalidTransferDescriptorBundleReference(usize::from(
                        qtd_index,
                    ))
                    .into());
                };
                match link {
                    QtdLink::Next => qtd.set_next_queue_transfer_descriptor_index(to),
                    QtdLink::AlternateNext => {
                        qtd.set_alternate_next_queue_transfer_descriptor_index(to)
                    }
                }
            }
        }
        Ok(())
    }

    /// Resolves all the logical links (set through [`Self::logically_link_queue_heads`]
    /// and [`Self::logically_link_qtds`]) into physical pointers, terminating any link
    /// left as `None`
    ///
    /// # Errors
    /// Fails if any logical link refers to a structure outside the bundle
    pub fn link_things_up(&mut self) -> error::Result<()> {
        let qh_ptrs = {
            let mut v: ArrayVec<_, MAX_QUEUE_HEADS> = ArrayVec::new();
            for slot in &self.queue_heads {
                v.try_push(&raw const **slot)?;
            }
            v
        };

        let qtds_ptrs = {
            let mut v: ArrayVec<_, MAX_QUEUE_TRANSFER_DESCRIPTORS> = ArrayVec::new();
            for slot in &self.queue_transfer_descriptors {
                v.try_push(&raw const **slot)?;
            }
            v
        };

        let buffers_ptrs = {
            let mut v: ArrayVec<_, MAX_BUFFER_PAGES> = ArrayVec::new();
            for slot in &self.buffers {
                v.try_push(&raw const **slot)?;
            }
            v
        };

        for qh in self.queue_heads_mut() {
            match *qh.queue_head_horizontal_link_pointer_mut() {
                Some(LogicalQueueHeadPointer::QueueHead(next_idx)) => {
                    let next_idx = usize::from(next_idx);
                    if next_idx >= qh_ptrs.len() {
                        return Err(Fault::InvalidQueueHeadBundleReference(next_idx).into());
                    }
                    let tgt_ptr = qh_ptrs[next_idx];
                    let ptr = QueueHeadPointer::QueueHead(tgt_ptr.cast());
                    let mut next_pointer = RawQueueHeadPointer::from(ptr);
                    next_pointer.clear_flag(QueueHeadPointerBit::Terminate);
                    qh.queue_head_link_pointer_mut().set(next_pointer);
                }
                None => {
                    qh.queue_head_link_pointer_mut()
                        .set(QueueHeadPointerBit::Terminate.into());
                }
                _ => {
                    todo!("Add support for IsochronousTransferDescriptor, SplitTransactionIsochronousTransferDescriptor,
    FrameSpanTraversalNode")
                }
            }

            let next_qtd = qh.next_queue_transfer_descriptor_index();
            Self::resolve_qtd_link(qh.next_qtd_pointer_mut(), next_qtd, &qtds_ptrs)?;
            let alternate_next_qtd = qh.alternate_next_queue_transfer_descriptor_index();
            Self::resolve_qtd_link(
                qh.alternate_next_qtd_pointer_mut(),
                alternate_next_qtd,
                &qtds_ptrs,
            )?;
        }

        for qtd in self.queue_transfer_descriptors_mut() {
            let next_qtd = qtd.next_queue_transfer_descriptor_index();
            Self::resolve_qtd_link(qtd.next_pointer_mut(), next_qtd, &qtds_ptrs)?;
            let alternate_next_qtd = qtd.alternate_next_queue_transfer_descriptor_index();
            Self::resolve_qtd_link(
                qtd.alternate_next_pointer_mut(),
                alternate_next_qtd,
                &qtds_ptrs,
            )?;

            for (pointer_index, buffer_index) in qtd.buffer_pointers().iter().enumerate() {
                if let &Some(buffer_index) = buffer_index {
                    let offset = buffer_index.offset();
                    let buffer_index = usize::from(buffer_index.index());
                    if buffer_index >= buffers_ptrs.len() {
                        return Err(Fault::InvalidBufferPageBundleReference(buffer_index).into());
                    }
                    let tgt_ptr = buffers_ptrs[buffer_index];
                    let mut pointer = BufferPointer::default();
                    pointer.set_address(tgt_ptr)?;
                    pointer.set_current_offset(offset)?;
                    qtd.raw_mut().buffer_pointers_mut()[pointer_index].set(pointer);
                }
            }
        }

        Ok(())
    }

    /// Resolves a logical qTD link into a physical pointer, terminating it if `to` is
    /// `None`
    fn resolve_qtd_link(
        pointer: &mut VolatileValue<QueueTransferDescriptorPointer>,
        to: Option<QueueTransferDescriptorIndex>,
        qtds_ptrs: &[*const QueueTransferDescriptor],
    ) -> error::Result<()> {
        match to {
            Some(idx) if usize::from(idx) < qtds_ptrs.len() => {
                let mut next_pointer =
                    QueueTransferDescriptorPointer::from(qtds_ptrs[usize::from(idx)]);
                next_pointer.clear_flag(QueueTransferDescriptorPointerBit::Terminate);
                pointer.set(next_pointer);
                Ok(())
            }
            Some(idx) => {
                Err(Fault::InvalidTransferDescriptorBundleReference(usize::from(idx)).into())
            }
            None => {
                pointer.set(QueueTransferDescriptorPointerBit::Terminate.into());
                Ok(())
            }
        }
    }

    pub fn initialize_control_queue_head(
        &mut self,
        address: Address,
        endpoint_speed: queue_head::EndpointSpeed,
        max_packet_length: Option<MaxPacketLength>,
    ) -> Result<(), error::Error> {
        use crate::usb::ehci::queue_head::EndpointCharacteristicsBit::HeadOfReclamationListFlag;
        use crate::usb::ehci::queue_head::HighBandwidthPipeMultiplier::OneTransactionPerMicroFrame;
        let qh = &mut self.queue_heads_mut()[0];
        qh.clear_overlay_area();
        let mut endpoint_characteristics = queue_head::EndpointCharacteristics::empty();
        endpoint_characteristics.set_max_packet_length(
            max_packet_length.unwrap_or(MaxPacketLength::DEFAULT_CONTROL_PIPE_MAX_PACKET_LENGTH),
        );
        endpoint_characteristics.set_flag(HeadOfReclamationListFlag);
        endpoint_characteristics.set_endpoint_speed(endpoint_speed);
        endpoint_characteristics.set_endpoint_number(0);
        endpoint_characteristics.set_device_address(address);
        qh.endpoint_characteristics_mut()
            .set(endpoint_characteristics);
        let mut endpoint_capabilities = queue_head::EndpointCapabilities::empty();
        // Table 3-20 of the EHCI spec: for the default control pipe of a high-speed
        // device (the only kind supported here), every field except the pipe multiplier
        // must be zero, and the multiplier must be 1 (one transaction per micro-frame)
        endpoint_capabilities.set_interrupt_schedule_mask(0);
        endpoint_capabilities.set_split_completion_mask(0);
        endpoint_capabilities.set_hub_address(0);
        endpoint_capabilities.set_port_number(0);
        endpoint_capabilities.set_high_bandwidth_pipe_multiplier(OneTransactionPerMicroFrame);
        qh.endpoint_capabilities_mut().set(endpoint_capabilities);
        // The head of the async reclamation list links to itself
        self.logically_link_queue_heads(QueueHeadIndex::from(0), Some(QueueHeadIndex::from(0)))?;
        self.logically_link_qtds(
            QtdLinkSource::QueueHead(QueueHeadIndex::from(0)),
            QtdLink::Next,
            Some(QueueTransferDescriptorIndex::from(0)),
        )?;
        Ok(())
    }

    pub fn initialize_setup_queue_transfer_descriptor(
        &mut self,
        setup_data: SetupData,
    ) -> Result<(), error::Error> {
        use crate::usb::ehci::transfer_descriptor::PacketId::*;
        use crate::usb::ehci::transfer_descriptor::QueueTransferDescriptorTokenBit::Active;
        use crate::usb::ehci::transfer_descriptor::QueueTransferDescriptorTokenBit::InterruptOnComplete;
        let td1 = &mut self.queue_transfer_descriptors_mut()[0];
        let mut token: transfer_descriptor::QueueTransferDescriptorToken = Default::default();
        token.set_total_bytes_to_transfer(size_of::<SetupData>() as u16)?;
        token.clear_flag(InterruptOnComplete);
        token.set_current_page(0)?;
        token.set_packet_id(Setup);
        token.set_flag(Active);
        td1.token_mut().set(token);
        td1.buffer_pointers_mut()[0] = Some(BufferIndex::new(0, 0)?);

        let buffer = &mut self.buffers_mut()[0];

        let page: *mut BufferPage = core::ptr::from_mut(&mut **buffer);
        let request = page.cast::<VolatileValue<SetupData>>();
        // SAFETY: page is mapped, 4096-aligned, zero-initialized; every bit
        // pattern is a valid SetupData (repr(C), integer fields); no other agent
        // accesses it concurrently
        unsafe { (&mut *request).set(setup_data) };

        self.logically_link_qtds(
            QtdLinkSource::QueueTransferDescriptor(QueueTransferDescriptorIndex::from(0)),
            QtdLink::Next,
            Some(QueueTransferDescriptorIndex::from(1)),
        )?;
        Ok(())
    }

    pub fn handshake_last_queue_transfer_descriptor(
        &mut self,
        packet_id: PacketId,
    ) -> Result<(), error::Error> {
        use crate::usb::ehci::transfer_descriptor::QueueTransferDescriptorTokenBit::Active;
        use crate::usb::ehci::transfer_descriptor::QueueTransferDescriptorTokenBit::InterruptOnComplete;
        let Some(last_td) = self.queue_transfer_descriptors_mut().last_mut() else {
            // Bundles are always allocated with at least one qTD
            unreachable!("bundles always contain at least one queue transfer descriptor")
        };
        let mut token: transfer_descriptor::QueueTransferDescriptorToken = Default::default();
        token.set_total_bytes_to_transfer(0)?;
        token.clear_flag(InterruptOnComplete);
        token.set_packet_id(packet_id);
        token.set_flag(Active);
        last_td.token_mut().set(token);
        Ok(())
    }

    pub fn queue_heads(&self) -> &ArrayVec<&'static mut queue_head::QueueHead, MAX_QUEUE_HEADS> {
        &self.queue_heads
    }

    pub fn queue_transfer_descriptors(
        &self,
    ) -> &ArrayVec<
        &'static mut transfer_descriptor::QueueTransferDescriptor,
        MAX_QUEUE_TRANSFER_DESCRIPTORS,
    > {
        &self.queue_transfer_descriptors
    }

    pub fn queue_heads_mut(
        &mut self,
    ) -> &mut ArrayVec<&'static mut queue_head::QueueHead, MAX_QUEUE_HEADS> {
        &mut self.queue_heads
    }

    pub fn queue_transfer_descriptors_mut(
        &mut self,
    ) -> &mut ArrayVec<
        &'static mut transfer_descriptor::QueueTransferDescriptor,
        MAX_QUEUE_TRANSFER_DESCRIPTORS,
    > {
        &mut self.queue_transfer_descriptors
    }

    pub fn first_queue_head_raw(&self) -> *const RawQueueHead {
        (&raw const *self.queue_heads()[0]).cast()
    }

    pub fn get_status(&self) -> transfer_descriptor::QueueTransferDescriptorToken {
        let token = self.queue_heads[0].execution_cache_area();
        token.read_with(|token| token.get_status())
    }

    pub fn buffers(&self) -> &StaticBuffersVec {
        &self.buffers
    }

    pub fn buffers_mut(&mut self) -> &mut StaticBuffersVec {
        &mut self.buffers
    }

    pub fn first_qh_was_fetched(&self) -> bool {
        !self.queue_heads()[0]
            .raw()
            .current_qtd_pointer()
            .read_with(|current_qtd_pointer| current_qtd_pointer.is_null())
    }
}

impl StaticBundleAllocator {
    pub fn allocate(&mut self, request: AllocationRequest) -> error::Result<StaticBundle> {
        let mut queue_heads: StaticQueueHeadsVec = ArrayVec::new();
        let mut queue_transfer_descriptors: StaticQueueTDsVec = ArrayVec::new();
        let mut buffers: StaticBuffersVec = ArrayVec::new();

        if request.n_queue_heads > queue_heads.capacity()
            || request.n_queue_transfer_descriptors > queue_transfer_descriptors.capacity()
            || request.n_buffers > buffers.capacity()
        {
            return Err(Fault::TooManyEHCIDataStructuresRequested.into());
        }

        if self.free_qh_bitmap.size() < request.n_queue_heads
            || self.free_qtd_bitmap.size() < request.n_queue_transfer_descriptors
            || self.free_buffers_bitmap.size() < request.n_buffers
        {
            return Err(Fault::OutOfEHCIDataStructures.into());
        }

        for pool_index in self.free_qh_bitmap.take(request.n_queue_heads) {
            let queue_head: &'static mut QueueHead =
                // SAFETY: BlankQueueHead has repr(C) and enough bytes to host a QueueHead, and is
                // statically allocated
                unsafe { transmute(&mut self.queue_heads[usize::from(pool_index)]) };
            queue_head.init(pool_index);
            queue_heads.try_push(queue_head)?;
        }

        for pool_index in self
            .free_qtd_bitmap
            .take(request.n_queue_transfer_descriptors)
        {
            let transfer_descriptor: &'static mut QueueTransferDescriptor =
                // SAFETY: BlankTransferDescriptor has repr(C) and enough bytes to host a
                // TransferDescriptor, and is statically allocated
                unsafe { transmute(&mut self.queue_transfer_descriptors[usize::from(pool_index)]) };
            transfer_descriptor.init(pool_index);
            queue_transfer_descriptors.try_push(transfer_descriptor)?;
        }

        for pool_index in self.free_buffers_bitmap.take(request.n_buffers) {
            // NOTE: we don't keep a self_index for BufferPage, alignment to 4096 would make
            // instances have size 8192, of which only ~4K used
            // Upon freeing, we use the address of the buffer to identify which item we got
            // Nevertheless, let's type check pool_index, to make sure we're popping from
            // the right set
            let _: BufferIndexNoOffset = pool_index;
            let buffer_page: &'static mut BufferPage =
                // SAFETY: even though we're borrowing mutably multiple times from self.buffers,
                // it's all disjoint borrows
                unsafe { transmute(&mut self.buffers[usize::from(pool_index)]) };
            buffer_page.clear();
            buffers.try_push(buffer_page)?;
        }

        Ok(StaticBundle {
            queue_heads,
            queue_transfer_descriptors,
            buffers,
        })
    }

    pub fn free_bundle(&mut self, bundle: &StaticBundle) {
        for qh in &bundle.queue_heads {
            // The pool index of an allocated queue head always comes from this same bit
            // set, so the insert below can't fail, but the type system doesn't know that
            let _ = self.free_qh_bitmap.insert(qh.pool_index());
        }
        for qtd in &bundle.queue_transfer_descriptors {
            // Same as above: pool indexes always come from this same bit set
            let _ = self.free_qtd_bitmap.insert(qtd.pool_index());
        }

        for buffer in &bundle.buffers {
            // SAFETY: `buffer` points to a page inside BLANK_BUFFER_PAGES, the same array
            // `buffer_page_ptr` points to (both come from the global allocator), so
            // `offset_from` is in-bounds of a single allocation, which makes it sound.
            // NOTE: for buffer pages, storing the pool index alongside the data would
            // double the memory cost (the 4096-byte alignment would round a
            // (4096 + metadata) struct up to 8192 bytes), so identifying the page via
            // pointer arithmetic is kind of forced
            #[allow(static_mut_refs)]
            let buffer_page_ptr: *const BufferPage =
                unsafe { transfer_descriptor::BLANK_BUFFER_PAGES.as_ptr().cast() };
            // SAFETY: see above
            let buffer_index =
                unsafe { (&raw const **buffer).offset_from(buffer_page_ptr) } as usize;
            // Same as above: the index is inside the allocation, so in bounds
            let _ = self
                .free_buffers_bitmap
                .insert(BufferIndexNoOffset::from(buffer_index));
        }
    }
}

impl Drop for StaticBundle {
    fn drop(&mut self) {
        // SAFETY: there is no other thread of execution than the main one, so the global
        // allocator is not being concurrently accessed
        #[allow(static_mut_refs)]
        unsafe {
            GLOBAL_BUNDLE_ALLOCATOR.free_bundle(self)
        };
    }
}

pub fn allocate_static_bundle(request: AllocationRequest) -> error::Result<StaticBundle> {
    // SAFETY: No threads, no problem
    unsafe {
        #[allow(static_mut_refs)]
        GLOBAL_BUNDLE_ALLOCATOR.allocate(request)
    }
}

#[allow(static_mut_refs)]
static mut GLOBAL_BUNDLE_ALLOCATOR: StaticBundleAllocator = const {
    StaticBundleAllocator {
        free_qh_bitmap: {
            // SAFETY: Comp-time asserts are fine, they are caught at build time
            let mut bitmap = unsafe { SmallBitSet::new(N_BLANK_QUEUE_HEADS) };
            bitmap.fill();
            bitmap
        },
        free_qtd_bitmap: {
            // SAFETY: Comp-time asserts are fine, they are caught at build time
            let mut bitmap = unsafe { SmallBitSet::new(N_BLANK_QUEUE_TRANSFER_DESCRIPTORS) };
            bitmap.fill();
            bitmap
        },
        free_buffers_bitmap: {
            // SAFETY: Comp-time asserts are fine, they are caught at build time
            let mut bitmap = unsafe { SmallBitSet::new(N_BLANK_BUFFER_PAGES) };
            bitmap.fill();
            bitmap
        },
        // SAFETY: There is no other thread of execution than the main one accessing this static
        queue_heads: unsafe { &mut queue_head::BLANK_QUEUE_HEADS },
        // SAFETY: There is no other thread of execution than the main one accessing this static
        queue_transfer_descriptors: unsafe {
            &mut transfer_descriptor::BLANK_QUEUE_TRANSFER_DESCRIPTORS
        },
        // SAFETY: There is no other thread of execution than the main one accessing this static
        buffers: unsafe { &mut transfer_descriptor::BLANK_BUFFER_PAGES },
    }
};
