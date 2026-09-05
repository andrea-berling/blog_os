use core::{
    fmt::Display,
    ops::{Deref, DerefMut},
};

use num_enum::TryFromPrimitive;

use crate::{
    bits, make_bitmap,
    mmio::VolatileValue,
    usb::{
        ehci::transfer_descriptor::{
            QueueTransferDescriptorIndex, QueueTransferDescriptorPointer,
            QueueTransferDescriptorToken,
        },
        setup::{Address, MaxPacketLength},
    },
};

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum QueueHeadPointer {
    IsochronousTransferDescriptor,
    QueueHead(*const RawQueueHead),
    SplitTransactionIsochronousTransferDescriptor,
    FrameSpanTraversalNode,
}

#[derive(Clone, Copy)]
#[repr(u32)]
pub enum QueueHeadPointerBit {
    Terminate = 1,
}

make_bitmap!(new_type: RawQueueHeadPointer, underlying_flag_type: QueueHeadPointerBit, repr: u32, nodisplay);

#[derive(Clone, Copy, TryFromPrimitive)]
#[repr(u8)]
pub enum EndpointSpeed {
    FullSpeed,
    LowSpeed,
    HighSpeed,
    Reserved,
}

#[derive(Clone, Copy, TryFromPrimitive)]
#[repr(u8)]
pub enum HighBandwidthPipeMultiplier {
    Reserved,
    OneTransactionPerMicroFrame,
    TwoTransactionPerMicroFrame,
    ThreeTransactionPerMicroFrame,
}

#[derive(Clone, Copy, TryFromPrimitive)]
#[repr(u64)]
pub enum EndpointCharacteristicsBit {
    InactivateOnNextTransaction = 1 << 7,
    DataToggleControl = 1 << 14,
    HeadOfReclamationListFlag = 1 << 15,
    ControlEndpoint = 1 << 27,
}

make_bitmap!(new_type: EndpointCharacteristics, underlying_flag_type: EndpointCharacteristicsBit, repr: u32, bit_skipper: |i| i != 7 && i != 14 && i != 15 && i != 27);

pub struct EndpointCapabilities {
    bits: u32,
}

#[repr(C)]
pub struct RawQueueHead {
    queue_head_link_pointer: VolatileValue<RawQueueHeadPointer>,
    endpoint_characteristics: VolatileValue<EndpointCharacteristics>,
    endpoint_capabilities: VolatileValue<EndpointCapabilities>,
    current_qtd_pointer: VolatileValue<QueueTransferDescriptorPointer>,
    next_qtd_pointer: VolatileValue<QueueTransferDescriptorPointer>,
    alternate_next_qtd_pointer: VolatileValue<QueueTransferDescriptorPointer>,
    execution_cache_area: VolatileValue<QueueTransferDescriptorToken>,
    word7: VolatileValue<u32>,
    word8: VolatileValue<u32>,
    word9: VolatileValue<u32>,
    word10: VolatileValue<u32>,
    word11: VolatileValue<u32>,
}

#[derive(Clone, Copy, Debug)]
pub struct QueueHeadIndex(usize);

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum LogicalQueueHeadPointer {
    IsochronousTransferDescriptor,
    QueueHead(QueueHeadIndex),
    SplitTransactionIsochronousTransferDescriptor,
    FrameSpanTraversalNode,
}

#[repr(C, align(32))]
pub struct QueueHead {
    raw: RawQueueHead,
    pool_index: QueueHeadIndex,
    queue_head_horizontal_link_pointer: Option<LogicalQueueHeadPointer>,
    next_queue_transfer_descriptor_index: Option<QueueTransferDescriptorIndex>,
    alternate_next_queue_transfer_descriptor_index: Option<QueueTransferDescriptorIndex>,
}

#[derive(Clone, Copy)]
#[repr(C, align(32))]
pub struct BlankQueueHead([u32; size_of::<QueueHead>() / (u32::BITS / u8::BITS) as usize]);

impl EndpointCharacteristics {
    pub fn get_nak_count_reload(&self) -> u8 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 4, starts_at_bit: 28, return_ty: u8)
    }

    pub fn set_max_packet_length(&mut self, max_packet_length: MaxPacketLength) {
        bits::set_bits!(bits_expr: self.bits, value: u16::from(max_packet_length), n_bits: 11, starts_at_bit: 16, bits_expr_ty: u32);
    }

    pub fn get_max_packet_length(&self) -> u16 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 11, starts_at_bit: 16, return_ty: u16)
    }

    /// # Panics
    /// Never: the 2-bit field can only hold values matching an `EndpointSpeed` variant,
    /// so the conversion below can not fail, but Rust's type checker doesn't know that
    pub fn get_endpoint_speed(&self) -> EndpointSpeed {
        bits::get_bits!(bits_expr: self.bits, n_bits: 2, starts_at_bit: 12, return_ty: u8)
            .try_into()
            .expect("the 2-bit field can only hold valid EndpointSpeed values")
    }

    pub fn set_endpoint_speed(&mut self, endpoint_speed: EndpointSpeed) {
        bits::set_bits!(bits_expr: self.bits, value: endpoint_speed, n_bits: 2, starts_at_bit: 12, bits_expr_ty: u32);
    }

    pub fn get_endpoint_number(&self) -> u8 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 4, starts_at_bit: 8, return_ty: u8)
    }

    pub fn set_endpoint_number(&mut self, endpoint_number: u8) {
        bits::set_bits!(bits_expr: self.bits, value: endpoint_number, n_bits: 4, starts_at_bit: 8, bits_expr_ty: u32);
    }

    pub fn get_device_address(&self) -> u8 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 7, starts_at_bit: 0, return_ty: u8)
    }

    pub fn set_device_address(&mut self, device_address: Address) {
        bits::set_bits!(bits_expr: self.bits, value: u8::from(device_address), n_bits: 7, starts_at_bit: 0, bits_expr_ty: u32);
    }
}

impl EndpointCapabilities {
    /// # Panics
    /// Never: the 2-bit field can only hold values matching a
    /// `HighBandwidthPipeMultiplier` variant, so the conversion below can not fail, but
    /// Rust's type checker doesn't know that
    pub fn get_high_bandwidth_pipe_multiplier(&self) -> HighBandwidthPipeMultiplier {
        bits::get_bits!(bits_expr: self.bits, n_bits: 2, starts_at_bit: 30, return_ty: u8)
            .try_into()
            .expect("the 2-bit field can only hold valid HighBandwidthPipeMultiplier values")
    }

    pub fn set_high_bandwidth_pipe_multiplier(
        &mut self,
        high_bandwidth_pipe_multiplier: HighBandwidthPipeMultiplier,
    ) {
        bits::set_bits!(bits_expr: self.bits, value: high_bandwidth_pipe_multiplier, n_bits: 2, starts_at_bit: 30, bits_expr_ty: u32);
    }

    pub fn get_port_number(&self) -> u8 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 7, starts_at_bit: 23, return_ty: u8)
    }

    pub fn set_port_number(&mut self, port_number: u8) {
        bits::set_bits!(bits_expr: self.bits, value: port_number, n_bits: 7, starts_at_bit: 23, bits_expr_ty: u32);
    }

    pub fn get_hub_address(&self) -> u8 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 7, starts_at_bit: 16, return_ty: u8)
    }

    pub fn set_hub_address(&mut self, hub_address: u8) {
        bits::set_bits!(bits_expr: self.bits, value: hub_address, n_bits: 7, starts_at_bit: 16, bits_expr_ty: u32);
    }

    pub fn get_split_completion_mask(&self) -> u8 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 8, starts_at_bit: 8, return_ty: u8)
    }

    pub fn set_split_completion_mask(&mut self, split_completion_mask: u8) {
        bits::set_bits!(bits_expr: self.bits, value: split_completion_mask, n_bits: 8, starts_at_bit: 8, bits_expr_ty: u32);
    }

    pub fn get_interrupt_schedule_mask(&self) -> u8 {
        bits::get_bits!(bits_expr: self.bits, n_bits: 8, starts_at_bit: 0, return_ty: u8)
    }

    pub fn set_interrupt_schedule_mask(&mut self, interrupt_schedule_mask: u8) {
        bits::set_bits!(bits_expr: self.bits, value: interrupt_schedule_mask, n_bits: 8, starts_at_bit: 0, bits_expr_ty: u32);
    }

    pub fn empty() -> EndpointCapabilities {
        Self { bits: 0 }
    }
}

impl RawQueueHead {
    pub fn get_current_offset(&self) -> u16 {
        self.word7.read_with(
            |word7| bits::get_bits!(bits_expr: word7, n_bits: 12, starts_at_bit: 0, return_ty: u16),
        )
    }

    pub fn endpoint_characteristics_mut(&mut self) -> &mut VolatileValue<EndpointCharacteristics> {
        &mut self.endpoint_characteristics
    }

    pub fn endpoint_capabilities_mut(&mut self) -> &mut VolatileValue<EndpointCapabilities> {
        &mut self.endpoint_capabilities
    }

    pub fn next_qtd_pointer_mut(&mut self) -> &mut VolatileValue<QueueTransferDescriptorPointer> {
        &mut self.next_qtd_pointer
    }

    pub fn alternate_next_qtd_pointer_mut(
        &mut self,
    ) -> &mut VolatileValue<QueueTransferDescriptorPointer> {
        &mut self.alternate_next_qtd_pointer
    }

    pub fn queue_head_link_pointer_mut(&mut self) -> &mut VolatileValue<RawQueueHeadPointer> {
        &mut self.queue_head_link_pointer
    }

    pub fn set_alternate_next_qtd_pointer(
        &mut self,
        alternate_next_qtd_pointer: VolatileValue<QueueTransferDescriptorPointer>,
    ) {
        self.alternate_next_qtd_pointer = alternate_next_qtd_pointer;
    }

    pub fn clear_overlay_area(&mut self) {
        self.current_qtd_pointer
            .set(QueueTransferDescriptorPointer::empty());
        self.next_qtd_pointer
            .set(QueueTransferDescriptorPointer::empty());
        self.alternate_next_qtd_pointer
            .set(QueueTransferDescriptorPointer::empty());
        self.execution_cache_area
            .set(QueueTransferDescriptorToken::empty());
        self.word7.set(0);
        self.word8.set(0);
        self.word9.set(0);
        self.word10.set(0);
        self.word11.set(0);
    }

    pub fn execution_cache_area(&self) -> &VolatileValue<QueueTransferDescriptorToken> {
        &self.execution_cache_area
    }

    fn clear(&mut self) {
        self.queue_head_link_pointer
            .set(RawQueueHeadPointer::empty());
        self.endpoint_characteristics
            .set(EndpointCharacteristics::empty());
        self.endpoint_capabilities
            .set(EndpointCapabilities::empty());
        self.clear_overlay_area();
    }

    pub fn current_qtd_pointer(&self) -> &VolatileValue<QueueTransferDescriptorPointer> {
        &self.current_qtd_pointer
    }
}

impl QueueHead {
    pub fn init(&mut self, self_index: QueueHeadIndex) {
        self.raw.clear();
        self.pool_index = self_index;
        self.queue_head_horizontal_link_pointer = None;
        self.alternate_next_queue_transfer_descriptor_index = None;
        self.next_queue_transfer_descriptor_index = None
    }

    pub fn set_self_index(&mut self, self_index: QueueHeadIndex) {
        self.pool_index = self_index;
    }

    pub fn set_next_queue_transfer_descriptor_index(
        &mut self,
        next_queue_transfer_descriptor_index: Option<QueueTransferDescriptorIndex>,
    ) {
        self.next_queue_transfer_descriptor_index = next_queue_transfer_descriptor_index
    }

    pub fn set_alternate_next_queue_transfer_descriptor_index(
        &mut self,
        alternate_next_queue_transfer_descriptor_index: Option<QueueTransferDescriptorIndex>,
    ) {
        self.alternate_next_queue_transfer_descriptor_index =
            alternate_next_queue_transfer_descriptor_index;
    }

    pub fn queue_head_horizontal_link_pointer_mut(
        &mut self,
    ) -> &mut Option<LogicalQueueHeadPointer> {
        &mut self.queue_head_horizontal_link_pointer
    }

    pub fn raw_mut(&mut self) -> &mut RawQueueHead {
        &mut self.raw
    }

    pub fn next_queue_transfer_descriptor_index(&self) -> Option<QueueTransferDescriptorIndex> {
        self.next_queue_transfer_descriptor_index
    }

    pub fn queue_head_horizontal_link_pointer(&self) -> Option<LogicalQueueHeadPointer> {
        self.queue_head_horizontal_link_pointer
    }

    pub fn alternate_next_queue_transfer_descriptor_index(
        &self,
    ) -> Option<QueueTransferDescriptorIndex> {
        self.alternate_next_queue_transfer_descriptor_index
    }

    pub fn alternate_next_queue_transfer_descriptor_index_mut(
        &mut self,
    ) -> &mut Option<QueueTransferDescriptorIndex> {
        &mut self.alternate_next_queue_transfer_descriptor_index
    }

    pub fn pool_index(&self) -> QueueHeadIndex {
        self.pool_index
    }

    pub fn raw(&self) -> &RawQueueHead {
        &self.raw
    }
}

impl BlankQueueHead {
    pub const fn blank() -> Self {
        Self([0; _])
    }
}

impl From<QueueHeadPointer> for RawQueueHeadPointer {
    fn from(value: QueueHeadPointer) -> Self {
        match value {
            QueueHeadPointer::IsochronousTransferDescriptor => todo!(),
            QueueHeadPointer::QueueHead(queue_head) => {
                let mut result = Self::default();
                bits::set_bits!(bits_expr: result.bits, value: 0b01, n_bits: 2, starts_at_bit: 1, bits_expr_ty: u32);
                // FIXME: today addresses are always physical, one day they'll be virtual. This will break
                // that day
                bits::set_bits!(bits_expr: result.bits, value: queue_head as u32 >> 5, n_bits: 27, starts_at_bit: 5, bits_expr_ty: u32);
                result
            }
            QueueHeadPointer::SplitTransactionIsochronousTransferDescriptor => todo!(),
            QueueHeadPointer::FrameSpanTraversalNode => todo!(),
        }
    }
}

impl From<QueueHeadIndex> for usize {
    fn from(value: QueueHeadIndex) -> Self {
        value.0
    }
}

impl From<usize> for QueueHeadIndex {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl core::fmt::Display for RawQueueHeadPointer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "Val: {:#x}, Address: {:#x}, Type: {:#b}, Terminate: {}",
            self.bits,
            bits::get_bits!(bits_expr: self.bits, n_bits: 27, starts_at_bit: 5, return_ty: u32)
                << 5,
            bits::get_bits!(bits_expr: self.bits, n_bits: 2, starts_at_bit: 1, return_ty: u32),
            self.is_set(QueueHeadPointerBit::Terminate)
        )
    }
}

impl Display for EndpointSpeed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EndpointSpeed::FullSpeed => write!(f, "Full Speed"),
            EndpointSpeed::LowSpeed => write!(f, "Low Speed"),
            EndpointSpeed::HighSpeed => write!(f, "High Speed"),
            EndpointSpeed::Reserved => write!(f, "Reserved"),
        }
    }
}

impl Display for HighBandwidthPipeMultiplier {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            HighBandwidthPipeMultiplier::Reserved => write!(f, "Reserved"),
            HighBandwidthPipeMultiplier::OneTransactionPerMicroFrame => {
                write!(f, "1 transaction per micro-frame")
            }
            HighBandwidthPipeMultiplier::TwoTransactionPerMicroFrame => {
                write!(f, "2 transactions per micro-frame")
            }
            HighBandwidthPipeMultiplier::ThreeTransactionPerMicroFrame => {
                write!(f, "3 transactions per micro-frame")
            }
        }
    }
}

impl core::fmt::Display for EndpointCharacteristicsBit {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EndpointCharacteristicsBit::InactivateOnNextTransaction => {
                write!(f, "Invalidate on Next Transaction")
            }
            EndpointCharacteristicsBit::DataToggleControl => write!(f, "DataToggleControl"),
            EndpointCharacteristicsBit::HeadOfReclamationListFlag => {
                write!(f, "HeadOfReclamationListFlag")
            }
            EndpointCharacteristicsBit::ControlEndpoint => write!(f, "ControlEndpoint"),
        }
    }
}

impl Display for EndpointCapabilities {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(
            f,
            "Interrupt Schedule Mask: {:#04x}",
            self.get_interrupt_schedule_mask()
        )?;
        writeln!(
            f,
            "Split Completion Mask: {:#04x}",
            self.get_split_completion_mask()
        )?;
        writeln!(f, "Hub Address: {}", self.get_hub_address())?;
        writeln!(f, "Port Number: {}", self.get_port_number())?;
        write!(
            f,
            "High Bandwidth Pipe Multiplier: {}",
            self.get_high_bandwidth_pipe_multiplier()
        )
    }
}

impl core::fmt::Display for RawQueueHead {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(
            f,
            "Queue Head Link Pointer: {}",
            self.queue_head_link_pointer
        )?;
        writeln!(
            f,
            "Endpoint Characteristics: {}",
            self.endpoint_characteristics
        )?;
        writeln!(f, "Endpoint Capabilities: {}", self.endpoint_capabilities)?;
        writeln!(f, "Current qTD Pointer: {}", self.current_qtd_pointer)?;
        writeln!(f, "Next qTD Pointer: {}", self.next_qtd_pointer)?;
        writeln!(
            f,
            "Alternate Next qTD Pointer: {}",
            self.alternate_next_qtd_pointer
        )?;
        writeln!(f, "Execution Cache Area: {}", self.execution_cache_area)?;
        writeln!(f, "Word 7: {:#x}", self.word7.read())?;
        writeln!(f, "Word 8: {:#x}", self.word8.read())?;
        writeln!(f, "Word 9: {:#x}", self.word9.read())?;
        writeln!(f, "Word 10: {:#x}", self.word10.read())?;
        writeln!(f, "Word 11: {:#x}", self.word11.read())?;
        Ok(())
    }
}

impl core::fmt::Display for QueueHead {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "Raw: {}", self.raw)?;
        writeln!(
            f,
            "QH horizontal link pointer: {:?}",
            self.queue_head_horizontal_link_pointer
        )?;
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
        Ok(())
    }
}

impl Deref for QueueHead {
    type Target = RawQueueHead;

    fn deref(&self) -> &Self::Target {
        &self.raw
    }
}

impl DerefMut for QueueHead {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.raw
    }
}

pub(super) const N_BLANK_QUEUE_HEADS: usize = 5;

pub(super) static mut BLANK_QUEUE_HEADS: [BlankQueueHead; N_BLANK_QUEUE_HEADS] =
    [BlankQueueHead::blank(); N_BLANK_QUEUE_HEADS];
