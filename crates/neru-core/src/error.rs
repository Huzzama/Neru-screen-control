use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no supported screen found")]
    NoDevice,

    #[error("device {0} not found")]
    DeviceNotFound(String),

    #[error("permission denied opening {0} — install the udev rules (`neru udev`) and replug the device")]
    Permission(PathBuf),

    #[error("usb error: {0}")]
    Usb(#[from] rusb::Error),

    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("timed out waiting for {0}")]
    Timeout(String),

    #[error("invalid value: {0}")]
    InvalidValue(String),

    #[error("image error: {0}")]
    Image(String),

    #[error("config error: {0}")]
    Config(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<image::ImageError> for Error {
    fn from(e: image::ImageError) -> Self {
        Error::Image(e.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Protocol(e.to_string())
    }
}
