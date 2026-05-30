use rfvp::host_api::RfvpError;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PspStatus {
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

impl PspStatus {
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

pub const fn psp_status_to_rfvp_error(status: PspStatus) -> RfvpError {
    match status {
        PspStatus::Ok => RfvpError::Backend,
        PspStatus::Io => RfvpError::Io,
        PspStatus::NotFound => RfvpError::NotFound,
        PspStatus::InvalidData => RfvpError::InvalidData,
        PspStatus::InvalidArgument => RfvpError::InvalidArgument,
        PspStatus::Unsupported => RfvpError::Unsupported,
        PspStatus::OutOfMemory => RfvpError::OutOfMemory,
        PspStatus::CapacityExceeded => RfvpError::CapacityExceeded,
        PspStatus::EndOfFile => RfvpError::EndOfFile,
        PspStatus::Backend => RfvpError::Backend,
    }
}

pub fn status_to_result(status: i32) -> rfvp::host_api::RfvpResult<()> {
    let status = PspStatus::from_i32(status);
    if status == PspStatus::Ok {
        Ok(())
    } else {
        Err(psp_status_to_rfvp_error(status))
    }
}

pub const fn rfvp_error_to_status(err: RfvpError) -> i32 {
    match err {
        RfvpError::Io => PspStatus::Io.as_i32(),
        RfvpError::NotFound => PspStatus::NotFound.as_i32(),
        RfvpError::InvalidData => PspStatus::InvalidData.as_i32(),
        RfvpError::InvalidArgument => PspStatus::InvalidArgument.as_i32(),
        RfvpError::Unsupported => PspStatus::Unsupported.as_i32(),
        RfvpError::OutOfMemory => PspStatus::OutOfMemory.as_i32(),
        RfvpError::CapacityExceeded => PspStatus::CapacityExceeded.as_i32(),
        RfvpError::EndOfFile => PspStatus::EndOfFile.as_i32(),
        RfvpError::Backend => PspStatus::Backend.as_i32(),
    }
}
