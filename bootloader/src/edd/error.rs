use thiserror::Error;

#[derive(Error, Debug, Clone, Copy)]
pub(crate) enum Facility {
    #[error("drive parameters")]
    DriveParameters,
    #[error("device path information")]
    DevicePathInformation,
    #[error("fixed disk parameter table")]
    FixedDiskParameterTable,
}
