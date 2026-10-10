use rfvp::host_api::RfvpError;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreeDsStatus {
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

impl ThreeDsStatus {
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

pub const fn three_ds_status_to_rfvp_error(status: ThreeDsStatus) -> RfvpError {
    match status {
        ThreeDsStatus::Ok => RfvpError::Backend,
        ThreeDsStatus::Io => RfvpError::Io,
        ThreeDsStatus::NotFound => RfvpError::NotFound,
        ThreeDsStatus::InvalidData => RfvpError::InvalidData,
        ThreeDsStatus::InvalidArgument => RfvpError::InvalidArgument,
        ThreeDsStatus::Unsupported => RfvpError::Unsupported,
        ThreeDsStatus::OutOfMemory => RfvpError::OutOfMemory,
        ThreeDsStatus::CapacityExceeded => RfvpError::CapacityExceeded,
        ThreeDsStatus::EndOfFile => RfvpError::EndOfFile,
        ThreeDsStatus::Backend => RfvpError::Backend,
    }
}

pub fn status_to_result(status: i32) -> rfvp::host_api::RfvpResult<()> {
    let status = ThreeDsStatus::from_i32(status);
    if status == ThreeDsStatus::Ok {
        Ok(())
    } else {
        Err(three_ds_status_to_rfvp_error(status))
    }
}

pub const fn rfvp_error_to_status(err: RfvpError) -> i32 {
    match err {
        RfvpError::Io => ThreeDsStatus::Io.as_i32(),
        RfvpError::NotFound => ThreeDsStatus::NotFound.as_i32(),
        RfvpError::InvalidData => ThreeDsStatus::InvalidData.as_i32(),
        RfvpError::InvalidArgument => ThreeDsStatus::InvalidArgument.as_i32(),
        RfvpError::Unsupported => ThreeDsStatus::Unsupported.as_i32(),
        RfvpError::OutOfMemory => ThreeDsStatus::OutOfMemory.as_i32(),
        RfvpError::CapacityExceeded => ThreeDsStatus::CapacityExceeded.as_i32(),
        RfvpError::EndOfFile => ThreeDsStatus::EndOfFile.as_i32(),
        RfvpError::Backend => ThreeDsStatus::Backend.as_i32(),
    }
}
