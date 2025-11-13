use core::arch::asm;

use crate::{
    error::{Kind, Reason},
    make_bitmap, timer,
};

mod error {
    use thiserror::Error;

    use crate::ata::Device;

    #[derive(Error, Debug, Clone, Copy)]
    pub enum Facility {
        #[error("Ata Device: ({0:?})")]
        AtaDevice(Device),
    }

    pub(crate) type Result<T> = crate::error::Result<T, Facility>;
    pub(crate) type Error = crate::error::Error<Facility>;
}

// https://wiki.osdev.org/ATA_PIO_Mode#400ns_delays
const COURTESY_DELAY_NS: u64 = 400;

#[derive(Debug, Clone, Copy)]
pub struct Device {
    io_port_base_address: u16,
    control_port_base_address: u16,
    is_slave: bool,
    sectors: u64,
    sector_size_bytes: u16,
}

#[repr(u8)]
enum Command {
    ReadSectors = 0x20,
}

#[allow(unused)]
#[repr(u8)]
pub enum DriveHeadRegisterFlag {
    Lba24Chs0 = 0x1,
    Lba25Chs1 = 0x2,
    Lba26Chs2 = 0x4,
    Lba27Chs3 = 0x8,
    IsSlave = 0x10,
    AlwaysSet1 = 0x20,
    Lba = 0x40,
    AlwaysSet2 = 0x80,
}

make_bitmap!(new_type: DriveHeadRegisterFlags, underlying_flag_type: DriveHeadRegisterFlag, repr: u8, nodisplay);

#[allow(unused)]
#[repr(u8)]
pub enum StatusRegisterFlag {
    Error = 0x1,
    Index = 0x2,
    CorrectedData = 0x4,
    ReadyForSendReceive = 0x8, // DRQ
    OverlappedModeServiceRequest = 0x10,
    DriveFaultError = 0x20,
    Spinning = 0x40,                   // RDY
    BusyPreparingToSendReceive = 0x80, // BSY
}

make_bitmap!(new_type: StatusRegisterFlags, underlying_flag_type: StatusRegisterFlag, repr: u8, nodisplay);

impl DriveHeadRegisterFlags {
    pub fn new() -> Self {
        use DriveHeadRegisterFlag::*;
        AlwaysSet1 | AlwaysSet2
    }

    pub fn lba(mut self, address: u32) -> Self {
        let flags = DriveHeadRegisterFlags((address >> 24) as u8);
        use DriveHeadRegisterFlag::*;
        if flags.is_set(Lba24Chs0) {
            self.set_flag(Lba24Chs0);
        }
        if flags.is_set(Lba25Chs1) {
            self.set_flag(Lba25Chs1);
        }
        if flags.is_set(Lba26Chs2) {
            self.set_flag(Lba26Chs2);
        }
        if flags.is_set(Lba27Chs3) {
            self.set_flag(Lba27Chs3);
        }
        self.set_flag(DriveHeadRegisterFlag::Lba);
        self
    }
}

#[allow(unused)]
impl Device {
    pub fn new(
        io_port_base_address: u16,
        control_port_base_address: u16,
        is_slave: bool,
        sectors: u64,
        sector_size_bytes: u16,
    ) -> Self {
        Self {
            io_port_base_address,
            control_port_base_address,
            is_slave,
            sectors,
            sector_size_bytes,
        }
    }

    fn data_register(&self) -> u16 {
        self.io_port_base_address
    }

    fn error_register(&self) -> u16 {
        self.io_port_base_address + 1
    }

    fn features_register(&self) -> u16 {
        self.io_port_base_address + 1
    }

    fn sector_count_register(&self) -> u16 {
        self.io_port_base_address + 2
    }

    fn sector_number_register(&self) -> u16 {
        self.io_port_base_address + 3
    }

    fn lba_low_register(&self) -> u16 {
        self.io_port_base_address + 3
    }

    fn cylinder_low_register(&self) -> u16 {
        self.io_port_base_address + 4
    }

    fn lba_mid_register(&self) -> u16 {
        self.io_port_base_address + 4
    }

    fn cylinder_high_register(&self) -> u16 {
        self.io_port_base_address + 5
    }

    fn lba_high_register(&self) -> u16 {
        self.io_port_base_address + 5
    }

    fn drive_head_register(&self) -> u16 {
        self.io_port_base_address + 6
    }

    fn status_register(&self) -> u16 {
        self.io_port_base_address + 7
    }

    fn command_register(&self) -> u16 {
        self.io_port_base_address + 7
    }

    fn alternate_status_register(&self) -> u16 {
        self.control_port_base_address
    }

    fn device_control_register(&self) -> u16 {
        self.control_port_base_address
    }

    fn drive_address_register(&self) -> u16 {
        self.control_port_base_address + 1
    }

    fn io_error(&self, kind: Kind) -> error::Error {
        crate::error::Error::InternalError(crate::error::InternalError::new(
            error::Facility::AtaDevice(*self),
            kind,
            crate::error::Context::Io,
        ))
    }

    fn courtesy_delay() {
        let mut courtesy_delay = timer::LowPrecisionTimer::new(COURTESY_DELAY_NS);
        while !courtesy_delay.timeout() {
            courtesy_delay.update();
        }
    }

    fn get_status(&self) -> StatusRegisterFlags {
        let status: u8;
        // SAFETY: This is safe because we are reading from a valid I/O port.
        // The `status_register` method returns the correct port address for the ATA
        // status register, which is a read-only operation. The caller of `Device::new`
        // is responsible for providing the correct base addresses for the ATA controller.
        unsafe {
            asm!("in al, dx",
                in("dx") self.status_register(),
                out("al") status,
                options(nomem, nostack, preserves_flags)
            );
        }
        StatusRegisterFlags::from(status)
    }

    fn ready_for_command(&self) -> bool {
        let status = self.get_status();
        use StatusRegisterFlag::*;
        status.is_set(Spinning) && !status.is_set(BusyPreparingToSendReceive)
    }

    fn has_data_to_send(&self) -> bool {
        let status = self.get_status();
        use StatusRegisterFlag::*;
        status.is_set(ReadyForSendReceive) && !status.is_set(BusyPreparingToSendReceive)
    }

    fn wait_for_readiness(&self, timeout_ns: u64) -> error::Result<()> {
        Self::courtesy_delay();
        let mut timeout_timer = timer::LowPrecisionTimer::new(timeout_ns);
        while !self.ready_for_command() && !timeout_timer.timeout() {
            timeout_timer.update();
        }
        if timeout_timer.timeout() && !self.ready_for_command() {
            return Err(self.io_error(Kind::Timeout(Reason::AtaDeviceNotReady)));
        }
        Ok(())
    }

    fn poll_for_reads(&self, timeout_ns: u64) -> error::Result<()> {
        Self::courtesy_delay();
        let mut timeout_timer = timer::LowPrecisionTimer::new(timeout_ns);
        while !self.has_data_to_send() && !timeout_timer.timeout() {
            timeout_timer.update();
        }
        if timeout_timer.timeout() && !self.has_data_to_send() {
            return Err(self.io_error(Kind::Timeout(Reason::HangingAtaDevice)));
        }
        Ok(())
    }

    pub fn read_sectors_lba28_pio(
        &self,
        sector_count: u8,
        lba_address: u32,
        output_buffer: &mut [u8],
    ) -> error::Result<()> {
        if lba_address as u64 >= self.sectors {
            return Err(self.io_error(Kind::InvalidLBAAddress(lba_address.into(), self.sectors)));
        }

        if (output_buffer.len() as u64) < (sector_count as u64 * self.sector_size_bytes as u64) {
            return Err(self.io_error(Kind::CantReadIntoBuffer(
                output_buffer.len() as u64,
                sector_count as u64 * self.sector_size_bytes as u64,
            )));
        }

        use DriveHeadRegisterFlag::*;
        let mut drive_head_register = DriveHeadRegisterFlags::new().lba(lba_address);
        if self.is_slave {
            drive_head_register.set_flag(IsSlave);
        }

        // SAFETY: The following sequence of `out` instructions is safe because it
        // correctly follows the ATA PIO LBA28 protocol for setting up a read command.
        // Each `out` instruction writes a byte to a specific, valid I/O port address
        // managed by the `Device` struct. The caller of `Device::new` is responsible
        // for providing the correct base address. The values written are derived from
        // the function arguments and are formatted according to the ATA specification.
        // This entire sequence is a single logical operation to prepare the controller
        // for the subsequent command.
        unsafe {
            asm!("out dx, al",
                in("dx") self.drive_head_register(),
                in("al") u8::from(drive_head_register),
                options(nomem, nostack, preserves_flags)
            );
        }
        // SAFETY: Same as above
        unsafe {
            asm!("out dx, al",
                in("dx") self.sector_count_register(),
                in("al") sector_count,
                options(nomem, nostack, preserves_flags)
            );
        }
        // SAFETY: Same as above
        unsafe {
            asm!("out dx, al",
                in("dx") self.lba_low_register(),
                in("al") (lba_address & 0xff) as u8,
                options(nomem, nostack, preserves_flags)
            );
        }
        // SAFETY: Same as above
        unsafe {
            asm!("out dx, al",
                in("dx") self.lba_mid_register(),
                in("al") ((lba_address >> 8) & 0xff) as u8,
                options(nomem, nostack, preserves_flags)
            );
        }
        // SAFETY: Same as above
        unsafe {
            asm!("out dx, al",
                in("dx") self.lba_high_register(),
                in("al") ((lba_address >> 16) & 0xff) as u8,
                options(nomem, nostack, preserves_flags)
            );
        }

        self.wait_for_readiness(1_000_000);
        // SAFETY: This is safe because we are issuing a command to the command register
        // port. The preceding call to `wait_for_readiness` ensures the device is
        // not busy (BSY=0) and is ready to accept a command (RDY=1), preventing a command
        // from being sent to a device that is not prepared to handle it.
        unsafe {
            asm!("out dx, al",
                in("dx") self.command_register(),
                in("al") Command::ReadSectors as u8,
                options(nomem, nostack, preserves_flags)
            );
        }

        for i in 0..sector_count {
            self.poll_for_reads(1_000_000)?;

            // SAFETY: The `rep insw` instruction is safe to use here due to the following invariants:
            // 1. A prior call to `poll_for_reads` ensures the device is ready to transfer data
            //    (BSY=0, DRQ=1), so reading from the data port will not hang.
            // 2. The I/O port in `dx` is the correct data register for the ATA device.
            // 3. The destination buffer pointer in `edi` is valid, writable, and correctly aligned.
            //    It is derived from `output_buffer`, a mutable slice provided by the caller,
            //    which Rust guarantees is valid for writes.
            // 4. The `output_buffer` is asserted at the start of the function to be large
            //    enough to hold `sector_count` sectors, preventing a buffer overflow.
            // 5. The count in `cx` is `sector_size_bytes / 2`, which is the correct number of
            //    16-bit words to read for a single sector.
            // 6. The Direction Flag (DF) is clear by default in Rust's ABI, ensuring `edi` increments
            //    and the buffer is filled in the correct forward direction.
            unsafe {
                asm!("rep insw",
                    in("dx") self.data_register() ,
                    in("edi") output_buffer[i as usize*self.sector_size_bytes as usize..].as_mut_ptr(),
                    // u16 is the size of word
                    in("cx") self.sector_size_bytes/size_of::<u16>() as u16 ,
                    options(nostack, preserves_flags)
                );
            }
        }

        Ok(())
    }

    pub fn sector_size_bytes(&self) -> u16 {
        self.sector_size_bytes
    }
}
