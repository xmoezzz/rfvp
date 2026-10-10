//! `LogManager` logger implementation

use alloc::string::ToString;

use super::*;
use crate::ipc::sf;
use crate::rrt0;
use crate::service;
use crate::service::lm::{self, ILoggerClient, ILoggingClient, LoggingService};
use crate::svc;

/// Represents a logger through [`LogService`][`lm::LoggingService`] services
pub struct LmLogger {
    logger: Option<lm::Logger>,
}

impl Logger for LmLogger {
    fn new() -> Self {
        Self {
            logger: service::new_service_object::<LoggingService>()
                .map(|mut log_srv| log_srv.open_logger(sf::ProcessId::new()).ok())
                .ok()
                .flatten(),
        }
    }

    fn log(&mut self, metadata: &LogMetadata) {
        if let Some(logger_obj) = self.logger.as_mut() {
            let mut log_packet = logpacket::LogPacket::new();

            if let Ok(process_id) = svc::get_process_id(svc::CURRENT_PROCESS_PSEUDO_HANDLE) {
                log_packet.set_process_id(process_id);
            }

            let cur_thread = unsafe { thread::current().as_ref().unwrap() };
            if let Ok(thread_id) = cur_thread.id() {
                log_packet.set_thread_id(thread_id);
            }

            log_packet.set_file_name(String::from(metadata.file_name));
            log_packet.set_function_name(String::from(metadata.fn_name));
            log_packet.set_line_number(metadata.line_number);

            let mod_name = rrt0::get_module_name()
                .get_name()
                .get_string()
                .unwrap_or("aarch64-switch-rs (invalid module name)".to_string());
            log_packet.set_module_name(mod_name);

            log_packet.set_text_log(metadata.msg.clone());

            let thread_name = cur_thread
                .name
                .get_str()
                .unwrap_or("aarch64-switch-rs (invalid thread name)");
            log_packet.set_thread_name(String::from(thread_name));

            for packet in log_packet.encode_packet() {
                let _ = logger_obj.log(sf::Buffer::from_array(&packet));
            }
        }
    }
}
