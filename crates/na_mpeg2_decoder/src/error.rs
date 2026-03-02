use thiserror::Error;

#[derive(Debug, Error)]
pub enum AvError {
    #[error("video decode error: {0}")]
    Video(#[from] crate::video::DecodeError),

    #[error("audio decode error: {0}")]
    Audio(String),

    #[error("symphonia error: {0}")]
    Symphonia(#[from] symphonia::core::errors::Error),
}

pub type Result<T> = std::result::Result<T, AvError>;
