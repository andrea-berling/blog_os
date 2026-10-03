use zerocopy::TryFromBytes;

#[derive(Clone, Copy, TryFromBytes)]
pub struct LogicalUnitNumber(u8);

impl From<u8> for LogicalUnitNumber {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<LogicalUnitNumber> for u8 {
    fn from(value: LogicalUnitNumber) -> Self {
        value.0
    }
}

pub mod cdb {
    pub mod command {

        use zerocopy::{Immutable, IntoBytes};

        use crate::{bits, error, scsi::LogicalUnitNumber};

        #[repr(u8)]
        #[derive(IntoBytes, Immutable)]
        enum OperationCode {
            TestUnitReady = 0x00,
            Inquiry = 0x12,
        }

        #[derive(IntoBytes, Immutable)]
        #[repr(C)]
        pub struct Inquiry {
            operation_code: OperationCode,
            logical_unit_number_byte: u8,
            reserved1: [u8; 2],
            allocation_length: u8,
            reserved2: u8,
            padding: [u8; 6],
        }

        #[repr(C)]
        #[derive(IntoBytes, Immutable)]
        pub struct TestUnitReady {
            operation_code: OperationCode,
            logical_unit_number_byte: u8,
            reserved1: [u8; 4],
            padding: [u8; 6],
        }

        pub enum Command {
            Inquiry(Inquiry),
            TestUnitReady(TestUnitReady),
        }

        impl Inquiry {
            pub fn new(logical_unit_number: LogicalUnitNumber) -> Self {
                let mut result = Self {
                    operation_code: OperationCode::Inquiry,
                    logical_unit_number_byte: 0,
                    reserved1: [0; _],
                    allocation_length: 0x24,
                    reserved2: 0,
                    padding: [0; _],
                };

                bits::set_bits!(bits_expr: result.logical_unit_number_byte, value: u8::from(logical_unit_number) & 0x7, n_bits: 3, starts_at_bit: 5, bits_expr_ty: u8);
                result
            }
        }

        impl TestUnitReady {
            pub fn new(logical_unit_number: LogicalUnitNumber) -> Self {
                let mut result = Self {
                    operation_code: OperationCode::TestUnitReady,
                    logical_unit_number_byte: 0,
                    reserved1: [0; _],
                    padding: [0; _],
                };

                bits::set_bits!(bits_expr: result.logical_unit_number_byte, value: u8::from(logical_unit_number) & 0x7, n_bits: 3, starts_at_bit: 5, bits_expr_ty: u8);
                result
            }
        }

        impl TryFrom<Inquiry> for crate::usb::bbb::CommandBlock {
            type Error = error::Error;

            fn try_from(value: Inquiry) -> error::Result<Self> {
                Self::try_from(value.as_bytes())
            }
        }

        impl TryFrom<TestUnitReady> for crate::usb::bbb::CommandBlock {
            type Error = error::Error;

            fn try_from(value: TestUnitReady) -> error::Result<Self> {
                Self::try_from(value.as_bytes())
            }
        }
    }

    pub mod data {
        use num_enum::TryFromPrimitive;
        use zerocopy::TryFromBytes;

        use crate::bits;

        #[derive(TryFromPrimitive)]
        #[repr(u8)]
        pub enum PeripheralDeviceType {
            SbcDirectAccessDevice = 0,
            CdRom = 0x5,
            OpticalMemoryDevice = 0x7,
            RbcDirectAccessDevice = 0xe,
        }

        #[derive(TryFromBytes, Clone)]
        #[repr(C)]
        pub struct Inquiry {
            pdt_word: u8,
            removable_word: u8,
            reserved1: [u8; 2],
            additional_length: u8,
            reserved2: [u8; 3],
            vendor_id: [u8; 8],
            product_id: [u8; 16],
            product_revision_level: [u8; 4],
        }

        impl Inquiry {
            pub fn get_peripheral_device_type(&self) -> Option<PeripheralDeviceType> {
                bits::get_bits!(bits_expr: self.pdt_word, n_bits: 5, starts_at_bit: 0, return_ty: u8).try_into().ok()
            }

            pub fn is_removable(&self) -> bool {
                bits::get_bits!(bits_expr: self.removable_word, n_bits: 1, starts_at_bit: 7, return_ty: u8)
                    != 0
            }
        }

        impl core::fmt::Display for Inquiry {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "Peripheral Device Type: ",)?;
                if let Some(pdt) = self.get_peripheral_device_type() {
                    writeln!(f, "{pdt}")?;
                } else {
                    writeln!(f, "UNKNOWN")?;
                }
                writeln!(f, "Removable: {}", self.is_removable())?;
                // FIXME: this is more general for stack allocated byte strings
                writeln!(
                    f,
                    "Vendor Identification: {:?}",
                    core::str::from_utf8(
                        &self.vendor_id[..self
                            .vendor_id
                            .iter()
                            .position(|&byte| !byte.is_ascii_graphic() && byte != b' ')
                            .unwrap_or(self.vendor_id.len())]
                    )
                )?;
                writeln!(
                    f,
                    "Product Identification: {:?}",
                    core::str::from_utf8(
                        &self.product_id[..self
                            .product_id
                            .iter()
                            .position(|&byte| !byte.is_ascii_graphic() && byte != b' ')
                            .unwrap_or(self.product_id.len())]
                    )
                )?;
                writeln!(
                    f,
                    "Product Revision: {:?}",
                    core::str::from_utf8(
                        &self.product_revision_level[..self
                            .product_revision_level
                            .iter()
                            .position(|&byte| !byte.is_ascii_graphic() && byte != b' ')
                            .unwrap_or(self.product_revision_level.len())]
                    )
                )?;
                Ok(())
            }
        }

        impl core::fmt::Display for PeripheralDeviceType {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                match self {
                    PeripheralDeviceType::SbcDirectAccessDevice => {
                        f.write_str("SBC Direct-access device")
                    }
                    PeripheralDeviceType::CdRom => f.write_str("CD-ROM device"),
                    PeripheralDeviceType::OpticalMemoryDevice => {
                        f.write_str("Optical memory device")
                    }
                    PeripheralDeviceType::RbcDirectAccessDevice => {
                        f.write_str("RBC Direct-access device")
                    }
                }
            }
        }
    }
}
