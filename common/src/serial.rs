use core::arch::asm;

use crate::{ioport::Port, make_bitmap};

const COM1: u16 = 0x3F8;

pub struct Com1;

static mut COM1_INITIALIZED: bool = false;

#[allow(unused)]
#[repr(u8)]
pub enum InterruptEnableFlag {
    ReceivedDataAvailable = 0x1,
    TransmitterHoldingRegisterEmpty = 0x2,
    ReceiverLineStatus = 0x4,
    ModemStatus = 0x8,
}

make_bitmap!(new_type: InterruptEnableFlags, underlying_flag_type: InterruptEnableFlag, repr: u8, nodisplay);

#[allow(unused)]
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum LineControlRegisterFlag {
    DataBits1 = 1 << 0,
    DataBits2 = 1 << 1,
    StopBits = 1 << 2,
    ParityBits1 = 1 << 3,
    ParityBits2 = 1 << 4,
    ParityBits3 = 1 << 5,
    BreakEnableBit = 1 << 6,
    DivisorLatchAcccessBit = 1 << 7,
}

make_bitmap!(new_type: LineControlRegisterFlags, underlying_flag_type: LineControlRegisterFlag, repr: u8, nodisplay);

#[allow(unused)]
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum FifoControlRegisterFlag {
    EnableFifo = 1 << 0,
    ClearReceiveFifo = 1 << 1,
    ClearTransmitFifo = 1 << 2,
    DMAModeSelect = 1 << 3,
    InterruptTriggerLevel1 = 1 << 6,
    InterruptTriggerLevel2 = 1 << 7,
}

#[allow(unused)]
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum ModemControlRegisterFlag {
    DataTerminalReady = 1 << 0,
    RequestToSend = 1 << 1,
    Out1 = 1 << 2,
    Out2 = 1 << 3,
    Loopback = 1 << 4,
}

make_bitmap!(new_type: ModemControlRegisterFlags, underlying_flag_type: ModemControlRegisterFlag, repr: u8, nodisplay);

#[allow(unused)]
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum LineStatusRegisterFlag {
    DataReady = 1 << 0,
    OverrunError = 1 << 1,
    ParityError = 1 << 2,
    FramingError = 1 << 3,
    BreakIndicator = 1 << 4,
    TransmitterHoldingRegisterEmpty = 1 << 5,
    TransmitterEmpty = 1 << 6,
    ImpendingError = 1 << 7,
}

make_bitmap!(new_type: LineStatusRegisterFlags, underlying_flag_type: LineStatusRegisterFlag, repr: u8, nodisplay);

impl Com1 {
    /// # Panics
    /// Uses Self::initialize under the hood, which may panic under certain conditions
    pub fn get() -> Self {
        if !Self::initialized() {
            Self::initialize()
        }
        Self {}
    }

    pub fn initialized() -> bool {
        // SAFETY: no threads, no data races
        unsafe { COM1_INITIALIZED }
    }

    fn interrupt_enable_register() -> Port {
        Port::new(COM1 + 1)
    }

    fn line_control_register() -> Port {
        Port::new(COM1 + 3)
    }

    fn divisor_register_low() -> Port {
        Port::new(COM1)
    }
    fn divisor_register_high() -> Port {
        Port::new(COM1 + 1)
    }

    fn modem_control_register() -> Port {
        Port::new(COM1 + 4)
    }

    fn line_status_register() -> Port {
        Port::new(COM1 + 5)
    }
    fn receive_register() -> Port {
        Port::new(COM1)
    }

    fn transmit_register() -> Port {
        Port::new(COM1)
    }

    /// # Panics
    /// Panics if COM1 doesn't exist or doesn't echo back its written char during loopback test
    /// TODO: Should we make it fallibe with Result instead?
    pub fn initialize() {
        // https://wiki.osdev.org/Serial_Ports#Initialization

        use LineControlRegisterFlag::*;
        use ModemControlRegisterFlag::*;

        Self::interrupt_enable_register().writeb(InterruptEnableFlags::empty().into());
        Self::line_control_register().writeb(DivisorLatchAcccessBit as u8);
        Self::divisor_register_low().writeb(3);
        Self::divisor_register_high().writeb(0);
        // 8 bits, one stop bit, no parity
        Self::line_control_register().writeb((DataBits1 | DataBits2).into());
        Self::modem_control_register().writeb((Loopback | Out1 | Out2 | RequestToSend).into());
        let test_byte = 0xae;
        Self::transmit_register().writeb(test_byte);
        if Self::receive_register().readb() != test_byte {
            panic!("COM1 initialization");
        }
        Self::modem_control_register().writeb(ModemControlRegisterFlags::empty().into());

        // SAFETY: no multitasking, no problem
        unsafe { COM1_INITIALIZED = true }
    }

    fn is_transmit_empty() -> bool {
        use LineStatusRegisterFlag::*;
        (LineStatusRegisterFlags {
            bits: Self::line_status_register().readb(),
        })
        .is_set(TransmitterHoldingRegisterEmpty)
    }

    fn send_byte(byte: u8) {
        loop {
            if Self::is_transmit_empty() {
                break;
            }
        }
        Self::transmit_register().writeb(byte);
    }
}

impl core::fmt::Write for Com1 {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.bytes() {
            Self::send_byte(byte);
        }
        Ok(())
    }
}

pub fn __writeln_no_sync(args: core::fmt::Arguments) -> core::fmt::Result {
    use core::fmt::Write;
    let mut serial_writer = Com1::get();
    serial_writer.write_fmt(args)?;
    writeln!(serial_writer)
}

#[macro_export]
macro_rules! serial_writeln_no_sync {
    ($format_string:literal$(, $args:expr)*) => {
        $crate::serial::__writeln_no_sync(::core::format_args!($format_string $(,$args)*,)).expect("couldn't write to COM1")
    };
}

pub use serial_writeln_no_sync as writeln_no_sync;
