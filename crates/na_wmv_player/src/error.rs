use thiserror::Error;

#[derive(Debug, Error)]
pub enum DecoderError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Unsupported feature: {0}")]
    Unsupported(String),

    #[error("End of stream")]
    EndOfStream,
}

pub type Result<T> = std::result::Result<T, DecoderError>;
