use rfvp::host_api::RfvpError;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsvStatus {
    Ok = 0,
    Io = -1,
    NotFound = -2,
    InvalidData = -3,
    InvalidArgument = -4,
    Unsupported = -5,
    OutOfMemory = -6,
    CapacityExceeded = -7,
    EndOfFile = -8,
    Backend = -9,
}

impl PsvStatus {
    pub const fn from_i32(value: i32) -> Self {
        match value {
            0 => Self::Ok,
            -1 => Self::Io,
            -2 => Self::NotFound,
            -3 => Self::InvalidData,
            -4 => Self::InvalidArgument,
            -5 => Self::Unsupported,
            -6 => Self::OutOfMemory,
            -7 => Self::CapacityExceeded,
            -8 => Self::EndOfFile,
            -9 => Self::Backend,
            _ => Self::Backend,
        }
    }

    pub const fn as_i32(self) -> i32 {
        self as i32
    }
}

pub const fn psv_status_to_rfvp_error(status: PsvStatus) -> RfvpError {
    match status {
        PsvStatus::Ok => RfvpError::Backend,
        PsvStatus::Io => RfvpError::Io,
        PsvStatus::NotFound => RfvpError::NotFound,
        PsvStatus::InvalidData => RfvpError::InvalidData,
        PsvStatus::InvalidArgument => RfvpError::InvalidArgument,
        PsvStatus::Unsupported => RfvpError::Unsupported,
        PsvStatus::OutOfMemory => RfvpError::OutOfMemory,
        PsvStatus::CapacityExceeded => RfvpError::CapacityExceeded,
        PsvStatus::EndOfFile => RfvpError::EndOfFile,
        PsvStatus::Backend => RfvpError::Backend,
    }
}

pub fn status_to_result(status: i32) -> rfvp::host_api::RfvpResult<()> {
    let status = PsvStatus::from_i32(status);
    if status == PsvStatus::Ok {
        Ok(())
    } else {
        Err(psv_status_to_rfvp_error(status))
    }
}
