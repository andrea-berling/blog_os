use thiserror::Error;

#[derive(Error, Debug, Clone, Copy)]
pub enum Facility {
    #[error("Bootloader")]
    Bootloader,
}

pub type Error = common::error::Error<Facility>;
pub type Result<T> = common::error::Result<T, Facility>;
