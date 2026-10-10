use rfvp::host_api::RfvpError;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PS3Status {
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

impl PS3Status {
    pub const fn as_i32(self) -> i32 {
        self as i32
    }
}

pub const fn ps3_status_to_rfvp_error(status: i32) -> RfvpError {
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
        RfvpError::Io => PS3Status::Io.as_i32(),
        RfvpError::NotFound => PS3Status::NotFound.as_i32(),
        RfvpError::InvalidData => PS3Status::InvalidData.as_i32(),
        RfvpError::InvalidArgument => PS3Status::InvalidArgument.as_i32(),
        RfvpError::Unsupported => PS3Status::Unsupported.as_i32(),
        RfvpError::OutOfMemory => PS3Status::OutOfMemory.as_i32(),
        RfvpError::CapacityExceeded => PS3Status::CapacityExceeded.as_i32(),
        RfvpError::EndOfFile => PS3Status::EndOfFile.as_i32(),
        RfvpError::Backend => PS3Status::Backend.as_i32(),
    }
}
