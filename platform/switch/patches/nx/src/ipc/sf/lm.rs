use crate::ipc::sf;
use crate::version;

define_bit_set! {
    LogDestination (u32) {
        Tma = bit!(0),
        Uart = bit!(1),
        UartSleeping = bit!(2),
        All = 0xFFFF
    }
}

#[nx_derive::ipc_trait]
#[default_client]
pub trait Logger {
    #[ipc_rid(0)]
    fn log(&self, log_buf: sf::InAutoSelectBuffer<'_, u8>);
    #[ipc_rid(1)]
    #[version(version::VersionInterval::from(version::Version::new(3, 0, 0)))]
    fn set_destination(&mut self, log_destination: LogDestination);
}

#[nx_derive::ipc_trait]
pub trait Logging {
    #[ipc_rid(0)]
    #[return_session]
    fn open_logger(&mut self, process_id: sf::ProcessId) -> Logger;
}
