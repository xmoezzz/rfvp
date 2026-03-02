use thiserror::Error;

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error("invalid data: {0}")]
    InvalidData(&'static str),

    #[error("bitstream overread")]
    Overread,

    #[error("unsupported: {0}")]
    Unsupported(&'static str),

    #[error("internal: {0}")]
    Internal(&'static str),
}

pub type Result<T> = std::result::Result<T, DecodeError>;
