use crate::ipc::sf;
use crate::result::*;

use nx_derive::{Request, Response};

#[derive(Request, Response, Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum FatalPolicy {
    ErrorReportAndErrorScreen,
    ErrorReport,
    ErrorScreen,
}

#[nx_derive::ipc_trait]
pub trait Fatal {
    #[ipc_rid(1)]
    fn throw_fatal_with_policy(
        &self,
        rc: ResultCode,
        policy: FatalPolicy,
        process_id: sf::ProcessId,
    );
}
