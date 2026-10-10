use rfvp::host_api::RfvpError;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ps2Status {
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

impl Ps2Status {
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

pub const fn ps2_status_to_rfvp_error(status: Ps2Status) -> RfvpError {
    match status {
        Ps2Status::Ok => RfvpError::Backend,
        Ps2Status::Io => RfvpError::Io,
        Ps2Status::NotFound => RfvpError::NotFound,
        Ps2Status::InvalidData => RfvpError::InvalidData,
        Ps2Status::InvalidArgument => RfvpError::InvalidArgument,
        Ps2Status::Unsupported => RfvpError::Unsupported,
        Ps2Status::OutOfMemory => RfvpError::OutOfMemory,
        Ps2Status::CapacityExceeded => RfvpError::CapacityExceeded,
        Ps2Status::EndOfFile => RfvpError::EndOfFile,
        Ps2Status::Backend => RfvpError::Backend,
    }
}

pub fn status_to_result(status: i32) -> rfvp::host_api::RfvpResult<()> {
    let status = Ps2Status::from_i32(status);
    if status == Ps2Status::Ok {
        Ok(())
    } else {
        Err(ps2_status_to_rfvp_error(status))
    }
}

pub const fn rfvp_error_to_status(err: RfvpError) -> i32 {
    match err {
        RfvpError::Io => Ps2Status::Io.as_i32(),
        RfvpError::NotFound => Ps2Status::NotFound.as_i32(),
        RfvpError::InvalidData => Ps2Status::InvalidData.as_i32(),
        RfvpError::InvalidArgument => Ps2Status::InvalidArgument.as_i32(),
        RfvpError::Unsupported => Ps2Status::Unsupported.as_i32(),
        RfvpError::OutOfMemory => Ps2Status::OutOfMemory.as_i32(),
        RfvpError::CapacityExceeded => Ps2Status::CapacityExceeded.as_i32(),
        RfvpError::EndOfFile => Ps2Status::EndOfFile.as_i32(),
        RfvpError::Backend => Ps2Status::Backend.as_i32(),
    }
}
