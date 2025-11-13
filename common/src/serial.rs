use core::arch::asm;

use crate::make_bitmap;

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

    const fn interrupt_enable_register() -> u16 {
        COM1 + 1
    }

    const fn line_control_register() -> u16 {
        COM1 + 3
    }

    const fn divisor_register_low() -> u16 {
        COM1
    }
    const fn divisor_register_high() -> u16 {
        COM1 + 1
    }

    const fn modem_control_register() -> u16 {
        COM1 + 4
    }

    const fn line_status_register() -> u16 {
        COM1 + 5
    }
    const fn receive_register() -> u16 {
        COM1
    }

    const fn transmit_register() -> u16 {
        COM1
    }

    pub fn initialize() {
        // https://wiki.osdev.org/Serial_Ports#Initialization
        use LineControlRegisterFlag::*;
        let enable_dlab: LineControlRegisterFlags = DivisorLatchAcccessBit.into();
        // 8 bits, one stop bit, no parity
        let line_control = DataBits1 | DataBits2;

        use ModemControlRegisterFlag::*;
        let loopback = Loopback | Out1 | Out2 | RequestToSend;
        // SAFETY: need some assembly to do serial I/O
        unsafe {
            asm!(
                "out dx, al",
                "mov al,bh",
                "mov dx,{line_control_register}",
                "out dx, al",
                "mov al,3",
                "mov dx,{divisor_register_low}",
                "out dx, al",
                "mov al,0",
                "mov dx,{divisor_register_high}",
                "out dx, al",
                "mov al,cl",
                "mov dx,{line_control_register}",
                "out dx, al",
                in("al") u8::from(InterruptEnableFlags::empty()),
                in("dx") Self::interrupt_enable_register(),
                in("bl") u8::from(enable_dlab),
                line_control_register = const Self::line_control_register(),
                divisor_register_low = const Self::divisor_register_low(),
                divisor_register_high = const Self::divisor_register_high(),
                in("cl") u8::from(line_control),
            )
        }

        // SAFETY: need some assembly to do serial I/O
        unsafe {
            asm!(
                "out dx, al",
                "mov dx,{transmit_register}",
                "mov al, 0xae",
                "out dx, al",
                in("al") u8::from(loopback),
                in("dx") Self::modem_control_register(),
                transmit_register = const Self::transmit_register()
            )
        }

        let test_byte: u8;
        // SAFETY: need some assembly to do serial I/O
        unsafe { asm!("in al, dx", out("al") test_byte, in("dx") Self::receive_register()) };

        if test_byte != 0xae {
            panic!("COM1 initialization");
        }

        // SAFETY: need some assembly to do serial I/O
        unsafe { asm!("out dx,al", in("dx") Self::modem_control_register(), in("al") 0u8) };

        // SAFETY: no multitasking, no problem
        unsafe { COM1_INITIALIZED = true }
    }

    fn is_transmit_empty() -> bool {
        let line_status: u8;

        // SAFETY: need some assembly to do serial I/O
        unsafe { asm!("in al, dx", out("al") line_status, in("dx") Self::line_status_register()) };

        let line_status = LineStatusRegisterFlags(line_status);
        line_status.is_set(LineStatusRegisterFlag::TransmitterHoldingRegisterEmpty)
    }

    fn send_byte(byte: u8) {
        loop {
            if Self::is_transmit_empty() {
                break;
            }
        }
        // SAFETY: need some assembly to do serial I/O
        unsafe { asm!("out dx, al", in("dx") Self::transmit_register(), in("al") byte) }
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
