use thiserror::Error;

#[derive(Error, Debug, Clone, Copy)]
pub enum Facility {
    #[error("EDD: drive parameters")]
    DriveParameters,
    #[error("EDD: device path information")]
    DevicePathInformation,
    #[error("EDD: fixed disk parameter table")]
    FixedDiskParameterTable,
}
