use rfvp::host_api::RfvpError;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WiiStatus {
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

impl WiiStatus {
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

pub const fn wii_status_to_rfvp_error(status: WiiStatus) -> RfvpError {
    match status {
        WiiStatus::Ok => RfvpError::Backend,
        WiiStatus::Io => RfvpError::Io,
        WiiStatus::NotFound => RfvpError::NotFound,
        WiiStatus::InvalidData => RfvpError::InvalidData,
        WiiStatus::InvalidArgument => RfvpError::InvalidArgument,
        WiiStatus::Unsupported => RfvpError::Unsupported,
        WiiStatus::OutOfMemory => RfvpError::OutOfMemory,
        WiiStatus::CapacityExceeded => RfvpError::CapacityExceeded,
        WiiStatus::EndOfFile => RfvpError::EndOfFile,
        WiiStatus::Backend => RfvpError::Backend,
    }
}

pub fn status_to_result(status: i32) -> rfvp::host_api::RfvpResult<()> {
    let status = WiiStatus::from_i32(status);
    if status == WiiStatus::Ok {
        Ok(())
    } else {
        Err(wii_status_to_rfvp_error(status))
    }
}

pub const fn rfvp_error_to_status(err: RfvpError) -> i32 {
    match err {
        RfvpError::Io => WiiStatus::Io.as_i32(),
        RfvpError::NotFound => WiiStatus::NotFound.as_i32(),
        RfvpError::InvalidData => WiiStatus::InvalidData.as_i32(),
        RfvpError::InvalidArgument => WiiStatus::InvalidArgument.as_i32(),
        RfvpError::Unsupported => WiiStatus::Unsupported.as_i32(),
        RfvpError::OutOfMemory => WiiStatus::OutOfMemory.as_i32(),
        RfvpError::CapacityExceeded => WiiStatus::CapacityExceeded.as_i32(),
        RfvpError::EndOfFile => WiiStatus::EndOfFile.as_i32(),
        RfvpError::Backend => WiiStatus::Backend.as_i32(),
    }
}
