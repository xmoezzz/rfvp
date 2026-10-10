use rfvp::host_api::RfvpError;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WiiUStatus {
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

impl WiiUStatus {
    pub const fn as_i32(self) -> i32 {
        self as i32
    }
}

pub const fn wiiu_status_to_rfvp_error(status: i32) -> RfvpError {
    match status {
        -1 => RfvpError::Io,
        -2 => RfvpError::NotFound,
        -3 => RfvpError::InvalidData,
        -4 => RfvpError::InvalidArgument,
        -5 => RfvpError::Unsupported,
        -6 => RfvpError::OutOfMemory,
        -7 => RfvpError::CapacityExceeded,
        -8 => RfvpError::EndOfFile,
        _ => RfvpError::Backend,
    }
}

pub const fn rfvp_error_to_status(err: RfvpError) -> i32 {
    match err {
        RfvpError::Io => WiiUStatus::Io.as_i32(),
        RfvpError::NotFound => WiiUStatus::NotFound.as_i32(),
        RfvpError::InvalidData => WiiUStatus::InvalidData.as_i32(),
        RfvpError::InvalidArgument => WiiUStatus::InvalidArgument.as_i32(),
        RfvpError::Unsupported => WiiUStatus::Unsupported.as_i32(),
        RfvpError::OutOfMemory => WiiUStatus::OutOfMemory.as_i32(),
        RfvpError::CapacityExceeded => WiiUStatus::CapacityExceeded.as_i32(),
        RfvpError::EndOfFile => WiiUStatus::EndOfFile.as_i32(),
        RfvpError::Backend => WiiUStatus::Backend.as_i32(),
    }
}
