//! `FsAccessLog` logger implementation

use super::*;
use crate::ipc::sf;
use crate::result::*;
use crate::service;
use crate::service::fsp::srv::{self, IFileSystemProxyClient};

/// Represents a logger though `FsAccessLog`s (see [`output_access_log_to_sd_card`][`srv::FileSystemProxyService::output_access_log_to_sd_card`])
pub struct FsAccessLogLogger {
    service: Result<srv::FileSystemProxyService>,
}

impl Logger for FsAccessLogLogger {
    fn new() -> Self {
        Self {
            service: service::new_service_object(),
        }
    }

    fn log(&mut self, metadata: &LogMetadata) {
        let msg = format_plain_string_log_impl(metadata, "FsAccessLog");
        if let Ok(ref mut fspsrv) = self.service {
            let _ = fspsrv.output_access_log_to_sd_card(sf::Buffer::from_array(msg.as_bytes()));
        }
    }
}
