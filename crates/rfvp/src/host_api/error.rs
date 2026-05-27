use core::fmt;

pub type RfvpResult<T> = core::result::Result<T, RfvpError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RfvpError {
    Io,
    NotFound,
    InvalidData,
    InvalidArgument,
    Unsupported,
    OutOfMemory,
    CapacityExceeded,
    EndOfFile,
    Backend,
}

impl RfvpError {
    pub const fn code(self) -> u32 {
        match self {
            Self::Io => 1,
            Self::NotFound => 2,
            Self::InvalidData => 3,
            Self::InvalidArgument => 4,
            Self::Unsupported => 5,
            Self::OutOfMemory => 6,
            Self::CapacityExceeded => 7,
            Self::EndOfFile => 8,
            Self::Backend => 9,
        }
    }
}

impl fmt::Display for RfvpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::Io => "I/O error",
            Self::NotFound => "not found",
            Self::InvalidData => "invalid data",
            Self::InvalidArgument => "invalid argument",
            Self::Unsupported => "unsupported operation",
            Self::OutOfMemory => "out of memory",
            Self::CapacityExceeded => "fixed capacity exceeded",
            Self::EndOfFile => "unexpected end of file",
            Self::Backend => "host backend error",
        };
        f.write_str(msg)
    }
}
