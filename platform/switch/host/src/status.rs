use nx::result::{ResultBase, ResultCode, ResultSuccess};
use rfvp::host_api::RfvpError;

pub fn horizon_status_to_result_code(err: RfvpError) -> ResultCode {
    match err {
        RfvpError::Io => ResultCode::new(0x2430_0001),
        RfvpError::NotFound => ResultCode::new(0x2430_0002),
        RfvpError::InvalidData => ResultCode::new(0x2430_0003),
        RfvpError::InvalidArgument => ResultCode::new(0x2430_0004),
        RfvpError::OutOfMemory => ResultCode::new(0x2430_0006),
        RfvpError::CapacityExceeded => ResultCode::new(0x2430_0007),
        RfvpError::EndOfFile => ResultCode::new(0x2430_0008),
        RfvpError::Backend => ResultCode::new(0x2430_0009),
        _ => ResultCode::new(0x2430_0009),
    }
}

pub fn result_code_ok() -> ResultCode {
    ResultSuccess::make()
}
