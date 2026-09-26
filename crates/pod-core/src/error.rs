#[derive(Debug, thiserror::Error)]
pub enum PodError {
    #[error("no POD X3 device found")]
    DeviceNotFound,

    #[error("USB error: {0}")]
    Usb(#[from] nusb::Error),

    #[error("USB transfer error: {0}")]
    Transfer(String),

    #[error("device did not respond in time (possible lockup)")]
    Timeout,

    #[error("malformed response from device: {0}")]
    Protocol(String),
}

pub type Result<T> = std::result::Result<T, PodError>;
