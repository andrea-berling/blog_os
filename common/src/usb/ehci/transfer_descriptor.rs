use core::ops::{Deref, DerefMut};

use num_enum::TryFromPrimitive;

use crate::{
    bits,
    error::{self, Fault},
    make_bitmap,
    mmio::VolatileValue,
};

#[derive(Clone, Copy)]
#[repr(u32)]
pub enum QueueTransferDescriptorPointerBit {
    Terminate = 1,
}

make_bitmap!(new_type: QueueTransferDescriptorPointer, underlying_flag_type: QueueTransferDescriptorPointerBit, repr: u32, nodisplay);

#[derive(TryFromPrimitive, Clone, Copy)]
#[repr(u8)]
pub enum PacketId {
    Out,
    In,
    Setup,
    Reserved,
}

#[derive(Clone, Copy, TryFromPrimitive)]
#[repr(u32)]
pub enum QueueTransferDescriptorTokenBit {
    PingState = 1,
    SplitTransactionState = 1 << 1,
    MissedMicroFrame = 1 << 2,
    TransactionError = 1 << 3,
    BabbleDetected = 1 << 4,
    DataBufferError = 1 << 5,
    Halted = 1 << 6,
    Active = 1 << 7,
    InterruptOnComplete = 1 << 15,
    DataToggle = 1 << 31,
}

make_bitmap!(new_type: QueueTransferDescriptorToken, underlying_flag_type: QueueTransferDescriptorTokenBit, repr: u32, nodisplay);

#[derive(Default)]
pub struct BufferPointer {
    bits: u32,
}

#[repr(align(4096))]
pub struct BufferPage([u8; 4096]);

#[derive(Clone, Copy, Debug)]
pub struct BufferIndexNoOffset(usize);

#[derive(Clone, Copy, Debug)]
pub struct BufferIndex {
    index: BufferIndexNoOffset,
    offset: u16,
}

#[repr(C)]
pub struct RawQueueTransferDescriptor {
    next_pointer: VolatileValue<QueueTransferDescriptorPointer>,
    alternate_next_pointer: VolatileValue<QueueTransferDescriptorPointer>,
    token: VolatileValue<QueueTransferDescriptorToken>,
    buffer_pointers: [VolatileValue<BufferPointer>; 5],
}

#[derive(Clone, Copy, Debug)]
pub struct QueueTransferDescriptorIndex(pub usize);

#[repr(C, align(32))]
pub struct QueueTransferDescriptor {
    raw: RawQueueTransferDescriptor,
    pool_index: QueueTransferDescriptorIndex,
    next_queue_transfer_descriptor_index: Option<QueueTransferDescriptorIndex>,
    alternate_next_queue_transfer_descriptor_index: Option<QueueTransferDescriptorIndex>,
    buffer_pointers: [Option<BufferIndex>; 5],
}

#[derive(Clone, Copy)]
#[repr(C, align(32))]
pub struct BlankQueueTransferDescriptor(
    [u32; size_of::<QueueTransferDescriptor>() / (u32::BITS / u8::BITS) as usize],
);

impl QueueTransferDescriptorPointer {
    pub fn with_terminate(mut self) -> Self {
        self.set_flag(QueueTransferDescriptorPointerBit::Terminate);
        self
    }

    pub fn is_null(&self) -> bool {
        self.bits == 0
    }
}

impl QueueTransferDescriptorToken {
    pub fn set_total_bytes_to_transfer(&mut self, total_bytes: u16) -> error::Result<()> {
        if total_bytes > 0x5000 {
            return Err(Fault::InvalidUSBTotalBytesToTransfer(total_bytes).into());
        }
        bits::set_bits!(bits_expr: self.bits, value: total_bytes, n_bits: 15, starts_at_bit: 16, bits_expr_ty: u32);
        Ok(())
    }

    pub fn get_total_bytes_to_transfer(&self) -> u16 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 15, starts_at_bit: 16, return_ty: u16)
    }

    pub fn set_current_page(&mut self, current_page: u8) -> error::Result<()> {
        if current_page > 0x4 {
            return Err(Fault::InvalidUSBCurrentPage(current_page).into());
        }
        bits::set_bits!(bits_expr: self.bits, value: current_page, n_bits: 3, starts_at_bit: 12, bits_expr_ty: u32);
        Ok(())
    }

    pub fn get_current_page(&self) -> u8 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 3, starts_at_bit: 12, return_ty: u8)
    }

    pub fn get_error_count(&self) -> u8 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 2, starts_at_bit: 10, return_ty: u8)
    }

    /// # Panics
    /// Never: the 2-bit field can only hold values matching a `PacketId` variant, so the
    /// conversion below can not fail, but Rust's type checker doesn't know that
    pub fn get_packet_id(&self) -> PacketId {
        bits::get_bits!(bits_expr: self.bits, n_bits: 2, starts_at_bit: 8, return_ty: u8)
            .try_into()
            .expect("the 2-bit field can only hold valid PacketId values")
    }

    pub fn set_packet_id(&mut self, packet_id: PacketId) {
        bits::set_bits!(bits_expr: self.bits, value: packet_id, n_bits: 2, starts_at_bit: 8, bits_expr_ty: u32);
    }

    pub fn get_status(&self) -> QueueTransferDescriptorToken {
        QueueTransferDescriptorToken {
            bits: bits::get_bits!(bits_expr: self.bits, n_bits: 8, starts_at_bit: 0, return_ty: u32),
        }
    }
}

impl BufferPointer {
    pub const ALIGNMENT: usize = 4096;
    pub const MAX_ALLOWED_OFFSET: usize = 0xfff;

    pub fn set_address(&mut self, address: *const BufferPage) -> error::Result<()> {
        if address as usize & (Self::ALIGNMENT - 1) != 0 {
            return Err(Fault::UnalignedEHCIBufferPagePointer(address as u32).into());
        }
        // FIXME: today addresses are always physical, one day they'll be virtual. This will break
        // that day
        bits::set_bits!(bits_expr: self.bits, value: address as u32 >> 12, n_bits: 20, starts_at_bit: 12, bits_expr_ty: u32);
        Ok(())
    }

    pub fn get_address(&self) -> u32 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 20, starts_at_bit: 12, return_ty: u32)
            << const { u32::ilog2(Self::ALIGNMENT as u32) }
    }

    pub fn set_current_offset(&mut self, offset: u16) -> error::Result<()> {
        if offset as usize > Self::MAX_ALLOWED_OFFSET {
            return Err(Fault::InvalidUSBCurrentBufferPagePointerOffset(offset).into());
        }
        bits::set_bits!(bits_expr: self.bits, value: offset, n_bits: 12, starts_at_bit: 0, bits_expr_ty: u32);
        Ok(())
    }

    pub fn get_current_offset(&self) -> u16 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 12, starts_at_bit: 0, return_ty: u16)
    }

    pub fn clear(&mut self) {
        self.bits = 0;
    }
}

impl BufferPage {
    pub fn clear(&mut self) {
        self.0.fill(0);
    }
}

impl BufferIndex {
    /// Creates a new [`BufferIndex`].
    ///
    /// # Errors
    /// Fails with [`Fault::InvalidEHCIBufferOffset`] if `offset` doesn't fit within a
    /// [`BufferPage`].
    pub fn new(index: usize, offset: usize) -> error::Result<Self> {
        let max = size_of::<BufferPage>() - 1;
        if offset > max {
            return Err(Fault::InvalidEHCIBufferOffset { offset, max }.into());
        }
        // offset <= max <= 0xfff, always fits in a u16
        Ok(Self {
            index: BufferIndexNoOffset(index),
            offset: offset as u16,
        })
    }

    pub fn index(&self) -> BufferIndexNoOffset {
        self.index
    }

    pub fn offset(&self) -> u16 {
        self.offset
    }
}

impl RawQueueTransferDescriptor {
    pub fn get_current_offset(&self) -> u16 {
        self.buffer_pointers[0].read_with(
            |buffer_pointer| bits::get_bits!(bits_expr: buffer_pointer.bits, n_bits: 12, starts_at_bit: 0, return_ty: u16),
        )
    }

    pub fn next_pointer_mut(&mut self) -> &mut VolatileValue<QueueTransferDescriptorPointer> {
        &mut self.next_pointer
    }

    pub fn alternate_next_pointer_mut(
        &mut self,
    ) -> &mut VolatileValue<QueueTransferDescriptorPointer> {
        &mut self.alternate_next_pointer
    }

    pub fn buffer_pointers_mut(&mut self) -> &mut [VolatileValue<BufferPointer>; 5] {
        &mut self.buffer_pointers
    }

    pub fn token_mut(&mut self) -> &mut VolatileValue<QueueTransferDescriptorToken> {
        &mut self.token
    }

    pub fn clear(&mut self) {
        self.next_pointer
            .set(QueueTransferDescriptorPointer::empty());
        self.alternate_next_pointer
            .set(QueueTransferDescriptorPointer::empty());
        self.token.set(QueueTransferDescriptorToken::empty());
        for buffer_pointer in &mut self.buffer_pointers[..] {
            buffer_pointer.update(|buffer_pointer| buffer_pointer.clear());
        }
    }
}

impl QueueTransferDescriptor {
    pub fn init(&mut self, self_index: QueueTransferDescriptorIndex) {
        self.raw.clear();
        self.pool_index = self_index;
        self.next_queue_transfer_descriptor_index = None;
        self.alternate_next_queue_transfer_descriptor_index = None;
        self.buffer_pointers = [None; _];
    }

    pub fn set_self_index(&mut self, self_index: QueueTransferDescriptorIndex) {
        self.pool_index = self_index;
    }

    pub fn set_next_queue_transfer_descriptor_index(
        &mut self,
        next_queue_transfer_descriptor_index: Option<QueueTransferDescriptorIndex>,
    ) {
        self.next_queue_transfer_descriptor_index = next_queue_transfer_descriptor_index;
    }

    pub fn set_alternate_next_queue_transfer_descriptor_index(
        &mut self,
        alternate_next_queue_transfer_descriptor_index: Option<QueueTransferDescriptorIndex>,
    ) {
        self.alternate_next_queue_transfer_descriptor_index =
            alternate_next_queue_transfer_descriptor_index;
    }

    pub fn buffer_pointers_mut(&mut self) -> &mut [Option<BufferIndex>; 5] {
        &mut self.buffer_pointers
    }

    pub fn next_queue_transfer_descriptor_index_mut(
        &mut self,
    ) -> &mut Option<QueueTransferDescriptorIndex> {
        &mut self.next_queue_transfer_descriptor_index
    }

    pub fn raw_mut(&mut self) -> &mut RawQueueTransferDescriptor {
        &mut self.raw
    }

    pub fn alternate_next_queue_transfer_descriptor_index_mut(
        &mut self,
    ) -> &mut Option<QueueTransferDescriptorIndex> {
        &mut self.alternate_next_queue_transfer_descriptor_index
    }

    pub fn next_queue_transfer_descriptor_index(&self) -> Option<QueueTransferDescriptorIndex> {
        self.next_queue_transfer_descriptor_index
    }

    pub fn alternate_next_queue_transfer_descriptor_index(
        &self,
    ) -> Option<QueueTransferDescriptorIndex> {
        self.alternate_next_queue_transfer_descriptor_index
    }

    pub fn buffer_pointers(&self) -> [Option<BufferIndex>; 5] {
        self.buffer_pointers
    }

    pub fn pool_index(&self) -> QueueTransferDescriptorIndex {
        self.pool_index
    }
}

impl BlankQueueTransferDescriptor {
    pub const fn blank() -> Self {
        Self([0; _])
    }
}

impl From<*const QueueTransferDescriptor> for QueueTransferDescriptorPointer {
    fn from(value: *const QueueTransferDescriptor) -> Self {
        let mut result = Self::default();
        // FIXME: today addresses are always physical, one day they'll be virtual. This will break
        // that day
        bits::set_bits!(bits_expr: result.bits, value: value as u32 >> 5, n_bits: 27, starts_at_bit: 5, bits_expr_ty: u32);
        result
    }
}

impl From<usize> for BufferIndexNoOffset {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<BufferIndexNoOffset> for usize {
    fn from(value: BufferIndexNoOffset) -> Self {
        value.0
    }
}

impl From<usize> for QueueTransferDescriptorIndex {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<QueueTransferDescriptorIndex> for usize {
    fn from(value: QueueTransferDescriptorIndex) -> usize {
        value.0
    }
}

impl core::fmt::Display for QueueTransferDescriptorPointer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Address: {:#x}, Terminate: {}",
            self.bits & !0x1,
            self.is_set(QueueTransferDescriptorPointerBit::Terminate)
        )
    }
}

impl core::fmt::Display for PacketId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PacketId::Out => write!(f, "OUT"),
            PacketId::In => write!(f, "IN"),
            PacketId::Setup => write!(f, "SETUP"),
            PacketId::Reserved => write!(f, "Reserved"),
        }
    }
}

impl core::fmt::Display for QueueTransferDescriptorTokenBit {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        //EndpointCapabilitiesBit::InactivateOnNextTransaction => write!(f, "Invalidate on Next Transaction"),
        match self {
            QueueTransferDescriptorTokenBit::PingState => write!(f, "PingState"),
            QueueTransferDescriptorTokenBit::SplitTransactionState => {
                write!(f, "SplitTransactionState")
            }
            QueueTransferDescriptorTokenBit::MissedMicroFrame => write!(f, "MissedMicroFrame"),
            QueueTransferDescriptorTokenBit::TransactionError => write!(f, "TransactionError"),
            QueueTransferDescriptorTokenBit::BabbleDetected => write!(f, "BabbleDetected"),
            QueueTransferDescriptorTokenBit::DataBufferError => write!(f, "DataBufferError"),
            QueueTransferDescriptorTokenBit::Halted => write!(f, "Halted"),
            QueueTransferDescriptorTokenBit::Active => write!(f, "Active"),
            QueueTransferDescriptorTokenBit::InterruptOnComplete => {
                write!(f, "InterruptOnComplete")
            }
            QueueTransferDescriptorTokenBit::DataToggle => write!(f, "DataToggle"),
        }
    }
}

impl core::fmt::Display for QueueTransferDescriptorToken {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use QueueTransferDescriptorTokenBit::*;
        let mut printed_once = false;
        for flag in [
            PingState,
            SplitTransactionState,
            MissedMicroFrame,
            TransactionError,
            BabbleDetected,
            DataBufferError,
            Halted,
            Active,
            InterruptOnComplete,
            DataToggle,
        ] {
            if self.is_set(flag) {
                if printed_once {
                    write!(f, "|")?;
                }
                write!(f, "{flag}")?;
                printed_once = true;
            }
        }
        if printed_once {
            writeln!(f)?;
        }
        writeln!(
            f,
            "Total Bytes to Transfer: {}",
            self.get_total_bytes_to_transfer()
        )?;
        writeln!(f, "Current Page: {}", self.get_current_page())?;
        writeln!(f, "Error Count: {}", self.get_error_count())?;
        write!(f, "Packet ID: {}", self.get_packet_id())
    }
}

impl core::fmt::Display for BufferPointer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Address: {:#x}, Current Offset: {}",
            self.get_address(),
            self.get_current_offset()
        )
    }
}

impl core::fmt::Display for RawQueueTransferDescriptor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "Next Pointer: {}", self.next_pointer)?;
        writeln!(f, "Alternate Next Pointer: {}", self.alternate_next_pointer)?;
        writeln!(f, "Token: {}", self.token)?;
        for (i, buffer_pointer) in self.buffer_pointers.iter().enumerate() {
            writeln!(f, "Buffer Pointer {i}: {buffer_pointer}")?;
        }
        Ok(())
    }
}

impl core::fmt::Display for QueueTransferDescriptor {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "Raw: {}", self.raw)?;
        writeln!(
            f,
            "qTD next pointer: {:?}",
            self.next_queue_transfer_descriptor_index
        )?;
        writeln!(
            f,
            "qTD alternate next pointer: {:?}",
            self.alternate_next_queue_transfer_descriptor_index
        )?;

        for (i, buffer_pointer) in self.buffer_pointers.iter().enumerate() {
            writeln!(f, "Buffer Pointer {i}: {buffer_pointer:?}")?;
        }
        Ok(())
    }
}

impl Deref for BufferPage {
    type Target = [u8; 4096];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for BufferPage {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Deref for QueueTransferDescriptor {
    type Target = RawQueueTransferDescriptor;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl DerefMut for QueueTransferDescriptor {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.raw
    }
}

pub(super) const N_BLANK_BUFFER_PAGES: usize = 10;

pub(super) static mut BLANK_BUFFER_PAGES: [BufferPage; N_BLANK_BUFFER_PAGES] =
    [const { BufferPage([0; _]) }; _];

pub(super) const N_BLANK_QUEUE_TRANSFER_DESCRIPTORS: usize = 10;

pub(super) static mut BLANK_QUEUE_TRANSFER_DESCRIPTORS: [BlankQueueTransferDescriptor;
    N_BLANK_QUEUE_TRANSFER_DESCRIPTORS] =
    [BlankQueueTransferDescriptor::blank(); N_BLANK_QUEUE_TRANSFER_DESCRIPTORS];
