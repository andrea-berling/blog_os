use core::arch::asm;

use crate::{
    error::{Context, Error, Facility, Fault},
    ioport::Port,
    make_bitmap, timer,
};

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
        let flags = DriveHeadRegisterFlags {
            bits: (address >> 24) as u8,
        };
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

    fn data_register(&self) -> Port {
        Port::new(self.io_port_base_address)
    }

    fn error_register(&self) -> Port {
        Port::new(self.io_port_base_address + 1)
    }

    fn features_register(&self) -> Port {
        Port::new(self.io_port_base_address + 1)
    }

    fn sector_count_register(&self) -> Port {
        Port::new(self.io_port_base_address + 2)
    }

    fn sector_number_register(&self) -> Port {
        Port::new(self.io_port_base_address + 3)
    }

    fn lba_low_register(&self) -> Port {
        Port::new(self.io_port_base_address + 3)
    }

    fn cylinder_low_register(&self) -> Port {
        Port::new(self.io_port_base_address + 4)
    }

    fn lba_mid_register(&self) -> Port {
        Port::new(self.io_port_base_address + 4)
    }

    fn cylinder_high_register(&self) -> Port {
        Port::new(self.io_port_base_address + 5)
    }

    fn lba_high_register(&self) -> Port {
        Port::new(self.io_port_base_address + 5)
    }

    fn drive_head_register(&self) -> Port {
        Port::new(self.io_port_base_address + 6)
    }

    fn status_register(&self) -> Port {
        Port::new(self.io_port_base_address + 7)
    }

    fn command_register(&self) -> Port {
        Port::new(self.io_port_base_address + 7)
    }

    fn alternate_status_register(&self) -> Port {
        Port::new(self.control_port_base_address)
    }

    fn device_control_register(&self) -> Port {
        Port::new(self.control_port_base_address)
    }

    fn drive_address_register(&self) -> Port {
        Port::new(self.control_port_base_address + 1)
    }

    fn io_error(&self, fault: Fault) -> Error {
        Error::new(
            fault,
            Context::Io,
            Facility::AtaDevice(self.io_port_base_address),
        )
    }

    fn courtesy_delay() {
        let mut courtesy_delay = timer::LowPrecisionTimer::new(COURTESY_DELAY_NS);
        while !courtesy_delay.timeout() {
            courtesy_delay.update();
        }
    }

    fn get_status(&self) -> StatusRegisterFlags {
        StatusRegisterFlags::from(self.status_register().readb())
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

    fn wait_for_readiness(&self, timeout_ns: u64) -> Result<(), Error> {
        Self::courtesy_delay();
        let mut timeout_timer = timer::LowPrecisionTimer::new(timeout_ns);
        while !self.ready_for_command() && !timeout_timer.timeout() {
            timeout_timer.update();
        }
        if timeout_timer.timeout() && !self.ready_for_command() {
            return Err(self.io_error(Fault::Timeout(timeout_ns)));
        }
        Ok(())
    }

    fn poll_for_reads(&self, timeout_ns: u64) -> Result<(), Error> {
        Self::courtesy_delay();
        let mut timeout_timer = timer::LowPrecisionTimer::new(timeout_ns);
        while !self.has_data_to_send() && !timeout_timer.timeout() {
            timeout_timer.update();
        }
        if timeout_timer.timeout() && !self.has_data_to_send() {
            return Err(self.io_error(Fault::Timeout(timeout_ns)));
        }
        Ok(())
    }

    pub fn read_sectors_lba28_pio(
        &self,
        sector_count: u8,
        lba_address: u32,
        output_buffer: &mut [u8],
    ) -> Result<(), Error> {
        if lba_address as u64 >= self.sectors {
            return Err(self.io_error(Fault::InvalidLBAAddress(lba_address.into(), self.sectors)));
        }

        if (output_buffer.len() as u64) < (sector_count as u64 * self.sector_size_bytes as u64) {
            return Err(self.io_error(Fault::CantReadIntoBuffer(
                output_buffer.len() as u64,
                sector_count as u64 * self.sector_size_bytes as u64,
            )));
        }

        use DriveHeadRegisterFlag::*;
        let mut drive_head_register_flags = DriveHeadRegisterFlags::new().lba(lba_address);
        if self.is_slave {
            drive_head_register_flags.set_flag(IsSlave);
        }

        self.drive_head_register()
            .writeb(drive_head_register_flags.into());
        self.sector_count_register().writeb(sector_count);
        self.lba_low_register().writeb(lba_address as u8);
        self.lba_mid_register().writeb((lba_address >> 8) as u8);
        self.lba_high_register().writeb((lba_address >> 16) as u8);

        self.wait_for_readiness(1_000_000);
        self.command_register().writeb(Command::ReadSectors as u8);

        for i in 0..sector_count {
            self.poll_for_reads(1_000_000)?;

            let start = i as usize * self.sector_size_bytes as usize;
            let end = start + (self.sector_size_bytes as usize);
            let n_words = self.sector_size_bytes as usize / size_of::<u16>();

            self.data_register()
                .rep_insw(&mut output_buffer[start..end], n_words as u16)
                .map_err(|n_words| {
                    self.io_error(Fault::CantReadIntoBuffer(
                        (n_words as usize * size_of::<u16>()) as u64,
                        self.sector_size_bytes as u64,
                    ))
                })?;
        }

        Ok(())
    }

    pub fn sector_size_bytes(&self) -> u16 {
        self.sector_size_bytes
    }
}
